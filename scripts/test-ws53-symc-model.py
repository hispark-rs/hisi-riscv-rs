#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32", "gmssl==3.2.2", "pycryptodomex==3.23.0"]
# ///
"""Execute production SYMC/KM Rust HAL, with host writes restricted to RAM mailbox.

No board access, hardware ownership claim, or CPU DMA handback is made. The host
oracle observes virtual RAM and QOM pending requests; Rust retains every buffer
and key reservation even on completion, cancellation, Drop and forget.
"""
import argparse
from contextlib import contextmanager
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import time
from elftools.elf.elffile import ELFFile
from ws53_qemu_fixture import connect, elf_check

NOTICES = ['ws53-km: assumed slot/clear-loader profile; no silicon ownership or erasure proof',
           'ws53-symc: assumed full-block REE/ring/ACK/DMA profile; no silicon handoff or physical drain proof']
PARAMS = dict(algorithm=5, mode=6, decrypt=7, channel=8, slot=9, parity=10, length=11, usage=12, budget=13)
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()


@contextmanager
def running(args, name, address):
    folder = args.output/name; folder.mkdir()
    log = []; stdout = stderr = b''
    with tempfile.TemporaryDirectory(prefix='ws53-symc-cpu-') as directory:
        tmp = Path(directory)
        argv = [str(args.qemu.resolve()), '-M', 'ws53', '-S', '-display', 'none', '-monitor', 'none',
            '-serial', 'null', '-kernel', str(args.elf.resolve()), '-global', 'ws53-km.assume-profile=on',
            '-global', 'ws53-symc.assume-profile=on', '-icount', 'shift=0,sleep=on',
            '-qtest', f'unix:{tmp}/qt,server=on,wait=off', '-qtest-log', '/dev/null',
            '-qmp', f'unix:{tmp}/qm,server=on,wait=off', '-d', 'guest_errors']
        proc = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            with connect(tmp/'qt', proc) as qs, qs.makefile('rwb', buffering=0) as qt, connect(tmp/'qm', proc) as ms, ms.makefile('rwb', buffering=0) as qm:
                json.loads(qm.readline())
                def qmp(name, arguments=None):
                    request = dict(execute=name)
                    if arguments is not None: request['arguments'] = arguments
                    qm.write(json.dumps(request).encode()+b'\n')
                    while True:
                        value = json.loads(qm.readline()); assert 'error' not in value, value
                        if 'return' in value:
                            log.append(dict(qmp=request, result=value['return']))
                            return value['return']
                def command(text):
                    parts = text.split()
                    if parts[0].startswith(('write', 'out')):
                        assert parts[0] == 'writel' and int(parts[1], 0) in {address+4*i for i in (0, *PARAMS.values())}, text
                    qt.write(text.encode()+b'\n'); value = qt.readline().decode().split()
                    assert value and value[0] == 'OK', (text, value)
                    log.append(dict(qtest=text, result=value[1:]))
                    return value[1:]
                def memory(a, n): return bytes.fromhex(command(f'read {a:#x} {n}')[0].removeprefix('0x'))
                def report():
                    value = struct.unpack('<32I', memory(address, 128))
                    assert value[14] == 0, ('Rust panic', value)
                    return value
                def wait(predicate):
                    deadline = time.monotonic()+5
                    while True:
                        value = report()
                        if predicate(value): return value
                        assert time.monotonic() < deadline, ('CPU timeout', value)
                        time.sleep(.001)
                def invoke(op, **params):
                    for key, value in params.items(): command(f'writel {address+4*PARAMS[key]:#x} {value}')
                    seq = report()[1]; command(f'writel {address:#x} {op}'); qmp('cont')
                    wait(lambda v: v[1] == seq+1); qmp('stop')
                    value = report(); assert value[0] == 0 and value[1] == seq+1
                    return value
                def get(name, device='symc'):
                    return qmp('qom-get', dict(path='/machine/'+device, property=name))
                def inject(name, value, device='symc'):
                    qmp('qom-set', dict(path='/machine/'+device, property='inject-'+name, value=value))
                def fault(ch, number, on=True):
                    command(f'set_irq_in /machine/symc fault {(ch-1)*9+number} {int(on)}')
                def advance():
                    # TCG time advances through CPU instructions, not qtest-only
                    # clock_step. The idle CPU has no command or MMIO work here.
                    qmp('cont')
                    deadline = time.monotonic()+5
                    while get('transfers') != 1:
                        assert time.monotonic() < deadline, 'late DMA never completed'
                        time.sleep(.001)
                    qmp('stop')
                def counts(): return (get('reads'), get('writes'), get('reads', 'km'), get('writes', 'km'))
                qmp('qmp_capabilities')
                try:
                    qmp('cont'); wait(lambda v: v[2] == 1); qmp('stop')
                    yield invoke, get, inject, fault, report, memory, counts, advance
                finally:
                    if proc.poll() is None: qmp('quit')
            stdout, stderr = proc.communicate(timeout=5)
            assert proc.returncode == 0 and not stdout
            lines = stderr.decode().splitlines()
            assert lines[:2] == NOTICES, lines
            assert all(line.startswith('ws53-symc: unimplemented/unavailable') for line in lines[2:]), lines
        finally:
            if proc.poll() is None: proc.terminate()
            remaining_out, remaining_err = proc.communicate(timeout=5)
            if not stdout: stdout = remaining_out
            if not stderr: stderr = remaining_err
            (folder/'stdout.log').write_bytes(stdout); (folder/'stderr.log').write_bytes(stderr)
            (folder/'operations.json').write_text(json.dumps(log, indent=2)+'\n')
            (folder/'command.json').write_text(json.dumps(argv, indent=2)+'\n')


def run(args):
    assert not args.output.exists()
    contract = elf_check.validate(args.elf)
    with args.elf.open('rb') as stream:
        elf = ELFFile(stream); table = elf.get_section_by_name('.symtab')
        symbols = {}
        for suffix, size in (('MODEL', 128), ('INPUT', 320), ('OUTPUT', 320), ('TABLE', 96)):
            entries = table.get_symbol_by_name('WS53_SYMC_'+suffix)
            assert len(entries) == 1 and entries[0]['st_size'] == size
            address = entries[0]['st_value']
            assert address % (4 if suffix == 'MODEL' else 32) == 0
            assert 0x20010000 <= address <= 0x20060000-size
            symbols[suffix] = address
    spec = importlib.util.spec_from_file_location('cpu_symc_reference', args.reference)
    reference = importlib.util.module_from_spec(spec); spec.loader.exec_module(reference)
    assert len(reference.known_answers()) == 13
    args.output.mkdir(parents=True)
    rows = []
    cases = []
    for ch in (1, 2):
        for algorithm in range(4):
            for mode in (1, 3, 6, 7, 8):
                for decrypt in (0, 1):
                    for parity in ((0, 1) if algorithm in (0, 3) else (0,)):
                        slot = (ch+algorithm+mode+parity) % 8
                        length = 16*(1+(ch+algorithm+mode+parity) % 16)
                        cases.append((f'normal-{ch}-{algorithm}-{mode}-{decrypt}-{parity}', 'normal',
                                      dict(channel=ch, algorithm=algorithm, mode=mode, decrypt=decrypt,
                                           parity=parity, slot=slot, length=length, usage=1+decrypt, budget=64)))
        for case in ('usage', 'capacity', 'clear-stall', 'reject-claim', 'ack-stuck', 'pre-error', 'key-error',
                     'dma-stall', 'read-error', 'write-error', 'suppress-done', 'cancel', 'drop', 'forget',
                     'owner-loss', 'runtime-error', 'cancel-posted'):
            params = dict(channel=ch, algorithm=0, mode=3, decrypt=0, parity=0, slot=ch-1,
                          length=512 if case == 'capacity' else 64, usage=2 if case == 'usage' else 3, budget=64)
            cases.append((f'{case}-{ch}', case, params))
    for name, case, params in cases:
        ch, slot = params['channel'], params['slot']
        with running(args, name, symbols['MODEL']) as (invoke, get, inject, fault, report, memory, counts, advance):
            value = invoke(1, **params)
            assert value[16:19] == (symbols['INPUT']+32, symbols['OUTPUT']+32, symbols['TABLE'])
            assert value[24:26] == (1 << slot, 1 << slot)
            input_data = bytes((i*17+params['mode']+slot) & 255 for i in range(256))
            assert memory(value[16], 256) == input_data
            descriptors = memory(value[18], 96)
            if case in ('usage', 'capacity'):
                assert value[4] == (22 if case == 'usage' else 24)
                assert get('reads') == get('writes') == 0 and descriptors == bytes(96)
            else:
                assert value[4] == 0 and value[3] == 1, (name, value)
                before = counts(); assert invoke(3)[4] == 1 and counts() == before
                faults = {'clear-stall': 0, 'reject-claim': 5, 'ack-stuck': 7,
                          'dma-stall': 1, 'read-error': 2, 'write-error': 3, 'suppress-done': 4}
                if case in faults: fault(ch, faults[case])
                if case in ('cancel', 'drop', 'forget', 'owner-loss', 'runtime-error'): fault(ch, 1)
                if case == 'cancel-posted': fault(ch, 8)
                if case == 'pre-error': inject('tee-cause', 0x80000000); inject('tee-status', 0x12345678)
                if case == 'key-error': inject('controller-error', 0x80000000, 'km')
                value = invoke(2)
                initial = {'clear-stall': 5, 'reject-claim': 3, 'ack-stuck': 6, 'pre-error': 8, 'key-error': 11}.get(case, 0)
                assert value[4] == initial, (name, value)
                if initial:
                    before = counts(); assert invoke(3)[4] == initial and counts() == before
                    assert memory(value[17], 256) == b'X'*256
                    if case == 'pre-error': assert value[27:29] == (0x80000000, 0x12345678)
                    if case == 'key-error': assert value[26] == 6
                elif case in ('drop', 'forget'):
                    before = counts(); value = invoke(6 if case == 'drop' else 7)
                    assert counts() == before and value[24:26] == (1 << slot, 1 << slot)
                    assert get('released-slots', 'km') == 0
                    fault(ch, 1, False); advance()
                    assert get('transfers') == 1
                else:
                    if case in ('cancel', 'cancel-posted'):
                        if case == 'cancel-posted': assert get('pending-reads') == 1
                        before = counts(); invoke(5); assert counts() == before
                    if case == 'owner-loss': fault(ch, 6)
                    if case == 'runtime-error': inject('ree-cause', 1); inject('ree-status', 0x87654321)
                    value = invoke(4)
                    expected = {'dma-stall': 9, 'read-error': 9, 'write-error': 9, 'suppress-done': 9,
                                'cancel': 10, 'cancel-posted': 10, 'owner-loss': 4, 'runtime-error': 8}.get(case, 0)
                    assert value[4] == expected, (name, value)
                    before = counts(); again = invoke(3)
                    assert again[4] == expected and counts() == before
                    if case == 'runtime-error': assert value[29:31] == (1, 0x87654321)
                    if expected == 0:
                        assert value[3] == 3 and value[19] == params['length']
                        assert get('transfers') == 1 and get('pending-reads') == get('pending-writes') == 0
                        algorithm = params['algorithm']; length = (16, 24, 32, 16)[algorithm]
                        key = bytes((i*13+slot*7+3) & 255 for i in range(length))
                        iv = bytes(16) if params['mode'] == 1 else bytes([255]*15+[254]) if params['mode'] == 6 else bytes(i*3+slot for i in range(16))
                        output, final_iv = reference.reference(5 if algorithm == 3 else 2, key, params['mode'],
                            input_data[:params['length']], iv, bool(params['decrypt']))
                        assert memory(value[17], 256) == output+b'X'*(256-len(output)), name
                        assert struct.pack('<4I', *value[20:24]) == final_iv
                    if case in ('cancel', 'cancel-posted', 'dma-stall'):
                        fault(ch, 8 if case == 'cancel-posted' else 1, False); advance()
                        assert get('transfers') == 1
                        before = counts(); assert invoke(3)[4] == expected and counts() == before
            value = report()
            assert memory(value[16], 256) == input_data and memory(value[18], 96) == descriptors
            for a, byte in ((value[16]-32, b'G'), (value[16]+256, b'H'),
                            (value[17]-32, b'I'), (value[17]+256, b'J')):
                assert memory(a, 32) == byte*32
            assert value[24] == 1 << slot and get('released-slots', 'km') == 0
            expected_rejections = 1 if case in ('read-error', 'write-error') else 0
            assert get('rejections') == expected_rejections and get('rejections', 'km') == 0, name
            rows.append(dict(name=name, case=case, parameters=params, final=list(value), passed=True))
        print('PASS '+name, flush=True)
    result = dict(passed=True, cases=len(rows), results=rows, elf_contract=contract,
        elf_sha256=sha(args.elf), qemu_sha256=sha(args.qemu), reference_sha256=sha(args.reference),
        runner_sha256=sha(Path(__file__)), hardware_accessed=False, host_writes_only_mailbox=True,
        no_memory_or_key_handback=True, physical_quiescence_proven=False)
    (args.output/'report.json').write_text(json.dumps(result, indent=2)+'\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'elf', 'reference', 'output'): parser.add_argument('--'+name, type=Path, required=True)
    run(parser.parse_args())
