#!/usr/bin/env bash
# Seed a Pages build directory from the gh-pages branch, if it exists.
set -euo pipefail

OUT="site"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --out) OUT="$2"; shift ;;
        *) echo "usage: $0 [--out site]" >&2; exit 2 ;;
    esac
    shift
done

mkdir -p "$OUT"
if git ls-remote --exit-code --heads origin gh-pages >/dev/null 2>&1; then
    git fetch origin gh-pages --depth=1
    git archive --format=tar origin/gh-pages | tar -x -C "$OUT"
else
    echo "seed-docs-site: no gh-pages branch yet; starting a fresh site"
fi
