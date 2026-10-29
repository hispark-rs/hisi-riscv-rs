#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Real Rust IPC receive + bounded TRNG service on an exclusive model image.

The host publishes SDK frames while CPU runs. Image address is metadata only;
no actual control core, live shared-random lease/cache/publication, entropy or
silicon qualification is inferred. TRNG setup/faults are explicit host MMIO.
"""
import argparse
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import time

from elftools.elf.elffile import ELFFile
import ws53_ipc_random_build as firmware
import ws53_ci_qemu_ipc_mask_build as policy
from ws53_qemu_fixture import connect, require

NOTICE = 'ws53-trng: assumed FIFO8 pair-ready startup200us pair10us deterministic-test-source; NOT entropy'
START = 0xc1000


def cases():
    def case(name, *, kind=0, start=START, length=128, registered=128, payload=None,
             identifier=18, expected=0, attempts=512, capacity=128, fault=None):
        return dict(name=name, payload_hex=(struct.pack('<III', kind, start, length) if payload is None else payload).hex(),
            identifier=identifier, expected=expected, attempts=attempts, capacity=capacity, fault=fault,
            registered=registered, offset=length // 2 if kind == 2 else 0,
            size=length if kind == 0 else length // 2)
    result = [case('all'), case('first-half', kind=1), case('second-half', kind=2)]
    result += [case(f'tail-{n}-{kind}', length=n, registered=n, kind=kind)
               for n in (1, 2, 3, 6, 10, 127) for kind in range(3) if kind == 0 or n % 2 == 0]
    errors = [case(f'payload-{n}', payload=bytes(n), expected=11) for n in (0, 11, 13, 60)]
    errors += [case('wrong-service', identifier=17, expected=10), case('bad-type', kind=0x100, expected=12),
        case('alias-input', start=0x100c1000, expected=14), case('subextent', length=64, expected=14),
        case('overlap', start=START-1, length=130, expected=14), case('zero', length=0, expected=13),
        case('local-wrap', start=0xfffffffc, length=8, expected=13),
        case('alias-wrap', start=0xeffffff0, length=16, expected=13),
        case('odd-half', kind=2, length=127, registered=127, expected=15),
        case('small-staging', capacity=127, expected=20),
        case('one-attempt', attempts=1, expected=31), case('ring-off', fault='ring', expected=30),
        case('clock-off', fault='clock', expected=31)]
    errors += [case(name, fault=index, expected=31) for index, name in enumerate(
        ('stall', 'not-ready', 'not-done', 'zero-candidate', 'ones-candidate', 'repeated-pair', 'ready-race'))]
    for error in errors:
        result += [error, case('recover-after-' + error['name'], kind=2)]
    return result


def run(a):
    global policy
    if a.model_policy == 'uart-clock':
        import ws53_ci_qemu_uart_clock_build as policy
    firmware.validate(a.receipt, a.elf)
    admitted = policy.validate_receipt(a.model_receipt, a.qemu, source_root=a.model_source)
    with a.elf.open('rb') as stream:
        table = ELFFile(stream).get_section_by_name('.symtab')
        def symbol(name, size):
            entries = table.get_symbol_by_name(name)
            require(entries is not None and len(entries) == 1 and entries[0]['st_size'] == size, name)
            address = entries[0]['st_value']
            require(address % 4 == 0 and 0x20010000 <= address <= 0x20060000-size, 'model RAM symbol')
            return address
        inbox = symbol('WS53_RANDOM_INBOX', 256)
        destination = symbol('WS53_RANDOM_DESTINATION', 132)
        report = symbol('WS53_RANDOM_REPORT', 48)
        control = symbol('WS53_RANDOM_CONTROL', 12)
    spans = sorted([(inbox, 256), (destination, 132), (report, 48), (control, 12)])
    require(all(x+n <= y for (x, n), (y, _) in zip(spans, spans[1:])), 'symbols overlap')
    a.output.mkdir(parents=True, exist_ok=False)
    paths = (Path(__file__), Path(firmware.__file__), Path(policy.__file__),
             Path(__file__).with_name('ws53_qemu_fixture.py'), a.elf, a.receipt, a.model_receipt)
    inputs = {str(p): policy.digest(p) for p in paths}
    result = dict(passed=False, model_admission=admitted, inputs_sha256=inputs, scenarios=[],
        actual_rust_cpu=True, concurrent_sdk_frame_publication=True, host_trng_setup=True,
        hardware_accessed=False, hardware_qualification_eligible=False, installed_ccore_qualified=False,
        live_random_buffer_qualified=False, entropy_quality_claimed=False, flash_tested=False)
    operations = []
    try:
        for seed in (1, 0x12345678):
            with tempfile.TemporaryDirectory(prefix='ws53-ipc-random-') as tmp, \
                    (a.output / f'stderr-{seed}.log').open('wb') as stderr:
                tmp = Path(tmp)
                command = [str(a.qemu), '-M', 'ws53', '-S', '-display', 'none', '-monitor', 'none',
                    '-serial', 'null', '-kernel', str(a.elf), '-icount', 'shift=0,sleep=on',
                    '-global', 'ws53-trng.assume-profile=on', '-global', f'ws53-trng.test-seed={seed}',
                    '-qtest', f'unix:{tmp}/qt,server=on,wait=off', '-qtest-log', '/dev/null',
                    '-qmp', f'unix:{tmp}/qm,server=on,wait=off', '-d', 'guest_errors']
                (a.output / f'command-{seed}.json').write_text(json.dumps(command, indent=2)+'\n')
                proc = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=stderr)
                try:
                    with connect(tmp/'qt', proc) as qs, qs.makefile('rwb', buffering=0) as qt, \
                            connect(tmp/'qm', proc) as ms, ms.makefile('rwb', buffering=0) as qm:
                        require('QMP' in json.loads(qm.readline()), 'QMP greeting')
                        running = False
                        def qmp(name, arguments=None):
                            nonlocal running
                            req = dict(execute=name)
                            if arguments is not None:
                                req['arguments'] = arguments
                            qm.write(json.dumps(req).encode()+b'\n')
                            while True:
                                response = json.loads(qm.readline())
                                require('error' not in response, str(response))
                                if 'return' in response:
                                    if name == 'cont':
                                        running = True
                                    operations.append(dict(seed=seed, qmp=req, result=response['return']))
                                    return response['return']
                        def cmd(text):
                            require(running, 'all host RAM/MMIO accesses must occur while CPU runs')
                            qt.write(text.encode()+b'\n')
                            response = qt.readline().decode().split()
                            require(response and response[0] == 'OK', (text, response))
                            operations.append(dict(seed=seed, qtest=text, result=response[1:]))
                            return response[1:]
                        def read(address, size):
                            require(any(x <= address < address+size <= x+n for x, n in spans), 'read outside fixture')
                            return bytes.fromhex(cmd(f'read {address:#x} {size}')[0].removeprefix('0x'))
                        def write(address, data):
                            require(any(x <= address < address+len(data) <= x+n
                                        for x, n in ((inbox+12, 244), (destination, 132), (control, 12))), 'write outside fixture')
                            cmd(f'write {address:#x} {len(data)} 0x{data.hex()}')
                        def words():
                            r = struct.unpack('<12I', read(report, 48))
                            require(r[1] == 0, f'Rust panic: {r}')
                            return r
                        def wait(fn, reason):
                            deadline = time.monotonic()+15
                            while True:
                                value = fn()
                                if value:
                                    return value
                                require(proc.poll() is None and time.monotonic() < deadline, reason)
                                time.sleep(.001)
                        def prop(name):
                            return qmp('qom-get', dict(path='/machine/trng', property=name))
                        def publish(payload):
                            meta = read(inbox, 12)
                            r, w, cap = struct.unpack_from('<HHI', meta, 4)
                            require(meta[:4] == bytes([0, 2, 1, 0]) and cap == 244, 'inbox metadata')
                            require((w-r) % cap == 1, 'previous message was not fully released')
                            # Split into small chunks, then publish W in one halfword.
                            remaining, cursor = payload, w
                            while remaining:
                                n = min(3, cap-cursor, len(remaining))
                                write(inbox+12+cursor, remaining[:n])
                                remaining, cursor = remaining[n:], (cursor+n) % cap
                            cmd(f'writew {inbox+6:#x} {cursor}')
                        sequence = []
                        state = seed
                        def random_bytes(index, size):
                            nonlocal state
                            end = index+2*((size+3)//4)
                            while len(sequence) < end:
                                state ^= (state << 13) & 0xffffffff
                                state ^= state >> 17
                                state ^= (state << 5) & 0xffffffff
                                sequence.append(state)
                            return b''.join(x.to_bytes(4, 'little') for x in sequence[index:end:2])[:size]
                        qmp('qmp_capabilities')
                        qmp('cont')
                        wait(lambda: words()[0] == 1, 'ready timeout')
                        require(words()[11] == 0x524e4431, 'fixture magic')
                        count = 0
                        all_cases = cases()
                        for index, case in enumerate(all_cases + [all_cases[0]]):
                            if index == len(all_cases):
                                qmp('system_reset')
                                wait(lambda: words()[0] == 1 and words()[2] == 0, 'reset initialization')
                                require(prop('consumed') == 0, 'model reset consumption')
                                sequence, state, count = [], seed, 0
                            cmd('writew 0x52001600 0')
                            for fault in range(7):
                                cmd(f'set_irq_in /machine/trng fault {fault} 0')
                            cmd('set_irq_in /machine/trng clock 0 1')
                            fault = case['fault']
                            if isinstance(fault, int):
                                cmd(f'set_irq_in /machine/trng fault {fault} 1')
                            if fault == 'clock':
                                cmd('set_irq_in /machine/trng clock 0 0')
                            sequence_index = prop('generated')
                            before = prop('consumed')
                            if fault != 'ring':
                                cmd('writew 0x52001600 1')
                            if fault not in ('ring', 'clock', 0):
                                wait(lambda: prop('fifo-count') == 8, 'FIFO prefill')
                            status_before = prop('status-reads')
                            write(destination, b'\xa5'*132)
                            write(control, struct.pack('<III', case['attempts'], case['capacity'], case['registered']))
                            payload = bytes.fromhex(case['payload_hex'])
                            frame = bytes([case['identifier'], 1])+struct.pack('<H', len(payload))+payload
                            publish(frame)
                            count += 1
                            wait(lambda: words()[2] == count, f'completion timeout: {case["name"]}')
                            actual = words()
                            require(actual[3] == case['expected'], (case, actual))
                            expected = bytearray(b'\xa5'*132)
                            if case['expected'] == 0:
                                offset, size = case['offset'], case['size']
                                require(actual[4:7] == (offset, size, 0), 'selected range')
                                expected[1+offset:1+offset+size] = random_bytes(sequence_index, size)
                            require(read(destination, 132) == expected, (case['name'], 'destination changed incorrectly'))
                            consumed = prop('consumed')-before
                            statuses = prop('status-reads')-status_before
                            if case['expected'] in (10, 11, 12, 13, 14, 15, 20):
                                require(consumed == statuses == 0, 'rejected request accessed TRNG')
                            elif case['expected'] == 0:
                                require(consumed == 2*((case['size']+3)//4), 'FIFO count for successful fill')
                            elif case['name'] == 'one-attempt':
                                require(consumed == 2 and actual[6] == 4, 'total budget and staged prefix')
                            elif fault in (3, 4, 5):
                                require(0 < consumed <= 2*case['attempts'] and consumed % 2 == 0, 'rejected pair budget')
                            else:
                                require(consumed == 0, 'fault consumed FIFO')
                            require(statuses <= 2*case['attempts'], 'status polling exceeded attempt budget')
                            require(actual[6] == (4 if case['name'] == 'one-attempt' else 0), 'failure prefix')
                            result['scenarios'].append(dict(seed=seed, reset_recovery=index == len(all_cases),
                                **case, consumed=consumed, status_reads=statuses, staged_prefix=actual[6],
                                destination_sha256=__import__('hashlib').sha256(expected).hexdigest()))
                        qmp('quit')
                    stdout, _ = proc.communicate(timeout=5)
                    require(proc.returncode == 0 and stdout == b'', 'model exit/output')
                    require((a.output / f'stderr-{seed}.log').read_text().splitlines() == [NOTICE], 'unimplemented MMIO/model diagnostic')
                finally:
                    if proc.poll() is None:
                        proc.terminate()
                        proc.communicate(timeout=5)
        firmware.validate(a.receipt, a.elf)
        policy.validate_receipt(a.model_receipt, a.qemu, source_root=a.model_source)
        require(inputs == {str(p): policy.digest(p) for p in paths}, 'inputs changed while testing')
        result['passed'] = True
    except BaseException as error:
        result.update(error=str(error), error_type=type(error).__name__)
        raise
    finally:
        trace = a.output/'operations.json'
        trace.write_text(json.dumps(operations, indent=2)+'\n')
        result['operations_sha256'] = policy.digest(trace)
        (a.output/'report.json').write_text(json.dumps(result, indent=2)+'\n')
        print(json.dumps(dict(passed=result['passed'], scenarios=len(result['scenarios']), hardware_accessed=False)))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--model-policy', choices=('ipc-mask', 'uart-clock'), default='ipc-mask')
    for name in ('qemu', 'model-receipt', 'model-source', 'elf', 'receipt', 'output'):
        parser.add_argument('--'+name, type=Path, required=True)
    args = parser.parse_args()
    for name, value in vars(args).items():
        if isinstance(value, Path):
            setattr(args, name, value.resolve())
    run(args)
