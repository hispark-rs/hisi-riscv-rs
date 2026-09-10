# NET0 Explicit Disconnect Stop Scope

Status: source fix verified; long HIL matrix failed. NET0 remains active.

Backend `9ee47bf2a4e3e36492b3f3d5a1c3a020db281fea` moves terminal RX stop
from shared hostap deauthentication to explicit controller Disconnect completion.
Initial association cleanup, recovery, cancellation and expired operations cannot
start that destructive step. A separate worker turn rechecks native receipts,
host TX and user cleanup before stop; it does not run in the network executor.

The exact-source [CI](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34346954976)
completed 23/23 jobs. Independent local checks passed 221 host tests, three
terminal-operation Miri tests, host/RV32 Clippy, final ELF call checks with 16
mutations and physical resource checks with eight mutations. The
[download receipt](net0-stop-scope-2026-09-10/download-verification.json) binds
three platform ZIP digests and all 21 member digests. These are downloaded
reports, not firmware downloads or a registry-only facade release.

## Fixed Artifact Results

STA ELF SHA-256:
`7bbc3ca17965b745f83cc94698f9025ff72cf0c38d3ad37220e95ec9a4cda2b9`.
AP ELF SHA-256:
`d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.

Only STA app flash changed: 3 MHz, full verify, 97.926 seconds. AP firmware,
public paired configuration and AP-then-STA nRST policy were unchanged.
The [collector and complete results](net0-stop-scope-2026-09-10/summary.json)
bind source/build-input hashes, ELF/image, capture byte lengths/digests and
whitelisted full UART lines. No credential file was used.

| Attempt | Requested | Observed | UDP sent/received | Result |
|---|---:|---:|---:|---|
| Preflight | 3 | 3/3 | 30/30 | Pass |
| Same-image matrix, no reflash | 20 | 5/6 | 50/50 before failure | Stopped on first failure |

The failed sixth round has no authorized L2 session or application UDP:

- RX stop installed=1, requested/entered/returned/post-returned=0, fault=0.
  The shared deauthentication path no longer causes premature destructive stop.
- Host TX accepted/processed/pending=0, rejected=8, closed=1, fault=0.
- Hostap EAPOL notification/read/feed/send counters each reach eight. Two
  association-success responses are recorded, but connection eventually times out.
- User delete/free each completed twice with no captured cleanup error.

This exposes a second scope problem: shared protocol cleanup permanently closes
queue-4 admission, preventing the next handshake's EAPOL submissions. It does
not identify the cause of the first association retry, and does not permit
ignoring it. A subsequent fix must distinguish temporary protocol TX closure
from terminal shutdown, check completed native cleanup/zero pending ownership,
and retain all RX-generation and native-fence restrictions.

All eight successful rounds observed MAC already disabled before terminal stop.
They therefore do not prove active DMA quiescence, safe descriptor reinitialization
or same-device reconnect. The earlier
[failed RX-stop candidates](net0-rx-stop-2026-09-09.md) remain unchanged.

## Reproduction And Boundaries

`net0-stop-scope-2026-09-10/collect-evidence.py` reuses the preceding RX/TX
UART contract and rejects changed captures, mismatched build inputs and partial
live matrices. It can reconstruct this record with explicit raw/build roots;
`--test` runs its inherited receipt negative tests without board access.

CI artifacts have finite Actions retention. No new crate tag was issued for
this diagnostic increment. Native RX/DMAC closure and bounded recovery remain
the next functional gate, not additional one-shot pass-count publication.
