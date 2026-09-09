# NET0 Control-Plane HIL (2026-09-09)

Status: **3/3 preflight and 20/20 nRST control-plane passes** on one unchanged
STA ELF. This is not NET0 data-plane, native producer-drain, reconnect, Embassy
Net, HTTPS, secure-boot, or release acceptance. The only active milestone remains
[NET0](../hisi-connectivity-stack.md#net0-net5-https).

## Exact Inputs

- Backend: `3914ff5424ee872bf1e4a5adfebc5c2070ba50b2`, independent Cargo.lock and
  public registry dependencies, built outside the parent workspace with
  `--locked --offline`. This source includes the native disconnect receipt and
  close-revision fixes, but not the later TX-admission commit `af81bf5`.
- Parent/AP composition: `beda64110d66397d14f4a6128a72c5662e0bf98d`; examples
  `7fe7bfd38df4e654fc12d186acca32e123958607`. AP uses the parent's explicit
  development patches; it is not a registry-only consumer acceptance result.
- Backend [CI 34305967044](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34305967044)
  passed 23/23; exact-parent [CI](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/34306627005)
  and [Documentation](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/34306627017)
  also passed. These are source checks, not downloaded HIL-binary acceptance.
- STA ELF: `85900df215755b764785d625c7bebe11a8a24c3b2a28cb1e46fa729343d93476`.
- AP ELF: `d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.

The [machine-readable record](net0-control-2026-09-09/summary.json) binds both
locks, tool identities, config hash, flash plans/images, raw UART hashes and
each published marker excerpt. The ELF/image files and original UART captures
remain in the local evidence directory, `/private/tmp/net0-control-hil-3914ff5`.
This is not yet the final independently downloadable NET0 evidence bundle.

The AP uses the existing committed non-production paired-board test config;
the STA build receives the same values through a temporary process environment.
No private AP credential file was used or modified. Ambient scan output is not
published: excerpts contain only exact whitelisted completion/heap lines and
are explicitly distinguished from the original captures by separate hashes.

## Build And Hardware Contract

The standalone STA builds `incremental_scan_profile` with `wpa2-personal`,
`smoltcp`, `incremental-backend-experiment`, `incremental-embassy-wait`,
`standard-l2`, `bootstrap-stage-diag`, `firmware-example` and
`incremental-connect-profile`. The AP builds `wifi_softap` with its existing
default WPA2 profile. Both use the pinned official nightly and
`-Zbuild-std=core,alloc`, release optimization and unchanged RF/task arenas.

| Device | Role | nRST samples | Result |
|---|---|---:|---|
| J-Link 23121310 / UART `wchusbserial1130` | Fixed WPA2 AP | 1 | `RFDBG_SOFTAP_READY` |
| J-Link 24060504 / UART `wchusbserial1140` | NET0 control STA, preflight | 3 | 3/3 |
| Same STA and ELF, AP left running | Control matrix | 20 | 20/20 |

Each app-only download used `hisi-fwpkg plan` and probe-rs raw-bin download at
3 MHz with full `--verify`. Flashboot/NV were not changed. The flash wrapper
completed in 89.64 s for AP and 89.46 s for STA. No reflash occurs between the
3-reset and 20-reset samples. The
[capture tool](net0-control-2026-09-09/capture-tool.py) opens both UARTs before
each J-Link nRST, saves each result immediately and stops on the first failure.
No persistent runner/service was installed.

All 23 STA samples contain `RFDBG_A5B_CONNECT_OK`,
`RFDBG_A5B_DISCONNECT_OK` and `RFDBG_A5B_CONNECT_PROFILE_OK`.
Firmware-measured connect time ranges from 1,678 to 13,192 ms; disconnect from
109 to 112 ms. These are separate from host capture duration, whose clock starts
after the reset helper returns. Recorded connected/disconnected heap snapshots
show zero allocator failures. The original pre-application flash-init warning
is not waived by this gate.

## Last-Reset Native Snapshot

After round 20, read-only probe reads at addresses derived from this exact ELF
found deauthentication `queued=1`, `completed=1`, `failed=0`, `dropped=0`.
This corroborates that the final sample executed a native worker call, not only
a hostap no-request transition. It is not a per-round native receipt snapshot.
The 23-byte `g_thruput_type` array was all zero, including index 18
(`THRUPUT_RESUME_FRW_RX_DATA`). The sampled configuration therefore did not select
the optional asynchronous message-595 RX path at that point; this is not a
permanent or enforced profile invariant. Raw bytes/addresses are preserved in
the machine record; they are not hard-coded product addresses.

The NET0 route is unbound/closed in this control-only fixture. No payload was
delivered through the new queues, no new-session fence was opened, and the test
does not prove user-free status, DMA drainage or old-frame rejection after
reassociation. Subsequent source changes require their own acceptance evidence.
