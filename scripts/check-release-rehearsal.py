#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Remove only this workflow's rehearsal draft; never treat network errors as absence."""
import argparse
import json
import os
import subprocess


def lookup(repo: str, version: str):
    result = subprocess.run(["gh", "api", f"repos/{repo}/releases?per_page=100", "--paginate", "--slurp"],
                            capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr)
    matches = [release for page in json.loads(result.stdout) for release in page if release["tag_name"] == version]
    if len(matches) > 1:
        raise RuntimeError("duplicate rehearsal releases")
    return matches[0] if matches else None


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
    parser.add_argument("--source", required=True)
    args = parser.parse_args()
    expected = f"v0.0.0-rehearsal.{os.environ['GITHUB_RUN_ID']}.{os.environ['GITHUB_RUN_ATTEMPT']}"
    if args.version != expected:
        raise SystemExit("refusing to clean a non-rehearsal release")
    repo = os.environ["GITHUB_REPOSITORY"]
    release = lookup(repo, args.version)
    if release is not None:
        if not release["draft"] or release["target_commitish"] != args.source:
            raise SystemExit("rehearsal was published or has a different source; manual audit required")
        subprocess.run(["gh", "api", "--method", "DELETE", f"repos/{repo}/releases/{release['id']}"], check=True)
    if lookup(repo, args.version) is not None:
        raise SystemExit("rehearsal draft cleanup failed")
    print("PASS: rehearsal has no public release and draft was removed")


if __name__ == "__main__":
    main()
