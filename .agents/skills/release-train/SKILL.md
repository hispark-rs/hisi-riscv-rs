---
name: release-train
description: Drive one hispark-rs repo's tag-triggered release to the finish line — push the v* tag, watch the multi-platform CI matrix to completion, report every leg, and verify the published GitHub release assets (or note a crates.io publish). Use to cut or babysit a release of the toolchain, QEMU fork, or a library crate.
disable-model-invocation: true
---

Turns the 15-minute release babysit into one command: tag → watch the matrix →
verify assets. Run it **from inside the target repo** (it uses `gh`, which resolves
the repo from the git remote). User-invoked because it pushes tags and triggers CI.

## Usage

```bash
cd <repo> && bash <path>/.agents/skills/release-train/train.sh <tag> [expected_asset_count]
```

Examples (the live repos in this ecosystem):

```bash
# QEMU fork — 4-platform release matrix
cd /root/ws63-qemu        && bash /root/ws63-rs/.agents/skills/release-train/train.sh v0.4.6 7

# Custom toolchain — 4 host tarballs + 4 .sha256
cd /root/ws63-rust-toolchain && bash /root/ws63-rs/.agents/skills/release-train/train.sh v1.96.0-2 8

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
| `hisi-riscv-rust-toolchain` | `build.yml` | `hisi-riscv-rust-<ver>-<host>.tar.gz` + `.sha256` × 4 hosts | 8 |
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
- Library crates (`publish.yml`) produce **no GitHub release** — a `success` conclusion
  means the publish step ran; crates.io index propagation lags, and the crates.io API
  may be unreachable from a sandbox, so confirm the new version separately.
- One run only: if a tag re-triggers multiple workflows, this watches the most recent.
  Re-run after deleting/re-pushing a tag to follow the new run.
- The release job in these workflows gates on `if: !cancelled()` — a *cancelled* run
  won't publish a partial release, but this script will still report it as non-success.
