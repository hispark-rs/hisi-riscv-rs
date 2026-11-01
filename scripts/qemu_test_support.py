#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""QMP/qtest control and named ELF observations, without hardware transports."""
from contextlib import ExitStack
import json
import socket
import subprocess
import time
from elftools.elf.elffile import ELFFile


def require(condition, message):
    if not condition:
        raise ValueError(message)


def connect(path, process, timeout=5):
    sock = socket.socket(socket.AF_UNIX)
    sock.settimeout(timeout)
    deadline = time.monotonic() + timeout
    try:
        while True:
            require(process.poll() is None, 'QEMU exited before opening socket')
            try:
                sock.connect(str(path))
                return sock
            except (FileNotFoundError, ConnectionRefusedError):
                require(time.monotonic() < deadline, 'QEMU did not open socket')
                time.sleep(min(.02, timeout))
    except BaseException:
        sock.close()
        raise


class Monitor:
    """One synchronous QMP/qtest client; skip QMP events, reject protocol errors.

    Own all handles even on partial initialization failure. Operations have a
    socket timeout; this is an observation failure, not evidence a guest stopped.
    """
    def __init__(self, directory, process):
        self._handles = ExitStack()
        try:
            qmp = self._handles.enter_context(connect(directory/'qmp.sock', process))
            qtest = self._handles.enter_context(connect(directory/'qtest.sock', process))
            self.control = self._handles.enter_context(qmp.makefile('rwb', buffering=0))
            self.bus = self._handles.enter_context(qtest.makefile('rwb', buffering=0))
            greeting = json.loads(self.control.readline())
            require(isinstance(greeting, dict) and 'QMP' in greeting, 'missing QMP greeting')
            self.execute('qmp_capabilities')
        except BaseException:
            self.close()
            raise

    def close(self):
        self._handles.close()

    def execute(self, name, arguments=None):
        request = {'execute': name}
        if arguments is not None:
            request['arguments'] = arguments
        self.control.write(json.dumps(request).encode() + b'\n')
        while True:
            line = self.control.readline()
            require(bool(line), 'QMP disconnected')
            reply = json.loads(line)
            require(isinstance(reply, dict) and 'error' not in reply, f'QMP {name}: {reply}')
            if 'return' in reply:
                return reply['return']
            require('event' in reply, 'QMP response has neither return nor event')

    def command(self, line):
        self.bus.write(line.encode() + b'\n')
        reply = self.bus.readline().decode().strip().split()
        require(bool(reply) and reply[0] == 'OK', f'qtest {line}: {reply}')
        return reply[1:]

    def read(self, address, width='l'):
        require(width in ('l', 'w'), 'invalid MMIO observation width')
        reply = self.command(f'read{width} {address:#x}')
        require(len(reply) == 1, 'malformed qtest read')
        return int(reply[0], 0)

    def words(self, address, count):
        return [self.read(address + 4*i) for i in range(count)]


def object_addresses(path, sizes, *, ram):
    """Require unique, aligned, nonoverlapping diagnostic objects in owned RAM."""
    addresses = {}
    with path.open('rb') as stream:
        table = ELFFile(stream).get_section_by_name('.symtab')
        for name, size in sizes.items():
            require(type(size) is int and size > 0, 'invalid diagnostic size')
            found = table.get_symbol_by_name(name) if table else None
            require(found is not None and len(found) == 1, 'missing/ambiguous object '+name)
            symbol = found[0]
            address = symbol['st_value']
            require(symbol['st_size'] == size and symbol['st_info']['type'] == 'STT_OBJECT'
                    and address % 4 == 0 and ram[0] <= address <= ram[1]-size,
                    'invalid diagnostic object '+name)
            addresses[name] = address
    ranges = sorted((addresses[name], addresses[name]+size) for name, size in sizes.items())
    require(all(left[1] <= right[0] for left, right in zip(ranges, ranges[1:])),
            'overlapping diagnostic objects')
    return addresses


def terminate(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
