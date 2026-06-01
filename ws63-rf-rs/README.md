# ws63-rf-rs

Rust **porting layer + FFI bindings** for the closed-source WS63 Wi-Fi/BLE radio
blobs delivered in [`ws63-RF`](../ws63-RF). It is the WS63 analogue of esp-hal's
`esp-radio` OS-adapter: it implements the **runtime-agnostic porting contract**
(`ws63-RF/include/port/*.h`) in Rust as `#[unsafe(no_mangle)] extern "C"`
symbols, so when a firmware links a vendor blob the linker resolves the blob's
`osal_* / oal_* / log_* / uapi_* / frw_* / hcc_* / wlan_*` references to these
implementations.

> **Design rule.** No Rust goes into `ws63-RF` — that delivery stays
> language-neutral so the blobs can be ported to *any* runtime. This crate is
> the ws63-rs runtime's implementation of `ws63-RF`'s C contract.

## ⚠️ Status: porting-layer scaffold — this does NOT make Wi-Fi work yet

This crate makes the porting contract **compile, link, and (where feasible)
actually run**, validated on `ws63-qemu` by the `rf_port_demo` example. It is a
foundation, not a working Wi-Fi stack. The honest picture:

### Implemented for real (usable today)

| Area | Symbols | Notes |
|------|---------|-------|
| Memory | `osal_kmalloc`, `osal_kfree` | real first-fit heap ([`linked_list_allocator`]) over a static pool; zero-initialised, 8-aligned |
| Logging | `osal_printk`, `log_event_wifi_print{0,1,2,4}` | routed to a settable [`set_log_sink`]; format `%` specifiers are **not** expanded (raw fmt string) |
| Safe C lib | `memset_s`, `memcpy_s` | faithful securec semantics (bounds-checked) |
| Time | `uapi_systick_get_ms`, `osal_udelay` | `mcycle`-based / busy-wait (approximate, uncalibrated) |
| IRQ critical section | `osal_irq_lock`, `osal_irq_restore` | real, via `mstatus.MIE` |
| Cache | `osal_flush_cache` | data `fence` (single-core, no MMU) |
| OAL pool | `oal_memory_init/exit`, `oal_mem_rsv`, `oal_mem_set_buf_size/skb_size`, … | bump reservation inside the 48 KB Wi-Fi packet RAM |
| ROM globals | `g_dmac_alg_main`, `g_mac_res_etc` | referenced by `libwifi_rom_data.a`, defined by **no** vendor lib → provided here (zeroed scaffold storage) |

### Typed, documented stubs (return error / null, do nothing useful yet)

| Area | Symbols | Blocked on |
|------|---------|-----------|
| Threads / wait | `osal_kthread_*`, `osal_wait_*` | a task **scheduler** (ws63-rs is bare-metal; ROADMAP phase 6 / an RTOS) |
| Per-line IRQ | `osal_irq_request/free/enable/disable` | trap-delivery wiring for the WLAN/MAC line (phase 4) |
| Framework | `frw_*` (21) | message framework + worker thread + timers |
| IPC transport | `hcc_*` (6) | host↔device-MAC shared-memory transport |
| WLAN rings / RF clk | `wlan_*`, `oal_ring_*` (12) | descriptor rings + vendor RF HAL |
| NV / tsensor | `uapi_nv_read`, `uapi_tsensor_get_current_temp` | flash-NV (RF cal + MAC) / ws63-hal tsensor |

### Out of scope here (why connectivity still does not work)

Linking the **code** blobs (`libwifi_driver_dmac.a` ~629 KB, `libbg_common.a`)
additionally needs, none of which is the runtime's job and none of which this
crate provides:

- **~118 vendor RF-front-end / MAC-HAL symbols** that `libwifi_driver_dmac.a`
  references but does **not** define (verified by `nm`: `fe_*` RF device driver,
  `hal_btcoex_*`, some `hal_al_rx_*`/`hal_machw_*`; the other ~443 `hal_*`/`fe_*`
  refs resolve inside dmac.a). These are the *radio implementation*, supplied by
  the vendor RF HAL/ROM, not by the porting layer. Re-implementing them in Rust
  would mean reverse-engineering the radio (explicitly a non-goal).
- **The host-MAC + public Wi-Fi API layer** (`wifi_init`, `wifi_sta_scan`,
  `wifi_sta_connect`, declared in `ws63-RF/include/api/wifi/`) lives in
  `libwifi_driver_hmac.a`, which is **not shipped in `ws63-RF/lib`**.
- **A real `.wifi_pkt_ram` NOLOAD region** in `ws63-rt` (here the linker symbols
  are a scaffold `--defsym`).
- **A task scheduler** for the FRW worker thread + the `osal_kthread_*`/wait
  primitives.

See the workspace [`ROADMAP.md`](../ROADMAP.md) phase 4 for the staged plan.

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
