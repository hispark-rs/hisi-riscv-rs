#!/usr/bin/env bash
# Persist the complete generated Pages site to gh-pages for the next build.
set -euo pipefail

SITE="site"
MESSAGE="Update docs site"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --site) SITE="$2"; shift ;;
        --message) MESSAGE="$2"; shift ;;
        *) echo "usage: $0 [--site site] [--message message]" >&2; exit 2 ;;
    esac
    shift
done

if [ ! -d "$SITE" ]; then
    echo "publish-docs-state: site directory does not exist: $SITE" >&2
    exit 1
fi

WORKTREE="${RUNNER_TEMP:-/tmp}/hisi-riscv-rs-gh-pages"
rm -rf "$WORKTREE"

if git ls-remote --exit-code --heads origin gh-pages >/dev/null 2>&1; then
    git fetch origin gh-pages --depth=1
    git worktree add "$WORKTREE" origin/gh-pages
    git -C "$WORKTREE" switch -C gh-pages
else
    git worktree add --detach "$WORKTREE"
    git -C "$WORKTREE" switch --orphan gh-pages
fi

find "$WORKTREE" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +
cp -R "$SITE"/. "$WORKTREE"/

git -C "$WORKTREE" config user.name "github-actions[bot]"
git -C "$WORKTREE" config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git -C "$WORKTREE" add -A
if git -C "$WORKTREE" diff --cached --quiet; then
    echo "publish-docs-state: no changes"
else
    git -C "$WORKTREE" commit -m "$MESSAGE"
    git -C "$WORKTREE" push origin gh-pages
fi
