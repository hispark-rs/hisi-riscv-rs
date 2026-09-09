#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Verify downloaded report bytes and exact-source three-host NET0 CI."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import zipfile


def gh(*args):
    return subprocess.check_output(["gh", *args])


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    if not __debug__:
        raise SystemExit("Evidence verification requires Python assertions; do not use optimized mode")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", type=int, required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    ci = json.loads(gh("run", "view", str(args.run), "--repo", "hispark-rs/hisi-rf-ws63",
                       "--json", "headSha,status,conclusion,url,jobs"))
    assert ci["headSha"] == args.source and ci["status"] == "completed" and ci["conclusion"] == "success"
    assert len(ci["jobs"]) == 23 and all(job["conclusion"] == "success" for job in ci["jobs"])
    listing = json.loads(gh("api", f"repos/hispark-rs/hisi-rf-ws63/actions/runs/{args.run}/artifacts?per_page=100"))
    assert len(listing["artifacts"]) == listing["total_count"]
    artifacts = {item["name"]: item for item in listing["artifacts"]}
    expected = {"consumer.json", "cleanup.json", "storage.json", "host-tx.json", "consumer.Cargo.toml", "consumer.Cargo.lock"}
    records = []
    reference = None
    for host in ("ubuntu-latest", "macos-14", "windows-latest"):
        item = artifacts["net0-external-consumer-" + host]
        assert not item["expired"] and item["workflow_run"]["head_sha"] == args.source
        raw = gh("api", f"repos/hispark-rs/hisi-rf-ws63/actions/artifacts/{item['id']}/zip")
        assert "sha256:" + sha(raw) == item["digest"]
        with zipfile.ZipFile(io.BytesIO(raw)) as archive:
            assert set(archive.namelist()) == expected and len(archive.namelist()) == len(expected)
            files = {name: archive.read(name) for name in expected}
        consumer = json.loads(files["consumer.json"])
        tx = json.loads(files["host-tx.json"])
        cleanup = json.loads(files["cleanup.json"])
        storage = json.loads(files["storage.json"])
        for report in (consumer, tx, cleanup, storage):
            assert report["status"] == "pass"
        for key in ("clean_offline", "incremental_unchanged", "restored_build", "space_unicode_path", "pinned_dependencies_unchanged"):
            assert consumer[key] is True
        assert consumer["consumer_build_script"] is False and consumer["consumer_wrap_flags"] is False
        assert consumer["consumer_lock_sha256"] == sha(files["consumer.Cargo.lock"])
        assert consumer["missing_metadata_rejected"] == ["hmac_user_del_etc", "hmac_res_free_mac_user_etc"]
        assert tx["schema"] == "net0-host-tx-link/v1" and tx["rejected_call_mutations"] == 11 and len(tx["edges"]) == 11
        assert cleanup["rejected_call_mutations"] == 6 and len(cleanup["edges"]) == 6
        assert tx["elf_sha256"] == cleanup["elf_sha256"] == storage["elf_sha256"]
        assert tx["metadata"]["bytes"] == 576 and tx["metadata"]["capacity"] == 32
        parity = {"tx_edges": [(edge["source"], edge["target"]) for edge in tx["edges"]],
                  "metadata": {k: v for k, v in tx["metadata"].items() if k != "address"},
                  "storage": {k: v for k, v in storage.items() if not k.endswith("address") and k != "elf_sha256"}}
        if reference is not None:
            assert reference == parity
        reference = parity
        records.append({"host": host, "id": item["id"], "expires_at": item["expires_at"],
                        "downloaded_zip_sha256": sha(raw), "github_digest": item["digest"],
                        "elf_sha256": tx["elf_sha256"], "metadata": tx["metadata"],
                        "files": [{"name": name, "sha256": sha(data), "bytes": len(data)} for name, data in sorted(files.items())]})
    result = {"schema": "net0-host-tx-downloads/v1", "source": args.source, "ci_url": ci["url"],
              "successful_jobs": 23, "artifacts": records, "semantic_parity": reference,
              "boundary": "Downloaded report ZIPs, not firmware. Packaged path dependency, not registry-only facade. Actions retention is finite."}
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"verified_zips": len(records), "member_digests": sum(len(r["files"]) for r in records)}))


if __name__ == "__main__":
    main()
