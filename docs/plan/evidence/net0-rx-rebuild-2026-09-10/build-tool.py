#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Build the diagnostic STA using the committed, non-production paired fixture."""
import argparse
import ast
import os
from pathlib import Path
import re
import subprocess

p = argparse.ArgumentParser()
p.add_argument("--config", type=Path, required=True)
p.add_argument("--source", type=Path, required=True)
p.add_argument("--initial-session-experiment", action="store_true")
p.add_argument("--cleanup-fault", action="store_true")
p.add_argument("--rx-stop-experiment", action="store_true")
args = p.parse_args()
text = args.config.read_text()
env = os.environ.copy()
for name, variable in [("SSID", "WS63_WIFI_SSID"), ("PASSPHRASE", "WS63_WIFI_PASSPHRASE")]:
    matches = re.findall(rf'^pub const {name}: &\[u8\] = (b"[^"\\]*");$', text, re.M)
    if len(matches) != 1:
        raise SystemExit("The committed paired fixture must contain one literal per field")
    env[variable] = ast.literal_eval(matches[0]).decode("ascii")
command = ["cargo", "build", "-Zbuild-std=core,alloc", "--locked", "--offline", "--release",
           "--target", "riscv32imfc-unknown-none-elf", "--example", "incremental_scan_profile",
           "--features", "wpa2-personal,smoltcp,incremental-backend-experiment,incremental-embassy-wait,standard-l2,bootstrap-stage-diag,firmware-example,incremental-connect-profile"]
if args.initial_session_experiment:
    command[-1] += ",standard-l2-initial-session-experiment"
if args.cleanup_fault:
    command[-1] += ",standard-l2-cleanup-fault-injection"
if args.rx_stop_experiment:
    command[-1] += ",standard-l2-rx-stop-experiment"
raise SystemExit(subprocess.run(command, cwd=args.source, env=env).returncode)
