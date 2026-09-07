#!/usr/bin/env bash
# Explicit, read-only release acceptance. Publish/tag operations are separate.
set -euo pipefail
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)"
exec uv run --script "$REPO_ROOT/scripts/verify-release.py" "$@"
