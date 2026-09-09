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
The range contains both `frw_rom_cb_register` (`0x128d4a`) and
`frw_get_rom_cb` (`0x128d60`). The latter accepts indices through 264 and loads
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

## Host Queue Topology Cross-check

A subsequent static audit distinguishes the queues hidden behind the similar
FRW names. It uses the same oracle above and the normalized target archive
`libwifi_driver_tcm.a` with SHA-256
`d0563f779957eb6df24cb27e102806105b09414991e60efcbe96f8e392a356e0`.
The extracted `frw_thread.c.obj` is
`42d7308f372412c93005fecd7b4abaebecc75f03f398a1775972429e8c736d4e`;
its ELF relocations independently bind `frw_thread_init` to
`frw_netbuf_que_handle` and `frw_task_process`, and `frw_host_post_data` to
`frw_host_post_async`. These are archive facts, not runtime addresses.

| Path in the pinned oracle | Queue/ownership observation | Fence consequence |
|---|---|---|
| `frw_thread_init` (`0x2651a0`) | Initializes two 136-byte thread controls, each with five 24-byte queue descriptors; queue 4 uses `frw_netbuf_que_handle`, while queues 0/1/3 use `frw_msg_que_handle`. Queue 2 is the synchronous response tracking list. | A single queue's empty flag cannot describe all work owned by both threads. |
| `frw_task_process` (`0x264c6e`) | Walks the five descriptors in order and calls their configured handlers. Message handling processes one fetched node; netbuf handling loops until its queue becomes empty. | A synchronous command response is not a marker placed after every queue's prior work. |
| `frw_host_post_msg_sync` / `frw_host_post_sync` | Local synchronous configuration is submitted to thread 0, queue 0. | Its completion does not establish a thread-1 RX message barrier. |
| `frw_host_post_msg` (`0x265124`) | Selects thread 1, queue 1. The throughput-18 RX branch submits message 595 here before user-reference acquisition. | Even completion of user deletion cannot alone account for this pre-reference queue. |
| `frw_host_post_data` (`0x26513a`) | Selects thread 1, queue 4. The public enum defines type 4 as `FRW_NETBUF_W2H_DATA_FRAME`. | This is a host-to-wireless data path, not evidence of an extra always-enabled RX queue. |
| `uapi_lwip_send` (`0x268cb8`) | Throughput flag 16 selects type-4 queued TX; the other branch calls `hmac_bridge_vap_xmit_etc` directly. | The data profile must account for its actual TX mode as well as RX flag 18. |
| `uapi_ioctl_send_eapol` (`0x29a9b8`) | Calls `frw_host_post_data(type=4)` and returns zero without propagating that helper's enqueue/free result. | Disabling ordinary queued TX does not remove the supplicant's queued EAPOL path. |
| `frw_rx_netbuf` (`0x264166`) | Copies the DMAC payload into a host buffer, frees the original DMAC netbuf, then invokes `frw_hmac_rcv_netbuf` (`0x264682`), which dispatches the registered hook. | Freeing the original DMAC buffer does not retire the copied host frame or prove no callback can run later. |

`frw_thread_exit` is also not a reconnect shortcut: it unregisters the receive
hook, disables queues, optionally destroys tasks, and frees queued nodes and
locks. It is a whole-framework teardown with distinct ownership requirements,
not a bounded per-station flush. It was not called on either board.

The next implementation must explicitly cover thread-1 message 595, queued
TX/EAPOL, the direct RX callback, native user/TID teardown and Rust tickets.
Do not infer queue ordering from a function name or turn a momentary empty
snapshot into a permanent producer fence. No product hook, fixed delay, raw
queue mutation or new open transition was added by this topology audit.

## Device Flush And Host Hook Limits

A later read of STA ROM `[0x128a00, 0x128e00)` produced 1,024 bytes with
SHA-256 `26b87a1b312ab2f05a575f101fb6be203465048bc4edef833a81b17df7fba63f`.
The STA was still running the bound-control ELF
`35fbdf1ceed30d1b82c105892ff953dd813e23717885e290516bdfdaaea6df5e`.
There was no reset, flash/NV write or target function invocation. The file is a
local audit input, not a downloaded release artifact or behavioral HIL result.

The existing Orb `dev` command channel did not return even for `uname`; only
the two temporary clients started for this audit were terminated, without
restarting the VM or its service. Rust's bundled LLVM object tools decoded
the standard RV32 instructions instead. Vendor instructions remain marked
unknown; no conclusions about those instructions are inferred from LLVM.

The standard instructions and the pinned ROM symbol table establish that
`frw_dmac_event_vap_flush_event` (`0x128b34`) calls `frw_flush_msg_que`
(`0x128a96`) with queue indices 0 and 1. The message-only wrapper at `0x128b48`
also visits those two queues. `frw_event_flush_callback` (`0x128b1e`) dispatches
optional ROM callback 227; its existence is not evidence that the callback is
installed or drains DMA. This still does not cover the separate host-thread
message-595 queue or TX/EAPOL queue described above. The SDK's `frw_flush_msg`
has a six-byte ABI (VAP flag, drop flag, VAP id, padding, message id), not an
unqualified all-producer barrier.

The same SDK's `frw_netbuf_hook_register` oracle (`0x2642c6`) rejects null
callbacks and types >= 5, and refuses to replace an occupied hook. The public
enum assigns those five types to device-to-host management/data/FTM and
wireless-to-host management/data. Thus a proposed queue-4 sentinel cannot
claim an undeclared type or overwrite an existing hook without a separate,
audited interception/lifetime contract. The current implementation does
neither. A future fence must establish actual producer closure and checked
FIFO progress, not rely on an invalid sentinel, queue-empty snapshot, fixed
delay or this static disassembly alone.

In a separate read of the same fixed STA, ELF symbol `g_thruput_type` resolves
to `0x00a32ed8`. All 23 flag bytes were zero (SHA-256
`015275e61fa0d0751c1d9f45541c7804c895404455470710ade3786f282f2da0`).
The SDK enum names index 16 `THRUPUT_RESUME_FRW_TX_DATA` and index 18
`THRUPUT_RESUME_FRW_RX_DATA`; both optional ordinary-data queue paths were
disabled at this observation. This narrows the current fixture, not every
profile: the final ELF still contains `hmac_set_thruput_test`, so the snapshot
does not establish an immutable mode or justify ignoring future mode changes.
Queued EAPOL is independent of index 16 and remains in the fence scope.

## Enqueue And VAP-stop Return-value Limits

Further inspection of the same pinned SDK disassembly confirms that the
ambiguous enqueue return is inside `frw_host_post_data` itself, not only its
EAPOL caller. At `0x26516c` the limit branch frees the netbuf and reaches the
zero return at `0x265186`. The normal branch calls `frw_host_post_async` at
`0x265194`; a nonzero result frees the netbuf at `0x26519a`, then also reaches
that zero return. A queue-4 barrier must therefore observe its own correlated
execution or a checked lower-level enqueue receipt. A zero from this wrapper
cannot prove admission, progress or drainage. No sentinel was submitted on the
basis of this observation.

The alternative `wal_stop_vap` / `wal_deinit_wlan_vap` route is also not an
already-verified replacement. `wal_stop_vap` (`0x2998f8`) calls `wal_down_vap`;
the down handler iterates user deletion and calls `hmac_vap_clear_tx_queue`
(`0x268358`), which sends device configuration 232. Its device handler
`dmac_vap_clear_tx_queue` (`0x2a03da`) walks and frees selected VAP TX lists;
this does not by itself cover the host queue-4 frames waiting to reach those
lists. `wal_deinit_wlan_vap` (`0x299a72`) issues configuration 322, then clears
the netdev's VAP pointer at `0x299acc` even after a nonzero result. Its no-VAP
branch returns zero. Consequently a null pointer or a successful outer return
is not a checked destroy receipt, and a failed destroy cannot be silently
followed by recreation.

For any future shim, use the explicit SDK declarations plus the actual
archive ABI, not the historical convenience declarations in `port_frw.h`:
that file's one-pointer `frw_send_msg_to_device` / `frw_rx_netbuf` declarations
do not match the four-argument / two-argument definitions inspected here.
The existing Cargo path does not acquire new FFI imports from that header in
this change. These observations add constraints to the pending native fence;
they are not a new runtime implementation, producer-drain proof or HIL result.

## Earliest Host-delivery Hook Snapshot

Three further read-only STA ROM ranges, still on the same bound-control ELF,
identify the host-delivery call boundary without installing a hook:

| Range | SHA-256 |
|---|---|
| `[0x128540, 0x128620)` | `432d50209b953e42b4d4f6eb5661a84c57782cb4d251755c4d7e84c80e96fd24` |
| `[0x128620, 0x1288a0)` | `69008d5d19978204c1894cd1ae7c411100cd136dd1b1a05ff8ef2c604e560cc2` |
| `[0x128cb0, 0x128d60)` | `1fe52a0df4819124524e38944bb39e58a9583740b0c1a518290cb3f351e37d68` |

The standard instructions show `frw_send_data_to_host` (`0x12877e`) selecting
type 3 and jumping to `dmac_frw_send_data` (`0x1285bc`). The latter calls
`hcc_slave_tx` (`0x128cea`), which looks up ROM callback **261**, passes the
netbuf and length registers to it, and returns its status. The bounded read at
`0x18141c` returned `0x0029792e`, which the fixed STA ELF resolves to
`frw_rx_netbuf`. The SDK oracle independently registers that function at slot
261 in `dmac_main_rom_cb_base_init_before_frw_init` (`0x2a4704`–`0x2a470e`).

This is a candidate earlier observation boundary than `driverif_input`: it is
before the host-buffer copy and the optional message-595 queue. It is not yet
an interception contract. An implementation must retain the correct callback
ABI/ownership, count callbacks already in progress, account for delayed work
before this boundary and message 595 after it, and reject conflicting hook
ownership. Unknown vendor instructions were not assigned semantics by LLVM.
No callback replacement, target function call, queue flush or radio reset was
performed; these local read artifacts do not close native quiescence or HIL.
