#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Enforce uv ownership for parent-repository Python entry points."""

from __future__ import annotations

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
SCRIPT_GLOBS = (
    "scripts/*.py",
    "hil/*.py",
    ".agents/skills/*/scripts/*.py",
    "chips/ws63/rf/tools/*.py",
)
INVOCATION_GLOBS = (
    ".github/workflows/*.yml",
    ".agents/skills/**/*.md",
    ".agents/skills/**/*.sh",
    "scripts/*.sh",
    "hil/*.sh",
    "chips/ws63/rf/tools/*.sh",
    "docs/src/**/*.md",
    "docs/plan/**/*.md",
    "hil/README.md",
    "docs/book.toml",
)
UV_SHEBANG = "#!/usr/bin/env -S uv run --script"


def fail(message: str) -> None:
    print(f"python uv contract: {message}", file=sys.stderr)
    raise SystemExit(1)


scripts = sorted({path for pattern in SCRIPT_GLOBS for path in ROOT.glob(pattern)})
if not scripts:
    fail("no parent-owned Python scripts found")

for path in scripts:
    lines = path.read_text(encoding="utf-8").splitlines()
    relative = path.relative_to(ROOT)
    if not lines or lines[0] != UV_SHEBANG:
        fail(f"{relative} is not a uv single-file script")
    header = "\n".join(lines[1:8])
    if "# /// script" not in header or "# requires-python" not in header:
        fail(f"{relative} has no PEP 723 metadata")
    if "# dependencies =" not in header or "# ///" not in header:
        fail(f"{relative} has an incomplete PEP 723 dependency block")

for pattern in INVOCATION_GLOBS:
    for path in ROOT.glob(pattern):
        if not path.is_file():
            continue
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if "python3" in line:
                fail(
                    f"{path.relative_to(ROOT)}:{number} invokes system python3; "
                    "use a uv single-file script or uv run"
                )
            if "uv python find" in line or "$PYTHON" in line or "PYTHON=" in line:
                fail(
                    f"{path.relative_to(ROOT)}:{number} bypasses uv execution; "
                    "invoke a PEP 723 script with uv run"
                )

for path in sorted((ROOT / ".github/workflows").glob("*.yml")):
    lines = path.read_text(encoding="utf-8").splitlines()
    for index, line in enumerate(lines):
        if "uses: astral-sh/setup-uv@v7" not in line:
            continue
        action_block = "\n".join(lines[index + 1 : index + 4])
        if 'cache-dependency-glob: "**/*.py"' not in action_block:
            fail(
                f"{path.relative_to(ROOT)}:{index + 1} must key the uv cache "
                "from PEP 723 Python scripts"
            )

print(f"python uv contract OK: {len(scripts)} parent-owned single-file scripts")
