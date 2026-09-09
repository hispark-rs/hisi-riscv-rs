# NET0 Native Fence Audit (2026-09-09)

Status: read-only source/disassembly evidence. **No native-quiescence or HIL
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

The existing released smoltcp path is unchanged. No board was flashed for this
audit, and no Embassy Net support claim follows from it.
