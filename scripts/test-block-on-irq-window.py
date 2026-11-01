#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""QEMU-only deterministic IRQ/WFI diagnostic; reproduction is NOT qualification."""
import argparse
from contextlib import closing
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile
import time
from elftools.elf.elffile import ELFFile
from qemu_test_support import Monitor, connect, require, terminate
from ws53_qemu_rsp import Rsp


def layout(path):
    with path.open('rb') as stream:
        elf = ELFFile(stream)
        require(elf['e_machine'] == 'EM_RISCV' and elf.elfclass == 32, 'RV32 ELF required')
        table = elf.get_section_by_name('.symtab')
        def one(name):
            rows = table.get_symbol_by_name(name)
            require(rows is not None and len(rows) == 1, 'missing/ambiguous symbol '+name)
            return rows[0]
        names = ('irq_race_wait', 'irq_race_done', 'SOFT_INT0', 'IRQ_RACE_STATE')
        result = {name:one(name)['st_value'] for name in names}
        require(one('IRQ_RACE_STATE')['st_size'] == 16 and result['IRQ_RACE_STATE'] % 4 == 0,
                'wrong observation layout')
        notify = [s for s in table.iter_symbols() if 'BLOCK_ON_NOTIFIED' in s.name]
        require(len(notify) == 1 and notify[0]['st_size'] == 1, 'wrong notification representation')
        result['notify'] = notify[0]['st_value']
        fn = one('irq_race_wait'); section = elf.get_section(fn['st_shndx'])
        require(section['sh_flags'] & 4 and fn['st_size'] > 0, 'wait must be executable')
        start = fn['st_value']-section['sh_addr']
        code = section.data()[start:start+fn['st_size']]
        offsets, pos = [], 0
        while pos < len(code):
            half = int.from_bytes(code[pos:pos+2], 'little')
            length = 4 if half & 3 == 3 else 2
            require(pos+length <= len(code) and (length == 2 or half & 31 != 31),
                    'unsupported instruction encoding')
            if code[pos:pos+length] == b'\x73\x00\x50\x10': offsets.append(pos)
            pos += length
        require(len(offsets) == 1, 'fixture must contain one actual HAL WFI')
        result['wfi'] = fn['st_value']+offsets[0]
        result['code_sha256'] = hashlib.sha256(code).hexdigest()
        result['code_size'] = len(code)
        return result


def run(args):
    symbols = layout(args.elf)
    args.output.mkdir(parents=True, exist_ok=False)
    report = dict(hardware_accessed=False, race_reproduced=False, correctness_passed=False,
                  timing=args.timing, symbols=symbols, injected_interrupts=0,
                  elf_sha256=hashlib.sha256(args.elf.read_bytes()).hexdigest(),
                  qemu_sha256=hashlib.sha256(args.qemu.read_bytes()).hexdigest())
    with tempfile.TemporaryDirectory(prefix='irq-window-') as tmp:
        tmp = Path(tmp)
        command = [str(args.qemu), '-M','ws63','-S','-display','none','-monitor','none',
                   '-serial','stdio','-kernel',str(args.elf),'-d','guest_errors',
                   '-qmp',f'unix:{tmp}/qmp.sock,server=on,wait=off',
                   '-qtest',f'unix:{tmp}/qtest.sock,server=on,wait=off','-qtest-log','/dev/null',
                   '-gdb',f'unix:{tmp}/gdb,server=on,wait=off']
        report['command'] = command
        with (args.output/'uart.log').open('wb') as out, (args.output/'qemu.log').open('wb') as err:
            proc = subprocess.Popen(command, stdout=out, stderr=err, stdin=subprocess.DEVNULL)
            try:
                with closing(Monitor(tmp,proc)) as monitor, connect(tmp/'gdb',proc) as sock:
                    debug = Rsp(sock)
                    def code_digest():
                        code = bytes(int(monitor.command(f'readb {symbols["irq_race_wait"]+i:#x}')[0],0)
                                     for i in range(symbols['code_size']))
                        return hashlib.sha256(code).hexdigest()
                    require(code_digest() == symbols['code_sha256'], 'loaded code differs from ELF')
                    def state(): return monitor.words(symbols['IRQ_RACE_STATE'],4)
                    def notify(): return int(monitor.command(f'readb {symbols["notify"]:#x}')[0],0)
                    def inject():
                        # Sole host mutation: set the guest-owned modeled SOFT_INT0 source.
                        monitor.command('writel 0x44000154 1')
                        report['injected_interrupts'] += 1
                    def registers():
                        text = monitor.execute('human-monitor-command', {'command-line':'info registers'})
                        match = re.search(r'\bpc\s+([0-9a-fA-F]+)',text)
                        require(match is not None, 'PC missing from CPU observation')
                        return dict(pc=int(match[1],16), text=text)
                    debug.breakpoint(symbols['wfi'],True)
                    debug.resume(); debug.stopped(wait=True); debug.expect_pc(symbols['wfi'])
                    require(state()[:3] == [1,0,0] and state()[3] & 8 and notify() == 0,
                            'did not reach first Pending with MIE enabled and no wake')
                    require(monitor.read(0x4400015c) == 0, 'unexpected initial software IRQ')
                    report['before'] = dict(state=state(),pc=debug.pc(),notified=notify())
                    debug.breakpoint(symbols['wfi'],False)
                    if args.timing == 'before-wfi':
                        debug.breakpoint(symbols['SOFT_INT0'],True)
                        inject(); debug.resume(); debug.stopped(wait=True)
                        debug.expect_pc(symbols['SOFT_INT0'])
                        debug.breakpoint(symbols['SOFT_INT0'],False)
                        debug.breakpoint(symbols['wfi'],True)
                        debug.resume(); debug.stopped(wait=True); debug.expect_pc(symbols['wfi'])
                        require(state()[:3] == [1,1,0] and notify() == 1
                                and monitor.read(0x4400015c) == 0,
                                'first IRQ must actually signal and clear before WFI')
                        report['after_irq_before_wfi'] = dict(state=state(),pc=debug.pc(),notified=notify())
                        debug.breakpoint(symbols['wfi'],False)
                    debug.breakpoint(symbols['irq_race_done'],True)
                    debug.resume()
                    # Bounded observation, not a physical latency or halted-state proof.
                    time.sleep(.1)
                    premature = debug.stopped()
                    if args.timing == 'before-wfi':
                        require(not premature, 'race no longer reproduces; review/update diagnostic')
                        cpu = registers()
                        require(cpu['pc'] == symbols['wfi']+4 and state()[:3] == [1,1,0]
                                and notify() == 1 and monitor.read(0x4400015c) == 0,
                                'expected recorded wake with no second poll after WFI')
                        report['race_reproduced'] = True
                    else:
                        require(not premature and state()[:3] == [1,0,0], 'control completed without IRQ')
                        cpu = registers(); require(cpu['pc'] == symbols['wfi']+4, 'control not waiting')
                    report['observed_wait'] = dict(state=state(),notified=notify(),cpu=cpu,seconds=.1)
                    inject(); debug.stopped(wait=True); debug.expect_pc(symbols['irq_race_done'])
                    require(state()[:3] == [2,report['injected_interrupts'],0] and notify() == 1,
                            'unrelated next IRQ did not release the wait as expected')
                    report['completion'] = dict(state=state(),pc=debug.pc(),notified=notify(),
                                                block_on_returned=True,done_callback_executed=False)
                    require(code_digest() == symbols['code_sha256'], 'execution breakpoints changed code')
                    report['code_unchanged'] = True
                    report['correctness_passed'] = args.timing == 'after-wfi'
                    monitor.execute('quit'); require(proc.wait(timeout=5)==0,'QEMU did not exit cleanly')
            except BaseException as error:
                report.update(error=str(error),error_type=type(error).__name__)
            finally:
                terminate(proc)
    errors=(args.output/'qemu.log').read_text().splitlines()
    report['unexpected_diagnostics']=[s for s in errors if not s.startswith('ws63-uart: baud divider set ')]
    report['diagnostic_completed']='error' not in report and not report['unexpected_diagnostics']
    (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))
    return 0 if report['diagnostic_completed'] and (report['correctness_passed'] or args.expect_race) else 1


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu',required=True,type=Path)
    parser.add_argument('--elf',required=True,type=Path)
    parser.add_argument('--output',required=True,type=Path)
    parser.add_argument('--timing',choices=['before-wfi','after-wfi'],required=True)
    parser.add_argument('--expect-race',action='store_true',help='Diagnostic success may exit 0; never changes correctness_passed')
    args=parser.parse_args()
    raise SystemExit(run(args))
