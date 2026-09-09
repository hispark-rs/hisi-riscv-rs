# NET0 Native Fence Audit (2026-09-09)

Status: read-only source/disassembly and two-board ROM-read evidence. **No native-quiescence or HIL
gate is closed by this audit.** The active implementation contract remains in
[the connectivity plan](../hisi-connectivity-stack.md#net0-net5-https).

## Input Identity

- Rust backend: `hisi-rf-ws63` `8f600b3c5ea9fea02fa3e396464c4a638cdce33a`.
- Radio artifact owner: `ws63-radio-sys`
  `06504ccac57cb21c7cfa5442256cf17b178e14e0`.
- Vendor oracle: explicitly supplied `fbb_ws63` SDK output
  `src/output/ws63/acore/ws63-liteos-app/ws63-liteos-app.asm`.
- Oracle SHA-256:
  `5aedd0fb8e916efc27325fc5190822e05a0cf84f0b4d602e12c6fa06b242bf2e`.
- Normalized `libwifi_driver_hmac.a` SHA-256, checked against the artifact
  manifest: `79642b77d90b3a8524b39818210c97bfac9ed6a9c0a2cd2f73a03abdd0de3dc6`.
- Extracted `hmac_user.c.obj` SHA-256:
  `becef517fae3ea4e897123d3f13311ea67171bc6cbf47adf9fba8cf4c978249e`.
  Its standard instructions and relocation targets independently corroborate
  the user-delete control flow below; vendor instructions remain an oracle
  boundary, not an LLVM disassembler support claim.

Addresses below identify this oracle only. They are not hard-coded runtime
addresses or proof that the currently normalized artifacts have identical
layout. Production integration must bind any selected hook/ABI to the actual
archive manifest and final ELF.

## Observations

| Boundary | Observed behavior | What it does not prove |
|---|---|---|
| Rust `queue_deauthentication` / `deauth_worker` | A bounded request is queued and later issued as WAL ioctl 15. The enqueue result precedes the worker's ioctl result. | Enqueue success is not disconnect completion or a native queue fence. |
| `uapi_ioctl_disconnect` (`0x29aec2`) | Resolves the netdev and calls `wal_cfg80211_disconnect`. | Does not itself drain RX/TX. |
| `wal_cfg80211_disconnect` (`0x2965bc`) | Issues HMAC configuration 345 using `wal_sync_post2hmac_no_rsp`; the no-user branch can return zero without issuing that command. | A zero return alone does not distinguish completed cleanup from no user being found. |
| `wal_sync_post2hmac_no_rsp` (`0x2973b2`) | Calls `frw_sync_host_post_msg`, then `frw_host_post_msg_sync` / `frw_host_post_sync.constprop.9` (`0x264cea`), which waits on a completion condition with a timeout. | The name `no_rsp` does not mean fire-and-forget, but completion is not automatically a barrier for every RX/TX queue. |
| `hmac_config_kick_user_etc` (`0x276368`) | Its unicast path reports the disconnect through `hmac_handle_disconnect_rsp_etc` before the later `hmac_user_del_etc` call. | A received disconnect event need not mean user cleanup has finished. |
| `hmac_sta_handle_disassoc_rsp_etc` (`0x281f84`) | Posts notification 604 through `frw_asyn_host_post_msg`. | Notification ordering alone does not establish cross-task drainage. |
| `hmac_user_del_etc` (`0x289ec0`) | Sends device message 93 synchronously, then clears admission and waits for the user reference count to reach zero, sleeping 1 ms between reads. Only afterward does it delete user/TID state. | This is a synchronous wait with no local deadline, not a deferred cleanup after return. Its return code alone does not certify final user free success. |
| `WLAN_UTIL_NOTIFIER_EVENT_DEL_USER_COMPLETE` (event 10) | The public notifier header names event 10; `hmac_user_del_etc` emits it at `0x28a080` with the VAP pointer after user/TID unlinking. The later `hmac_user_free_etc` call is at `0x28a096`. | The notification is before final resource release, carries no final-free status, and is not a completed native-producer fence. |
| `hmac_user_free_etc` (`0x2893de`) | Calls feature deletion, `hmac_res_free_mac_user_etc`, then `_mac_res_free_hmac_user`; returns the resource-free status to its caller. | Observing entry or the preceding notification cannot substitute for checking this return and the enclosing teardown identity. |
| `frw_stop_hcc_service` (`0x264160`) | The inspected implementation immediately returns zero. | It does not stop or drain RX producers. |
| `frw_event_flush_event_queue` (`0x264550`) | Sends device message 172 with a caller-selected message filter and logs a nonzero result. | This selective device-event operation is not a barrier for every host RX/TX queue; the wrapper does not return a checked drain receipt. |
| `hmac_rx_data_event_adapt` (`0x266586`) | Normally calls `hmac_rx_process_data_msg` directly. When throughput feature 18 is enabled it instead posts host message 595 through `frw_host_post_msg` at `0x2665e8`. | An assumed always-synchronous RX call graph is insufficient. The selected artifact/profile must establish the feature state and ownership of queued pre-delete frames. |
| `uapi_wifi_sta_stop` (`0x2bf5c8`) | Uses vendor WPA interface/stop management (`wifi_remove_iface` or `wifi_wpa_stop`). | It is not a drop-in fence for the upstream-native supplicant lane and must not reintroduce vendor WPA as a product dependency. |

## Integration Consequences

Correction to the initial audit: the remaining-reference branch does not
schedule cleanup in another task and return. In the normalized object's
`.text.hmac_user_del_etc`, the atomic-read call at `+0xce` branches through
`osal_msleep(1)` at `+0x250` and back to the read. The synchronous device-message
result is saved before that loop. The final `hmac_user_free_etc` call at `+0x230`
can fail and log without replacing that saved result; a preceding successful
device message can therefore still produce an outer zero status. These offsets
are object-section offsets, not runtime addresses.

Close Rust admission and invalidate the current L2 epoch before native teardown.
Keep separate observations for deauthentication queued, ioctl completed,
disconnect event received, native producers drained, and Rust callbacks drained.
Neither a fixed delay nor `in_flight == 0` substitutes for native progress.

Before enabling reconnect on the new route, identify and verify a bounded
completion point that covers user-reference drainage, TID cleanup, outstanding native TX,
and RX already queued before the disconnect. Failure/timeout must keep the
route closed. The actual archive/ELF call graph and injected delayed-frame HIL
must corroborate that point; this oracle does not authorize a guessed hook.
The native wait's lack of a local deadline and the lost final-free error must
both be accounted for; waiting forever or trusting WAL zero is not a bounded
Rust lifecycle contract.

The notifier ABI comes from the same explicit SDK's
`src/protocol/wifi/source/host/frw/frw_util_notifier.h`: `DEL_USER` is event 8,
`DEL_USER_FEATURE` is 9, and `DEL_USER_COMPLETE` is 10. The event-8 callback at
`0x289f7a` receives the user pointer after reference drainage; event 10 receives
the VAP pointer before the final free. These are different observations, not an
interchangeable completion token. No notifier or hard-coded runtime address was
added to the product on the strength of the name alone.

## Read-only DMAC ROM Cross-check

On 2026-09-09, `probe-rs 0.31.0` (`ed6ac741`) read `[0x126700, 0x127700)`
from each connected WS63 at 3 MHz. The two 4,096-byte files compare equal:
SHA-256 `5349642db14516dff00e79ea9b7d2de98e55740810684651b6d6df8750ec7de7`.
The first board also supplied `[0x126000, 0x126800)`, SHA-256
`94a4d374305028f4b91c098d9b9baaf928d2484fe56be6b7aed7bcf87ab796f1`.
These were read-only debug operations, without `--connect-under-reset`, power
cycling, flash/NV writes, or a firmware change. They are not reset/traffic HIL.

The explicit vendor oracle objdump was run through the existing Orb `dev` VM,
using binary/RV32 mode and the read range's base address. Tool SHA-256:
`6a735ac656dc43b7a12588972241afd28be8ed668aab833fe6f40aeac76b1bc9`.
This maintainer-only disassembly does not add a vendor-tool dependency to Cargo.
Symbols are resolved from the pinned artifact owner's `rom/ws63_acore_rom.lds`;
no runtime function pointer was guessed from an unverified name.

| ROM observation | Evidence boundary |
|---|---|
| `dmac_user_del` at `0x1267ba` looks up the user, checks ROM callback 211, then calls `dmac_tid_clear`, STA PM/TWT cleanup, algorithm notification and `dmac_user_del_reset_settings`. | The callback can replace the normal flow; the actual firmware's callback selection must be checked separately. |
| `dmac_tid_clear` at `0x126214` walks eight 40-byte TID records and conditionally invokes `dmac_ba_reset_rx_handle` / `dmac_ba_reset_tx_handle`. | This confirms BA/TID cleanup calls, not full host RX-queue or DMA drainage. |
| `dmac_user_del_offload` at `0x126762` unlinks the VAP user and calls `dmac_user_free` at `0x126370`. Free failure is logged and converted to zero in this path. The enclosing reset-settings routine logs, and `dmac_user_del` ultimately returns zero. | A successful device-message reply cannot alone prove successful resource release. The same limitation exists independently of the HMAC final-free error described above. |

The next native-fence experiment must account for both the user-delete return
and the RX producer/queue boundary. In particular, test delayed message 595 and
late netbuf delivery across deletion/reassociation, check final-free errors,
and retain a closed route on timeout. A Rust callback counter, a successful WAL
return, event 10, and a queue-empty snapshot each cover only part of that chain.

## Installed Callback Snapshot

A subsequent read-only check used the same two boards, still running the
previous bootstrap artifact (ELF SHA-256
`ed798e7ee4c45ef898467de26b7f81d537fb3cc26b375da78aa239a8baca2a63`).
It does not exercise the newer teardown or close-revision commits. No flashing,
reset, function invocation, or NV write was performed.

The 64-byte ROM range `[0x128d4a, 0x128d8a)` has SHA-256
`b80a5ca4b7cb2879fda9f16b5af605e8d46c90ffbfeaf2f4fc93bd8993be05e6`.
Disassembly confirms `frw_get_rom_cb` accepts indices through 264 and loads
`g_frw_rom_cb[index]` from `0x181008 + 4 * index`. Accordingly, the bounded
four-byte read of callback 211 at `0x181354` returned `0x002810ae` on both
boards (each file SHA-256
`8bcac958a3d10c50c9a94ab679b870db07f0846ee72b2d13707dfb6a7d0ef5b2`).

The known ELF identifies that address as the 24-byte
`dmac_common_hook_del_user`. A read of those 24 flash bytes on the first board
matches the ELF disassembly, SHA-256
`5f1a87ec164f6bf41604037320a9a9fe07ba2ef64190f206daec5136f96b999e`.
The hook logs null arguments and otherwise returns 2; the enclosing ROM path
therefore continues its default deletion sequence. For this sampled artifact
the hook is not an extra queue-drain implementation. This is a configuration
snapshot, not evidence that a disconnect exercised the hook or that later
firmware cannot install a different callback.

The SDK RX call graph also sharpens the outstanding host-queue boundary:
`hmac_rx_process_data_msg` acquires the user reference at `0x2662a2` and
releases it through `hmac_user_use_cnt_dec` at `0x266564`, after its processing
path. Throughput-mode message 595 is queued by `hmac_rx_data_event_adapt`
**before** this processing/reference-acquisition boundary. Thus user-reference
zero alone cannot certify that no older unprocessed message 595 exists. The
native fence must either cover that queue or establish and enforce the actual
profile's synchronous RX mode; neither condition has been closed by this audit.

The existing released smoltcp path is unchanged. No board was flashed for this
audit, and no Embassy Net support claim follows from it. The ROM reads identify
code bytes and control flow only; they do not prove those branches were exercised
by a disconnect in the current Rust firmware.
