# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Read-only WS63 ACPU NV parser with page/key/CRC validation and host tests.
- App-owned PMP/memory-attribute setup matching the official Wi-Fi layout.
- Netif→smoltcp bridge (feature `net`) — TCP/IP stack integration for frame round-trip (replaces lwip behind netif seam)
- Real software-timer service (frw_dmac_timer_* / osal_adapt_timer_* implementations)
- Real FRW worker thread + HCC transport (data path) — framework message delivery infrastructure
- Netif/LITOS seam + full MAC link → Wi-Fi-init symbol closure (native linker closure of vendor blobs)
- OSAL condition variable (osal_wait_*) support + libc (memset_s/memcpy_s) + OAL/UAPI leaf symbols
- OSAL adapter shim (33 symbols) + real timed blocking (osal_adapt_* for timeout support)
- Complete OSAL implementations — spinlock/atomic/queue/event-group/vmalloc/string functions/time utilities
- Scheduler-backed osal_sem_* / osal_mutex_* (real semaphore and mutual exclusion)
- Real cooperative scheduler backing the full OSAL contract (task management + synchronization)
- log_event_wifi_print3 function + vendored Wi-Fi libraries update (ws63-RF bump)

### Changed
- Use the HAL's unstable `SharedMemory` controller for the official default
  shared-RAM bank configuration before vendor Wi-Fi init.
- Make standard libc symbol exports and RISC-V context-switch assembly
  target-specific so host unit tests do not override the test process libc.
- Restructured crate (ws63-RF nested under ws63-rf-rs to prevent lateral dependencies)
- Made internal scheduler/runtime not a public API (encapsulation)

### Fixed
- Match the vendor lwIP `PBUF_ZERO_COPY_RESERVE=80` layout so
  `oal_pbuf_netbuf_alloc` exposes valid HCC/FRW/MAC headroom instead of copying
  received frames over adjacent heap metadata; real-silicon STA scan now
  completes with `RF3_SCAN_OK`.
- Let the vendor init closure register its own ROM callbacks instead of writing
  guessed enum IDs from the smoke application; conditional Wi-Fi features make
  those IDs build-specific and the previous values could write past the table.
- Stop overriding the WS63 mask-ROM `frw_rom_cb_register` with a one-argument
  Rust stub; the full image now resolves the official two-argument ROM service.
- Match the vendor `uapi_tsensor_get_current_temp(int8_t *) -> errcode_t` ABI;
  the conservative scaffold now writes its output instead of returning the
  temperature as an error code.
- Corrected symbol-closure documentation story (ROM table references, not RF reverse-engineering)
- Fixed rustdoc broken-link warnings (code-span formatting in module docs)

## [0.1.0] - 2026-06-02

### Added
- Initial Rust porting layer + FFI bindings for closed-source WS63 Wi-Fi/BLE radio blobs
- Runtime-agnostic porting contract implementation (osal_* / oal_* / log_* / uapi_* C interfaces)
- Memory management: osal_kmalloc / osal_kfree (linked_list_allocator heap over static pool, zero-initialized, 8-byte aligned)
- Logging: osal_printk, log_event_wifi_print{0,1,2,4} (routed to settable log sink, raw format strings)
- Safe C library: memset_s, memcpy_s (bounds-checked securec semantics)
- Time functions: uapi_systick_get_ms, osal_udelay (mcycle-based, busy-wait)
- IRQ critical section: osal_irq_lock, osal_irq_restore (via mstatus.MIE)
- Cache management: osal_flush_cache (data fence for single-core)
- OAL packet-RAM pool: oal_memory_init/exit, oal_mem_rsv, oal_mem_set_buf_size/skb_size (48 KB packet RAM)
- ROM state linkage: `g_dmac_alg_main` and `g_mac_res_etc` resolve to the fixed
  mask-ROM BSS objects from `ws63_acore_rom.lds`; the Rust port does not shadow
  them with guessed storage.
- Example: rf_port_demo (validates porting contract on ws63-qemu)
