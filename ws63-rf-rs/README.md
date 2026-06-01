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

### What a full Wi-Fi link still needs (NOT radio reverse-engineering)

`nm` on `libwifi_driver_dmac.a` shows 1080 undefined symbols, but they are
almost all **obtainable from the vendor delivery** (see `ws63-RF/LIB_EXTRACT.md`):

- **~422 are WS63 mask-ROM functions** (`fe_*` RF front-end, `hal_machw_*`,
  `hal_al_rx_*`, `hal_btcoex_*`, …). Their addresses are in the ROM symbol table
  `ws63-RF/rom/ws63_acore_rom.lds` (link with `-T`). They are **not** something
  the runtime reimplements — the radio lives in the on-chip mask ROM. (The
  addresses only execute on real silicon, so this path is HIL, not QEMU.)
- **~618 are defined by other vendor Wi-Fi `.a` libs** the original ws63-RF
  extraction omitted: `libwifi_driver_hmac.a` (host MAC + public `wifi_*` API),
  `libwifi_driver_tcm.a`, `libwifi_btcoex.a`, `libwifi_alg_*.a`,
  `libwpa_supplicant.a` — all present in the C SDK (`LIB_EXTRACT.md` lists paths).
- **~40 are the runtime's job — and ~all are what THIS crate implements**: the
  `osal_*`/`oal_*`/`log_*`/`uapi_*` porting contract + compiler-rt builtins +
  `g_dmac_alg_main`/`g_mac_res_etc` + the `__wifi_pkt_ram_*` linker symbols.

Still genuinely remaining for the runtime (beyond the contract above):

- **A task scheduler** for the FRW worker thread + `osal_kthread_*`/wait (those
  are still stubs here — ROADMAP phase 6 / an RTOS).
- **A real `.wifi_pkt_ram` NOLOAD region** in `ws63-rt` (here the symbols are a
  scaffold `--defsym`).
- Completing the **omitted Wi-Fi `.a` set** in `ws63-RF/lib` (`LIB_EXTRACT.md`).

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
