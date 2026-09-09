#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Test only the capture classifier, without opening serial or resetting hardware."""
import ast
import argparse
from pathlib import Path
import re

p = argparse.ArgumentParser()
p.add_argument("capture_tool", type=Path)
args = p.parse_args()
source = ast.parse(args.capture_tool.read_text())
function = next(node for node in source.body if isinstance(node, ast.FunctionDef) and node.name == "host_delivery")
namespace = {"re": re}
exec(compile(ast.Module(body=[function], type_ignores=[]), "capture-classifier", "exec"), namespace)
classify = namespace["host_delivery"]

def line(phase, count):
    values = dict(entered=count, returned=count, abandoned=0, in_flight=0,
                  peak=int(count > 0), closed=count, crossed_close=0, nonzero=0)
    return ("RFDBG_NET0_HOST_DELIVERY phase=" + phase + "".join(
        f" {key}=0x{value:016x}" for key, value in values.items()) + " exhausted=0\r\n").encode()

valid = line("bootstrap", 0) + line("connected", 4) + line("disconnected", 5)
assert classify(valid)["pass"]
for invalid in [valid.replace(b"returned=0x0000000000000004", b"returned=0x0000000000000003"),
                valid.replace(b" exhausted=0", b" exhausted=1", 1),
                valid + line("disconnected", 5), line("connected", 4), b"",
                line("bootstrap", 0) + line("connected", 0) + line("disconnected", 0),
                valid.replace(b"closed=0x0000000000000005", b"closed=0x0000000000000004")]:
    assert not classify(invalid)["pass"]
print("PASS: observer capture classifier, positive plus seven negative cases")
