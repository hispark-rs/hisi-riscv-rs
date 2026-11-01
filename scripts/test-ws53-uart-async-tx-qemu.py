#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Actual WS53 async TX/flush/cancellation via the modeled UART pads; no hardware."""
import argparse
import hashlib
import json
from pathlib import Path
from ws53_qemu_fixture import run_fixture

BASE = b'WS53 RAM data/bss OK\r\nWS53 PASS timer_irq=53 count=3\r\n'
ARM = b'WS53 UART2 TX RX ARM\r\n'
PASS = b'WS53 PASS uart2_async_tx bytes=257 cancel=3 guards=3 rx=3 flush=1 wake=1\r\n'
EXPECTED = (BASE + b'WS53 UART2 ASYNC TX DATA ' + bytes(97+i % 26 for i in range(257))
            + b'\r\nWS53 UART2 TX CANCEL FULL ' + b'C'*64
            + b'\r\nWS53 UART2 TX CANCEL LATCHED ' + b'L'*64
            + b'\r\nWS53 UART2 TX CANCEL FLUSH F\r\n' + ARM
            + b'WS53 UART2 TX RX HELD\r\n' + PASS)


def run(args):
    args.output.mkdir(parents=True, exist_ok=False)
    checks = []
    for port in range(3):
        for prop in ('rx-peer-frames', 'pad-rx-frames'):
            checks.append((f'/machine/uart{port}', prop, 3 if port == 2 else 0))
        checks.append((f'/machine/uart{port}', 'pad-rx-aborted', 0))
    checks.append(('/machine/pinctrl', 'uart-tx-bytes-33', len(EXPECTED)))
    diagnostics = [
        'ws53-uart: assumed byte DMA requests, FIFO64 and timed serializer',
        'ws53-uart: TX pad8N1 profile assumes stable32MHz; RX separate profile',
        'ws53-uart: RX pad8N1 center sampling assumes stable32MHz; physical timing unqualified',
        'ws53-uart: host8N1 peer drives fixed RX board pad; no flow control',
    ] * 3
    result = run_fixture(str(args.qemu.resolve()), args.elf.resolve(), device='uart-pads',
        begin='', passed='', done=PASS.decode().removesuffix('\n'), expected_output=EXPECTED,
        input_stages=((ARM, b'\x00\xff\x53'),), qom_checks=checks,
        register_checks=((0x57036004, 1), (0x57036008, 1), (0x57036024, 4),
                         (0x57036028, 4), (0x57014018, 0), (0x5701404c, 0)),
        qemu_options=('-global', 'ws53-uart.assume-dma-profile=on',
                      '-global', 'ws53-uart.assume-pad-tx=on',
                      '-global', 'ws53-uart.assume-pad-rx=on',
                      '-global', 'ws53-uart.assume-rx-peer=on',
                      '-global', 'ws53-pinctrl.assume-uart-tx=on',
                      '-global', 'ws53-pinctrl.assume-uart-rx=on',
                      '-global', 'ws53-pinctrl.uart-console-profile=on'),
        expected_diagnostics=diagnostics)
    result.update(passed=True, hardware_accessed=False, physical_timing_qualified=False,
                  qemu_sha256=hashlib.sha256(args.qemu.read_bytes()).hexdigest(),
                  elf_sha256=hashlib.sha256(args.elf.read_bytes()).hexdigest())
    (args.output/'report.json').write_text(json.dumps(result, indent=2)+'\n')
    (args.output/'uart.bin').write_bytes(result['uart_raw'].encode())
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--elf', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
