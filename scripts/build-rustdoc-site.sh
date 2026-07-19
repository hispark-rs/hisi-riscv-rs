#!/usr/bin/env bash
# Compatibility entrypoint; the implementation reads docs/chips.toml.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec uv run "$ROOT/scripts/build-rustdoc-site.py" "$@"
