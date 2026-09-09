#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Check exact-source downloaded NET0 physical reports and the independent build copy."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("--repo", type=Path, required=True)
parser.add_argument("--build-copy", type=Path, required=True)
parser.add_argument("--reports", type=Path, required=True)
parser.add_argument("--local-report", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
source = "97840727c2da80812ada30459aaaaad08c5dff37"
run = 34326046377

def call(*command):
    return subprocess.check_output(command, text=True)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

if call("git", "-C", str(args.repo), "rev-parse", "HEAD").strip() != source:
    raise SystemExit("Wrong backend source")
if call("git", "-C", str(args.repo), "status", "--porcelain").strip():
    raise SystemExit("Backend source is dirty")
tracked = subprocess.check_output(["git", "-C", str(args.repo), "ls-files", "-z"]).split(b"\0")
copied = []
for entry in tracked:
    if not entry:
        continue
    name = entry.decode()
    expected, actual = args.repo / name, args.build_copy / name
    if not expected.is_file() or not actual.is_file() or digest(expected) != digest(actual):
        raise SystemExit(f"Independent source copy mismatch: {name}")
    copied.append({"path": name, "sha256": digest(actual)})

ci = json.loads(call("gh", "run", "view", str(run), "--repo", "hispark-rs/hisi-rf-ws63",
                     "--json", "headSha,status,conclusion,url,jobs"))
if (ci["headSha"] != source or ci["status"] != "completed" or ci["conclusion"] != "success"
        or len(ci["jobs"]) != 23 or any(j["conclusion"] != "success" for j in ci["jobs"])):
    raise SystemExit("Exact-source CI not fully successful")
metadata = json.loads(call("gh", "api", f"repos/hispark-rs/hisi-rf-ws63/actions/runs/{run}/artifacts?per_page=100"))
artifacts = {a["name"]: a for a in metadata["artifacts"]}
local = json.loads(args.local_report.read_text())
dimensions = [k for k in local if k.endswith("_bytes") or k in ("l2_offset", "rx_slots", "tx_slots", "mtu")]
variants = ("net0-storage.json", "net0-incremental-storage.json", "net0-payload-storage.json")
reports = []
reference = {}
for os_name in ("ubuntu-latest", "macos-14", "windows-latest"):
    for variant in variants:
        artifact_name = ("net0-linked-storage-" if variant == variants[0]
                         else "net0-incremental-linked-storage-") + os_name
        artifact = artifacts[artifact_name]
        if artifact["expired"] or artifact["workflow_run"]["head_sha"] != source:
            raise SystemExit("Artifact expired or belongs to another source")
        path = args.reports / artifact_name / variant
        data = json.loads(path.read_text())
        if (data["schema"] != "net0-linked-storage/v2" or data["status"] != "pass"
                or re.fullmatch(r"[0-9a-f]{64}", data["elf_sha256"]) is None):
            raise SystemExit("Malformed target storage report")
        if any(data[k] != local[k] for k in dimensions):
            raise SystemExit("Target resource dimensions differ from the local HIL fixture")
        structural = {k: v for k, v in data.items() if k != "elf_sha256"}
        if variant in reference and reference[variant] != structural:
            raise SystemExit("Cross-host physical report disagreement")
        reference[variant] = structural
        reports.append({"host": os_name, "variant": variant, "sha256": digest(path),
                        "artifact": {k: artifact[k] for k in ("id", "name", "digest", "expired", "expires_at")},
                        "report": data})
result = {"schema": "net0-storage-download-verification/v1", "source_commit": source,
          "ci_url": ci["url"], "ci_successful_jobs": len(ci["jobs"]),
          "local_report_sha256": digest(args.local_report), "local_elf_sha256": local["elf_sha256"],
          "source_copy": copied, "downloaded_reports": reports,
          "boundary": "Downloaded report bytes, not downloaded CI ELF bytes; no native fence or release acceptance"}
args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
print(json.dumps({"source_files_matched": len(copied), "reports_matched": len(reports), "ci_jobs": 23}))
