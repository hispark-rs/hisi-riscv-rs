#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Run real CPU IRQ46/waker/retained futures, using only a virtual SDIO peer.

Host writes are confined to the declared command mailbox. No MMIO or CPU
register setup, physical board, cache coherence or bus drain is qualified.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import time
from elftools.elf.elffile import ELFFile
from ws53_qemu_fixture import elf_check

spec = importlib.util.spec_from_file_location('sdio_init_runner',
    Path(__file__).with_name('test-ws53-sdio-init-model.py'))
init = importlib.util.module_from_spec(spec)
spec.loader.exec_module(init)
NOTICE = 'ws53-sdio: assumed CMD53-length-delimited ADMA/latched-status/ACK profile; no CIS, electrical or drain qualification'


def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def pattern(length): return bytes((i*37+19)&255 for i in range(length))


def run(args):
    assert not args.output.exists()
    contract = elf_check.validate(args.elf)
    sizes = dict(MODEL=80, DATA=96, TABLE=1036)
    addresses = {}
    with args.elf.open('rb') as source:
        elf = ELFFile(source)
        for suffix, size in sizes.items():
            symbols = elf.get_section_by_name('.symtab').get_symbol_by_name('WS53_SDIO_ASYNC_'+suffix)
            assert len(symbols) == 1 and symbols[0]['st_size'] == size
            address = symbols[0]['st_value']
            assert address % 4 == 0 and 0x20010000 <= address <= 0x20060000-size
            addresses[suffix] = address
    regions = sorted((addresses[n], addresses[n]+s) for n, s in sizes.items())
    assert all(a[1] <= b[0] for a, b in zip(regions, regions[1:]))
    args.output.mkdir(parents=True)
    results = []
    cases = ('host-read', 'host-write', 'message-then-read', 'request-before-bind',
             'cancel-before-request', 'no-request-cancel', 'posted-write-after-cancel',
             'read-bus-error', 'write-bus-error', 'completion-suppressed',
             'host-clock-loss', 'length-mismatch', 'unknown-event', 'masked-unknown-event', 'function-reset')
    for case in cases:
        with init.running(args, case, addresses['MODEL'], words=20,
                additional_options=('-global', 'ws53-sdio.assume-adma-profile=on'),
                notice=NOTICE) as (cmd, qmp, invoke, report):
            def signal(group, index, value=1):
                cmd(f'set_irq_in /machine/sdio {group} {index} {value}')
            def get(name): return qmp('qom-get', dict(path='/machine/sdio', property=name))
            def set_(name, value): qmp('qom-set', dict(path='/machine/sdio', property=name, value=value))
            def read(address, size):
                return bytes.fromhex(cmd(f'read {address:#x} {size}')[0].removeprefix('0x'))
            def run_until(predicate):
                qmp('cont')
                deadline = time.monotonic()+5
                try:
                    while not predicate():
                        assert time.monotonic() < deadline, ('CPU progress timeout', report())
                        time.sleep(.001)
                finally: qmp('stop')
            def wake_count(n):
                run_until(lambda: report()[10:12] == (n,n))
                assert report()[15] == 0, report()
            def pending(op=4):
                value = invoke(op)
                assert value[3:5] == (20,0) and value[15] == value[17] == 1, value
                return value
            def cancel():
                value = invoke(6)
                assert value[3:5] == (21,0) and value[15] == value[17] == 0, value

            assert invoke(1)[3:5] == (1,0)
            signal('host',0)
            assert invoke(1)[3:5] == (2,0)
            signal('host',1)
            assert invoke(1)[3:5] == (3,0)
            write = case in ('host-write','write-bus-error','posted-write-after-cancel')
            assert invoke(3 if write else 2)[3:5] == (10,0)
            assert invoke(8)[4] == 201  # Local IRQ already enabled: refuse binding.
            value = invoke(7)  # Dropping unpolled future changes no transfer state.
            assert value[3:5] == (22,0) and value[10:12] == (0,0) and value[15:19] == (0,0,0,0)
            initial = read(addresses['DATA'],96)
            assert initial == b'\xa5'*16+(b'\xa5'*64 if write else pattern(64))+b'\xa5'*16
            table = (struct.pack('<HHI',0x21,64,addresses['DATA']+16)
                + struct.pack('<HHI',0x21,4,addresses['TABLE']+1032) + bytes(1036-16))
            assert read(addresses['TABLE'],1036) == table
            assert get('read-notifications') == (0 if write else 1)
            expected_wakes = 0
            if case != 'request-before-bind': pending()
            if case == 'message-then-read':
                set_('host-message',0xa5a51234)
                signal('inject',9)
                expected_wakes += 1
                wake_count(expected_wakes)
                value = invoke(5)
                assert value[3:5] == (10,0) and value[7:10] == (0x200,1,0xa5a51234), value
                assert value[15] == value[17] == 0
                pending()
            if case in ('no-request-cancel','cancel-before-request'):
                pending(5)
                cancel()
                if case == 'cancel-before-request':
                    set_('host-command',0x10000040)
                    signal('host',2)
                assert invoke(9)[4] == 3
                assert get('dma-bytes') == 0 and not get('dma-active')
            elif case in ('unknown-event','masked-unknown-event','function-reset'):
                # Seed 0xa55a5aa5 deliberately preserves unknown enables: bit14
                # is enabled, bit15 is masked. Neither observation may be ACKed.
                signal('inject', {'unknown-event':14,'masked-unknown-event':15,'function-reset':8}[case])
                before = get('acknowledgements')
                if case != 'masked-unknown-event':
                    expected_wakes += 1
                    wake_count(expected_wakes)
                value = invoke(5)  # Unknown masked bit: explicit diagnostic repoll.
                assert value[4] == 9 and get('acknowledgements') == before, value
            else:
                signal('fault',1)  # Deterministically separate request/completion.
                length = 63 if case == 'length-mismatch' else 64
                set_('host-command',(int(write)<<31)|(1<<28)|length)
                set_('host-block-size',0)
                if write: set_('host-write-data',pattern(length).hex())
                signal('host',2)
                if case == 'request-before-bind':
                    value = invoke(4)
                else:
                    expected_wakes += 1
                    wake_count(expected_wakes)
                    value = invoke(5)
                assert value[15] == value[17] == 0, value
                if case == 'length-mismatch':
                    assert value[4] == 8 and not get('dma-active') and get('dma-bytes') == 0, value
                else:
                    assert value[3:5] == (11,0), value
                    pending()
                    fault = {'read-bus-error':2, 'write-bus-error':3,
                             'completion-suppressed':5, 'posted-write-after-cancel':6}.get(case)
                    if fault is not None: signal('fault',fault)
                    if case == 'host-clock-loss':
                        signal('host',0,0)
                        assert invoke(5)[4] == 5
                    else:
                        signal('fault',1,0)
                        if case == 'posted-write-after-cancel':
                            run_until(lambda: get('dma-posted-write'))
                            assert read(addresses['DATA'],96) == initial
                            cancel()
                            signal('fault',6,0)
                            run_until(lambda: not get('dma-active'))
                            assert get('dma-transfers') == 1 and get('dma-bytes') == 64
                            assert read(addresses['DATA'],96) == b'\xa5'*16+pattern(64)+b'\xa5'*16
                        elif case == 'completion-suppressed':
                            run_until(lambda: not get('dma-active'))
                            assert get('dma-transfers') == 1 and get('dma-bytes') == 64
                            pending(5)
                            cancel()
                        else:
                            expected_wakes += 1
                            wake_count(expected_wakes)
                            value = invoke(5)
                            if case in ('read-bus-error','write-bus-error'):
                                assert value[4] == 10 and get('dma-errors') == 1, value
                            else:
                                assert value[3:5] == (12,0), value
                                assert get('dma-transfers') == 1 and get('dma-bytes') == 64
                                if write: assert read(addresses['DATA'],96) == b'\xa5'*16+pattern(64)+b'\xa5'*16
                                else: assert bytes.fromhex(get('device-read-data')) == pattern(64)
            # No owner returned: terminal wait rebind and regular poll must fail.
            before = get('acknowledgements')
            assert invoke(4)[4] == 200
            assert invoke(9)[4] == 3
            assert get('acknowledgements') == before
            value = report()
            assert value[10:12] == (expected_wakes,expected_wakes) and value[15] == value[17] == 0, value
            raw = read(addresses['DATA'],96)
            assert raw[:16] == raw[-16:] == b'\xa5'*16 and read(addresses['TABLE'],1036) == table
            if get('dma-transfers') == 1:
                assert get('dma-bytes') == 64
                if write: assert raw[16:80] == pattern(64)
                else: assert bytes.fromhex(get('device-read-data')) == pattern(64)
            assert get('rejections') == 0
            results.append(dict(case=case,passed=True,final_mailbox=list(value),
                cpu_irq46_count=value[10],waker_count=value[11],dma_bytes=get('dma-bytes'),
                dma_transfers=get('dma-transfers'),dma_errors=get('dma-errors'),
                all_static_allocations_retained=True))
    result = dict(passed=True,cases=results,addresses=addresses,elf_contract=contract,
        elf_sha256=digest(args.elf),qemu_sha256=digest(args.qemu),
        script_sha256=digest(Path(__file__)),shared_runner_sha256=digest(Path(init.__file__)),
        cpu_irq46_delivery_exercised=True,host_paced_fixture=True,host_mmio_setup=False,
        hardware_accessed=False,physical_drain_verified=False,cache_coherency_verified=False,
        cis_or_complete_sdio_protocol=False,termination='assumed CMD53 byte count')
    (args.output/'report.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ('qemu','elf','output'): parser.add_argument('--'+option,type=Path,required=True)
    run(parser.parse_args())
