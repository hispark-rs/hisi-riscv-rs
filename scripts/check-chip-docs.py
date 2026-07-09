#!/usr/bin/env python3
"""Validate chip metadata used by mdBook and rustdoc publishing."""

from __future__ import annotations

from pathlib import Path
import re
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]
CHIPS_TOML = ROOT / "docs" / "chips.toml"
DOCS_SRC = ROOT / "docs" / "src"

REQUIRED = {
    "display_name",
    "family",
    "status",
    "hil",
    "qemu_machine",
    "hal_features",
    "rt_features",
    "pac_crate",
    "api_chip",
    "app_addr",
    "fwpkg_chip",
    "probe_chip",
    "rustdoc_packages",
    "rustdoc_features",
    "rustdoc_no_default_features",
    "notes",
}

CHIP_FIELD_RE = re.compile(r"\{\{#chip-field\s+([A-Za-z0-9_.-]+)\s*\}\}")
CHIP_IF_RE = re.compile(r"\{\{#chip-if\s+([A-Za-z0-9_.-]+)\s*\}\}")
API_LINK_RE = re.compile(r"\{\{#api-link\s+([^|}]+)\|([^}]+)\}\}")


def main() -> int:
    data = tomllib.loads(CHIPS_TOML.read_text(encoding="utf-8"))
    errors: list[str] = []
    chips = data.get("chips")
    selectable = data.get("selectable", [])
    default = data.get("default")

    if not isinstance(chips, dict):
        errors.append("docs/chips.toml must define [chips.<id>] tables")
        chips = {}
    if default not in chips:
        errors.append(f"default chip is not defined: {default}")
    for chip in selectable:
        if chip not in chips:
            errors.append(f"selectable chip is not defined: {chip}")

    for chip, meta in chips.items():
        missing = sorted(REQUIRED - set(meta))
        if missing:
            errors.append(f"{chip}: missing fields: {', '.join(missing)}")
        status = meta.get("status")
        if status not in {"stable", "experimental", "planned"}:
            errors.append(f"{chip}: invalid status {status!r}")
        api_chip = meta.get("api_chip")
        if status != "planned" and api_chip not in chips:
            errors.append(f"{chip}: api_chip must refer to a defined chip")
        if api_chip and chips.get(api_chip, {}).get("api_chip") != api_chip:
            errors.append(f"{chip}: api_chip must refer to a concrete rustdoc chip")
        if chip in selectable and status == "planned":
            errors.append(f"{chip}: planned chips must not be selectable")
        packages = meta.get("rustdoc_packages")
        if not isinstance(packages, list):
            errors.append(f"{chip}: rustdoc_packages must be a list")
        if meta.get("rustdoc_no_default_features") not in {True, False}:
            errors.append(f"{chip}: rustdoc_no_default_features must be boolean")
        if status != "planned" and api_chip == chip and not packages:
            errors.append(f"{chip}: concrete rustdoc chips must list rustdoc_packages")

    for path in sorted(DOCS_SRC.rglob("*.md")):
        rel = path.relative_to(ROOT)
        text = path.read_text(encoding="utf-8")
        for lineno, line in enumerate(text.splitlines(), start=1):
            for match in CHIP_FIELD_RE.finditer(line):
                field = match.group(1)
                for chip in selectable:
                    if field not in chips.get(chip, {}):
                        errors.append(f"{rel}:{lineno}: chip field {field!r} missing for {chip}")
            for match in CHIP_IF_RE.finditer(line):
                chip = match.group(1)
                if chip not in chips:
                    errors.append(f"{rel}:{lineno}: unknown chip in chip-if: {chip}")
            for match in API_LINK_RE.finditer(line):
                api_path = match.group(1).strip()
                label = match.group(2).strip()
                if not api_path or not label:
                    errors.append(f"{rel}:{lineno}: api-link needs path and label")

    if errors:
        print("chip docs metadata errors:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
