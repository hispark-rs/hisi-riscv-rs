#!/usr/bin/env bash
# Compatibility wrapper. The executable source of truth for tutorial and happy
# path commands is scripts/tutorial-contracts.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec "$ROOT/scripts/tutorial-contracts.sh" "$@"
