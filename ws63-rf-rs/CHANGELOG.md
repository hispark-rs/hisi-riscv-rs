# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
- Restructured crate (ws63-RF nested under ws63-rf-rs to prevent lateral dependencies)
- Made internal scheduler/runtime not a public API (encapsulation)

### Fixed
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
- ROM globals: g_dmac_alg_main, g_mac_res_etc (zeroed scaffold storage for vendor lib references)
- Example: rf_port_demo (validates porting contract on ws63-qemu)
