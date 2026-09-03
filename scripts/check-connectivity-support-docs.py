#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# ///
"""Check connectivity reference facts against crate and artifact metadata."""

from __future__ import annotations

import json
from pathlib import Path
import re
import tomllib


ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs/src/reference/12-connectivity-support.md"
FACADE_MANIFEST = ROOT / "crates/hisi-rf/Cargo.toml"
BACKEND_MANIFEST = ROOT / "crates/chips/ws63/hisi-rf-ws63/Cargo.toml"
BLOB_MANIFEST = ROOT / "crates/chips/ws63/ws63-radio-sys/crates/ws63-radio-blob/Cargo.toml"
BLOB_FACTS = (
    ROOT
    / "crates/chips/ws63/ws63-radio-sys/crates/ws63-radio-blob/artifacts/manifest.json"
)
PROFILE_SOURCE = ROOT / "crates/chips/ws63/hisi-rf-ws63/src/profile.rs"
FACADE_SOURCE = ROOT / "crates/hisi-rf/src/lib.rs"
ROM_MANIFEST = ROOT / "crates/chips/ws63/hisi-rom-sys-ws63/assets/ws63/manifest.txt"


def package_version(path: Path) -> str:
    with path.open("rb") as source:
        return tomllib.load(source)["package"]["version"]


def require(text: str, value: str, errors: list[str], label: str) -> None:
    if value not in text:
        errors.append(f"missing {label}: {value}")


def main() -> int:
    document = DOC.read_text()
    facade = tomllib.loads(FACADE_MANIFEST.read_text())
    profile_source = PROFILE_SOURCE.read_text()
    facade_source = FACADE_SOURCE.read_text()
    blob = json.loads(BLOB_FACTS.read_text())
    rom = ROM_MANIFEST.read_text()
    errors: list[str] = []

    for label, path in (
        ("hisi-rf version", FACADE_MANIFEST),
        ("hisi-rf-ws63 version", BACKEND_MANIFEST),
        ("ws63-radio-blob version", BLOB_MANIFEST),
    ):
        require(document, package_version(path), errors, label)

    profile_names = sorted(
        name for name in facade["features"] if name.startswith("profile-")
    )
    for profile in profile_names:
        require(document, f"`{profile}`", errors, "named profile")

    for pattern, label in (
        (r'RESOURCE_REPORT_SCHEMA: &str = "([^"]+)"', "Wi-Fi report schema"),
        (r'PROFILE_REVISION: &str = "([^"]+)"', "Wi-Fi profile revision"),
        (r'RADIO_RESOURCE_REPORT_SCHEMA: &str = "([^"]+)"', "BLE/SLE report schema"),
    ):
        source = profile_source if label.startswith("Wi-Fi") else facade_source
        match = re.search(pattern, source)
        if match is None:
            errors.append(f"cannot resolve {label} from source")
        else:
            require(document, match.group(1), errors, label)

    require(document, blob["profile_revision"], errors, "blob profile revision")
    upstream = blob["native_supplicant"]["upstream"]
    for key in ("tag", "commit", "release_archive_sha256"):
        require(document, upstream[key], errors, f"hostap {key}")
    require(document, blob["ble_profile"]["revision"], errors, "BLE ABI revision")
    require(document, blob["sle_profile"]["revision"], errors, "SLE ABI revision")

    for line in rom.splitlines():
        if line.startswith("source_sha256."):
            require(document, line.split("=", 1)[1], errors, "ROM source hash")

    if "R0（完成，alpha 发布资料）" not in (
        ROOT / "docs/plan/hisi-connectivity-stack.md"
    ).read_text():
        errors.append("connectivity plan does not link the completed R0 alpha material")

    if errors:
        for error in errors:
            print(f"connectivity-support-docs: {error}")
        return 1
    print(
        "connectivity-support-docs: OK "
        f"({len(profile_names)} profiles, {package_version(FACADE_MANIFEST)})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
