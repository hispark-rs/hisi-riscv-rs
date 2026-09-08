# Release Trust Reacceptance Receipts

These machine-readable receipts were produced by revalidating downloaded bytes,
not by counting green workflow badges. The decision and limits live in the
[acceptance report](../../release-evidence-acceptance-2026-09-07.md).

| Receipt | Source of downloaded bytes |
| --- | --- |
| `parent-final-reverified.json` | [Parent rehearsal 34110566113](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/34110566113), `parent-release-candidate` |
| `rf-final-reverified.json` | [RF dry-run 34109641559](https://github.com/hispark-rs/hisi-rf/actions/runs/34109641559), candidate crate and twelve `candidate-evidence-*` artifacts |
| `kani-final-reverified.json`, `tla-final-reverified.json` | [RTOS CI 34107500673](https://github.com/hispark-rs/hisi-rtos/actions/runs/34107500673), proof contract, per-item receipts and raw logs |
| `fwpkg-033-release.json` | [fwpkg Publish 34108710617](https://github.com/hispark-rs/hisi-fwpkg/actions/runs/34108710617), actual registry library/CLI packages |
| `template-final-reverified.json` | [Template CI 34174872035](https://github.com/hispark-rs/hisi-rs-template/actions/runs/34174872035), seven consumer reports and three profiles on each of three host OSes |

Parent revalidation uses `scripts/release-bundle.py verify`: all seven assets,
SHA256SUMS coverage, exact source/version, and ELF-to-image/FlashPlan equivalence
using the recorded public `hisi-fwpkg 0.3.3`. The JSON lists the six hashed payloads;
the seventh asset is SHA256SUMS itself, which is checked against that exact set.

RF revalidation recomputed each downloaded `.crate`, consumer `.Cargo.lock`,
ELF and report digest. It required all three host OSes and four profiles exactly
once, the same source/run/candidate identity, complete dependency-graph edges,
and one resolved facade/core/backend/sys/blob package each. The twelve ELF hashes
are per-host build identities, not a claim of bit-identical cross-host ELF files.

Template revalidation required all sixteen resource reports exactly once, ran the
versioned resource-accounting checker from source `a0a3519`, and recomputed each
report/consumer lock digest. Schema/accounting validity is not HIL calibration;
final ELF builds and selected image recipes are separately evidenced by CI jobs.

RTOS receipts were regenerated with `scripts/proof-evidence.py record-kani` and
`record-tla` at the recorded child commit, using the downloaded proof contract,
individual receipts and logs. The validators check the expected harness/model
set, source/config digests, commands, exit codes, required invariants and actual
tool output. A legacy counterexample succeeding means its expected violation was
observed; it is not counted as a proof of the old design.

The negative parent rehearsal is
[34110573399](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/34110573399):
the only failed step was the deliberate pre-publication injection. Promotion was
skipped and cleanup succeeded. A paginated release API check on 2026-09-08 found
neither rehearsal tag present. Neither workflow_dispatch publishes a product.

These receipts do not replace the raw artifacts or prove an experiment's physical
authenticity. GitHub Actions retention is finite. Future revalidation must obtain
the actual artifacts; if unavailable, report that limit instead of reconstructing
a pass from the receipts. No new HIL, RF product tag, parent product release or
self-hosted runner is part of this acceptance.
