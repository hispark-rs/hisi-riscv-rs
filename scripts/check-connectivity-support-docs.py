#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Check connectivity reference facts against crate and artifact metadata."""

from __future__ import annotations

import argparse
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


def string_variants(source: str, name: str) -> tuple[str, ...]:
    """Accept the two reviewed constant shapes; unknown Rust fails closed."""
    definitions = re.findall(rf'\b{re.escape(name)}: &str\s*=\s*(.*?);', source, re.S)
    if len(definitions) != 1:
        raise ValueError(f"expected one {name} definition")
    expression = definitions[0].strip()
    literal = re.fullmatch(r'"([^"\\]+)"', expression)
    if literal:
        return (literal[1],)
    conditional = re.fullmatch(
        r'if\s+cfg!\(feature\s*=\s*"[a-z0-9-]+"\)\s*'
        r'\{\s*"([^"\\]+)"\s*\}\s*else\s*\{\s*"([^"\\]+)"\s*\}',
        expression,
    )
    if conditional:
        return conditional[1], conditional[2]
    raise ValueError(f"unsupported {name} initializer")


def self_test() -> None:
    assert string_variants('const SCHEMA: &str = "v13";', "SCHEMA") == ("v13",)
    conditional = 'const SCHEMA: &str = if cfg!(feature = "standard-l2") { "v14" } else { "v13" };'
    assert string_variants(conditional, "SCHEMA") == ("v14", "v13")
    errors: list[str] = []
    for value in string_variants(conditional, "SCHEMA"):
        require("document v13", value, errors, "schema")
    assert errors == ["missing schema: v14"]
    for source in ("", conditional + conditional, 'const SCHEMA: &str = unknown("v14");'):
        try:
            string_variants(source, "SCHEMA")
        except ValueError:
            pass
        else:
            raise AssertionError(f"accepted ambiguous/unknown initializer: {source}")
    print("connectivity-support-docs: constant parser mutations OK")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    if parser.parse_args().self_test:
        self_test()
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

    for name, label in (
        ("RESOURCE_REPORT_SCHEMA", "Wi-Fi report schema"),
        ("PROFILE_REVISION", "Wi-Fi profile revision"),
        ("RADIO_RESOURCE_REPORT_SCHEMA", "BLE/SLE report schema"),
    ):
        source = profile_source if label.startswith("Wi-Fi") else facade_source
        try:
            for value in string_variants(source, name):
                require(document, value, errors, label)
        except ValueError as error:
            errors.append(f"cannot resolve {label}: {error}")

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
