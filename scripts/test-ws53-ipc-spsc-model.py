#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Actual Rust CPU + concurrent peer using the known SDK ring algorithms.

The peer accesses only complementary fields/data while CPU RUNS. No per-message
CPU stop, host MMIO, result injection, cache or actual vendor-Ccore qualification.
"""
import argparse
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import time
from elftools.elf.elffile import ELFFile
import ws53_ipc_spsc_build as firmware
import ws53_ci_qemu_ipc_mask_build as policy
from ws53_qemu_fixture import connect


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def wire(n):
    length = 60 if n < 3 else 47 if n == 3 else n % 61
    return bytes([n % 19, n % 2, length, 0]) + bytes((n * 17 + i * 37) & 255 for i in range(length))


def hash_frames(count):
    h = 2166136261
    for n in range(count):
        for b in wire(n):
            h = ((h ^ b) * 16777619) & 0xffffffff
    return h


def run(a):
    global firmware, policy
    if a.endpoint:
        import ws53_ipc_endpoint_build as firmware
    if a.model_policy == 'uart-clock':
        import ws53_ci_qemu_uart_clock_build as policy
    built = firmware.validate(a.receipt, a.elf)
    admitted = policy.validate_receipt(a.model_receipt, a.qemu, source_root=a.model_source)
    with a.elf.open('rb') as stream:
        elf = ELFFile(stream)
        symbols = elf.get_section_by_name('.symtab')
        def symbol(name, size):
            entries = symbols.get_symbol_by_name(name)
            assert len(entries) == 1 and entries[0]['st_size'] == size
            value = entries[0]['st_value']
            assert 0x20010000 <= value <= 0x20060000 - size and value % 4 == 0
            return value
        report_address = symbol('WS53_SPSC_REPORT', 96)
        tx = symbol('WS53_SPSC_TX', 256)
        rx = symbol('WS53_SPSC_RX', 256)
        spans = sorted([(report_address, 96), (tx, 256), (rx, 256)])
        assert all(x + n <= y for (x, n), (y, _) in zip(spans, spans[1:]))
    a.output.mkdir(parents=True, exist_ok=False)
    inputs = {str(p): policy.digest(p) for p in (Path(__file__), a.elf, a.receipt, a.model_receipt,
              Path(firmware.__file__), Path(policy.__file__))}
    result = dict(passed=False, model_admission=admitted, elf_sha256=policy.digest(a.elf),
        inputs_sha256=inputs, rounds=[], actual_cpu_concurrent_storage=True, serialized_peer_only=False,
        host_mmio_setup=False, hardware_accessed=False, installed_ccore_qualified=False,
        cache_coherence_qualified=False, doorbell_rearm_qualified=False, hardware_qualification_eligible=False,
        polling_endpoint=a.endpoint)
    operations = []
    try:
        with tempfile.TemporaryDirectory(prefix='ws53-spsc-') as tmp, (a.output / 'stderr.log').open('wb') as stderr:
            tmp = Path(tmp)
            argv = [str(a.qemu), '-M', 'ws53', '-S', '-display', 'none', '-monitor', 'none', '-serial', 'null',
                    '-kernel', str(a.elf), '-global', 'ws53-ipc.assume-profile=on', '-icount', 'shift=0,sleep=on',
                    '-qtest', f'unix:{tmp}/qt,server=on,wait=off', '-qtest-log', '/dev/null',
                    '-qmp', f'unix:{tmp}/qm,server=on,wait=off', '-d', 'guest_errors']
            save(a.output / 'command.json', argv)
            process = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=stderr)
            try:
                with connect(tmp / 'qt', process) as qs, qs.makefile('rwb', buffering=0) as qt, \
                        connect(tmp / 'qm', process) as ms, ms.makefile('rwb', buffering=0) as qm:
                    assert 'QMP' in json.loads(qm.readline())
                    paused = True
                    peer_stopped = True
                    def qmp(name, args=None):
                        nonlocal paused
                        request = dict(execute=name)
                        if args is not None:
                            request['arguments'] = args
                        qm.write(json.dumps(request).encode() + b'\n')
                        while True:
                            response = json.loads(qm.readline())
                            assert 'error' not in response, response
                            if 'return' in response:
                                if name == 'stop':
                                    assert peer_stopped, 'must not stop CPU to make peer transactions succeed'
                                    paused = True
                                elif name == 'cont':
                                    paused = False
                                operations.append(dict(qmp=request, result=response['return']))
                                return response['return']

                    def command(text):
                        parts = text.split()
                        name = parts[0]
                        if name.startswith(('write', 'out')):
                            assert not paused, 'all peer writes happen while CPU runs'
                            address = int(parts[1], 0)
                            if name == 'writel':
                                assert address == report_address and int(parts[2], 0) in ((1, 3, 4, 8, 16) if a.endpoint else (1, 3, 4, 8))
                            else:
                                assert not peer_stopped
                                if name == 'writew':
                                    assert address in (tx + 4, rx + 6) and 0 <= int(parts[2], 0) < 244
                                elif name == 'writeb':
                                    allowed = ((tx + 3, 1), (rx + 3, 0)) + (((rx + 3, 9),) if a.endpoint else ())
                                    assert (address, int(parts[2], 0)) in allowed
                                else:
                                    assert name == 'write'
                                    size = int(parts[2], 0)
                                    assert rx + 12 <= address < address + size <= rx + 256
                                    assert len(bytes.fromhex(parts[3].removeprefix('0x'))) == size
                        else:
                            assert name == 'read'
                            address, size = int(parts[1], 0), int(parts[2], 0)
                            assert any(start <= address < address + size <= start + extent for start, extent in spans)
                        qt.write(text.encode() + b'\n')
                        response = qt.readline().decode().split()
                        assert response and response[0] == 'OK', (text, response)
                        operations.append(dict(qtest=text, result=response[1:]))
                        return response[1:]

                    def read(address, size):
                        return bytes.fromhex(command(f'read {address:#x} {size}')[0].removeprefix('0x'))
                    def report():
                        words = struct.unpack('<24I', read(report_address, 96))
                        assert words[2] == 0, ('Rust panic', words)
                        return words
                    def wait(predicate):
                        end = time.monotonic() + 15
                        while True:
                            words = report()
                            if predicate(words):
                                return words
                            assert process.poll() is None and time.monotonic() < end, ('CPU timeout', words)
                            time.sleep(.001)
                    def state(base, route, empty):
                        b = read(base, 12)
                        assert b[:3] == bytes(route) and b[3] == empty
                        r, w, c = struct.unpack_from('<HHI', b, 4)
                        assert c == 244 and r < c and w < c
                        return r, w, c
                    def ring_read(base, cursor, size):
                        tail = min(size, 244 - cursor)
                        b = read(base + 12 + cursor, tail)
                        return b + (read(base + 12, size - tail) if size > tail else b'')
                    def ring_write(cursor, b):
                        # Split even a contiguous header to exercise publication
                        # visibility while the other CPU is running.
                        while b:
                            size = min(len(b), 244 - cursor, 3 if len(b) > 3 else len(b))
                            command(f'write {rx + 12 + cursor:#x} {size} 0x{b[:size].hex()}')
                            b = b[size:]
                            cursor = (cursor + size) % 244
                    def peer_pop():
                        # Original SDK cursor-first receive check. Re-read after
                        # releasing R and delay its conditional empty1 write.
                        r, w, c = state(tx, (2, 0, 0), 1)
                        if r == w:
                            return None
                        header = ring_read(tx, r, 4)
                        size = struct.unpack_from('<H', header, 2)[0] + 4
                        assert 4 <= size <= 64 and header[0] < 19 and header[1] < 2
                        frame = ring_read(tx, r, size)
                        command(f'writew {tx + 4:#x} {(r + size) % c}')
                        after_r, after_w, _ = state(tx, (2, 0, 0), 1)
                        if after_r == after_w:
                            report(); report()  # CPU may publish between compare and flag store.
                            command(f'writeb {tx + 3:#x} 1')
                        return frame
                    def peer_push(frame):
                        # Original SDK free-space and equal-cursor/empty test,
                        # including full-capacity allowance. No gap knowledge.
                        r, w, c = state(rx, (0, 2, 1), 0)
                        free = c - (w + c - r) % c
                        if (free == c and read(rx + 3, 1)[0] != 1) or len(frame) > free:
                            return False
                        ring_write(w, frame)
                        command(f'writew {rx + 6:#x} {(w + len(frame)) % c}')
                        if read(rx + 3, 1)[0] == 1:
                            command(f'writeb {rx + 3:#x} 0')
                        return True
                    qmp('qmp_capabilities')
                    for iteration in range(3):
                        if iteration:
                            qmp('system_reset')
                        qmp('cont')
                        wait(lambda w: w[1] == 1 and w[3] == w[4] == 0 and w[13] == w[14] == 0)
                        if a.endpoint:
                            assert report()[11] == 1, 'small-queue size and backpressure checks not executed'
                        assert report()[19:] == (1, 0x1001e300, 0xc00, 5, 0x1001ef00), 'typed SDK layout/refusals not executed'
                        assert read(tx, 256) == bytes([2, 0, 0, 1]) + bytes(4) + struct.pack('<I', 244) + bytes(244)
                        assert read(rx, 256) == bytes([0, 2, 1, 0]) + struct.pack('<HHI', 243, 0, 244) + bytes(244)
                        peer_stopped = False
                        command(f'writel {report_address:#x} 1')
                        initial = wait(lambda w: w[3] == 4 and w[5] > 0)
                        assert state(tx, (2, 0, 0), 1)[:2] == (0, 243)
                        for n in range(4):
                            assert peer_push(wire(n))
                        assert not peer_push(wire(4)), 'original producer must see full sentinel occupancy'
                        assert state(rx, (0, 2, 1), 0)[:2] == (243, 243)
                        assert report()[4] == 0
                        command(f'writel {report_address:#x} 3')
                        consumed, produced = 0, 4
                        end = time.monotonic() + 30
                        while consumed < 512 or produced < 512:
                            assert time.monotonic() < end and process.poll() is None, ('duplex timeout', consumed, produced, report())
                            if produced < 512 and peer_push(wire(produced)):
                                produced += 1
                            if consumed < 512:
                                received = peer_pop()
                                if received is not None:
                                    assert received == wire(consumed), ('peer data mismatch', consumed, received)
                                    consumed += 1
                        words = wait(lambda w: w[3] == w[4] == 512 and w[12] == 1)
                        assert words[7] == words[8] == hash_frames(512)
                        r, w, c = state(tx, (2, 0, 0), 1)
                        assert r == w
                        r, w, c = state(rx, (0, 2, 1), 0)
                        assert (r + 1) % c == w
                        bad_read = r
                        assert peer_push(bytes([255, 1, 0, 0]))
                        command(f'writel {report_address:#x} 8')
                        fault = wait(lambda w: w[17] == 27 and w[18] == 1)
                        assert state(rx, (0, 2, 1), 0)[0] == bad_read
                        if a.endpoint and iteration == 0:
                            command(f'writeb {rx + 3:#x} 9')
                            command(f'writel {report_address:#x} 16')
                            wait(lambda w: w[16] == 1)
                            # Restore metadata only; the old session must stay invalid.
                            command(f'writeb {rx + 3:#x} 0')
                        # Peer lifecycle handoff is explicit and input-only.
                        peer_stopped = True
                        command(f'writel {report_address:#x} 4')
                        final = wait(lambda w: w[13] == w[14] == 1)
                        if a.endpoint:
                            assert final[15] == 1, 'short receive and retained retry not exercised'
                        for prop in ('ack-writes', 'send-writes', 'rejections'):
                            expected = int(a.endpoint and prop == 'send-writes')
                            assert qmp('qom-get', dict(path='/machine/ipc', property=prop)) == expected
                        result['rounds'].append(dict(iteration=iteration, transmitted=consumed, received=produced,
                            initial_full_counter=initial[5], mailbox=list(final),
                            tx_and_rx_maximum_data_bytes=243, peer_full_refused=True,
                            unknown_id_not_consumed=True, error_output_unchanged=fault[18] == 1,
                            runtime_pauses_during_peer_transactions=0,
                            explicit_quiescent_reclaim=not a.endpoint,
                            explicit_session_invalidation=a.endpoint and iteration != 0,
                            detected_corruption_latches_invalidation=a.endpoint and iteration == 0,
                            small_queue_size_and_backpressure=a.endpoint,
                            real_hal_doorbell_pending_result=a.endpoint,
                            short_receive_preserved=a.endpoint))
                        qmp('stop')
                    qmp('quit')
                    stdout = process.communicate(timeout=5)[0]
                    assert process.returncode == 0 and not stdout
            finally:
                if process.poll() is None:
                    process.terminate(); process.wait(timeout=5)
        assert (a.output / 'stderr.log').read_text().splitlines() == [
            'ws53-ipc: assumed digital two-channel doorbell/ACK/clock profile; no ring or peer ownership proof']
        assert firmware.validate(a.receipt, a.elf) == built
        assert policy.validate_receipt(a.model_receipt, a.qemu, source_root=a.model_source) == admitted
        assert all(policy.digest(Path(p)) == h for p, h in inputs.items())
        result['passed'] = True
    except BaseException as error:
        result['error'] = repr(error)
        raise
    finally:
        save(a.output / 'operations.json', operations); save(a.output / 'report.json', result)
    print(json.dumps(dict(passed=True, rounds=3, actual_cpu_transmitted=1536, actual_cpu_received=1536,
                         pauses_during_peer_transactions=0, hardware_qualification_eligible=False)))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'model-receipt', 'model-source', 'elf', 'receipt', 'output'):
        p.add_argument('--' + name, type=Path, required=True)
    p.add_argument('--endpoint', action='store_true', help='test endpoint contract instead of direct queue reclaim')
    p.add_argument('--model-policy', choices=('ipc-mask', 'uart-clock'), default='ipc-mask',
                   help='select the pinned source and build receipt policy')
    a = p.parse_args()
    for name in ('qemu', 'model_receipt', 'model_source', 'elf', 'receipt', 'output'):
        setattr(a, name, getattr(a, name).resolve())
    run(a)
