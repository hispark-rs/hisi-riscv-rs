#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Enforce the ws63-radio-sys archive profile as the operational fact source."""

import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROFILE = ROOT / "crates/chips/ws63/ws63-radio-sys/crates/hisi-rf-link/profiles/ws63.toml"
OPERATIONAL = [
    ROOT / "examples/ws63/wifi_init_smoke/build.rs",
    ROOT / "examples/ws63/wifi_blob_link/build.rs",
    *sorted((ROOT / "chips/ws63/rf/tools").glob("*.sh")),
]


def fail(message: str) -> None:
    print(f"radio profile drift: {message}", file=sys.stderr)
    raise SystemExit(1)


profile = tomllib.loads(PROFILE.read_text())
wifi = profile["wifi_archives"]
wpa = profile["wpa_archives"]
names = [entry["name"] for entry in wifi + wpa]
if len(names) != len(set(names)):
    fail("archive names are not unique")
for key in ("link_order", "transform_order"):
    values = [entry[key] for entry in wifi]
    if len(values) != len(set(values)):
        fail(f"wifi {key} values are not unique")
if len(profile["wifi_root_symbols"]) != len(set(profile["wifi_root_symbols"])):
    fail("Wi-Fi root symbols are not unique")

allowed = set(names) | {"rom_callback"}
archive_pattern = re.compile(r"lib([A-Za-z0-9_]+)\.a")
for path in OPERATIONAL:
    text = path.read_text()
    if "chips/ws63/rf/ws63-RF" in text:
        fail(f"legacy payload path in {path.relative_to(ROOT)}")
    unknown = sorted(set(archive_pattern.findall(text)) - allowed)
    if unknown:
        fail(f"unprofiled archives in {path.relative_to(ROOT)}: {unknown}")

print(f"radio profile OK: {len(wifi)} Wi-Fi + {len(wpa)} WPA archives")
