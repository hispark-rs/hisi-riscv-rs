#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Verify exact-source external-consumer CI artifacts and preserve byte digests."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tomllib
import zipfile

p = argparse.ArgumentParser()
p.add_argument("--source", required=True)
p.add_argument("--run", type=int, required=True)
p.add_argument("--output", type=Path, required=True)
a = p.parse_args()

def gh(*args):
    return subprocess.check_output(["gh", *args])

def sha(data):
    return hashlib.sha256(data).hexdigest()

ci = json.loads(gh("run", "view", str(a.run), "--repo", "hispark-rs/hisi-rf-ws63",
                  "--json", "headSha,status,conclusion,url,jobs"))
assert ci["headSha"] == a.source and ci["status"] == "completed" and ci["conclusion"] == "success"
assert len(ci["jobs"]) == 23 and all(job["conclusion"] == "success" for job in ci["jobs"])
listing = json.loads(gh("api", f"repos/hispark-rs/hisi-rf-ws63/actions/runs/{a.run}/artifacts?per_page=100"))
assert len(listing["artifacts"]) == listing["total_count"]
by_name = {item["name"]: item for item in listing["artifacts"]}
records = []
reference = None
names = {"consumer.json", "cleanup.json", "storage.json", "consumer.Cargo.toml", "consumer.Cargo.lock"}
for host in ("ubuntu-latest", "macos-14", "windows-latest"):
    artifact = by_name["net0-external-consumer-" + host]
    assert not artifact["expired"] and artifact["workflow_run"]["head_sha"] == a.source
    data = gh("api", f"repos/hispark-rs/hisi-rf-ws63/actions/artifacts/{artifact['id']}/zip")
    assert "sha256:" + sha(data) == artifact["digest"]
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        assert set(archive.namelist()) == names and len(archive.namelist()) == len(names)
        files = {name: archive.read(name) for name in names}
    consumer = json.loads(files["consumer.json"])
    cleanup = json.loads(files["cleanup.json"])
    storage = json.loads(files["storage.json"])
    assert consumer["schema"] == "net0-transitive-consumer/v1" and consumer["status"] == "pass"
    for key in ("clean_offline", "incremental_unchanged", "restored_build", "space_unicode_path", "pinned_dependencies_unchanged"):
        assert consumer[key] is True
    assert consumer["consumer_build_script"] is False and consumer["consumer_wrap_flags"] is False
    assert consumer["missing_metadata_rejected"] == ["hmac_user_del_etc", "hmac_res_free_mac_user_etc"]
    assert consumer["consumer_lock_sha256"] == sha(files["consumer.Cargo.lock"])
    manifest = tomllib.loads(files["consumer.Cargo.toml"].decode("utf-8"))
    assert "build" not in manifest["package"] and "patch" not in manifest
    assert manifest["dependencies"]["hisi-rf-ws63"]["path"].startswith("../hisi-rf-ws63-")
    assert b"--wrap" not in files["consumer.Cargo.toml"]
    assert cleanup["schema"] == "net0-cleanup-link/v1" and cleanup["status"] == "pass"
    assert len(cleanup["edges"]) == 6 and cleanup["rejected_call_mutations"] == 6
    assert storage["schema"] == "net0-linked-storage/v3" and storage["status"] == "pass"
    assert storage["elf_sha256"] == cleanup["elf_sha256"]
    # Compare ownership/layout sizes and resolved graph, not host-dependent
    # ROM trampoline addresses or binaries that these artifacts do not include.
    parity = {"edges": [(edge["source"], edge["target"]) for edge in cleanup["edges"]],
              "storage": {k: v for k, v in storage.items() if not k.endswith("address") and k != "elf_sha256"}}
    if reference is not None:
        assert parity == reference
    reference = parity
    records.append({"host": host, "id": artifact["id"], "name": artifact["name"],
                    "expires_at": artifact["expires_at"], "digest": artifact["digest"],
                    "downloaded_zip_sha256": sha(data),
                    "files": [{"name": name, "sha256": sha(raw), "bytes": len(raw)} for name, raw in sorted(files.items())],
                    "consumer": consumer, "cleanup": cleanup, "storage": storage})
result = {"schema": "net0-transitive-downloads/v1", "source": a.source, "ci_url": ci["url"],
          "successful_jobs": 23, "artifacts": records,
          "boundary": "Downloaded report ZIP bytes verified. Packaged path dependency, not registry-only facade; no downloaded firmware or new HIL claim. Actions retention is finite."}
a.output.write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps({"source": a.source, "verified_zips": len(records), "member_digests": sum(len(r["files"]) for r in records)}))
