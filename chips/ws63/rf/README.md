# ws63-rf-rs

Rust **porting layer + FFI bindings** for the closed-source WS63 Wi-Fi/BLE radio
blobs delivered in [`ws63-RF`](ws63-RF) (a submodule nested under this crate). It is the WS63 analogue of esp-hal's
`esp-radio` OS-adapter: it implements the **runtime-agnostic porting contract**
(`ws63-rf-rs/ws63-RF/include/port/*.h`) in Rust as `#[unsafe(no_mangle)] extern "C"`
symbols, so when a firmware links a vendor blob the linker resolves the blob's
`osal_* / oal_* / log_* / uapi_* / frw_* / hcc_* / wlan_*` references to these
implementations.

> **Design rule.** No Rust goes into `ws63-RF` — that delivery stays
> language-neutral so the blobs can be ported to *any* runtime. This crate is
> the ws63-rs runtime's implementation of `ws63-RF`'s C contract.

## ⚠️ Status: symbol closure for Wi-Fi init ACHIEVED; runtime + data path implemented; runnable image is HIL

**Project context:** ws63-rs is in **phase 7 (HAL polishing & release)** with **phase 1 preparation** (real board bring-up framework) underway (see [ROADMAP.md](../../../ROADMAP.md) §Current focus). This crate is **phase 4** (porting layer + HCC IPC); phases 4–5 (real Wi-Fi link) await hardware-in-the-loop validation.

This crate makes the porting contract **compile, link, and actually run** — the
runtime and data-path plumbing (scheduler, OSAL, FRW worker + HCC, software
timers, netif→smoltcp) are implemented and self-tested standalone on `ws63-qemu`
(`rf_port_demo`, plus the crate's `sched_selftest` / `frw_hcc_selftest` /
`netif_smoltcp_selftest`). It is **not yet a working Wi-Fi stack**: a real link
is hardware-in-the-loop (the ROM symbols are real-silicon addresses and the
HiSilicon blobs carry custom relocations stock `lld` cannot resolve — see
below). The honest picture:

### Implemented for real (usable today)

| Area | Symbols | Notes |
|------|---------|-------|
| Memory | `osal_kmalloc`/`osal_kfree`, `malloc`/`free`/`memalign`, `oal_mem_*` | real first-fit heap over a static pool; zero-initialised, 8-aligned |
| Scheduler | `osal_kthread_*`, `osal_sem_*`, `osal_mutex_*`, `osal_wait_*`, queues + event groups | real cooperative scheduler with **timed** blocking (`*_timeout` deadlines); validated by `sched_selftest` |
| Sync / IRQ | `osal_irq_lock`/`restore`, spinlocks, atomics, `ArchIntLock`/`Restore` | real, via `mstatus.MIE` |
| Timers | `osal_adapt_timer_*`, `frw_dmac_timer_*` | real ms software-timer service, fired from the FRW worker loop |
| FRW / HCC data path | `frw_*`, `hcc_*` | real message-node pool + WiFi worker thread (on the scheduler) + host↔device FIFO; validated by `frw_hcc_selftest` |
| netif → smoltcp | `netif` / `netif_smoltcp` (feature `net`) | real `smoltcp::phy::Device` behind the netif seam; `driverif_input` feeds RX, `TxToken` calls the TX sink; validated by `netif_smoltcp_selftest` (ARP round-trip) |
| Logging / securec | `osal_printk`, `log_event_wifi_print{0,1,2,4}`, `memset_s`/`memcpy_s` | log routed to a settable [`set_log_sink`]; `%` specifiers not expanded (raw fmt) |
| Time leaves | `uapi_systick_get_ms`, `osal_udelay` | `mcycle`-based / busy-wait (approximate, uncalibrated) |
| Adaptation shim | full `osal_adapt_*` (33) | forwards to the OSAL / event / irq / kthread / wait impls |
| ROM globals | `g_dmac_alg_main`, `g_mac_res_etc` | referenced by `libwifi_rom_data.a`, defined by **no** vendor lib → provided here |

### Scaffolds (defined + documented; need hardware or the real blob)

| Area | Symbols | Needs |
|------|---------|-------|
| netif pbuf layout | `pbuf_*` (`netif`) | offsets reconciled with the WiFi build's `lwipopts.h`; the smoltcp TX sink pointed at the blob's real transmit symbol (mismatch corrupts silently) |
| Per-line IRQ | `osal_irq_request/free/enable/disable` | trap-delivery wiring for the WLAN/MAC line |
| WLAN rings / RF clk | `wlan_*`, `oal_ring_*` | descriptor rings + vendor RF HAL (on-silicon) |
| eFuse / TRNG / NV / tsensor | `uapi_nv_read`, `uapi_tsensor_get_current_temp`, … | scaffold values; a HW run needs real ones |

### What a full Wi-Fi link still needs (NOT radio reverse-engineering)

`nm` on `libwifi_driver_dmac.a` shows 1080 undefined symbols, but they are
almost all **obtainable from the vendor delivery** (see `ws63-rf-rs/ws63-RF/LIB_EXTRACT.md`):

- **~422 are WS63 mask-ROM functions** (`fe_*` RF front-end, `hal_machw_*`,
  `hal_al_rx_*`, `hal_btcoex_*`, …). Their addresses are in the ROM symbol table
  `ws63-rf-rs/ws63-RF/rom/ws63_acore_rom.lds` (link with `-T`). They are **not** something
  the runtime reimplements — the radio lives in the on-chip mask ROM. (The
  addresses only execute on real silicon, so this path is HIL, not QEMU.)
- **~618 are defined by other vendor Wi-Fi `.a` libs** the original ws63-RF
  extraction omitted: `libwifi_driver_hmac.a` (host MAC + public `wifi_*` API),
  `libwifi_driver_tcm.a`, `libwifi_btcoex.a`, `libwifi_alg_*.a`,
  `libwpa_supplicant.a` — all present in the C SDK (`LIB_EXTRACT.md` lists paths).
- **~40 are the runtime's job — and ~all are what THIS crate implements**: the
  `osal_*`/`oal_*`/`log_*`/`uapi_*` porting contract + compiler-rt builtins +
  `g_dmac_alg_main`/`g_mac_res_etc` + the `__wifi_pkt_ram_*` linker symbols.

Still genuinely remaining for the runtime (beyond the contract above — note the
scheduler + FRW worker thread are now **implemented**, see the status table):

- **A real `.wifi_pkt_ram` NOLOAD region** in `hisi-riscv-rt` (here the symbols are a
  scaffold `--defsym`).
- **Pinning the netif pbuf layout** to the WiFi build's `lwipopts.h` and the
  smoltcp TX sink to the blob's transmit symbol (on hardware).
- Completing the **omitted Wi-Fi `.a` set** in `ws63-rf-rs/ws63-RF/lib` (`LIB_EXTRACT.md`).

See the workspace [`ROADMAP.md`](../../../ROADMAP.md) phase 4 for the staged plan.

## Validate

```bash
cargo build -p rf_port_demo --release
# run on ws63-qemu (prints "RF PORT DEMO: PASS"):
qemu-system-riscv32 -M ws63 -nographic -serial mon:stdio \
  -kernel target/riscv32imfc-unknown-none-elf/release/rf_port_demo
```

`rf_port_demo` exercises the implemented porting functions and links the vendor
ROM-data blob *through* this crate (its `g_dmac_alg_main` / `g_mac_res_etc`
resolve here). Wired into `ws63-qemu/scripts/smoke-test.sh`.

[`linked_list_allocator`]: https://crates.io/crates/linked_list_allocator
