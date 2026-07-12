#!/usr/bin/env python3
"""Build chip-specific rustdoc from docs/chips.toml metadata."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tomllib


ROOT = Path(__file__).resolve().parents[1]
CHIPS_TOML = ROOT / "docs" / "chips.toml"


def load_build_chips(requested: list[str] | None) -> list[tuple[str, dict[str, object]]]:
    data = tomllib.loads(CHIPS_TOML.read_text(encoding="utf-8"))
    chips: dict[str, dict[str, object]] = data["chips"]
    selectable: list[str] = data["selectable"]
    wanted = requested or selectable
    seen_api_chips: set[str] = set()
    result: list[tuple[str, dict[str, object]]] = []

    for chip in wanted:
        if chip not in chips:
            raise SystemExit(f"unknown chip: {chip}")
        meta = chips[chip]
        api_chip = str(meta.get("api_chip") or "")
        if not api_chip:
            continue
        if api_chip != chip:
            continue
        if api_chip in seen_api_chips:
            continue
        packages = meta.get("rustdoc_packages")
        if not isinstance(packages, list) or not packages:
            continue
        seen_api_chips.add(api_chip)
        result.append((chip, meta))

    if not result:
        raise SystemExit("no rustdoc-capable chips selected")
    return result


def split_csv(value: object) -> list[str]:
    if not isinstance(value, str) or not value.strip():
        return []
    return [part.strip() for part in value.split(",") if part.strip()]


def build_chip(chip: str, meta: dict[str, object], version: str, out: Path, target: str) -> None:
    packages = [str(package) for package in meta["rustdoc_packages"]]  # checked by load_build_chips
    features = split_csv(meta.get("rustdoc_features"))
    dest = out / "api" / version / chip

    target_doc = ROOT / "target" / target / "doc"
    if target_doc.exists():
        shutil.rmtree(target_doc)

    cmd = ["cargo", "doc", "-Zbuild-std=core,alloc"]
    for package in packages:
        cmd.extend(["-p", package])
    cmd.append("--no-deps")
    if bool(meta.get("rustdoc_no_default_features")):
        cmd.append("--no-default-features")
    if features:
        cmd.extend(["--features", ",".join(features)])

    print(f"rustdoc-site: building {chip} API docs")
    subprocess.run(cmd, cwd=ROOT, check=True)

    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True, exist_ok=True)
    shutil.copytree(target_doc, dest, dirs_exist_ok=True)
    (dest / "index.html").write_text(
        '<!doctype html><meta http-equiv="refresh" content="0; url=hisi_hal/index.html">\n',
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", default=os.environ.get("DOC_VERSION", "main"))
    parser.add_argument("--out", default=os.environ.get("DOC_SITE_DIR", "site"))
    parser.add_argument("--chip", action="append")
    args = parser.parse_args()

    target = os.environ.get("TARGET", "riscv32imfc-unknown-none-elf")
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    for chip, meta in load_build_chips(args.chip):
        build_chip(chip, meta, args.version, out, target)

    api_latest = out / "api" / "latest"
    latest_version = os.environ.get("LATEST_VERSION", args.version)
    if args.version == latest_version:
        if api_latest.exists():
            shutil.rmtree(api_latest)
        api_latest.mkdir(parents=True, exist_ok=True)
        shutil.copytree(out / "api" / args.version, api_latest, dirs_exist_ok=True)

    (out / "api").mkdir(parents=True, exist_ok=True)
    (out / "api" / "index.html").write_text(
        f'<!doctype html><meta http-equiv="refresh" content="0; url={args.version}/ws63/">\n',
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
