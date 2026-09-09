# NET0 Transitive Native Link Contract (2026-09-09)

Status: **implemented; three-host CI and downloaded reports accepted**.
This is a packaging/link gate, not native queue
drainage, reconnection, Embassy Net or HTTPS acceptance.

## Defect And Fix

The checked HMAC cleanup fix used package-local `cargo:rustc-link-arg` output.
An independent Cargo binary could compile the backend but failed its final
link with unresolved `__real_hmac_user_del_etc` and
`__real_hmac_res_free_mac_user_etc`. The backend's own examples did not expose
this gap because their link received the package-local arguments.

Backend `e680319cd1683babd55007bc0f31f9562709928b` moves only these two
NET0 wrapper arguments into Rust native-link metadata. The pinned official
nightly enables `link_arg_attribute` only for RV32 Wi-Fi with `standard-l2`.
Each argument has a separate extern declaration to satisfy Clippy. Consumers
do not repeat the wrapper flags or add an application build script. Other
diagnostic/profile linker arguments are unchanged.

## External Consumer Gate

The repository-owned `check-net0-consumer.py` harness packages an isolated
source copy, extracts the Cargo package, and materializes the existing
incremental firmware fixture as a separate dependency consumer. Its temporary
path contains spaces and non-ASCII characters; no parent workspace patch or
profile participates. Its ordinary Cargo config supplies only the runtime
link script, no-relax policy, target and build-std settings.

The initial offline build adds the new root package to the copied lock.
Every dependency identity/checksum must remain in the packaged lock. Subsequent
builds are locked and offline. The gate checks the six actual cleanup call
edges, six missing-call mutations and the physical storage descriptor. It
then removes both native-link attributes from the extracted copy: final link
must fail at both native aliases. Restoring the exact source bytes must restore
the resolved graph. An unchanged incremental build must retain its ELF hash.

The first CI attempt, [34334846831](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34334846831),
is preserved as failed. The harness originally requested metadata for every
platform, causing offline resolution to request uncached, unused packages
(including `aho-corasick 1.1.5`). Commit
`1830bc9ff87654b0870e39e288958605fcbe435c` resolves the actual target build
instead, while checking unchanged dependency identities. No network access
was enabled inside the consumer build to hide the failure.

## Evidence Boundary

Local checks passed: 203 integration tests, RV32 Clippy with warnings denied,
format, packaged offline consumer, resolved-call mutations and target storage.
The CI harness is a maintainer tool using uv; the generated Cargo build has no
Python, shell, GCC or vendor-SDK build step.

Exact-source [CI 34335647477](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34335647477)
passed all 23 jobs at `1830bc9`, including the external-consumer gate on Ubuntu,
macOS ARM64 and native Windows. The
[download receipt](net0-transitive-link-2026-09-09/download-verification.json)
binds three downloaded ZIP digests to GitHub's artifact metadata and retains
15 member-byte digests. It independently checks locked dependency identity,
missing-metadata rejection, restored build, each final call graph, matching
ELF identity between call/storage reports and cross-host resource-size parity.
The ZIPs contain reports and consumer manifests/locks, not firmware binaries;
Actions retention remains finite. The verifier and bundle checksums are retained.

This is a **packaged path dependency** test. It is not the later crates.io-only
public facade/template gate, nor proof of read-only registry sources, all path
lengths, firmware-byte reproducibility or new HIL. Per-host ELF hashes and ROM
trampoline addresses can differ while the resolved graph and resource sizes
agree. No new crate version or support claim is published here. Prior HIL
remains tied to the exact images in the
[cleanup evidence](net0-user-cleanup-2026-09-09.md).
