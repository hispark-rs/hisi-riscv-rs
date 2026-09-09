# NET0 Native Host-delivery Observation (2026-09-09)

Status: **3/3 preflight and 20/20 same-image nRST samples passed**. This is
call-lifetime/control evidence while the caller-owned L2 route remains closed,
not native queue drainage, new-path Ethernet traffic, reconnect or NET0 completion.

## Source And Delivery

- Backend `b69727512fb71baac57d66fbb33b06cd4f1bb3c4`; its independent lockfile
  resolves public registry packages without parent patches. Local offline
  validation passed 184 host tests, 35 L2/pbuf Miri tests, Clippy and final RV32
  linking. [Exact-source CI](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34316064614)
  passed all 23 jobs, including three host OSes and final links.
- `ws63-radio-sys`, `ws63-radio-blob` and `hisi-rf-link` `0.1.0-alpha.26` were
  published from `63b3de4e9787e1f24aa693df5c7a3755d16c3098` by
  [Publish](https://github.com/hispark-rs/ws63-radio-sys/actions/runs/34315272264).
  Downloaded registry packages passed checksum, non-yanked version and clean
  source-SHA verification. The target archive bytes are unchanged; this adds
  the SDK-verified unsafe FRW ABI, not a native fence.
- STA ELF SHA-256:
  `c52bcc67ef92aa5f3d2a09de5df1c9a531fbc8ca544c86fcc6aa6cf734d2c4c4`.
- AP ELF SHA-256:
  `d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.
  It remained running and unchanged from the
  [prior control fixture](net0-control-2026-09-09.md). Only the committed public
  paired-board configuration was used; no private credential file was read.

The [machine record](net0-host-delivery-2026-09-09/summary.json) preserves package
acceptance, lock/toolchain, capture classifier and its negative tests, downloaded
CI resource reports, image plan and per-round marker excerpts with hashes.
Original UART hashes are retained, but ambient scan output is not published.
ELF/images and raw logs remain under `/private/tmp/net0-host-delivery-b697275`;
the final downloadable NET0 firmware bundle remains an open gate.

## Observations

| Check | Result | Boundary |
| --- | --- | --- |
| STA app-only download | 3 MHz, full verify, wrapper 97.09 s | No flashboot/NV rewrite, AP reflash or reflash between resets. |
| Hook registration | Expected native owner checked, observer installed/read back | Slot 261 ABI is bound to the matching SDK and linked symbol, not a guessed image address. |
| Native calls per round | 124-182 entered and returned at the final sample | Includes management traffic; not an Ethernet packet count. |
| Every phase sample | `entered = returned + abandoned + in_flight`; no exhaustion, abandoned/nonzero return or residual in-flight call | No claim about work before entry or queued after return. |
| Closed-route forwarding | All observed calls forwarded while L2 stayed closed | Management/EAPOL needed for connection was not blocked. |
| Control operations | Connect 1,766-3,118 ms; disconnect 109-111 ms | Ioctl/control completion is not a native producer fence. |
| Three-OS linked-storage fixtures | Control 21,440 B, L2 12,560 B, payload 12,112 B, metadata 448 B | These are CI bootstrap ELFs, not the STA HIL ELF or peak RAM measurements. |
| Existing boot warning | `Flash Init Fail! ret = 0x80001341` in all 23 rounds | Preserved; not claimed fixed or silently waived. |

Host tests additionally reject open with an active native call, a call that
enters and returns between prepare/commit, and exhausted counters. They exercise
reentrant forwarding and unwind retirement without holding a critical section
across the native call. The normal HIL workload did not exercise a cross-close
call or prove those injected cases on silicon.

## Remaining Native Fence

The observer forwards exactly once and counts only the dynamic extent of
`frw_rx_netbuf`. The native function can return zero after allocation/copy
failure, and its optional MSG595 host copy can outlive that return. Earlier
device RX and queued TX/EAPOL are not covered. Therefore neither these 23 passes
nor a zero in-flight snapshot permits reopening a new epoch. Continue the
[native-fence audit](net0-native-fence-audit-2026-09-09.md) before new-queue
payload/reconnect HIL; NET1 remains queued.
