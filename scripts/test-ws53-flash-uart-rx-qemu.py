#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Execute the controlled Flash UART2 async RX ELF and real IRQ42 pad path.

This models an observed FlashBoot handoff, not vendor BootROM/FlashBoot itself.
The peer drives digital AGPIO2 through the RX sampler; it does not push FIFO
bytes or write guest completion state. No board or Flash programming is used.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT/'scripts'/filename)
    result = importlib.util.module_from_spec(spec); spec.loader.exec_module(result)
    return result


import qemu_test_support as base
builder = module('uart_rx_builder', 'ws53_uart_rx_build.py')
require = base.require
STAGES = ((b'WS53 UART2 ASYNC ARM 0\r\n', b'\x00\xff\x51'),
          (b'WS53 UART2 ASYNC ARM 1\r\n', b'\xa5\x5a\x53'),
          (b'WS53 UART2 ASYNC CANCEL\r\n', b'\x3c'))
EXPECTED = (b'WS53 FLASH UART2 RX BEGIN clock_hz=32000000 baud=115200 pads=33,34 route=1 guards=2\r\n'
            +b''.join(marker for marker, _ in STAGES)
            +b'WS53 PASS uart2_async rx=7 wake=1 cancel=1 empty=1 guards=2\r\n'
            +b'WS53 FLASH UART2 RX PASS irq=42 rx=7 cancel=1\r\n')


def digest(path):
    with path.open('rb') as stream: return hashlib.file_digest(stream, 'sha256').hexdigest()


def save(path, value): path.write_text(json.dumps(value, indent=2)+'\n')


def diagnostics(text):
    expected = [
        'ws53-wdt: assumed timing low-byte=255 expiry=count+1 stage=reload clock=32000',
        'qemu-system-riscv32: ws53-uart-l0-route: explicit command1-to-acore profile; initial route, ccore destination and physical readback are not modeled',
    ]
    expected += [
        'ws53-uart: assumed byte DMA requests, FIFO64 and timed serializer',
        'ws53-uart: TX pad8N1 profile assumes stable32MHz; RX separate profile',
        'ws53-uart: RX pad8N1 center sampling assumes stable32MHz; physical timing unqualified',
        'ws53-uart: host8N1 peer drives fixed RX board pad; no flow control',
    ]*3
    expected += ['ws53-uart-reset: observed L0 profile; divisor zero and in-flight cancellation assumed']
    idle = 'qemu-system-riscv32: warning: icount sleep disabled and no active timers'
    lines = text.splitlines()
    require(lines.count(idle) <= 1 and sorted(x for x in lines if x != idle) == sorted(expected),
            'unexpected/missing UART model diagnostic: '+repr(lines))
    return lines


def run(args):
    build = builder.validate(args.build_receipt, args.elf)
    addresses = base.object_addresses(args.elf, dict(WS53_FLASH_UART_RX_PANIC=4,
        WS53_FLASH_UART_RX_PHASE=4, WS53_FLASH_CAPTURE_WINDOW=12), ram=builder.common.load_validator().RAM)
    args.output.mkdir(parents=True, exist_ok=False)
    uart, errors, trace = (args.output/name for name in ('uart.raw', 'qemu.stderr', 'pads.csv'))
    argv = [str(args.qemu), '-M', 'ws53,flashboot-handoff=on', '-S', '-display', 'none',
        '-monitor', 'none', '-serial', 'null', '-serial', 'null', '-serial', 'stdio',
        '-kernel', str(args.elf), '-global', 'ws53-wdt.assume-timing=on',
        '-global', 'ws53-uart-l0-route.assume-profile=on',
        '-global', 'ws53-uart-reset.assume-l0-profile=on',
        '-global', 'ws53-uart.assume-dma-profile=on',
        '-global', 'ws53-uart.assume-pad-tx=on', '-global', 'ws53-uart.assume-pad-rx=on',
        '-global', 'ws53-uart.assume-rx-peer=on',
        '-global', 'ws53-pinctrl.assume-uart-tx=on', '-global', 'ws53-pinctrl.assume-uart-rx=on',
        '-global', 'ws53-pinctrl.uart-console-profile=on',
        '-chardev', f'file,id=padtrace,path={trace}', '-global', 'ws53-pinctrl.pad-trace=padtrace',
        '-qtest', f'unix:{args.output/"qtest.sock"},server=on,wait=off', '-qtest-log', '/dev/null',
        '-qmp', f'unix:{args.output/"qmp.sock"},server=on,wait=off', '-d', 'guest_errors',
        '-icount', 'shift=8,align=off,sleep=off']
    save(args.output/'command.json', argv)
    report = dict(passed=False, hardware_accessed=False, flash_programmed=False,
        cold_boot='not_verified', elf_sha256=digest(args.elf), qemu_sha256=digest(args.qemu),
        build_receipt_sha256=build['receipt_sha256'], rounds=[], scenario=args.scenario,
        virtual_peer_injected_with_cpu_paused=True,
        script_sha256=digest(Path(__file__)), support_sha256=digest(Path(base.__file__)), physical_uart_timing_verified=False)
    with uart.open('wb') as stdout, errors.open('wb') as stderr:
        process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=stdout, stderr=stderr)
        monitor = None
        try:
            monitor = base.Monitor(args.output, process)
            count = args.rounds if args.scenario == 'normal' else 3
            for iteration in range(count):
                if iteration: monitor.execute('system_reset')
                fault = args.scenario if iteration < 2 else 'normal'
                monitor.command(f'set_irq_in /machine/uart-l0-route block-route 0 {int(fault == "blocked-route")}')
                start = uart.stat().st_size; trace_start = trace.stat().st_size
                require(monitor.read(addresses['WS53_FLASH_UART_RX_PANIC']) == 0xa5a5a5a5, 'BSS not poisoned')
                require(monitor.read(0x5703096c) == 0, 'control CPU/domain not held')
                require(monitor.execute('qom-get', dict(path='/machine/uart-l0-route', property='commands')) == 0,
                        'route unexpectedly preinitialized')
                monitor.execute('cont')
                sent = set(); observed_delay = False; deadline = time.monotonic()+180
                while True:
                    require(process.poll() is None, 'QEMU exited: '+errors.read_text())
                    output = uart.read_bytes()[start:]
                    require(time.monotonic() < deadline, 'Flash UART RX timed out: '+repr(output))
                    for index, (marker, payload) in enumerate(STAGES):
                        if fault == 'cancel-peer-timeout' and index == 2: continue
                        if index not in sent and marker in output:
                            require(output.count(marker) == 1, 'repeated input request')
                            # Pause only for deterministic virtual peer ingress:
                            # the main-loop chardev callback must start its frame
                            # at a synchronized icount timestamp. No guest RAM,
                            # registers, elapsed counters or result are changed.
                            monitor.execute('stop')
                            trace_before = trace.stat().st_size
                            process.stdin.write(payload); process.stdin.flush(); sent.add(index)
                            ingress_deadline = time.monotonic()+2
                            while trace.stat().st_size == trace_before:
                                require(time.monotonic() < ingress_deadline, 'virtual peer did not start frame')
                                time.sleep(.001)
                            monitor.execute('cont')
                    # Avoid APB reads of application RAM during ownership gates.
                    if monitor.read(0x57030234) != 0x3f8 or monitor.read(0x57030238) != 0x3c:
                        time.sleep(.01); continue
                    phase = monitor.read(addresses['WS53_FLASH_CAPTURE_WINDOW'])
                    if phase == 1:
                        require(not output, 'UART activity before autonomous delay')
                        observed_delay = True
                    panic = monitor.read(addresses['WS53_FLASH_UART_RX_PANIC'])
                    if panic not in (0, 0xa5a5a5a5): break
                    if monitor.read(addresses['WS53_FLASH_UART_RX_PHASE']) == 5: break
                    time.sleep(.01)
                monitor.execute('stop')
                output = uart.read_bytes()[start:]
                window = monitor.words(addresses['WS53_FLASH_CAPTURE_WINDOW'], 3)
                require(observed_delay and window[0] == 2 and window[1] == 960_000_000
                        and 960_000_000 <= window[2] <= 960_032_000, 'autonomous delay not proved')
                panic = monitor.read(addresses['WS53_FLASH_UART_RX_PANIC'])
                if fault == 'normal':
                    require(output == EXPECTED and len(sent) == 3, 'incomplete/unexpected transcript: '+repr(output))
                    require(panic == 0, 'panic published')
                    expected_frames = 7
                    expected_uart = EXPECTED
                    site = None
                else:
                    name = 'async-wake-timeout' if fault == 'blocked-route' else 'cancel-input-timeout'
                    site = build['fault_binding']['sites'][name]
                    require(panic == site['line'], 'fault missed compiled panic site: '+repr((panic, site)))
                    marker = STAGES[0 if fault == 'blocked-route' else 2][0]
                    expected_uart = EXPECTED.split(marker)[0]+marker
                    require(output == expected_uart, 'fault at wrong receive stage: '+repr(output))
                    require(len(sent) == (1 if fault == 'blocked-route' else 2), 'unexpected input stages')
                    expected_frames = 3 if fault == 'blocked-route' else 6
                    require(monitor.read(addresses['WS53_FLASH_UART_RX_PHASE']) == 3, 'fault marked complete')
                checks = {}
                for path, prop, expected in (
                    ('uart-l0-route', 'commands', 1), ('uart-l0-route', 'routed', True),
                    ('uart-l0-route', 'source-pending', fault == 'blocked-route'),
                    ('uart-l0-route', 'blocked', fault == 'blocked-route'),
                    ('uart2', 'rx-peer-frames', expected_frames), ('uart2', 'pad-rx-frames', expected_frames),
                    ('uart2', 'pad-rx-aborted', 0), ('pinctrl', 'uart-tx-bytes-33', len(expected_uart))):
                    value = monitor.execute('qom-get', dict(path='/machine/'+path, property=prop))
                    require(type(value) is type(expected) and value == expected, f'{path}.{prop}: {value}')
                    checks[path+'.'+prop] = value
                registers = {hex(addr): monitor.read(addr) for addr in
                    (0x57014018, 0x5701404c, 0x57036004, 0x57036008, 0x52002210, 0x5703096c)}
                # async_receiver arms RX-data AND line-status (IER=5).
                require(registers == {'0x57014018': 5 if fault == 'blocked-route' else 0,
                    '0x5701404c': 3 if fault == 'blocked-route' else 0, '0x57036004': 1,
                    # Last independent-delay snapshot leaves CNT_REQ/LOCK
                    # set (0x60); ENABLE=0 and periodic mode=0x2 remain.
                    '0x57036008': 1, '0x52002210': 0x62 if fault == 'normal' else 0x63,
                    '0x5703096c': 0}, 'cleanup/pad mismatch: '+repr(registers))
                raw = args.output/f'round{iteration}.uart.raw'; raw.write_bytes(output)
                pads = trace.read_bytes()[trace_start:]; (args.output/f'round{iteration}.pads.csv').write_bytes(pads)
                monitor.execute('cont'); time.sleep(.15); monitor.execute('stop')
                require(uart.read_bytes()[start:] == output and trace.read_bytes()[trace_start:] == pads,
                        'terminal state produced more output')
                report['rounds'].append(dict(iteration=iteration, fault=fault, panic_line=panic,
                    compiled_panic_site=site, uart_sha256=digest(raw), capture_window=window,
                    qom=checks, registers=registers, peer_frames=expected_frames, input_stages=len(sent),
                    guest_phase=5 if fault == 'normal' else 3,
                    irq42_waker_and_cancel_assertions_completed=fault == 'normal', terminal_observation_seconds=.15))
            monitor.execute('quit'); process.wait(timeout=5)
            require(process.returncode == 0, 'nonzero QEMU exit')
            report['diagnostics'] = diagnostics(errors.read_text())
            require(builder.validate(args.build_receipt, args.elf) == build, 'controlled build inputs changed')
            report['passed'] = True
        except BaseException as error:
            report.update(error=str(error), error_type=type(error).__name__)
            raise
        finally:
            try:
                if monitor and process.poll() is None:
                    monitor.execute('quit'); process.wait(timeout=5)
            finally:
                base.terminate(process)
                if monitor: monitor.close()
                save(args.output/'report.json', report)
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--elf', required=True, type=Path)
    parser.add_argument('--build-receipt', required=True, type=Path)
    parser.add_argument('--qemu', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--rounds', type=int, default=2)
    parser.add_argument('--scenario', choices=('normal', 'blocked-route', 'cancel-peer-timeout'), default='normal')
    args = parser.parse_args()
    require(1 <= args.rounds <= 20, 'round count out of range')
    for name in ('elf', 'build_receipt', 'qemu', 'output'): setattr(args, name, getattr(args, name).resolve())
    print(json.dumps(run(args), indent=2))
