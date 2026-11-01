#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Local synthetic-model execution of one explicitly supplied RAM HAL ELF.

No hardware/Flash/XIP qualification, no host MMIO writes or timebase preload.
QMP reset and fault-input changes are model-only controls. CPU execution starts
from the ELF entry. The diagnostic BSS is deliberately poisoned in ordinary RAM
before each boot; qtest never writes SFC/clock registers or initializes a timebase.
"""
import argparse
from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import time
from elftools.elf.elffile import ELFFile

sys.dont_write_bytecode = True
# The enclosing integration runner binds source, build receipt and ELF bytes.
# These scenarios grant synthetic read-only model evidence only.
IMMUTABLE = ('peer-sha256', 'peer-operations', 'peer-commits', 'peer-wel', 'peer-volatile-enable',
             'peer-sr1', 'peer-sr2', 'peer-nv-sr1', 'peer-nv-sr2', 'mutating-commands-started')


def inspect_elf(path):
    with path.open('rb') as stream:
        elf = ELFFile(stream)
        require(elf.elfclass == 32 and elf.little_endian and elf['e_machine'] == 'EM_RISCV', 'ELF architecture')
        require(elf['e_entry'] == 0x28000, 'RAM entry differs')
        table = elf.get_section_by_name('.symtab')
        matches = table.get_symbol_by_name('WS53_SFC_READ_MODEL')
        require(len(matches) == 1, 'diagnostic symbol absent/ambiguous')
        symbol = matches[0]
        require(symbol['st_value'] == 0x20050000 and symbol['st_size'] == 96, 'diagnostic ABI changed')
        loads = []
        for segment in elf.iter_segments():
            if segment['p_type'] != 'PT_LOAD':
                continue
            address, size = segment['p_paddr'], segment['p_memsz']
            require(segment['p_vaddr'] == address, 'load/execution address alias')
            require(any(start <= address and address+size <= end
                        for start, end in ((0x28000, 0x40000), (0x20050000, 0x20060000))),
                    'load outside explicit application RAM')
            loads.append({'address': address, 'file_size': segment['p_filesz'], 'memory_size': size,
                          'flags': segment['p_flags'], 'data_sha256': hashlib.sha256(segment.data()).hexdigest()})
        require(loads, 'no load segments')
    return {'entry': 0x28000, 'diagnostic_address': symbol['st_value'], 'diagnostic_size': symbol['st_size'],
            'loads': loads, 'flash_segments_present': False}


@contextmanager
def session(qemu, elf, out, values):
    with tempfile.TemporaryDirectory(prefix='sfc-hal-') as tmp:
        qt, qm = Path(tmp)/'qt', Path(tmp)/'qm'
        argv = [str(qemu), '-M', 'ws53', '-accel', 'tcg', '-S',
                '-icount', 'shift=7,align=off,sleep=off', '-display', 'none', '-monitor', 'none',
                '-serial', 'null', '-nic', 'none', '-qtest', f'unix:{qt},server=on,wait=off',
                '-qtest-log', '/dev/null', '-qmp', f'unix:{qm},server=on,wait=off',
                '-d', 'guest_errors', '-device', f'loader,file={elf},cpu-num=0', *flags(values)]
        (out/'command.json').write_text(json.dumps(argv, indent=2)+'\n')
        with (out/'stderr.log').open('wb') as err, (out/'traffic.jsonl').open('w') as traffic:
            process = subprocess.Popen(argv, stdout=subprocess.DEVNULL, stderr=err)
            try:
                with connect(qt, process) as bus, connect(qm, process) as control, \
                     bus.makefile('rwb', buffering=0) as io, control.makefile('rwb', buffering=0) as qmio:
                    require('QMP' in json.loads(qmio.readline()), 'QMP greeting')
                    peer = Peer(io, qmio, traffic)
                    peer.qmp('qmp_capabilities')
                    yield peer
                    peer.qmp('quit')
                    require(process.wait(timeout=5) == 0, 'QEMU did not exit cleanly')
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)


def diagnostic(p):
    raw = bytes.fromhex(p.qt('read 0x20050000 96')[0].removeprefix('0x'))
    phase, error = struct.unpack_from('<II', raw)
    first_error, followup_error, followup_read_error, followup_buffer_intact = struct.unpack_from('<IIII', raw, 80)
    return {'phase': phase, 'error': error, 'jedec': list(raw[8:12]), 'status': list(raw[12:16]),
            'data': raw[16:80].hex(), 'first_error': first_error, 'followup_error': followup_error,
            'followup_read_error': followup_read_error, 'followup_buffer_intact': followup_buffer_intact}


def observe_terminal(p):
    deadline = time.monotonic()+10
    p.qmp('cont')
    while True:
        phase = int(p.qt('readl 0x20050000')[0], 0)
        if phase in (2, 3):
            break
        require(time.monotonic() < deadline, 'guest did not publish bounded completion')
        time.sleep(.002)
    p.qmp('stop')
    return diagnostic(p)


def run_case(args, name, values, expected_error, first_error, *, stalled=False):
    out = args.output/name
    out.mkdir(exist_ok=False)
    result = {'name': name, 'passed': False, 'values': values, 'controller_stall': stalled,
              'same_elf': True, 'host_mmio_writes': False, 'diagnostic_bss_poisoned': True, 'rounds': []}
    try:
        with session(args.qemu, args.elf, out, values) as p:
            before = {key: p.get(key) for key in IMMUTABLE}
            for iteration in range(3 if stalled else 2):
                recovery = stalled and iteration == 2
                p.fault('controller-stall', stalled and not recovery)
                p.qmp('system_reset')
                # A stale terminal phase cannot pass the next round before the
                # guest has run. The runtime must clear its own diagnostic BSS;
                # this is a RAM-only negative fixture, not register initialization.
                poison = bytes(4)+b'\xa5'*92
                p.qt(f'write 0x20050000 96 0x{poison.hex()}')
                counts = {k: p.get(k) for k in ('commands-started', 'commands-completed')}
                data = observe_terminal(p)
                target_error = 0 if recovery else expected_error
                require(data['phase'] == (2 if target_error == 0 else 3) and data['error'] == target_error,
                        'terminal report mismatch: '+repr(data))
                if target_error == 0:
                    require(data['jedec'] == [0x7f, 0x53, 1, 0] and data['status'] == [0, 0, 0xa7, 0]
                            and data['data'] == 'ff'*64 and data['first_error'] == data['followup_error'] ==
                            data['followup_read_error'] == data['followup_buffer_intact'] == 0,
                            'normal payload mismatch')
                    expected_started = expected_completed = 9
                elif target_error == 1:
                    require(data['first_error'] == first_error and data['followup_error'] ==
                            data['followup_read_error'] == data['followup_buffer_intact'] == 0,
                            'admission error attribution differs')
                    expected_started = expected_completed = 0
                else:
                    require(data['first_error'] == first_error and data['followup_error'] ==
                            data['followup_read_error'] == data['followup_buffer_intact'] == 1,
                            'deadline/poison attribution differs')
                    expected_started, expected_completed = 1, 0
                if target_error:
                    require(data['jedec'] == data['status'] == [0]*4 and data['data'] == '00'*64,
                            'failure published successful payload')
                delta = {k: p.get(k)-v for k, v in counts.items()}
                require(delta == {'commands-started': expected_started, 'commands-completed': expected_completed},
                        'unexpected command count: '+repr(delta))
                require(p.get('rejections') == 0, 'unexpected model register/opcode rejection')
                after = {key: p.get(key) for key in IMMUTABLE}
                require(after == before and after['mutating-commands-started'] == 0,
                        'HAL modified NOR or write-enable state')
                controller = {'config': p.rd(0x300), 'instruction': p.rd(0x308), 'address': p.rd(0x30c),
                              'data': p.words(), 'global': p.rd(0x100), 'dma': p.rd(0x240),
                              'clock': int(p.qt('readl 0x52000570')[0], 0)}
                require([controller['global'], controller['dma'], controller['clock']] ==
                        [values['global_value'], values['dma'], values['clock']], 'HAL changed configuration')
                require(bool(controller['config'] & 1) == (target_error == 2), 'unexpected pending START')
                if target_error == 2:
                    require(controller['instruction'] == 0x05 and controller['data'] == [0]*16,
                            'stalled readiness command/data changed')
                # The actual follow-up is performed by the firmware, not by the
                # runner. A further CPU interval confirms no hidden late command.
                terminal_counts = {k: p.get(k) for k in counts}
                p.qmp('cont')
                time.sleep(.01)
                p.qmp('stop')
                require(diagnostic(p) == data and {k: p.get(k) for k in counts} == terminal_counts,
                        'terminal loop issued a later command or changed result')
                result['rounds'].append({'iteration': iteration, 'recovery_after_stall_clear': recovery,
                    'diagnostic': data, 'command_delta': delta, 'controller': controller,
                    'nor_before': before, 'nor_after': after,
                    'subsequent_poison_call_executed': target_error == 2,
                    'no_command_after_completion_interval': True})
        check_diagnostics(out, assumed=True, enabled=True, rejections=0, status3=True)
        result['passed'] = True
    except BaseException as error:
        result['error'] = repr(error)
        raise
    finally:
        (out/'report.json').write_text(json.dumps(result, indent=2)+'\n')
    return result



def run(args):
    import sys
    sys.path.insert(0, str(args.model_source/'scripts'))
    global Peer, connect, require, sha, flags, check_diagnostics
    from ws53_sfc_contract_test import Peer, connect, require, sha
    from ws53_sfc_snapshot_test import flags, check_diagnostics
    args.output.mkdir(parents=True, exist_ok=False)
    report = dict(passed=False, hardware_accessed=False, flash_programmed=False,
                  physical_drain_or_ownership_qualified=False, elf=inspect_elf(args.elf),
                  qemu_sha256=sha(args.qemu), elf_sha256=sha(args.elf), cases=[])
    try:
        good = {'global_value': 0, 'dma': 0, 'clock': 0xd, 'status3': 0xa7}
        for name, values, error, reason, stalled in [
                ('normal-source6', good, 0, 0, False),
                ('normal-source0', good | {'clock': 1}, 0, 0, False),
                ('clock-disabled', good | {'clock': 0xc}, 1, 0x103, False),
                ('clock-source-unknown', good | {'clock': 7}, 1, 0x104, False),
                ('dma-busy', good | {'dma': 1}, 1, 0x102, False),
                ('spi-mode-unsupported', good | {'global_value': 1}, 1, 0x105, False),
                ('four-byte-unsupported', good | {'global_value': 4}, 1, 0x106, False),
                ('controller-stall-recovery', good, 2, 6, True)]:
            report['cases'].append(run_case(args, name, values, error, reason, stalled=stalled))
        report['passed'] = True
    except BaseException as error:
        report['error'] = repr(error)
        raise
    finally:
        (args.output/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    return report
