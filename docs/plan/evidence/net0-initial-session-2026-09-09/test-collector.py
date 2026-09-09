#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Negative checks for exact-byte/manual-HIL report collection."""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile

p = argparse.ArgumentParser()
p.add_argument("--collector", type=Path, required=True)
p.add_argument("--inputs", type=Path, required=True)
a = p.parse_args()
spec = importlib.util.spec_from_file_location("collector", a.collector)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
inputs = json.loads(a.inputs.read_text())
source_case = next(c for c in inputs["cases"] if c["name"] == "paired-reset3")
with tempfile.TemporaryDirectory(prefix="net0-evidence-negative-") as temp:
    base = Path(temp)
    raw = base / "raw"
    shutil.copytree(source_case["directory"], raw)
    original = json.loads((raw / "summary.json").read_text())
    case = dict(source_case, directory=str(raw))
    passed = 0
    for mutation in ("control", "uart-bytes", "elf-hash", "summary-payload", "summary-pass", "source"):
        summary = copy.deepcopy(original)
        uart = raw / "run-01.target.uart.log"
        content = uart.read_bytes()
        if mutation == "uart-bytes":
            uart.write_bytes(content + b"altered\n")
        elif mutation == "elf-hash":
            summary["elf_sha256"] = "0" * 64
        elif mutation == "summary-payload":
            summary["runs"][0]["payload"]["received"] = 9
        elif mutation == "summary-pass":
            summary["runs"][0]["pass"] = False
        elif mutation == "source":
            summary["source_commit"] = "1" * 40
        (raw / "summary.json").write_text(json.dumps(summary))
        out = base / mutation
        out.mkdir()
        try:
            result = module.summarize_case(case, out)
        except ValueError:
            if mutation == "control":
                raise
            passed += 1
        else:
            if mutation != "control" or result["totals"] != {"rounds": 3, "passed": 3, "sent": 30, "received": 30}:
                raise AssertionError("tampered evidence was accepted")
            passed += 1
        finally:
            uart.write_bytes(content)
    for unsafe in (b"neighbor SSID RFDBG_NET0_PAYLOAD_OK", b"RFDBG_NET0_PAYLOAD_ERR reason=private value",
                   b"RFDBG_NET0_HOST_DELIVERY phase=password"):
        assert not module.MARKERS.fullmatch(unsafe)
    print(f"evidence collector: {passed}/6 control/tamper checks; 3/3 whitelist negatives")
