#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyyaml==6.0.2"]
# ///
"""Negative release acceptance tests, including real ELF/image equivalence."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import yaml

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BUNDLE = load("release-bundle")
VERIFIER = load("verify-release")


def elf_fixture():
    # Generic RV32 ELF with a .data load address and an erased gap.
    result = bytearray(0x208)
    result[:52] = struct.pack("<16sHHIIIIIHHHHHH", b"\x7fELF\x01\x01\x01" + bytes(9),
                             2, 243, 1, 0x230300, 52, 0, 4, 52, 32, 2, 0, 0, 0)
    for index, (offset, vaddr, paddr, size) in enumerate(((0x100, 0x230300, 0x230300, 16), (0x200, 0xA0C000, 0x230330, 8))):
        result[52 + index * 32:84 + index * 32] = struct.pack("<IIIIIIII", 1, offset, vaddr, paddr, size, size, 7, 4)
        result[offset:offset + size] = bytes(range(1, size + 1))
    return result


def candidate(root):
    (root / "blinky.elf").write_bytes(elf_fixture())
    plan = BUNDLE.plan(root / "blinky.elf", root / "blinky.img")
    BUNDLE.write_json(root / "blinky.plan.json", plan)
    (root / "Cargo.lock").write_text("test-only lock")
    (root / "rust-toolchain.toml").write_text("test-only toolchain")
    manifest = {"schema": 1, "version": "v0.0.0-test", "source_commit": "1" * 40,
                "source_ci": "test-only", "submodules": [{"path": "fixture", "commit": "2" * 40}],
                "tools": {"cargo": "test-only", "rustc": "test-only", "hisi-fwpkg": BUNDLE.run("hisi-fwpkg", "--version")}}
    refresh(root, manifest)
    return manifest


def refresh(root, manifest):
    manifest["artifacts"] = {name: BUNDLE.sha(root / name) for name in BUNDLE.INPUTS}
    BUNDLE.write_json(root / "release-manifest.json", manifest)
    (root / "SHA256SUMS").write_text("".join(f"{BUNDLE.sha(root / name)}  {name}\n" for name in BUNDLE.ASSETS if name != "SHA256SUMS"))


class ReleaseTests(unittest.TestCase):
    def test_real_elf_gap_and_byte_equivalence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = candidate(root)
            BUNDLE.verify(root, "1" * 40, "v0.0.0-test")
            original = (root / "blinky.img").read_bytes()
            # Even recomputing all external checksums cannot hide header/body damage.
            (root / "blinky.img").write_bytes(bytes(8) + original[8:])
            refresh(root, manifest)
            with self.assertRaises(ValueError):
                BUNDLE.verify(root, "1" * 40, "v0.0.0-test")
            (root / "blinky.img").write_bytes(original)
            data = bytearray((root / "blinky.elf").read_bytes())
            data[0x100] ^= 1
            (root / "blinky.elf").write_bytes(data)
            refresh(root, manifest)
            with self.assertRaisesRegex(ValueError, "reproduced"):
                BUNDLE.verify(root, "1" * 40, "v0.0.0-test")

    def test_checksums_are_exact_and_safe(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate(root)
            (root / "extra").write_text("undeclared")
            with self.assertRaisesRegex(ValueError, "exact asset set"):
                BUNDLE.verify_checksums(root)
            (root / "SHA256SUMS").write_text("0" * 64 + "  ../escape\n")
            with self.assertRaisesRegex(ValueError, "unsafe"):
                BUNDLE.verify_checksums(root)

    def test_wrong_or_skipped_workflow_fails(self):
        run = {"head_sha": "a", "conclusion": "success", "status": "completed", "path": ".github/workflows/release.yml"}
        jobs = [{"name": "publish", "status": "completed", "conclusion": "success"}]
        VERIFIER.validate_run(run, jobs, "a", "release.yml", [])
        for key, value in (("head_sha", "b"), ("conclusion", "failure"), ("path", ".github/workflows/ci.yml")):
            changed = dict(run, **{key: value})
            with self.assertRaises(ValueError):
                VERIFIER.validate_run(changed, jobs, "a", "release.yml", [])
        for conclusion in ("skipped", "cancelled", "failure", None):
            with self.assertRaises(ValueError):
                VERIFIER.validate_run(run, [dict(jobs[0], conclusion=conclusion)], "a", "release.yml", [])

    def test_missing_release_never_falls_back_to_registry(self):
        run = {"head_sha": "a", "conclusion": "success", "status": "completed", "path": ".github/workflows/release.yml", "html_url": "test-only", "run_attempt": 1}
        jobs = {"jobs": [{"name": "publish", "status": "completed", "conclusion": "success"}]}
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "report.json"
            argv = ["verify-release", "v1.0.0", "--kind", "github", "--repo", "test/test", "--workflow", "release.yml",
                    "--run-id", "1", "--asset", "firmware.img", "--report", str(report)]
            with patch.object(sys, "argv", argv), patch.object(VERIFIER, "gh", side_effect=[{"sha": "a"}, run, [jobs], subprocess.CalledProcessError(1, "gh")]):
                with self.assertRaises(subprocess.CalledProcessError):
                    VERIFIER.main()
            self.assertFalse(report.exists())

    def test_draft_or_missing_asset_is_not_released(self):
        for release in ({"tag_name": "v1", "draft": True}, {"tag_name": "v1", "draft": False, "assets": []}):
            with self.assertRaises(ValueError):
                VERIFIER.validate_release(release, "v1", ["firmware.img"])

    def test_publication_follows_all_gates(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/release.yml").read_text())
        steps = workflow["jobs"]["release"]["steps"]
        names = [step.get("name", "") for step in steps]
        publish = names.index("Publish only after every gate")
        for gate in ("Check versioned docs site", "Download and reverify draft release bytes", "Verify live versioned documentation", "Inject rehearsal failure before publication"):
            self.assertLess(names.index(gate), publish)
        self.assertEqual(steps[publish]["if"], "github.event_name == 'push'")
        self.assertNotIn("always", steps[publish]["if"])
        self.assertIn("--draft", steps[names.index("Create private draft candidate")]["run"])


if __name__ == "__main__":
    unittest.main()
