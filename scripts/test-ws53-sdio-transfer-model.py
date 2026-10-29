#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Execute retained SDIO ADMA Rust owners against an explicit assumed profile.

The host writes only a declared command mailbox. Virtual peer inputs provide
CMD53 requests/payloads; the CPU constructs descriptors and configures MMIO.
No hardware, CIS enumeration, cache coherency or physical drain is qualified.
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
    addresses = {}
    with args.elf.open('rb') as source:
        elf = ELFFile(source)
        for suffix, size in (('MODEL', 64), ('DATA', 96), ('TABLE', 1036)):
            symbol = elf.get_section_by_name('.symtab').get_symbol_by_name('WS53_SDIO_TRANSFER_'+suffix)
            assert len(symbol) == 1 and symbol[0]['st_size'] == size
            address = symbol[0]['st_value']
            assert address % 4 == 0 and 0x20010000 <= address <= 0x20060000-size
            addresses[suffix] = address
    regions = sorted((addresses[n], addresses[n]+s) for n, s in
                     (('MODEL',64), ('DATA',96), ('TABLE',1036)))
    assert all(a[1] <= b[0] for a, b in zip(regions, regions[1:]))
    args.output.mkdir(parents=True)
    results = []
    cases = ('host-read', 'host-write', 'no-request-timeout', 'dma-stall-timeout',
             'length-mismatch', 'opposite-direction', 'host-clock-loss',
             'read-bus-error', 'write-bus-error', 'unknown-event', 'function-reset',
             'completion-suppressed', 'posted-write-after-cancel', 'cancel-before-read-request')
    for case in cases:
        with init.running(args, case, addresses['MODEL'], words=16,
                additional_options=('-global','ws53-sdio.assume-adma-profile=on'),
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
                        assert time.monotonic() < deadline, 'DMA progress timeout'
                        time.sleep(.001)
                finally:
                    qmp('stop')
            assert invoke(1)[3:5] == (1,0)
            signal('host',0)
            assert invoke(1)[3:5] == (2,0)
            signal('host',1)
            assert invoke(1)[3:5] == (3,0)
            write = case in ('host-write','write-bus-error','posted-write-after-cancel')
            assert invoke(3 if write else 2)[3:5] == (10,0)
            assert get('read-notifications') == (0 if write else 1)
            assert get('read-offer-pending') is (not write)
            assert get('read-offer-length') == (0 if write else 64)
            assert get('dma-transfers') == 0 and not get('dma-active')
            initial = read(addresses['DATA'],96)
            assert initial == b'\xa5'*16+(b'\xa5'*64 if write else pattern(64))+b'\xa5'*16
            expected_table = (struct.pack('<HHI',0x21,64,addresses['DATA']+16)
                + struct.pack('<HHI',0x21,4,addresses['TABLE']+1032) + bytes(1036-16))
            assert read(addresses['TABLE'],1036) == expected_table
            if case == 'cancel-before-read-request':
                assert invoke(6)[4] == 0
                assert get('read-offer-pending')
                set_('host-command',0x10000040)
                set_('host-block-size',0)
                signal('host',2)
                assert invoke(4)[4] == 3
                assert not get('dma-active') and get('dma-bytes') == 0
            elif case == 'no-request-timeout':
                value = invoke(5)
                assert value[4] == 11 and value[11] == 6, value
            elif case in ('unknown-event','function-reset'):
                signal('inject',16 if case == 'unknown-event' else 8)
                before = get('acknowledgements')
                value = invoke(4)
                assert value[4] == 9 and get('acknowledgements') == before, value
            else:
                request_write = not write if case == 'opposite-direction' else write
                length = 63 if case == 'length-mismatch' else 64
                set_('host-command',(int(request_write)<<31)|(1<<28)|length)
                set_('host-block-size',0)
                if request_write: set_('host-write-data',pattern(length).hex())
                signal('host',2)
                before = get('acknowledgements')
                fault = {'dma-stall-timeout':1, 'read-bus-error':2,
                         'write-bus-error':3, 'completion-suppressed':5,
                         'posted-write-after-cancel':6}.get(case)
                if fault is not None: signal('fault',fault)
                if case == 'host-clock-loss': signal('fault',1)
                value = invoke(4)
                if case in ('length-mismatch','opposite-direction'):
                    assert value[4] == (8 if case == 'length-mismatch' else 9), value
                    assert get('acknowledgements') == before and not get('dma-active')
                else:
                    assert value[3:5] == (11,0), value
                    if case == 'posted-write-after-cancel':
                        run_until(lambda: get('dma-posted-write'))
                    elif case not in ('dma-stall-timeout','host-clock-loss'):
                        run_until(lambda: not get('dma-active'))
                    if case == 'posted-write-after-cancel':
                        assert get('dma-posted-write') and get('dma-bytes') == 0
                        assert read(addresses['DATA'],96) == initial
                        assert invoke(6)[4] == 0
                        assert invoke(4)[4] == 3
                        signal('fault',6,0)
                        run_until(lambda: not get('dma-active'))
                        assert get('dma-transfers') == 1 and get('dma-bytes') == 64
                        assert read(addresses['DATA'],96) == b'\xa5'*16+pattern(64)+b'\xa5'*16
                    elif case == 'host-clock-loss':
                        signal('host',0,0)
                        assert invoke(4)[4] == 5
                    elif case in ('dma-stall-timeout','completion-suppressed'):
                        value = invoke(5)
                        assert value[4] == 11 and value[11] == 6, value
                        if case == 'completion-suppressed':
                            assert get('dma-transfers') == 1 and get('dma-bytes') == 64
                            assert bytes.fromhex(get('device-read-data')) == pattern(64)
                        else: assert get('dma-bytes') == 0
                    elif case in ('read-bus-error','write-bus-error'):
                        assert invoke(4)[4] == 10 and get('dma-errors') == 1
                    else:
                        value = invoke(4)
                        assert value[3:5] == (12,0), value
                        assert get('dma-transfers') == 1 and get('dma-bytes') == 64
                        if write:
                            assert read(addresses['DATA'],96) == b'\xa5'*16+pattern(64)+b'\xa5'*16
                        else: assert bytes.fromhex(get('device-read-data')) == pattern(64)
            # Every terminal path keeps static ownership and makes later polling
            # inert; a completion flag is never treated as physical drain proof.
            before = get('acknowledgements')
            assert invoke(4)[4] == 3
            assert get('acknowledgements') == before
            raw = read(addresses['DATA'],96)
            assert raw[:16] == raw[-16:] == b'\xa5'*16
            assert read(addresses['TABLE'],1036) == expected_table
            assert get('rejections') == 0
            results.append(dict(case=case,passed=True,final_mailbox=list(report()),
                dma_bytes=get('dma-bytes'),dma_transfers=get('dma-transfers'),
                dma_errors=get('dma-errors'),read_notifications=get('read-notifications'),
                read_offer_pending=get('read-offer-pending'),buffer_retained=True))
    result = dict(passed=True,cases=results,addresses=addresses,elf_contract=contract,
        elf_sha256=digest(args.elf),qemu_sha256=digest(args.qemu),
        script_sha256=digest(Path(__file__)),shared_runner_sha256=digest(Path(init.__file__)),
        host_paced_fixture=True,host_mmio_setup=False,hardware_accessed=False,
        physical_drain_verified=False,cache_coherency_verified=False,
        cis_or_complete_sdio_protocol=False,termination='assumed CMD53 byte count')
    (args.output/'report.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ('qemu','elf','output'): parser.add_argument('--'+option,type=Path,required=True)
    run(parser.parse_args())
