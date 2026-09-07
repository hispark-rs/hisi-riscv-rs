#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Fail-closed acceptance of an explicitly identified release, never infer its kind."""
from __future__ import annotations
import argparse
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
import urllib.request


def gh(*args: str):
    return json.loads(subprocess.check_output(["gh", *args], text=True))


def validate_run(run: dict, jobs: list[dict], source: str, workflow: str, required: list[str]) -> None:
    if (run.get("head_sha") != source or run.get("conclusion") != "success"
            or run.get("status") != "completed"
            or run.get("path", "").split("@")[0] != f".github/workflows/{workflow}"):
        raise ValueError("release run SHA/workflow/completion does not match")
    if not jobs:
        raise ValueError("release run has no jobs")
    names = {job["name"] for job in jobs}
    if not set(required) <= names:
        raise ValueError("required release jobs are missing")
    selected = [job for job in jobs if not required or job["name"] in required]
    if any(job.get("conclusion") != "success" or job.get("status") != "completed" for job in selected):
        raise ValueError("required release job was skipped, failed or incomplete")


def validate_release(release: dict, tag: str, assets: list[str]) -> None:
    if release.get("tag_name") != tag or release.get("draft") is not False:
        raise ValueError("expected a published release for the exact tag")
    names = [asset["name"] for asset in release.get("assets", [])]
    if len(names) != len(set(names)) or not set(assets) <= set(names):
        raise ValueError("required named release assets missing or duplicated")
    if "SHA256SUMS" not in names:
        raise ValueError("release lacks SHA256SUMS")


def fetch(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "hispark-rs-release-verifier/1"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def verify_registry(package: str, source: str) -> dict:
    name, version = package.split("=", 1)
    if not re.fullmatch(r"[a-z0-9_-]+", name) or not re.fullmatch(r"[0-9A-Za-z.+-]+", version):
        raise ValueError("package must be name=exact-version")
    metadata = json.loads(fetch(f"https://crates.io/api/v1/crates/{name}/{version}"))["version"]
    if metadata["num"] != version or metadata["yanked"]:
        raise ValueError("registry version missing or yanked")
    data = fetch(f"https://crates.io/api/v1/crates/{name}/{version}/download")
    checksum = hashlib.sha256(data).hexdigest()
    if checksum != metadata["checksum"]:
        raise ValueError("registry package checksum mismatch")
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        vcs = json.load(archive.extractfile(f"{name}-{version}/.cargo_vcs_info.json"))
    if vcs["git"]["sha1"] != source or vcs["git"].get("dirty", False):
        raise ValueError("published crate was not packaged from the exact clean tag commit")
    return {"package": name, "version": version, "sha256": checksum, "source_commit": source}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("tag")
    parser.add_argument("--kind", choices=("github", "crates-io", "both"), required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--workflow", required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--required-job", action="append", default=[])
    parser.add_argument("--asset", action="append", default=[])
    parser.add_argument("--package", action="append", default=[])
    parser.add_argument("--firmware-bundle", action="store_true")
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if args.kind in ("github", "both") and not args.asset:
        parser.error("GitHub acceptance requires --asset with exact required names")
    if args.kind in ("crates-io", "both") and not args.package:
        parser.error("registry acceptance requires --package name=exact-version")
    source = gh("api", f"repos/{args.repo}/commits/{args.tag}")["sha"]
    run = gh("api", f"repos/{args.repo}/actions/runs/{args.run_id}")
    pages = gh("api", f"repos/{args.repo}/actions/runs/{args.run_id}/jobs?per_page=100", "--paginate", "--slurp")
    jobs = [job for page in pages for job in page["jobs"]]
    validate_run(run, jobs, source, args.workflow, args.required_job)
    report = {"schema": 1, "kind": args.kind, "source_commit": source,
              "run_url": run["html_url"], "run_attempt": run["run_attempt"],
              "jobs": [{"name": job["name"], "conclusion": job["conclusion"]} for job in jobs]}
    if args.kind in ("github", "both"):
        release = gh("api", f"repos/{args.repo}/releases/tags/{args.tag}")
        validate_release(release, args.tag, args.asset)
        with tempfile.TemporaryDirectory(prefix="release-download-") as directory:
            root = Path(directory)
            subprocess.run(["gh", "release", "download", args.tag, "--repo", args.repo, "--dir", directory], check=True)
            spec = importlib.util.spec_from_file_location("release_bundle", Path(__file__).with_name("release-bundle.py"))
            bundle = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(bundle)
            report["github_assets"] = bundle.verify_checksums(root)
            if args.firmware_bundle:
                report["firmware"] = bundle.verify(root, source, args.tag)
    if args.kind in ("crates-io", "both"):
        report["registry"] = [verify_registry(package, source) for package in args.package]
    report["status"] = "pass"
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"PASS: {args.repo} {args.tag}; downloaded artifacts verified ({args.kind})")


if __name__ == "__main__":
    main()
