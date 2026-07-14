# ws63-rf-rs

Rust **porting layer + FFI bindings** for the closed-source WS63 Wi-Fi/BLE radio
blobs delivered by [`ws63-radio-sys`](../../../crates/chips/ws63/ws63-radio-sys) (which nests the language-neutral `ws63-RF` payload). It is the WS63 analogue of esp-hal's
`esp-radio` OS-adapter: it implements the **runtime-agnostic porting contract**
(`ws63-radio-sys/ws63-RF/include/port/*.h`) in Rust as `#[unsafe(no_mangle)] extern "C"`
symbols, so when a firmware links a vendor blob the linker resolves the blob's
`osal_* / oal_* / log_* / uapi_* / frw_* / hcc_* / wlan_*` references to these
implementations.

> **Design rule.** No Rust goes into `ws63-RF` — that delivery stays
> language-neutral so the blobs can be ported to *any* runtime. This crate is
> the ws63-rs runtime's implementation of `ws63-RF`'s C contract.

Application-facing radio types now come from the independent
[`hisi-rf`](https://crates.io/crates/hisi-rf) crate. The
`ws63_rf_rs::radio` module is only a deprecated migration facade: it remains
through the parent 0.7.x release train and is removed no earlier than parent
v0.8.0. New code should import `hisi_rf` directly and use this crate only for
the WS63 backend/ABI adapter.

## Status: Wi-Fi init, scan, WPA2 connect, and ping verified on silicon

**Project context:** ws63-rs is now focused on the connectivity milestones in
[`ROADMAP.md`](../../../ROADMAP.md). This crate owns the RF runtime half of C1-C5:
turning the implemented porting/runtime seams into a real WS63 Wi-Fi image,
then proving init, scan, connect, and ping on hardware.

This crate makes the porting contract **compile, link, and actually run** — the
runtime and data-path plumbing (scheduler, OSAL, FRW worker + HCC, software
timers, netif→smoltcp) are implemented and self-tested standalone on `ws63-qemu`
(`rf_port_demo`, plus the crate's `sched_selftest` / `frw_hcc_selftest` /
`netif_smoltcp_selftest`). The guarded full-init image now boots on a real WS63,
initializes `wlan0`, returns real STA scan results, associates through the cropped
WPA2-Personal supplicant, obtains a DHCP lease, resolves the gateway with ARP, and
receives an ICMP Echo Reply through the Rust-visible L2 path. Real RF behavior
remains hardware-in-the-loop because the ROM symbols are silicon addresses.

### Implemented for real (usable today)

| Area | Symbols | Notes |
|------|---------|-------|
| Memory | `osal_kmalloc`/`osal_kfree`, `malloc`/`free`/`memalign`, `oal_mem_*` | WS63 C ABI adapter over [`hisi-alloc`]; linker-owned arena, zero-initialized and aligned |
| Scheduler | `osal_kthread_*`, `osal_sem_*`, `osal_mutex_*`, `osal_wait_*`, queues + event groups | real cooperative scheduler with **timed** blocking (`*_timeout` deadlines); validated by `sched_selftest` |
| Sync / IRQ | `osal_irq_lock`/`restore`, spinlocks, atomics, `ArchIntLock`/`Restore` | real, via `mstatus.MIE` |
| Timers | `osal_adapt_timer_*`, `frw_dmac_timer_*` | real ms software-timer service, fired from the FRW worker loop |
| FRW / HCC data path | `frw_*`, `hcc_*` | real message-node pool + WiFi worker thread (on the scheduler) + host↔device FIFO; validated by `frw_hcc_selftest` |
| netif → smoltcp | `netif` / `netif_smoltcp` (feature `net`) | real `smoltcp::phy::Device` behind the netif seam; `driverif_input` feeds RX, `TxToken` calls the TX sink; validated by `netif_smoltcp_selftest` (ARP round-trip) |
| WS63 pbuf headroom | `pbuf_*` (`netif`) | vendor `PBUF_ZERO_COPY_RESERVE=80` layout; validated by real-silicon STA scan and RX frame copies |
| Logging / securec | `osal_printk`, `log_event_wifi_print{0,1,2,4}`, `memset_s`/`memcpy_s` | log routed to a settable [`set_log_sink`]; `%` specifiers not expanded (raw fmt) |
| Time leaves | `uapi_systick_get_ms`, `osal_udelay` | `mcycle`-based / busy-wait (approximate, uncalibrated) |
| Adaptation shim | full `osal_adapt_*` (33) | forwards to the OSAL / event / irq / kthread / wait impls |
| ROM state | `g_dmac_alg_main`, `g_mac_res_etc` | fixed mask-ROM BSS objects at `0x180b2c` / `0x1823f8`, resolved from `ws63_acore_rom.lds`; Rust does not redefine them |

### Scaffolds (defined + documented; need hardware or the real blob)

| Area | Symbols | Needs |
|------|---------|-------|
| WLAN TX/RX | `driverif_input`, blob transmit adapter | bounded Rust-visible L2 queue; DHCP, ARP and ICMP passed on silicon |
| eFuse / TRNG / tsensor | `uapi_efuse_*`, `uapi_tsensor_get_current_temp`, … | scaffold values; a HW run needs real ones |
| NV read | `uapi_nv_read` | C-ABI adapter over `hisi-nvs::NvReader`; KV page/key/CRC format and errors are owned by `hisi-nvs` |

### What a full Wi-Fi link still needs (NOT radio reverse-engineering)

`nm` on `libwifi_driver_dmac.a` shows 1080 undefined symbols, but they are
almost all **obtainable from the vendor delivery** (see `ws63-radio-sys/ws63-RF/LIB_EXTRACT.md`):

- **~422 are WS63 mask-ROM functions** (`fe_*` RF front-end, `hal_machw_*`,
  `hal_al_rx_*`, `hal_btcoex_*`, …). Their addresses are in the ROM symbol table
  `ws63-radio-sys/ws63-RF/rom/ws63_acore_rom.lds` (link with `-T`). They are **not** something
  the runtime reimplements — the radio lives in the on-chip mask ROM. (The
  addresses only execute on real silicon, so this path is HIL, not QEMU.)
- **~618 are defined by other vendor Wi-Fi `.a` libs** the original ws63-RF
  extraction omitted: `libwifi_driver_hmac.a` (host MAC + public `wifi_*` API),
  `libwifi_driver_tcm.a`, `libwifi_btcoex.a`, `libwifi_alg_*.a`,
  `libwpa_supplicant.a` — all present in the C SDK (`LIB_EXTRACT.md` lists paths).
- **~40 are the runtime's job — and ~all are what THIS crate implements**: the
  `osal_*`/`oal_*`/`log_*`/`uapi_*` porting contract + compiler-rt builtins +
  compiler-rt leaves + the `__wifi_pkt_ram_*` linker symbols. ROM-owned state
  such as `g_dmac_alg_main`/`g_mac_res_etc` resolves from the ROM symbol table.

Still genuinely remaining for the runtime (beyond the contract above — note the
scheduler + FRW worker thread are now **implemented**, see the status table):

- **Component extraction without behavior drift.** The guarded RF image and its
  PMP/shared-memory/NV platform setup are verified through init, active scan,
  WPA2 association, DHCP, ARP and ICMP. A1-A4 now move ownership into independent
  crates while preserving those HIL markers.
- Generating checks for the remaining optional pbuf fields from the WiFi build's
  headers, and connecting the smoltcp TX sink to the blob's transmit symbol.
- Completing the **omitted Wi-Fi `.a` set** in `ws63-radio-sys/ws63-RF/lib` (`LIB_EXTRACT.md`).

See the workspace [`ROADMAP.md`](../../../ROADMAP.md) and
[`docs/plan/hisi-connectivity-stack.md`](../../../docs/plan/hisi-connectivity-stack.md)
for the staged plan.

## Validate

```bash
cargo build -p rf_port_demo --release
# run on ws63-qemu (prints "RF PORT DEMO: PASS"):
qemu-system-riscv32 -M ws63 -nographic -serial mon:stdio \
  -kernel target/riscv32imfc-unknown-none-elf/release/rf_port_demo
```

`rf_port_demo` exercises the allocator/securec/log porting functions without a
vendor archive. `wifi_blob_link` owns the minimal raw-archive link check, while
`wifi_init_smoke` owns the complete runtime and connectivity path. The port demo
is wired into `ws63-qemu/scripts/smoke-test.sh`; it does not claim RF behavior.

[`hisi-alloc`]: https://github.com/hispark-rs/hisi-alloc
