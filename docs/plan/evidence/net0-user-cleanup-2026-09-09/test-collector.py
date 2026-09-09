#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Verify cleanup evidence classification, including the real failed fixture."""
import argparse
import importlib.util
import json
from pathlib import Path
import tempfile

p = argparse.ArgumentParser()
p.add_argument("--collector", type=Path, required=True)
p.add_argument("--inputs", type=Path, required=True)
a = p.parse_args()
spec = importlib.util.spec_from_file_location("collector", a.collector)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
inputs = json.loads(a.inputs.read_text())
expected = [(3, 3), (14, 13), (1, 0), (3, 3)]
with tempfile.TemporaryDirectory(prefix="net0-cleanup-collector-") as tmp:
    for index, case in enumerate(inputs["cases"]):
        output = Path(tmp) / str(index)
        output.mkdir()
        result = m.summarize_case(case, output)
        assert (result["totals"]["rounds"], result["totals"]["passed"]) == expected[index]
    good = Path(inputs["cases"][-1]["directory"]) / "run-01.target.uart.log"
    data = good.read_bytes()
    assert m.cleanup_observation(data, True)["pass"]
    mutations = [
        data.replace(b"backend=0x5732d064", b"backend=0x00000064"),
        data.replace(b"kind=hostap_status value=0x00000064", b"kind=hostap_status value=0x00000065"),
        data.replace(b"RFDBG_NET0_CLEANUP_FAULT_REJECTED", b"missing"),
        data + b"\n[PANIC] failed assertion\n",
        data + b"\nRFDBG_A5B_DISCONNECT_OK elapsed_ms=0x00000001\n",
        data + b"\nRFDBG_NET0_USER_CLEANUP" + b" 0x00000000" * 10 + b"\n",
    ]
    for mutation in mutations:
        assert not m.cleanup_observation(mutation, True)["pass"]
    for unsafe in (b"neighbor RFDBG_NET0_CLEANUP_FAULT_REJECTED",
                   b"RFDBG_A5B_DISCONNECT_ERR code=private stage=disconnect backend=0x00000064"):
        assert not m.MARKERS.fullmatch(unsafe)
print("cleanup collector: 4 real cases, 6 tamper cases, 2 whitelist negatives passed")
