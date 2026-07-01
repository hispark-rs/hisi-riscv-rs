#!/usr/bin/env bash
set -u

usage() {
  cat >&2 <<'EOF'
usage: regression.sh [--run RUN_ID] [--history]

Diagnose HIL workflow failures from GitHub Actions metadata/logs.
Requires gh auth for private repos. This script reads logs only; it does not flash hardware.
EOF
}

ROOT="$(git rev-parse --show-toplevel)"
WORKFLOW="${HIL_WORKFLOW:-hil.yml}"
RUN_ID=""
HISTORY=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --run)
      [ "$#" -ge 2 ] || { usage; exit 2; }
      RUN_ID="$2"
      shift 2
      ;;
    --history)
      HISTORY=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

command -v gh >/dev/null 2>&1 || {
  echo "gh is required to inspect GitHub Actions runs" >&2
  exit 1
}

if [ "$HISTORY" -eq 1 ]; then
  gh run list --workflow "$WORKFLOW" --limit 30 \
    --json databaseId,createdAt,headBranch,headSha,conclusion,displayTitle \
    --template '{{range .}}{{printf "%-12v %-20v %-10v %.12s %s\n" .databaseId .createdAt .conclusion .headSha .displayTitle}}{{end}}'
  exit 0
fi

if [ -z "$RUN_ID" ]; then
  RUN_ID="$(gh run list --workflow "$WORKFLOW" --limit 20 \
    --json databaseId,conclusion \
    --jq '[.[] | select(.conclusion != "success")][0].databaseId')"
fi

if [ -z "$RUN_ID" ] || [ "$RUN_ID" = "null" ]; then
  echo "No failed/non-successful $WORKFLOW run found." >&2
  exit 1
fi

run_json="$(gh run view "$RUN_ID" --json databaseId,url,headSha,headBranch,displayTitle,conclusion,createdAt)"
branch="$(printf '%s\n' "$run_json" | jq -r '.headBranch')"
sha="$(printf '%s\n' "$run_json" | jq -r '.headSha')"
pass_id="$(gh run list --workflow "$WORKFLOW" --branch "$branch" --limit 30 \
  --json databaseId,conclusion,headSha \
  --jq '[.[] | select(.conclusion == "success")][0].databaseId')"

fail_log="/tmp/hil-fail-$RUN_ID.log"
pass_log=""
gh run view "$RUN_ID" --log > "$fail_log" 2>/dev/null || true

if [ -n "$pass_id" ] && [ "$pass_id" != "null" ]; then
  pass_log="/tmp/hil-pass-$pass_id.log"
  gh run view "$pass_id" --log > "$pass_log" 2>/dev/null || true
fi

echo "# HIL Regression Report"
echo
echo "## Failed Run"
printf '%s\n' "$run_json" | jq -r '"- Run: \(.databaseId)\n- URL: \(.url)\n- Branch: \(.headBranch)\n- SHA: \(.headSha)\n- Conclusion: \(.conclusion)\n- Title: \(.displayTitle)\n- Created: \(.createdAt)"'
echo
echo "## Last Passing Run On Same Branch"
if [ -n "$pass_id" ] && [ "$pass_id" != "null" ]; then
  echo "- Run: $pass_id"
else
  echo "- Not found in recent $WORKFLOW history for branch $branch"
fi
echo
echo "## Diff Context"
if [ -n "$pass_id" ] && [ "$pass_id" != "null" ]; then
  pass_sha="$(gh run view "$pass_id" --json headSha --jq '.headSha')"
  echo "- Passing SHA: $pass_sha"
  echo "- Failing SHA: $sha"
  echo
  echo '```text'
  git -C "$ROOT" log --oneline "$pass_sha..$sha" 2>/dev/null || true
  git -C "$ROOT" diff --stat "$pass_sha..$sha" 2>/dev/null || true
  echo '```'
else
  echo "- No passing run available for diff context."
fi
echo
echo "## Failure Signals"
echo
echo '```text'
if [ -s "$fail_log" ]; then
  grep -Ei 'fail|failed|panic|timeout|error|probe|uart|flash|disconnect|no test result|boot' "$fail_log" | tail -80 || true
else
  echo "No log output captured."
fi
echo '```'
echo
echo "## Initial Classification"
if [ -s "$fail_log" ] && grep -Eiq 'USB|disconnect|probe.*(failed|error)|No such device|timeout.*probe' "$fail_log"; then
  echo "**RIG** - log contains probe/USB/device failure signals."
elif [ -s "$fail_log" ] && grep -Eiq 'panic|FAILED|assert|No test result|timeout' "$fail_log"; then
  echo "**CODE or TEST** - log contains test failure, panic, or timeout signals. Compare diff above."
else
  echo "**UNKNOWN** - inspect full logs:"
  echo "- Failed log: $fail_log"
  [ -n "$pass_log" ] && echo "- Passing log: $pass_log"
fi
