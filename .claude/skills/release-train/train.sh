#!/usr/bin/env bash
# release-train driver — drive one repo's tag-triggered release to the finish line:
#   1. ensure the v* tag is on the remote (push it if local-only)
#   2. find the workflow run that tag triggered, watch the matrix to completion
#   3. report every leg's conclusion
#   4. verify the published GitHub release assets (or note a crates.io publish)
#
# Run from INSIDE the target repo — gh resolves the repo from its git remote.
#   bash train.sh <tag> [expected_asset_count]
#
# Env overrides:
#   WAIT_RUN   seconds to wait for the run to appear after the tag push (default 90)
#   REPO       owner/name to override gh's auto-detection (default: cwd remote)
#   WORKFLOW   workflow file/name to pin (e.g. release.yml). Default: auto-pick the
#              release/build/publish run on the tag over a CI run (a tag often fires both).
set -uo pipefail

TAG="${1:-}"
EXPECT="${2:-}"
[ -n "$TAG" ] || { echo "usage: train.sh <tag> [expected_asset_count]   (run inside the repo)"; exit 2; }
WAIT_RUN="${WAIT_RUN:-90}"
GHR=(); [ -n "${REPO:-}" ] && GHR=(-R "$REPO")

command -v gh >/dev/null 2>&1 || { echo "FATAL: gh CLI not found"; exit 2; }
git rev-parse --git-dir >/dev/null 2>&1 || { echo "FATAL: not a git repo"; exit 2; }
SLUG="$(gh "${GHR[@]}" repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null)"
[ -n "$SLUG" ] || { echo "FATAL: gh can't resolve the repo (auth? remote?)"; exit 2; }
echo "════════ release-train: $SLUG @ $TAG ════════"

# ── 1. ensure the tag is on the remote ───────────────────────────────────────
if git ls-remote --tags origin "refs/tags/$TAG" 2>/dev/null | grep -q "$TAG"; then
    echo "==> tag $TAG already on origin"
else
    git rev-parse -q --verify "refs/tags/$TAG" >/dev/null \
        || { echo "FATAL: tag $TAG does not exist locally — create it first (git tag $TAG)"; exit 2; }
    echo "==> pushing tag $TAG to origin"
    git push origin "$TAG" || { echo "FATAL: tag push failed"; exit 2; }
fi

# ── 2. find the run the tag triggered (it may take a few s to register) ───────
# A tag often fires BOTH a release workflow and CI. Prefer the release/build/publish
# run over a CI run so the per-leg report shows the host build matrix, not qtest.
# `WORKFLOW=` pins it explicitly.
echo "==> waiting for the workflow run on $TAG (≤${WAIT_RUN}s)"
pick_run() {
    if [ -n "${WORKFLOW:-}" ]; then
        gh "${GHR[@]}" run list --workflow "$WORKFLOW" --branch "$TAG" --limit 1 \
            --json databaseId -q '.[0].databaseId' 2>/dev/null; return
    fi
    # rank candidate runs on the tag: release/publish > build > non-CI > anything
    gh "${GHR[@]}" run list --branch "$TAG" --limit 20 \
        --json databaseId,workflowName 2>/dev/null | jq -r '
        ([.[] | select(.workflowName|test("release|publish|deploy";"i"))] +
         [.[] | select(.workflowName|test("build";"i"))] +
         [.[] | select((.workflowName|test("^ci|continuous";"i"))|not)] +
         .) | .[0].databaseId // empty'
}
RUN_ID=""
for _ in $(seq 1 $((WAIT_RUN / 5))); do
    RUN_ID="$(pick_run)"
    [ -n "$RUN_ID" ] && [ "$RUN_ID" != "null" ] && break
    sleep 5
done
[ -n "$RUN_ID" ] && [ "$RUN_ID" != "null" ] || { echo "FATAL: no run found for $TAG after ${WAIT_RUN}s"; exit 2; }
echo "    run: $(gh "${GHR[@]}" run view "$RUN_ID" --json workflowName,url -q '"[\(.workflowName)] \(.url)"')"

# ── 3. watch to completion, then dump per-leg conclusions ─────────────────────
echo "==> watching run $RUN_ID …"
gh "${GHR[@]}" run watch "$RUN_ID" --interval 15 --exit-status >/dev/null 2>&1
WATCH_RC=$?
CONCL="$(gh "${GHR[@]}" run view "$RUN_ID" --json conclusion -q .conclusion)"
echo "==> run conclusion: ${CONCL:-?}  (watch rc=$WATCH_RC)"
echo "    per-leg:"
gh "${GHR[@]}" run view "$RUN_ID" --json jobs \
    -q '.jobs[] | "      \(.name): \(.conclusion // .status)"'

# ── 4. verify the published release assets ───────────────────────────────────
echo "==> release assets for $TAG:"
if gh "${GHR[@]}" release view "$TAG" >/dev/null 2>&1; then
    mapfile -t ASSETS < <(gh "${GHR[@]}" release view "$TAG" --json assets -q '.assets[].name')
    for a in "${ASSETS[@]}"; do echo "      $a"; done
    N="${#ASSETS[@]}"
    echo "    asset count: $N${EXPECT:+  (expected $EXPECT)}"
    printf '%s\n' "${ASSETS[@]}" | grep -qiE 'SHA256SUMS|\.sha256$' \
        && echo "    ✓ checksums present" || echo "    ⚠ no SHA256SUMS / .sha256 asset"
    if [ -n "$EXPECT" ] && [ "$N" -ne "$EXPECT" ]; then
        echo "    ⚠ asset count $N ≠ expected $EXPECT"; STATUS_ASSET=1
    else STATUS_ASSET=0; fi
else
    echo "      (no GitHub release for $TAG — this repo likely publishes to crates.io"
    echo "       via publish.yml; the run conclusion above is the signal. Verify the"
    echo "       new version on crates.io once its index propagates.)"
    STATUS_ASSET=0
fi

# ── verdict ──────────────────────────────────────────────────────────────────
if [ "$CONCL" = success ] && [ "${STATUS_ASSET:-0}" -eq 0 ]; then
    echo "════════ PASS: $SLUG @ $TAG released ════════"; exit 0
else
    echo "════════ FAIL: $SLUG @ $TAG (conclusion=$CONCL, assets ${STATUS_ASSET:-?}) ════════"; exit 1
fi
