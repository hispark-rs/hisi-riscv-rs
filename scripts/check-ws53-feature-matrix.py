#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Compile chip feature boundaries; never connects to or programs hardware."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
POSITIVE = (
    "chip-ws53,rt,unstable",
    "chip-ws53,rt,unstable,async",
    "chip-ws53,rt,unstable,embassy",
    "chip-ws53,rt,unstable,async,embassy",
    "chip-ws63,rt",
    "chip-ws63,rt,unstable",
    "chip-ws63,rt,async,embassy",
    "chip-ws63,rt,async,unstable",
    "chip-ws63,rt,async,embassy,unstable",
    "chip-bs21,rt,unstable",
)
NEGATIVE = (
    ("", "hisi-hal: no chip selected"),
    ("chip-ws53", "chip-ws53 requires unstable"),
    ("chip-ws53,chip-ws63,unstable", "select exactly ONE chip feature"),
    ("chip-ws53,chip-bs21,unstable", "select exactly ONE chip feature"),
    ("chip-ws63,chip-bs21,unstable", "select exactly ONE chip feature"),
    ("chip-bs21", "BS2X support is experimental"),
    ("chip-ws53,embassy", "chip-ws53 requires unstable"),
    ("chip-ws53,rt,unstable,embassy,embassy-time-driver/tick-hz-32_768", "WS53 Embassy requires tick-hz-1_000_000"),
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    results = []
    cases = [(features, None) for features in POSITIVE] + list(NEGATIVE)
    for index, (features, diagnostic) in enumerate(cases):
        command = ["cargo", "clippy" if diagnostic is None else "check",
                   "-Zbuild-std=core,alloc", "--locked", "-p", "hisi-hal",
                   "--no-default-features"]
        if features:
            command.extend(["--features", features])
        if diagnostic is None:
            command.extend(["--", "-D", "warnings"])
        result = subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=300)
        log = args.output / f"{index:02d}.log"
        log.write_text(result.stdout)
        passed = (result.returncode == 0 if diagnostic is None else
                  result.returncode != 0 and diagnostic in result.stdout)
        results.append({"features": features, "expected": "success" if diagnostic is None else diagnostic,
                        "passed": passed, "exit_code": result.returncode, "log": log.name})
        print(f"{'PASS' if passed else 'FAIL'} {features or '(no chip)'}", flush=True)
    (args.output / "report.json").write_text(json.dumps(results, indent=2) + "\n")
    return 0 if all(row["passed"] for row in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
