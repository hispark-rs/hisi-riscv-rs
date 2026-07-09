---
name: release-train
description: Drive one hispark-rs repo's tag-triggered release to the finish line — push the v* tag, watch the multi-platform CI matrix to completion, report every leg, and verify the published GitHub release assets (or note a crates.io publish). Use to cut or babysit a release of the QEMU fork or a library crate; the old custom Rust toolchain tarball release path is legacy.
disable-model-invocation: true
---

Turns the 15-minute release babysit into one command: tag → watch the matrix →
verify assets. Run it **from inside the target repo** (it uses `gh`, which resolves
the repo from the git remote). User-invoked because it pushes tags and triggers CI.
Full repository procedure and ownership rules live in
`docs/src/how-to/11-release.md`.

## Parent Release Train Anchor

For the parent repository `hisi-riscv-rs`, a `v*` tag is an ecosystem release
train snapshot, not a crates.io package release. When the parent tag shares a
version with a main crate, that crate is the release train anchor and the parent
`CHANGELOG.md` must say so explicitly.

Current rule/example:

- `hisi-riscv-rs v0.6.0-alpha.2` anchor: `hisi-riscv-hal 0.6.0-alpha.2`.

Only cut a parent tag when the docs, submodule pointers, template contracts,
toolchain policy, image tooling, and firmware assets should become a versioned
ecosystem snapshot. A crate-only crates.io release does not by itself require a
parent tag.

## Preflight

For every independently published Rust crate in this ecosystem
(`hisi-riscv-hal`, `hisi-riscv-rt`, `ws63-pac`, `bs2x-pac`), `Cargo.lock` is part
of the release input even when the crate is a library. Before tagging:

```bash
python3 /path/to/hisi-riscv-rs/scripts/cargo-standalone-lock.py . --update
python3 /path/to/hisi-riscv-rs/scripts/cargo-standalone-lock.py . --package
git diff --exit-code -- Cargo.lock
```

Why the helper exists: when a crate repo is checked out as a submodule under
`hisi-riscv-rs`, Cargo can discover the parent workspace and its
`[patch.crates-io]` entries. Running Cargo directly from the submodule path can
therefore generate a parent-influenced lockfile instead of the standalone
crates.io lockfile that CI/publish uses. The helper copies the crate to a temp
directory outside the parent workspace, runs Cargo there, and compares or updates
the real `Cargo.lock`.

If the crate lives as a submodule in `hisi-riscv-rs`, commit any `Cargo.lock`
change **inside that submodule** before bumping the parent pointer.

## Usage

```bash
cd <repo> && bash <path>/.agents/skills/release-train/train.sh <tag> [expected_asset_count]
```

Examples (the live repos in this ecosystem):

```bash
# QEMU fork — 4-platform release matrix
cd /root/ws63-qemu        && bash /root/ws63-rs/.agents/skills/release-train/train.sh v0.4.6 7

# A library crate — crates.io publish (no GitHub assets; watches publish.yml)
cd /root/ws63-rs/crates/hisi-riscv-hal && bash /root/ws63-rs/.agents/skills/release-train/train.sh v0.3.1
```

## What it does

1. **Tag** — if `<tag>` is already on `origin`, reuses it; if it exists locally but
   isn't pushed, pushes it; if it doesn't exist at all, **fails** (it won't invent a
   tag — create it yourself with the right HEAD first).
2. **Find the run** — polls `gh run list --branch <tag>` (tag-triggered runs carry the
   tag as `headBranch`) for up to `WAIT_RUN`s until the run registers.
3. **Watch** — `gh run watch --exit-status` streams to completion, then prints each
   matrix leg's conclusion (per-host build legs + the publish/release job).
4. **Verify assets** — if a GitHub release exists for the tag, lists every asset,
   counts them (compares to `expected_asset_count` if given), and checks a
   `SHA256SUMS`/`.sha256` is present. If there's no release, reports that the repo
   publishes to crates.io (the run conclusion is then the signal).

Exit 0 only if the run concluded `success` **and** the asset check passed.

## Expected asset sets (this ecosystem)

| repo | workflow | assets | count |
|------|----------|--------|-------|
| `hisi-riscv-rust-toolchain` | `build.yml` | no release assets; this repo is now official nightly radar | — |
| `hisi-riscv-qemu` | `release.yml` | `hisi-riscv-qemu-<host>.{tar.gz,zip}` × 4 + legacy binary + `SHA256SUMS` + src tarball | 7 |
| `hisi-riscv-hal` / `-rt` / `ws63-pac` / `bs2x-pac` | `publish.yml` | none (crates.io) | — |

Hosts = `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `aarch64-apple-darwin`,
`x86_64-pc-windows-msvc`.

## Env overrides

| var | default | purpose |
|-----|---------|---------|
| `WAIT_RUN` | `90` | seconds to wait for the run to appear after the tag push |
| `REPO` | cwd remote | `owner/name` to override `gh`'s auto-detection |

## Gotchas

- Needs `gh` authenticated with access to the repo.
- Library crate repos intentionally commit `Cargo.lock`; a missing or dirty lockfile
  is a release blocker, not a harmless library-crate convention.
- Library crates (`publish.yml`) produce **no GitHub release** — a `success` conclusion
  means the publish step ran; crates.io index propagation lags, and the crates.io API
  may be unreachable from a sandbox, so confirm the new version separately.
- One run only: if a tag re-triggers multiple workflows, this watches the most recent.
  Re-run after deleting/re-pushing a tag to follow the new run.
- The release job in these workflows gates on `if: !cancelled()` — a *cancelled* run
  won't publish a partial release, but this script will still report it as non-success.
