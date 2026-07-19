#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Reject host-specific dependencies from the WS63 radio Cargo build path."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PRODUCTION_FILES = [
    ROOT / "chips/ws63/rf/build.rs",
    ROOT / "examples/ws63/wifi_init_smoke/build.rs",
    ROOT / "crates/chips/ws63/ws63-radio-sys/crates/ws63-radio-sys/Cargo.toml",
    ROOT / "crates/chips/ws63/ws63-radio-sys/crates/ws63-radio-sys/build.rs",
    ROOT / "crates/chips/ws63/ws63-radio-sys/crates/ws63-radio-blob/Cargo.toml",
    ROOT / "crates/chips/ws63/ws63-radio-sys/crates/ws63-radio-blob/build.rs",
]

FORBIDDEN = {
    "macOS user path": re.compile(r"/Users/[^/]+/"),
    "Linux user path": re.compile(r"/home/[^/]+/"),
    "Windows drive path": re.compile(r"(?i)(?:^|[\"'])\s*[a-z]:[\\/]"),
    "system Python process": re.compile(r"Command::new\([^\n]*(?:python3?|/usr/bin/env)"),
    "shell process": re.compile(r"Command::new\([^\n]*(?:bash|sh)\""),
    "external RISC-V tool": re.compile(
        r"(?:riscv64-unknown-elf|riscv32-linux-musl)-(?:gcc|cc|ar|nm|objcopy|ld)"
    ),
}

FORBIDDEN_CONTRACT_TOKENS = {
    ROOT / "examples/ws63/wifi_init_smoke/build.rs": ["WS63_RF_LIB_DIR"],
    ROOT / "crates/chips/ws63/ws63-radio-sys/crates/ws63-radio-sys/build.rs": [
        "../../ws63-RF",
        "../hisi-rf-link/profiles",
        "../../linker",
        "WS63_RF_ROOT",
    ],
}


def main() -> int:
    errors: list[str] = []
    for path in PRODUCTION_FILES:
        if not path.is_file():
            errors.append(f"missing production input: {path.relative_to(ROOT)}")
            continue
        text = path.read_text(encoding="utf-8")
        for label, pattern in FORBIDDEN.items():
            for match in pattern.finditer(text):
                line = text.count("\n", 0, match.start()) + 1
                errors.append(f"{path.relative_to(ROOT)}:{line}: {label}")
        for token in FORBIDDEN_CONTRACT_TOKENS.get(path, []):
            if token in text:
                errors.append(
                    f"{path.relative_to(ROOT)}: forbidden production contract token {token!r}"
                )

    if errors:
        print("RF production path drift detected:")
        for error in errors:
            print(f"  - {error}")
        return 1

    print(f"RF production path OK: checked {len(PRODUCTION_FILES)} files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
