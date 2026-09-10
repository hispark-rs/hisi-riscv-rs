# NET0 Handshake Admission And Preserved Session Rejection

Status: bounded queue-4 repair; NET0 remains active, NET1 remains queued.

Backend `0f7486431335e94e4ec8bf6292f3e9a60d6e8a16` separates temporary
protocol cleanup from terminal shutdown. Handshake TX can resume only after
native disconnect/user cleanup completed, zero pending ownership and no sticky
fault. Terminal RX stop permanently seals it for that boot. This does not reopen
L2 or relax the one-shot/generation guard.

Local validation passed 225 host tests, host/RV32 Clippy, eleven host-TX and ten
user-cleanup Miri tests, final-call checks and resource mutation tests. Disabling
the production handshake reopen in an isolated copy made its regression fail;
the source was restored before building the recorded firmware.

## Fixed Firmware Results

STA SHA-256: `a234a29552117d828e2c2e0209a1d8ab1fcd9c0e1c6bdce82752a39643df2926`.
AP SHA-256: `d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.
Only the STA application was flashed, at 3 MHz with full verify (98.612 seconds).
The AP firmware, public paired configuration and AP-then-STA reset order stayed
unchanged. No private credential file was used.

| Attempt | Requested | Observed | UDP sent/received | Result |
|---|---:|---:|---:|---|
| Preflight | 3 | 3/3 | 30/30 | Pass |
| Same-image matrix, no reflash | 20 | 1/2 | 10/10 before rejection | Stopped on first failure |

The second matrix round completed Wi-Fi connection after two association calls,
but the one-shot L2 guard correctly rejected it as `initial-rejected`. There was
no application UDP session; this is not measured packet loss. Its host-TX
accepted/processed=2, pending/rejected/fault=0 and closed=0, unlike the preceding
[queue-4 rejection](net0-stop-scope-2026-09-10.md). RX stop was never requested.
The captured host delivery calls remained closed to L2.

This sample no longer exhibits the earlier queue-4 rejection. It does not prove
all recovery paths are correct, explain the first association retry, or authorize
removing the initial-session guard. The next functional gate remains a genuine
native producer fence and bounded same-device recovery, not more one-shot
pass-count publication. All successful terminal observations still found MAC
already disabled, so they are not active-DMA-stop evidence.

## Reproducible Evidence

Exact-source [CI](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34423106930)
passed 23/23 jobs, including Linux, macOS and Windows final linking.

The [collector](net0-handshake-admission-2026-09-10/collect-evidence.py) verifies
committed build inputs, original capture hashes and complete marker contracts.
It preserves the failed round instead of replacing it with a later success.
The record includes the independent lock/toolchain, image plan, native call and
physical resource reports. CI download receipts have finite Actions retention;
they are report downloads, not registry-only facade or firmware acceptance.
