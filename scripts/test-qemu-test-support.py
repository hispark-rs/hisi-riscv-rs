#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Host protocol/cleanup tests; no emulator or hardware required."""
from contextlib import ExitStack
import io
from pathlib import Path
import unittest
from unittest.mock import Mock, patch
import qemu_test_support as support


class Channel:
    def __init__(self, replies):
        self.input = io.BytesIO(replies)
        self.output = bytearray()
    def readline(self):
        return self.input.readline()
    def write(self, value):
        self.output.extend(value)
    def close(self):
        self.input.close()
    def __enter__(self):
        return self
    def __exit__(self, *args):
        self.close()


class Socket(Channel):
    def makefile(self, *args, **kwargs):
        return self


class MonitorTests(unittest.TestCase):
    def monitor(self, qmp=b'', bus=b''):
        m = support.Monitor.__new__(support.Monitor)
        m._handles = ExitStack()
        m.control, m.bus = Channel(qmp), Channel(bus)
        self.addCleanup(m.close)
        return m

    def test_qmp_events_are_skipped_and_arguments_preserved(self):
        m = self.monitor(b'{"event":"STOP"}\n{"return":false}\n')
        self.assertIs(m.execute('qom-get', {'path':'/machine/uart2'}), False)
        self.assertIn(b'"path": "/machine/uart2"', m.control.output)

    def test_qmp_errors_eof_and_invalid_replies_are_rejected(self):
        for message in (b'', b'{"error":{"class":"GenericError"}}\n', b'[]\n', b'{}\n', b'bad\n'):
            with self.subTest(message=message), self.assertRaises(ValueError):
                self.monitor(message).execute('cont')

    def test_qtest_errors_and_malformed_reads_are_rejected(self):
        for reply in (b'', b'FAIL Unknown device\n', b'OK\n', b'OK 0 1\n', b'OK invalid\n'):
            with self.subTest(reply=reply), self.assertRaises(ValueError):
                self.monitor(bus=reply).read(0x100)
        m = self.monitor(bus=b'OK 0x1234\nOK 0xabcd\n')
        self.assertEqual(m.words(0x100, 2), [0x1234, 0xabcd])
        self.assertEqual(m.bus.output, b'readl 0x100\nreadl 0x104\n')
        with self.assertRaises(ValueError):
            m.read(0x100, 'b')

    def test_partial_initialization_closes_first_socket(self):
        first = Socket(b'')
        with patch.object(support, 'connect', side_effect=[first, TimeoutError('qtest missing')]):
            with self.assertRaises(TimeoutError):
                support.Monitor(Path('/unused'), Mock())
        self.assertTrue(first.input.closed)

    def test_bad_greeting_closes_all_handles(self):
        first, second = Socket(b'{}\n'), Socket(b'')
        with patch.object(support, 'connect', side_effect=[first, second]):
            with self.assertRaisesRegex(ValueError, 'greeting'):
                support.Monitor(Path('/unused'), Mock())
        self.assertTrue(first.input.closed and second.input.closed)

    def test_initialization_and_explicit_close(self):
        first, second = Socket(b'{"QMP":{}}\n{"return":{}}\n'), Socket(b'')
        with patch.object(support, 'connect', side_effect=[first, second]):
            m = support.Monitor(Path('/unused'), Mock())
        self.assertIn(b'qmp_capabilities', first.output)
        m.close()
        m.close()
        self.assertTrue(first.input.closed and second.input.closed)

    def test_connect_failure_closes_socket_and_reports_dead_process(self):
        sock = Mock()
        process = Mock()
        process.poll.return_value = 1
        with patch.object(support.socket, 'socket', return_value=sock):
            with self.assertRaisesRegex(ValueError, 'exited'):
                support.connect(Path('/unused'), process)
        sock.close.assert_called_once()
        sock.connect.assert_not_called()

    def test_connect_deadline_closes_socket(self):
        sock, process = Mock(), Mock()
        sock.connect.side_effect = FileNotFoundError()
        process.poll.return_value = None
        with patch.object(support.socket, 'socket', return_value=sock), patch.object(support.time, 'monotonic', side_effect=[0, 6]):
            with self.assertRaisesRegex(ValueError, 'did not open'):
                support.connect(Path('/unused'), process)
        sock.close.assert_called_once()


if __name__ == '__main__':
    unittest.main()
