#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Actual HAL nb Write under synthetic TX backpressure, without hardware access."""
import argparse
from contextlib import closing
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile
import time
from qemu_test_support import Monitor, connect, terminate


def run(args):
    args.output.mkdir(parents=True, exist_ok=False)
    output, errors = args.output/'uart.log', args.output/'qemu.log'
    report = dict(passed=False, hardware_accessed=False, machine=args.machine,
                  capacity=args.capacity, physical_fifo_depth_verified=False,
                  serializer_timing_verified=False, legacy_async_tx_qualified=False)
    received = b''
    released = False
    with tempfile.TemporaryDirectory(prefix='hisi-uart-nb-') as tmp:
        directory = Path(tmp)
        peer = directory/'peer'
        command = [str(args.qemu), '-M', args.machine, '-S', '-display', 'none', '-monitor', 'none',
                   '-qmp', f'unix:{directory}/qmp.sock,server=on,wait=off',
                   '-qtest', f'unix:{directory}/qtest.sock,server=on,wait=off', '-qtest-log', '/dev/null',
                   '-serial', 'stdio', '-chardev', f'socket,id=peer,path={peer},server=on,wait=off',
                   '-serial', 'chardev:peer', '-serial', 'null',
                   '-global', f'ws63-uart.tx-fifo-size={args.capacity}',
                   '-kernel', str(args.elf), '-icount', 'shift=0,sleep=off', '-d', 'guest_errors']
        report['command'] = command
        with output.open('wb') as stdout, errors.open('wb') as stderr:
            proc = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr)
            try:
                with closing(Monitor(directory, proc)) as monitor, connect(peer, proc) as sock:
                    matches = []
                    for item in monitor.execute('qom-list', {'path': '/machine/unattached'}):
                        if item['type'] == 'child<ws63-uart>':
                            path = '/machine/unattached/'+item['name']
                            if monitor.execute('qom-get', {'path':path, 'property':'chardev'}) == 'peer':
                                matches.append(path)
                    assert len(matches) == 1, matches
                    monitor.execute('qom-set', {'path': matches[0], 'property':'tx-stalled', 'value':True})
                    monitor.execute('cont')
                    deadline = time.monotonic()+10
                    while proc.poll() is None:
                        raw = output.read_bytes()
                        if b'UART NB: FULL\r\n' in raw and not released:
                            monitor.execute('qom-set', {'path': matches[0], 'property':'tx-stalled', 'value':False})
                            released = True
                        if b'UART NB: PASS\r\n' in raw:
                            break
                        if time.monotonic() >= deadline:
                            report['timeout'] = True
                            break
                        time.sleep(.001)
                    if released:
                        sock.settimeout(2)
                        while len(received) < args.capacity+1:
                            part = sock.recv(args.capacity+1-len(received))
                            assert part, 'peer disconnected'
                            received += part
                    monitor.execute('quit')
                    proc.wait(timeout=5)
            except (OSError, ValueError, AssertionError, subprocess.TimeoutExpired) as error:
                report['error'] = str(error)
            finally:
                terminate(proc)
        raw = output.read_bytes()
        lines = errors.read_text().splitlines()
        unexpected = [line for line in lines if not line.startswith('ws63-uart: baud divider set ')
                      and not re.fullmatch(r'qemu-system-riscv32: terminating on signal 15 from pid \d+ .*', line)]
        report.update(passed=raw == b'UART NB: BEGIN\r\nUART NB: FULL\r\nUART NB: PASS\r\n'
                      and released and received == bytes(range(args.capacity))+b'\xa5' and not unexpected,
                      peer_hex=received.hex(), released=released, stdout=raw.decode(errors='replace'),
                      unexpected_diagnostics=unexpected, returncode=proc.returncode,
                      elf_sha256=hashlib.sha256(args.elf.read_bytes()).hexdigest(),
                      qemu_sha256=hashlib.sha256(args.qemu.read_bytes()).hexdigest())
    (args.output/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report, indent=2))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--elf', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--machine', choices=['ws63', 'bs21'], required=True)
    parser.add_argument('--capacity', type=int, choices=[1, 16], required=True)
    raise SystemExit(run(parser.parse_args()))
