#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Exercise actual WS63 HAL RX/IRQ/cancellation using a virtual UART1 peer."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile
import time
from qemu_test_support import connect, terminate

STAGES = [(b'UART ASYNC: ARM\r\n', b'\x00\xff\x53'),
          (b'UART ASYNC: CANCEL\r\n', b'\x3c'),
          (b'UART ASYNC: REARM\r\n', b'\x7e')]
PASS = b'UART ASYNC: PASS irq=54 wakes=3 burst=3 cancel=1 empty=1\r\n'


def run(args):
    args.output.mkdir(parents=True, exist_ok=False)
    output, errors = args.output/'uart.log', args.output/'qemu.log'
    sent = set()
    report = {'passed':False, 'hardware_accessed':False,
              'fifo_capacity':args.capacity, 'physical_fifo_depth_verified':False}
    with tempfile.TemporaryDirectory(prefix='ws63-uart-async-') as tmp:
        peer = Path(tmp)/'peer'
        command = [str(args.qemu), '-M', 'ws63', '-display', 'none', '-monitor', 'none',
                   '-serial', 'stdio', '-chardev', f'socket,id=peer,path={peer},server=on,wait=off',
                   '-serial', 'chardev:peer', '-serial', 'null',
                   '-global', f'ws63-uart.rx-fifo-size={args.capacity}',
                   '-kernel', str(args.elf), '-icount', 'shift=0,sleep=off', '-d', 'guest_errors']
        report['command'] = command
        with output.open('wb') as stdout, errors.open('wb') as stderr:
            proc = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr)
            try:
                with connect(peer, proc) as sock:
                    deadline = time.monotonic()+15
                    while proc.poll() is None:
                        raw = output.read_bytes()
                        if b'FAIL' in raw or PASS in raw:
                            break
                        for index, (marker, payload) in enumerate(STAGES):
                            if marker in raw and index not in sent:
                                sock.sendall(payload)
                                sent.add(index)
                        if time.monotonic() >= deadline:
                            report['timeout'] = True
                            break
                        time.sleep(.001)
            except (OSError, ValueError) as error:
                report['error'] = str(error)
            finally:
                terminate(proc)
        raw = output.read_bytes()
        lines = errors.read_text().splitlines()
        unexpected = [line for line in lines if not line.startswith('ws63-uart: baud divider set ')
                      and not re.fullmatch(r'qemu-system-riscv32: terminating on signal 15 from pid \d+ .*', line)]
        report.update(passed=raw == b''.join(m for m,_ in STAGES)+PASS and len(sent)==3 and not unexpected,
                      input_stages=len(sent), stdout=raw.decode(errors='replace'), stderr=lines,
                      unexpected_diagnostics=unexpected, returncode=proc.returncode,
                      elf_sha256=hashlib.sha256(args.elf.read_bytes()).hexdigest(),
                      qemu_sha256=hashlib.sha256(args.qemu.read_bytes()).hexdigest())
    (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu',type=Path,required=True)
    parser.add_argument('--elf',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--capacity',type=int,choices=(1,16),default=16)
    args=parser.parse_args()
    for name in ('qemu','elf','output'):
        setattr(args,name,getattr(args,name).resolve())
    raise SystemExit(run(args))
