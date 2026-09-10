# NET0 Direct RX Ownership And Rejected Queue Mode

Status: selected direct-RX path closed; NET0 remains active, NET1 queued.

Backend `9afac3366e1a75cb7f734266a0d4a0322462716e` makes the new standard-L2
profile reject optional host-queued RX message 595 from boot. The wrapper closes
Rust admission and records a sticky fault, returning native status 103. It does
not read, retain or free the payload: the pinned native producer retains its
existing free-on-103 branch. Other messages, including TID message 597, forward
unchanged. This does not modify the released legacy smoltcp profile.

## Source And Link Gates

The independent source build passed 228 host tests, host/RV32 Clippy and three
Miri tests for the production RX-mode contract. The final ELF checker verifies
six call edges, the native message/free branch and one byte of physical fault
storage; eight instruction/ownership mutations must fail. Existing RX-stop,
host-TX and physical-storage mutation checks also passed.

The first external-consumer negative test found a real link-contract gap:
removing `--wrap=frw_host_post_msg` could garbage-collect the unreferenced wrapper
and still link. The committed fix roots the wrapper from `force_link_contract`.
The repeated negative test now fails on `__real_frw_host_post_msg`, and the
restored offline build passes. A successful link alone was not accepted.

Exact-source [CI](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34424883763)
passed 23/23 jobs. Its Linux/macOS/Windows report downloads are bound by the
[download verifier](net0-direct-rx-2026-09-10/verify-downloads.py).
The receipt records job conclusions, GitHub ZIP digests and all 24 member hashes.
These are finite-retention Actions reports, not a new registry release or
registry-only facade acceptance.

## Silicon Experiment

Normal STA ELF SHA-256:
`9f1e8d4341eb53c321b3793c391a94bb26b77856887a4c77a82e30732b109a1c`.
Fixed AP ELF SHA-256:
`d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.
Only the STA application was flashed. All downloads used 3 MHz and full verify;
the public paired configuration and AP-then-STA reset order stayed unchanged.
No private credential file or persistent runner was used.

| Case | Observed | UDP sent/received | Sticky rejection | Flash time |
|---|---:|---:|---:|---:|
| Normal preflight | 3/3 | 30/30 | 0 | 98.829 s |
| Forced native queue-595 branch | 0/1 normal connections, expected rejection | No application session | 1 | 98.676 s |
| Original ELF restored | 1/1 | 10/10 | 0 | 98.375 s |

The negative ELF changes exactly two bytes at the reviewed producer branch,
from conditional `c.bnez` to an equal-size `c.j` targeting the same existing
queue path. No symbol, section, wrapper or buffer layout changes. Its SHA-256 is
`bc5b1cd5ad6995d34e8d09b3142440eba1e892ffc271adc3ba066779803d55f9`.
The mutation is a test-only artifact, never a release candidate. The native
producer still builds its real message and owns the rejected netbuf.

The normal-success harness retains `pass=false`: association succeeded, but
EAPOL did not reach hostap, connection timed out, and no L2 application session
opened. A subsequent one-byte probe read confirmed the wrapper's sticky fault.
Timeout alone is not treated as proof of rejection. The original ELF was then
fully reflashed, passed connection/UDP/disconnect and read back fault=0.

The [collector](net0-direct-rx-2026-09-10/collect-evidence.py) verifies committed
build inputs, both ELF hashes, the exact two-byte difference, original UART
hashes, complete public marker contracts and fault readbacks. Its negative
tests reject extra ELF changes, missing/zero fault evidence and successful
markers in a purported rejected run.

## Remaining Boundary

This closes the optional message-595 bypass in the pinned direct-call profile.
It does not prove arbitrary indirect-call completeness, absence of native
memory leaks, DMA quiescence, bounded descriptor reinitialization or reconnect
generation isolation. All normal terminal samples again found MAC already
disabled; they do not exercise stopping active DMA. The prior failed long
matrix remains preserved in the [handshake evidence](net0-handshake-admission-2026-09-10.md).
No one-shot guard was removed and no NET1/HTTPS support is claimed.
