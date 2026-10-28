#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Shared RAM-ELF/QMP/qtest runner for named WS53 HAL fixtures; no physical devices."""
import importlib.util
import json
import os
from pathlib import Path
import selectors
import socket
import subprocess
import tempfile
import time

spec = importlib.util.spec_from_file_location("elf_check", Path(__file__).with_name("check-ws53-ram-elf.py"))
elf_check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(elf_check)
require = elf_check.require


def connect(path, proc):
    sock = socket.socket(socket.AF_UNIX)
    sock.settimeout(5)
    deadline = time.monotonic() + 5
    try:
        while True:
            try:
                sock.connect(str(path))
                return sock
            except (FileNotFoundError, ConnectionRefusedError):
                require(proc.poll() is None and time.monotonic() < deadline, "QEMU did not open socket")
                time.sleep(.02)
    except BaseException:
        sock.close()
        raise


def run_fixture(qemu, elf, *, device, begin, done, passed, fault=None,
                qemu_options=(), expected_diagnostics=(), qom_checks=(), register_checks=(), expected_output=None,
                input_stages=(), control_stages=(), register_setup=(), output_validator=None, dma_setup=(), dma_stages=(), stage_qom_checks=(), ram_regions=(), register_mask_checks=(), i2c_stages=()):
    for address, mask, value in register_mask_checks:
        require(type(mask) is int and type(value) is int and 0 < mask <= 0xffffffff
                and 0 <= value <= 0xffffffff and value & ~mask == 0, 'invalid register observation mask')
    for address, count in ram_regions:
        require(type(address) is int and type(count) is int and address % 4 == 0 and 0 < count <= 1024
                and 0x20050000 <= address < address + count * 4 <= 0x2005ff00, 'RAM snapshot outside owned DTCM')
    snapshots = []
    require(device in ("mdma", "gpio", "timer", "uart-tx", "uart-pads", "wdt", "tcxo", "rtc", "pwm", "i2c", "spi", "tsensor", "i2s", "adc"), "unknown fixture device")
    require(not (expected_output is not None and output_validator is not None), 'choose exact output or structured validator')
    require(not register_setup or device == 'uart-tx', 'register setup restricted to reviewed TX fixture')
    for address, value in register_setup:
        require((address, value) in ((0x57014018, 5), (0x57014024, 0x15)), 'unreviewed TX setup')
    require(device != "uart-pads" or (fault is None and expected_output is not None), "pad fixture requires exact transcript and no generic fault")
    require(fault in (None, "clock", "snapshot", "expiry", "control", "i2c-arbitration", "i2c-stuck", "i2c-mixed", "adc-manual-stall",
                      "adc-cap-stall", "adc-stale", "adc-cap-error"), "unknown fault selector")
    require(not (fault or '').startswith('i2c-') or device == 'i2c', 'I2C fault requires I2C fixture')
    require(not (fault or '').startswith('adc-') or device == 'adc', 'ADC fault requires ADC fixture')
    require(fault != 'expiry' or device == 'rtc', 'expiry race requires RTC fixture')
    require(fault != 'control' or device == 'tsensor', 'control fault requires temperature fixture')
    require(not control_stages or device == 'tcxo', 'dynamic control requires TCXO fixture')
    require(not i2c_stages or device == 'i2c', 'I2C stage requires I2C fixture')
    for marker, signal, level in i2c_stages:
        require(marker == b'WS53 I2C clear fault\n' and signal in ('arbitration', 'inject-sda-stuck', 'nack-address')
                and level == 0, 'unreviewed I2C fault-stage control')
    for _, signal, level in control_stages:
        require(signal in ('clock', 'stall-snapshot') and level in (0, 1), 'invalid TCXO fixture control')
    require(not (dma_setup or dma_stages or stage_qom_checks) or device == 'mdma', 'DMA injection requires MDMA fixture')
    for signal, channel, level in list(dma_setup)+[(s,c,v) for _,s,c,v in dma_stages]:
        require(signal in ('stall','bus-stall','read-error','write-error') and channel in range(4) and level in (0,1), 'invalid MDMA control')
    for marker, prop, expected in stage_qom_checks:
        require(marker in [m for m,_,_,_ in dma_stages] and prop in ('reads','writes','errors','transfers','pending-writes') and isinstance(expected,int) and expected>=0, 'invalid MDMA staged observation')
    contract = elf_check.validate(elf)
    with tempfile.TemporaryDirectory(prefix=f"ws53-{device}-hal-") as tmp:
        qmp_path, qt_path = Path(tmp) / "qmp", Path(tmp) / "qtest"
        proc = subprocess.Popen([qemu, "-M", "ws53", "-S", "-display", "none", "-monitor", "none",
            "-serial", "null", "-serial", "null", "-serial", "stdio", "-kernel", str(elf),
            "-icount", "shift=0,sleep=on", *qemu_options,
            "-qmp", f"unix:{qmp_path},server=on,wait=off", "-qtest", f"unix:{qt_path},server=on,wait=off",
            "-qtest-log", "/dev/null", "-d", "guest_errors"],
            stdin=subprocess.PIPE if input_stages else subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        output = bytearray()
        sent = set()
        controlled = set()
        try:
            with connect(qt_path, proc) as qt_sock, qt_sock.makefile("rwb", buffering=0) as qt:
                with connect(qmp_path, proc) as qmp_sock, qmp_sock.makefile("rwb", buffering=0) as qmp:
                    json.loads(qmp.readline())
                    def execute(name, arguments=None):
                        request = {"execute": name}
                        if arguments is not None:
                            request['arguments'] = arguments
                        qmp.write(json.dumps(request).encode() + b"\n")
                        while True:
                            response = json.loads(qmp.readline())
                            require("error" not in response, f"QMP error: {response}")
                            if "return" in response:
                                return response["return"]
                    execute("qmp_capabilities")
                    try:
                        if fault is not None:
                            targets = ('rtc0', 'rtc1') if device == 'rtc' else (device,)
                            for target in targets:
                                signals = {'clock': ('clock 0 0',), 'snapshot': ('stall-snapshot 0 1',),
                                          'expiry': ('expire-on-request 0 1',),
                                          'control': ('corrupt-control 0 1',), 'i2c-arbitration': ('arbitration 0 1',),
                                          'i2c-stuck': ('inject-sda-stuck 0 1',),
                                          'i2c-mixed': ('inject-sda-stuck 0 1', 'nack-address 0 1'),
                                          'adc-manual-stall': ('stall 1 1',), 'adc-cap-stall': ('stall 2 1',),
                                          'adc-stale': ('stale 0 1',), 'adc-cap-error': ('cap-error 0 1',)}[fault]
                                for signal in signals:
                                    command = f"set_irq_in /machine/{target} {signal}"
                                    qt.write(command.encode() + b"\n")
                                    require(qt.readline().strip() == b"OK", "qtest fault setup failed")
                        for address, value in register_setup:
                            qt.write(f'writel {address:#x} {value:#x}\n'.encode())
                            require(qt.readline().strip() == b'OK', 'TX fixture setup failed')
                        for signal, channel, level in dma_setup:
                            qt.write(f'set_irq_in /machine/mdma {signal} {channel} {level}\n'.encode())
                            require(qt.readline().strip()==b'OK','MDMA initial setup failed')
                        execute("cont")
                        deadline = time.monotonic() + 10
                        with selectors.DefaultSelector() as selector:
                            selector.register(proc.stdout, selectors.EVENT_READ)
                            while time.monotonic() < deadline:
                                for key, _ in selector.select(.02):
                                    chunk = os.read(key.fd, 4096)
                                    require(bool(chunk), "QEMU closed stdout before HAL test completed")
                                    output.extend(chunk)
                                for index, (marker, signal, level) in enumerate(control_stages):
                                    if index not in controlled and marker in output:
                                        qt.write(f'set_irq_in /machine/tcxo {signal} 0 {level}\n'.encode())
                                        require(qt.readline().strip() == b'OK', 'TCXO stage control failed')
                                        controlled.add(index)
                                for index, (marker, signal, level) in enumerate(i2c_stages):
                                    key = ('i2c', index)
                                    if key not in controlled and marker in output:
                                        qt.write(f'set_irq_in /machine/i2c {signal} 0 {level}\n'.encode())
                                        require(qt.readline().strip() == b'OK', 'I2C stage control failed')
                                        controlled.add(key)
                                for index, (marker, signal, channel, level) in enumerate(dma_stages):
                                    key=('dma',index)
                                    if key not in controlled and marker in output:
                                        for check_marker, prop, expected in stage_qom_checks:
                                            if check_marker==marker:
                                                actual=execute('qom-get',{'path':'/machine/mdma','property':prop})
                                                require(actual==expected,f'MDMA staged {prop}: {actual} != {expected}')
                                        qt.write(f'set_irq_in /machine/mdma {signal} {channel} {level}\n'.encode())
                                        require(qt.readline().strip()==b'OK','MDMA stage control failed')
                                        controlled.add(key)
                                for index, (marker, payload) in enumerate(input_stages):
                                    if index not in sent and marker in output:
                                        proc.stdin.write(payload)
                                        proc.stdin.flush()
                                        sent.add(index)
                                if (done + "\n").encode() in output:
                                    break
                            else:
                                raise ValueError(f"{device} HAL test timed out ({fault}): {output.decode(errors='replace')}")
                        require(len(sent) == len(input_stages), 'missing UART input stages')
                        require(len(controlled) == len(control_stages)+len(dma_stages)+len(i2c_stages), 'missing control stages')
                        for path, prop, expected in qom_checks:
                            actual = execute('qom-get', {'path': path, 'property': prop})
                            require(actual == expected, f'QOM {path}.{prop}: {actual} != {expected}')
                        for address, mask, expected in [*((a, 0xffffffff, e) for a, e in register_checks), *register_mask_checks]:
                            require(0 <= address <= 0xfffffffc and address % 4 == 0, 'unaligned register check')
                            qt.write(f'readl {address:#x}\n'.encode())
                            response = qt.readline().split()
                            require(len(response) == 2 and response[0] == b'OK', 'register read failed')
                            actual = int(response[1], 0)
                            require(actual & mask == expected, f'MMIO {address:#x}: {actual:#x} & {mask:#x} != {expected:#x}')
                        for address, count in ram_regions:
                            words = []
                            for offset in range(count):
                                qt.write(f'readl {address+offset*4:#x}\n'.encode())
                                response = qt.readline().split()
                                require(len(response) == 2 and response[0] == b'OK', 'RAM snapshot read failed')
                                words.append(int(response[1], 0))
                            snapshots.append(dict(address=address, words=words))
                    finally:
                        # Keep qtest alive until QMP quit avoids its log teardown race.
                        if proc.poll() is None:
                            execute("quit")
            remaining, diagnostics = proc.communicate(timeout=5)
            output.extend(remaining)
            require(proc.returncode == 0, f"QEMU failed: {proc.returncode}")
            require(diagnostics.decode().splitlines() == list(expected_diagnostics), diagnostics.decode(errors="replace"))
            lines = output.decode().splitlines()
            require(output_validator(bytes(output)) if output_validator is not None else (bytes(output) == expected_output if expected_output is not None else lines == [begin, passed, done]), f"unexpected output: {lines}")
            return {"elf": contract, "fault": fault, "uart": lines, "uart_raw": output.decode(), "input_stages_sent": len(sent), "diagnostics": list(expected_diagnostics),
                    "stage_qom_checks": [(m.decode(), p, e) for m,p,e in stage_qom_checks], "qom_checks": list(qom_checks), "register_checks": list(register_checks), "hardware_verified": False,
                    **({"ram_snapshots": snapshots} if ram_regions else {}),
                    **({"register_mask_checks": list(register_mask_checks)} if register_mask_checks else {})}
        finally:
            if proc.poll() is None:
                proc.terminate()
                proc.communicate(timeout=5)

