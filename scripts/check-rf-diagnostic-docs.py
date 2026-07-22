#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Ensure every public hisi-rf diagnostic fragment resolves in the handbook."""

from __future__ import annotations

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates" / "hisi-rf-core" / "src" / "diagnostics.rs"
REFERENCE = ROOT / "docs" / "src" / "reference" / "11-rf-diagnostics.md"
SUMMARY = ROOT / "docs" / "src" / "SUMMARY.md"

CODE_ANCHOR = re.compile(r'DiagnosticCode::\w+\s*=>\s*"(errors-[a-z0-9-]+)"')
DOC_ANCHOR = re.compile(r'<a id="(errors-[a-z0-9-]+)"></a>')


def fail(messages: list[str]) -> None:
    for message in messages:
        print(f"RF diagnostic docs: {message}", file=sys.stderr)
    raise SystemExit(1)


source_anchors = CODE_ANCHOR.findall(SOURCE.read_text(encoding="utf-8"))
document_anchors = DOC_ANCHOR.findall(REFERENCE.read_text(encoding="utf-8"))
errors: list[str] = []

if not source_anchors:
    errors.append(f"no docs_anchor values found in {SOURCE.relative_to(ROOT)}")
if len(source_anchors) != len(set(source_anchors)):
    errors.append("Rust docs_anchor values must be unique")
if len(document_anchors) != len(set(document_anchors)):
    errors.append("reference page anchors must be unique")

missing = sorted(set(source_anchors) - set(document_anchors))
extra = sorted(set(document_anchors) - set(source_anchors))
if missing:
    errors.append(f"missing reference anchors: {', '.join(missing)}")
if extra:
    errors.append(f"anchors without a public diagnostic: {', '.join(extra)}")
if "reference/11-rf-diagnostics.md" not in SUMMARY.read_text(encoding="utf-8"):
    errors.append("SUMMARY.md does not include reference/11-rf-diagnostics.md")

if errors:
    fail(errors)

print(f"RF diagnostic docs check passed: {len(source_anchors)} public anchors")
