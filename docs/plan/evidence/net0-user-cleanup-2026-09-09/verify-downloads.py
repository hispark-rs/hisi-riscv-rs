#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Verify exact-source CI archives and NET0 resolved-call/resource reports."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import zipfile

p = argparse.ArgumentParser()
p.add_argument("--repo", type=Path, required=True)
p.add_argument("--build-copy", type=Path, required=True)
p.add_argument("--source", required=True)
p.add_argument("--run", type=int, required=True)
p.add_argument("--output", type=Path, required=True)
a = p.parse_args()

def call(*args):
    return subprocess.check_output(args)

def sha(data):
    return hashlib.sha256(data).hexdigest()

source = call("git", "-C", str(a.repo), "rev-parse", "HEAD").decode().strip()
assert source == a.source
assert not call("git", "-C", str(a.repo), "status", "--porcelain").strip()
files = []
for name in call("git", "-C", str(a.repo), "ls-files", "-z").decode().split("\0"):
    if not name:
        continue
    expected = (a.repo / name).read_bytes()
    assert (a.build_copy / name).read_bytes() == expected, name
    files.append({"path": name, "sha256": sha(expected)})
ci = json.loads(call("gh", "run", "view", str(a.run), "--repo", "hispark-rs/hisi-rf-ws63",
                     "--json", "headSha,status,conclusion,url,jobs"))
assert ci["headSha"] == source and ci["status"] == "completed" and ci["conclusion"] == "success"
assert len(ci["jobs"]) == 23 and all(j["conclusion"] == "success" for j in ci["jobs"])
metadata = json.loads(call("gh", "api", f"repos/hispark-rs/hisi-rf-ws63/actions/runs/{a.run}/artifacts?per_page=100"))
assert metadata["total_count"] == len(metadata["artifacts"])
artifacts = {v["name"]: v for v in metadata["artifacts"]}
reports, archives, reference = [], [], {}
for host in ("ubuntu-latest", "macos-14", "windows-latest"):
    for prefix, names in (("net0-linked-storage-", ["net0-storage.json"]),
                          ("net0-incremental-linked-storage-", ["net0-incremental-storage.json", "net0-payload-storage.json", "net0-cleanup-link.json", "net0-cleanup-fault-link.json"])):
        artifact = artifacts[prefix + host]
        assert not artifact["expired"] and artifact["workflow_run"]["head_sha"] == source
        blob = call("gh", "api", f"repos/hispark-rs/hisi-rf-ws63/actions/artifacts/{artifact['id']}/zip")
        assert "sha256:" + sha(blob) == artifact["digest"], "downloaded ZIP digest mismatch"
        archives.append({k: artifact[k] for k in ("id", "name", "digest", "expires_at")} | {"zip_sha256": sha(blob)})
        with zipfile.ZipFile(io.BytesIO(blob)) as archive:
            assert sorted(archive.namelist()) == sorted(names), "unexpected artifact members"
            for name in names:
                assert archive.getinfo(name).file_size < 100000
                raw = archive.read(name)
                report = json.loads(raw)
                assert report["status"] == "pass"
                if "cleanup" in name:
                    assert report["schema"] == "net0-cleanup-link/v1" and len(report["edges"]) == 6
                    if name == "net0-cleanup-link.json":
                        assert report["rejected_call_mutations"] == 6
                else:
                    assert report["schema"] == "net0-linked-storage/v3"
                structural = {k: v for k, v in report.items() if k != "elf_sha256"}
                if "cleanup" in name:
                    # ROM-call trampolines can occupy different final addresses
                    # across hosts. Preserve those addresses below; parity is the
                    # resolved source/callee graph, not byte-identical firmware.
                    structural["edges"] = [{k: edge[k] for k in ("source", "target")}
                                           for edge in report["edges"]]
                if name in reference:
                    assert reference[name] == structural, "cross-host report mismatch: " + name
                reference[name] = structural
                reports.append({"host": host, "artifact_id": artifact["id"], "file": name, "sha256": sha(raw), "report": report})
result = {"schema": "net0-cleanup-downloads/v1", "source": source, "ci_url": ci["url"],
          "ci_successful_jobs": len(ci["jobs"]), "source_copy": files, "archives": archives,
          "reports": reports, "boundary": "Verified downloaded ZIP digests, original report bytes, resource layout and resolved source/callee parity. ROM trampoline addresses differ across hosts; no claim of byte-identical or downloaded firmware ELF or permanent release storage"}
a.output.write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps({"source_files": len(files), "verified_zips": len(archives), "reports": len(reports)}))
