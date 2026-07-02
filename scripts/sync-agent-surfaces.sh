#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"

usage() {
  cat >&2 <<'USAGE'
usage:
  scripts/sync-agent-surfaces.sh --check
  scripts/sync-agent-surfaces.sh --sync
  scripts/sync-agent-surfaces.sh --from-path PATH

Keeps mirrored agent surfaces aligned:
  - AGENTS.md -> CLAUDE.md symlink.
  - .claude/skills -> ../.agents/skills symlink.
  - .codex/agents/unsafe-auditor.toml <-> .claude/agents/unsafe-auditor.md.
USAGE
}

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

ensure_root_symlink() {
  local link="$ROOT/AGENTS.md"
  if [ -L "$link" ] && [ "$(readlink "$link")" = "CLAUDE.md" ]; then
    return 0
  fi
  if [ -e "$link" ] && [ ! -L "$link" ]; then
    echo "warning: AGENTS.md is a regular file; replace it with a symlink manually or via git" >&2
    return 0
  fi
  ln -s CLAUDE.md "$link"
}

ensure_skills_symlink() {
  local link="$ROOT/.claude/skills"
  if [ -L "$link" ] && [ "$(readlink "$link")" = "../.agents/skills" ]; then
    return 0
  fi
  if [ -e "$link" ] && [ ! -L "$link" ]; then
    echo "warning: .claude/skills is a real directory; replace it with symlink ../.agents/skills manually or via git" >&2
    return 0
  fi
  ln -s ../.agents/skills "$link"
}

write_claude_unsafe_agent_from_codex() {
  local src="$ROOT/.codex/agents/unsafe-auditor.toml"
  local dst="${1:-$ROOT/.claude/agents/unsafe-auditor.md}"
  mkdir -p "$(dirname "$dst")"
  {
    echo "---"
    awk -F' = ' '/^name = / { gsub(/^"|"$/, "", $2); print "name: " $2 }' "$src"
    awk -F' = ' '/^description = / { gsub(/^"|"$/, "", $2); print "description: " $2 }' "$src"
    echo "tools: Read, Grep, Glob, Bash, Edit"
    echo "model: inherit"
    echo "---"
    echo
    awk '
      /^developer_instructions = """$/ { in_body = 1; next }
      /^"""$/ && in_body { in_body = 0; next }
      in_body { print }
    ' "$src"
  } > "$dst"
}

write_codex_unsafe_agent_from_claude() {
  local src="$ROOT/.claude/agents/unsafe-auditor.md"
  local dst="${1:-$ROOT/.codex/agents/unsafe-auditor.toml}"
  mkdir -p "$(dirname "$dst")"
  if [ "$dst" = "$ROOT/.codex/agents/unsafe-auditor.toml" ] \
    && ! ( : > "$ROOT/.codex/agents/unsafe-auditor.toml.sync-test" ) 2>/dev/null; then
    echo "warning: cannot write .codex/agents/unsafe-auditor.toml in this environment; run --check from a writable agent surface to verify drift" >&2
    return 0
  fi
  rm -f "$ROOT/.codex/agents/unsafe-auditor.toml.sync-test"
  {
    awk '
      BEGIN { name = ""; description = "" }
      /^name: / { name = substr($0, 7) }
      /^description: / { description = substr($0, 14) }
      /^---$/ && seen { exit }
      /^---$/ { seen = 1 }
      END {
        gsub(/"/, "\\\"", name);
        gsub(/"/, "\\\"", description);
        print "name = \"" name "\"";
        print "description = \"" description "\"";
        print "developer_instructions = \"\"\"";
      }
    ' "$src"
    awk '
      BEGIN { in_body = 0; fence_count = 0 }
      /^---$/ { fence_count++; if (fence_count == 2) { in_body = 1; next } }
      in_body { print }
    ' "$src"
    echo '"""'
  } > "$dst"
}

sync_all_from_agents() {
  ensure_root_symlink
  ensure_skills_symlink
  write_claude_unsafe_agent_from_codex
}

sync_from_path() {
  local path="$1"
  case "$path" in
    "$ROOT/CLAUDE.md"|CLAUDE.md)
      ensure_root_symlink
      ;;
    "$ROOT/AGENTS.md"|AGENTS.md)
      ensure_root_symlink
      ;;
    "$ROOT/.agents/skills/"*|.agents/skills/*|"$ROOT/.claude/skills/"*|.claude/skills/*)
      ensure_skills_symlink
      ;;
    "$ROOT/.codex/agents/unsafe-auditor.toml"|.codex/agents/unsafe-auditor.toml)
      write_claude_unsafe_agent_from_codex
      ;;
    "$ROOT/.claude/agents/unsafe-auditor.md"|.claude/agents/unsafe-auditor.md)
      write_codex_unsafe_agent_from_claude
      ;;
  esac
}

check_sync() {
  local failed=0

  if [ ! -L "$ROOT/AGENTS.md" ] || [ "$(readlink "$ROOT/AGENTS.md" 2>/dev/null || true)" != "CLAUDE.md" ]; then
    echo "AGENTS.md must be a symlink to CLAUDE.md" >&2
    failed=1
  fi

  if [ ! -L "$ROOT/.claude/skills" ] || [ "$(readlink "$ROOT/.claude/skills" 2>/dev/null || true)" != "../.agents/skills" ]; then
    echo ".claude/skills must be a symlink to ../.agents/skills" >&2
    failed=1
  fi

  write_claude_unsafe_agent_from_codex "$tmpdir/unsafe-auditor.md"
  if ! diff -u "$tmpdir/unsafe-auditor.md" "$ROOT/.claude/agents/unsafe-auditor.md"; then
    failed=1
  fi

  if [ "$failed" -ne 0 ]; then
    echo "agent surfaces are out of sync; run scripts/sync-agent-surfaces.sh --sync" >&2
    exit 1
  fi
}

case "${1:-}" in
  --check)
    check_sync
    ;;
  --sync)
    sync_all_from_agents
    ;;
  --from-path)
    [ $# -eq 2 ] || { usage; exit 2; }
    sync_from_path "$2"
    ;;
  *)
    usage
    exit 2
    ;;
esac
