#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Verify exact-source RX-deadline CI and actual cross-host report ZIP bytes."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import zipfile

SOURCE = "8a6908c193fc6756d5f68b9e919c09903189a5e2"
RUN = 34430064193


def gh(*args):
    return subprocess.check_output(["gh", *args])


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    if not __debug__:
        p.error("Evidence assertions must remain enabled")
    ci = json.loads(gh("run", "view", str(RUN), "--repo", "hispark-rs/hisi-rf-ws63",
                       "--json", "headSha,status,conclusion,url,jobs"))
    assert ci["headSha"] == SOURCE
    if ci["status"] != "completed" or ci["conclusion"] != "success":
        raise SystemExit("Exact-source CI has not passed; no acceptance receipt written")
    assert len(ci["jobs"]) == 23 and all(job["conclusion"] == "success" for job in ci["jobs"])
    listing = json.loads(gh("api", f"repos/hispark-rs/hisi-rf-ws63/actions/runs/{RUN}/artifacts?per_page=100"))
    assert len(listing["artifacts"]) == listing["total_count"]
    artifacts = {item["name"]: item for item in listing["artifacts"]}
    expected = {"consumer.json", "cleanup.json", "storage.json", "host-tx.json", "rx-stop.json",
                "rx-mode.json", "consumer.Cargo.toml", "consumer.Cargo.lock"}
    records, reference = [], None
    for host in ("ubuntu-latest", "macos-14", "windows-latest"):
        item = artifacts["net0-external-consumer-" + host]
        assert not item["expired"] and item["workflow_run"]["head_sha"] == SOURCE
        raw = gh("api", f"repos/hispark-rs/hisi-rf-ws63/actions/artifacts/{item['id']}/zip")
        assert "sha256:" + sha(raw) == item["digest"]
        with zipfile.ZipFile(io.BytesIO(raw)) as archive:
            assert set(archive.namelist()) == expected and len(archive.namelist()) == len(expected)
            files = {name: archive.read(name) for name in expected}
        consumer, cleanup, storage, tx, rx, mode = [json.loads(files[name + ".json"])
            for name in ("consumer", "cleanup", "storage", "host-tx", "rx-stop", "rx-mode")]
        assert all(report["status"] == "pass" for report in (consumer, cleanup, storage, tx, rx, mode))
        assert all(consumer[key] is True for key in ("clean_offline", "incremental_unchanged", "restored_build",
            "space_unicode_path", "pinned_dependencies_unchanged", "rx_stop_experiment_link_verified",
            "direct_rx_link_verified", "missing_rx_mode_metadata_rejected"))
        assert consumer["consumer_build_script"] is False and consumer["consumer_wrap_flags"] is False
        assert consumer["consumer_lock_sha256"] == sha(files["consumer.Cargo.lock"])
        assert consumer["missing_metadata_rejected"] == ["hmac_user_del_etc", "hmac_res_free_mac_user_etc"]
        assert tx["elf_sha256"] == cleanup["elf_sha256"] == storage["elf_sha256"]
        assert len(tx["edges"]) == tx["rejected_call_mutations"] == 11 and tx["metadata"]["bytes"] == 584
        assert len(cleanup["edges"]) == cleanup["rejected_call_mutations"] == 6
        assert rx["schema"] == "net0-rx-stop-link/v3" and rx["rejected_call_and_address_mutations"] == 25
        assert len(rx["edges"]) == 18 and len(rx["veneers"]) == 7 and rx["metadata"]["bytes"] == 80
        assert mode["schema"] == "net0-direct-rx-link/v1" and mode["rejected_mutations"] == 8
        assert mode["message"] == 595 and mode["reject_status"] == 103 and len(mode["edges"]) == 6
        assert mode["metadata"]["bytes"] == 1 and mode["metadata"]["packet_payload_bytes"] == 0
        assert mode["elf_sha256"] == rx["elf_sha256"] != tx["elf_sha256"]
        parity = {"stop_edges": [(e["source"], e["target"]) for e in rx["edges"]],
                  "mode_edges": [(e["source"], e["target"]) for e in mode["edges"]],
                  "ownership_bytes": [check["bytes"] for check in mode["ownership_checks"]],
                  "veneers": [(v["name"], v["target"], v["target_address"]) for v in rx["veneers"]]}
        if reference is not None:
            assert reference == parity
        reference = parity
        records.append({"host": host, "id": item["id"], "expires_at": item["expires_at"],
            "downloaded_zip_sha256": sha(raw), "github_digest": item["digest"],
            "rx_mode_elf_sha256": mode["elf_sha256"],
            "files": [{"name": name, "sha256": sha(data), "bytes": len(data)} for name, data in sorted(files.items())]})
    result = {"schema": "net0-rx-deadline-downloads/v1", "source": SOURCE, "ci_url": ci["url"],
              "successful_jobs": 23, "artifacts": records, "semantic_parity": reference,
              "boundary": "Downloaded report ZIPs, not firmware or registry-only facade acceptance; finite Actions retention"}
    a.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"verified_zips": len(records), "member_digests": sum(len(r["files"]) for r in records)}))


if __name__ == "__main__":
    main()
