#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Reject host-specific dependencies from the WS63 radio Cargo build path."""

from __future__ import annotations

import re
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PRODUCTION_FILES = [
    ROOT / "chips/ws63/rf/build.rs",
    ROOT / "examples/ws63/wifi_init_smoke/build.rs",
    ROOT / "examples/ws63/wifi_connectivity/Cargo.toml",
    ROOT / "examples/ws63/wifi_connectivity/build.rs",
    ROOT / "crates/chips/ws63/ws63-radio-sys/crates/ws63-radio-sys/Cargo.toml",
    ROOT / "crates/chips/ws63/ws63-radio-sys/crates/ws63-radio-sys/build.rs",
    ROOT / "crates/chips/ws63/ws63-radio-sys/crates/ws63-radio-blob/Cargo.toml",
    ROOT / "crates/chips/ws63/ws63-radio-sys/crates/ws63-radio-blob/build.rs",
    ROOT / "crates/chips/ws63/hisi-rf-ws63/Cargo.toml",
    ROOT / "crates/chips/ws63/hisi-rf-ws63/build.rs",
    ROOT / "crates/hisi-rf/Cargo.toml",
    ROOT / "crates/hisi-rf/.github/fixtures/ws63-consumer/Cargo.toml",
    ROOT / "crates/hisi-rf/.github/fixtures/ws63-consumer/.cargo/config.toml",
]

CONSUMER_MANIFESTS = [
    ROOT / "crates/hisi-rf/.github/fixtures/ws63-consumer/Cargo.toml",
]

APPLICATION_MANIFEST_ROOTS = [
    ROOT / "examples/ws63",
    ROOT / "crates/hisi-rs-template",
]

FORBIDDEN_CONSUMER_DEPENDENCIES = {
    "hisi-rf-core",
    "hisi-rf-rtos-driver",
    "hisi-rf-ws63",
    "ws63-radio-blob",
    "ws63-radio-sys",
}

FORBIDDEN_APPLICATION_DEPENDENCIES = FORBIDDEN_CONSUMER_DEPENDENCIES | {
    "ws63-rf-rs",
}

# These are bounded maintainer fixtures, not the user-facing RF happy path.
# Shrink this list when each migration oracle/conformance fixture moves behind
# its owner; adding an entry requires an architecture review.
LEGACY_APPLICATION_ALLOWLIST = {
    ROOT / "examples/ws63/net_ping/Cargo.toml": {"ws63-rf-rs"},
    ROOT / "examples/ws63/rf_port_demo/Cargo.toml": {"ws63-rf-rs"},
    ROOT / "examples/ws63/rtos_budget_enforcement/Cargo.toml": {
        "hisi-rf-rtos-driver"
    },
    ROOT / "examples/ws63/rtos_embassy_coexist/Cargo.toml": {
        "hisi-rf-rtos-driver"
    },
    ROOT / "examples/ws63/rtos_preemption/Cargo.toml": {"hisi-rf-rtos-driver"},
    ROOT / "examples/ws63/rtos_priority_inheritance/Cargo.toml": {
        "hisi-rf-rtos-driver"
    },
    ROOT / "examples/ws63/rtos_scheduler_stress/Cargo.toml": {
        "hisi-rf-rtos-driver"
    },
    ROOT / "examples/ws63/wifi_blob_link/Cargo.toml": {"ws63-radio-sys"},
    ROOT / "examples/ws63/wifi_init_smoke/Cargo.toml": {
        "hisi-rf-core",
        "hisi-rf-rtos-driver",
        "ws63-radio-sys",
        "ws63-rf-rs",
    },
}

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


def dependency_names(path: Path) -> set[str]:
    text = path.read_text(encoding="utf-8")
    try:
        manifest = tomllib.loads(text)
    except tomllib.TOMLDecodeError:
        # cargo-generate manifests contain template expressions and are not TOML
        # until rendered. Hidden crate names are plain dependency keys, so a
        # conservative line-oriented extraction is sufficient for this gate.
        return {
            match.group(1)
            for match in re.finditer(r"(?m)^([A-Za-z0-9_-]+)\s*=", text)
        }
    return set(manifest.get("dependencies", {}))


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

    for path in CONSUMER_MANIFESTS:
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        dependencies = set(manifest.get("dependencies", {}))
        hidden = sorted(dependencies & FORBIDDEN_CONSUMER_DEPENDENCIES)
        if hidden:
            errors.append(
                f"{path.relative_to(ROOT)}: application directly depends on hidden RF crates: "
                + ", ".join(hidden)
            )
        if (path.parent / "build.rs").exists():
            errors.append(
                f"{path.relative_to(ROOT)}: external RF consumer must not own build.rs"
            )

    application_manifests = sorted(
        path
        for root in APPLICATION_MANIFEST_ROOTS
        for path in root.rglob("Cargo.toml")
        if path.is_file()
    )
    for path in application_manifests:
        dependencies = dependency_names(path)
        hidden = dependencies & FORBIDDEN_APPLICATION_DEPENDENCIES
        allowed = LEGACY_APPLICATION_ALLOWLIST.get(path, set())
        unexpected = sorted(hidden - allowed)
        stale = sorted(allowed - hidden)
        if unexpected:
            errors.append(
                f"{path.relative_to(ROOT)}: application directly depends on hidden RF crates: "
                + ", ".join(unexpected)
            )
        if stale:
            errors.append(
                f"{path.relative_to(ROOT)}: shrink stale RF migration allowlist: "
                + ", ".join(stale)
            )

    if errors:
        print("RF production path drift detected:")
        for error in errors:
            print(f"  - {error}")
        return 1

    print(
        "RF production path OK: "
        f"checked {len(PRODUCTION_FILES)} files and "
        f"{len(CONSUMER_MANIFESTS)} consumer manifests plus "
        f"{len(application_manifests)} application manifests"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
