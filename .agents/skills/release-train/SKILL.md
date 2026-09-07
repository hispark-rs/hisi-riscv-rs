---
name: release-train
description: Accept an explicitly identified hispark-rs release using exact source CI, downloaded artifacts, registry checksums and immutable evidence. Use for release preparation, rehearsal, observation and final acceptance.
disable-model-invocation: true
---

# Release Train

Follow `docs/src/how-to/11-release.md`. Each crate owns its release; the parent
tag is an ecosystem snapshot. The anchor comes from the current CHANGELOG,
not a version list in this skill. Commit/push children before parent pointers.

## Preflight

1. Check clean tracked source, independent Cargo.lock, exact dependencies and
   the approved version/tag. Never move/recreate a published tag to hide failure.
2. Isolate child package checks from parent patches:
   `uv run --script scripts/cargo-standalone-lock.py <repo> --package`.
   Supply the required chip/profile features. Lock updates are reviewed changes,
   not an automatic release side effect.
3. Push only the explicitly authorized commit/tag. Observe the exact workflow,
   source SHA and required matrix jobs; a successful unrelated CI is insufficient.
4. For parent changes, run Release workflow_dispatch first. This creates a private
   candidate, builds docs, downloads/reverifies bytes, then removes the draft.
   Repeat with failure_stage=before-finalize; it must fail with no public release.
5. For hisi-rf, Publish workflow_dispatch tests the immutable candidate across
   three host OSes and four profiles, then runs cargo publish --dry-run. It does
   not publish a new crate version. Published-consumer checks run on real tags.

## Acceptance

`train.sh` is now a read-only verifier, not an implicit tag pusher. Ambiguous
asset-count arguments are rejected. Specify kind, exact workflow and run ID:

```bash
bash .agents/skills/release-train/train.sh <tag> \
  --kind github --repo hispark-rs/hisi-riscv-rs \
  --workflow release.yml --run-id <id> \
  --asset blinky.elf --asset blinky.img --asset blinky.plan.json \
  --asset release-manifest.json --asset Cargo.lock \
  --asset rust-toolchain.toml --asset SHA256SUMS \
  --firmware-bundle --report /path/to/acceptance.json

bash .agents/skills/release-train/train.sh <tag> \
  --kind crates-io --repo hispark-rs/<repo> \
  --workflow publish.yml --run-id <id> \
  --package <crate>=<exact-version> --report /path/to/acceptance.json
```

Use `--kind both` when both deliveries are required. By default every job must
succeed. Explicit `--required-job <exact-name>` may select mandatory jobs when
a workflow contains intentionally skipped optional lanes; document that set.

GitHub acceptance requires a non-draft release and named assets, downloads every
asset and checks complete SHA256SUMS coverage. Parent acceptance also regenerates
the image/plan from the released ELF using the recorded hisi-fwpkg version.
Registry acceptance checks the exact non-yanked version, downloads the .crate,
validates its registry checksum and clean .cargo_vcs_info source commit.

A missing release, network/auth failure, skipped required job, absent checksum,
wrong source/version, missing artifact or altered byte fails closed. Never infer
"probably crates.io" from missing GitHub assets. Exit 0 means only the explicitly
selected delivery contract passed.

## Evidence Boundaries

Preserve candidate package hashes, full resolved dependency graph/features,
consumer Cargo.lock, rustc identity, final ELF hash, source SHA and run attempt.
Proof inventory is not proof execution. Kani/TLC need complete successful
per-item receipts bound to source/model/config/tool identity and raw logs.
HIL declared hashes are not artifact re-verification, and 20/20 is statistical
evidence for that fixture/environment, not proof of universal RF reliability.
