#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Deploy a Pages artifact using an explicit generated-site commit SHA.

This follows the GitHub-owned actions/deploy-pages API flow. The official action
always uses the source GITHUB_SHA as pages_build_version; that deduplicates a tag
deployment when main already deployed the same source commit. Versioned docs use
the generated gh-pages state commit instead.
"""

from __future__ import annotations

import argparse
import json
import os
import time
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen


FINAL_FAILURES = {
    "deployment_failed",
    "deployment_content_failed",
    "deployment_cancelled",
    "deployment_lost",
}


def request_json(
    url: str,
    *,
    token: str,
    method: str = "GET",
    payload: dict[str, object] | None = None,
) -> dict[str, object]:
    data = None if payload is None else json.dumps(payload).encode()
    request = Request(
        url,
        data=data,
        method=method,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "hisi-versioned-docs-deployer",
        },
    )
    try:
        with urlopen(request, timeout=30) as response:
            return json.loads(response.read())
    except HTTPError as error:
        body = error.read().decode(errors="replace")
        raise RuntimeError(f"{method} {url}: HTTP {error.code}: {body}") from error


def oidc_token() -> str:
    request_url = os.environ["ACTIONS_ID_TOKEN_REQUEST_URL"]
    request_token = os.environ["ACTIONS_ID_TOKEN_REQUEST_TOKEN"]
    response = request_json(request_url, token=request_token)
    value = response.get("value")
    if not isinstance(value, str) or not value:
        raise RuntimeError("Actions OIDC response has no token value")
    return value


def deployment_payload(artifact_id: int, build_version: str, oidc: str) -> dict[str, object]:
    if len(build_version) != 40 or any(c not in "0123456789abcdef" for c in build_version):
        raise ValueError("build version must be a lowercase 40-character Git commit SHA")
    return {
        "artifact_id": artifact_id,
        "pages_build_version": build_version,
        "oidc_token": oidc,
    }


def write_output(name: str, value: str) -> None:
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with Path(output).open("a", encoding="utf-8") as stream:
            stream.write(f"{name}={value}\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-id", type=int, required=True)
    parser.add_argument("--build-version", required=True)
    parser.add_argument("--timeout", type=int, default=600)
    parser.add_argument("--interval", type=float, default=5.0)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    if args.dry_run:
        print(json.dumps(deployment_payload(args.artifact_id, args.build_version, "OIDC")))
        return 0

    repository = os.environ["GITHUB_REPOSITORY"]
    github_token = os.environ["GITHUB_TOKEN"]
    api = os.environ.get("GITHUB_API_URL", "https://api.github.com").rstrip("/")
    payload = deployment_payload(args.artifact_id, args.build_version, oidc_token())
    print(
        "creating Pages deployment:",
        json.dumps({**payload, "oidc_token": "***"}, indent=2),
    )
    deployment = request_json(
        f"{api}/repos/{repository}/pages/deployments",
        token=github_token,
        method="POST",
        payload=payload,
    )
    deployment_id = deployment.get("id")
    if deployment_id is None:
        status_url = str(deployment.get("status_url", ""))
        deployment_id = status_url.rstrip("/").rsplit("/", 1)[-1]
    if not deployment_id:
        raise RuntimeError(f"Pages deployment response has no id: {deployment}")

    page_url = str(deployment.get("page_url", ""))
    write_output("page_url", page_url)
    deadline = time.monotonic() + args.timeout
    status_url = f"{api}/repos/{repository}/pages/deployments/{deployment_id}"
    while time.monotonic() < deadline:
        time.sleep(args.interval)
        status = str(request_json(status_url, token=github_token).get("status", ""))
        print(f"Pages deployment {deployment_id}: {status}")
        if status == "succeed":
            return 0
        if status in FINAL_FAILURES:
            raise RuntimeError(f"Pages deployment failed: {status}")
    raise RuntimeError(f"Pages deployment timed out after {args.timeout}s")


if __name__ == "__main__":
    raise SystemExit(main())
