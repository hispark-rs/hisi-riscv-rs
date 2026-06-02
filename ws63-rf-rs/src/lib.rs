//! # ws63-rf-rs — Rust porting layer for the closed-source WS63 RF blobs
//!
//! The WS63 Wi-Fi/BLE/SLE radio ships as closed-source vendor static libraries
//! in the [`ws63-RF`] delivery (`libwifi_driver_dmac.a`, `libbg_common.a`, …)
//! plus the **runtime-agnostic porting contract** in `ws63-rf-rs/ws63-RF/include/port/`:
//! ~77 C functions any host runtime must implement (OSAL, OAL, FRW, HCC, WLAN,
//! log, UAPI) so the blobs can run on it.
//!
//! This crate is the **Rust implementation of that contract** for the `ws63-rs`
//! runtime — analogous to esp-hal's `esp-radio` OS-adapter. It defines the
//! contract functions as `#[unsafe(no_mangle)] extern "C"` symbols; when a
//! firmware links a vendor blob, the linker resolves the blob's undefined
//! `osal_* / oal_* / log_* / uapi_*` references to these Rust implementations.
//! It does **not** put any Rust into `ws63-RF` (that delivery stays
//! language-neutral so it can be ported to any runtime).
//!
//! ## Status — symbol closure ACHIEVED for Wi-Fi init; runnable image is HIL
//!
//! Implemented for real (usable today):
//! - **Memory** — `osal_kmalloc`/`osal_kfree` over a real heap ([`alloc`]);
//!   `malloc`/`free`/`memalign`/`oal_mem_*` back onto it ([`libc`], [`oal`]).
//! - **Scheduler** — a real cooperative scheduler (`sched`, internal) backs
//!   `osal_kthread_*`, the counting `Semaphore` behind
//!   `osal_sem_*`/`osal_mutex_*`/`osal_wait_*`, message queues + event groups
//!   ([`osal_queue`]) and **timed** blocking (`*_timeout` via deadlines).
//! - **Sync** — spinlocks + atomics ([`osal_sync`]); IRQ lock/restore (real
//!   `mstatus` CSR) + `ArchIntLock`/`ArchIntRestore` ([`osal`], [`litos`]).
//! - **Logging / securec** — `osal_printk`, `log_event_*`, `memset_s`/`memcpy_s`
//!   ([`log`]); string/time leaves ([`osal_ext`]).
//! - **Adaptation** — the full `osal_adapt_*` shim ([`osal_adapt`]).
//! - **Globals** — `g_dmac_alg_main` / `g_mac_res_etc` ([`globals`]).
//!
//! Seams + scaffolds (defined, documented, not a working data path yet):
//! - **netif/lwip** ([`netif`]) — `pbuf_*` / `driverif_input` / `netifapi_*` /
//!   `tcpip_callback`: the smoltcp integration seam (RX is dropped+counted; the
//!   pbuf layout must be reconciled with the WiFi build's `lwipopts.h`).
//! - **FRW/HCC** ([`frw`], [`hcc`]) — the host↔device message framework +
//!   transport: a real node pool, worker thread (on `sched`) and message FIFO;
//!   the blob's protocol half drives them. Validated by `frw_hcc_selftest`.
//! - **eFuse/TRNG/NV** ([`uapi`]) — scaffold values; a HW run needs real ones.
//!
//! **What "symbol closure" means here.** The vendor blobs
//! (`libwifi_driver_{hmac,dmac,tcm}.a`, `libbg_common.a`, `libwifi_alg_*.a`,
//! `libwifi_rom_data.a`) link as one relocatable object against this crate, the
//! WS63 mask-ROM symbol table (`ws63-rf-rs/ws63-RF/rom/ws63_acore_rom.lds`) and compiler-rt
//! with **zero duplicate symbols**, and a `--gc-sections` link rooted at
//! `uapi_wifi_init` leaves a **residual of just two symbols**
//! (`__wifi_pkt_ram_begin__`/`__wifi_pkt_ram_end__` — linker `--defsym` region
//! bounds, supplied by the firmware link). Reproduce with
//! `ws63-rf-rs/tools/mac-link-residual.sh`. The earlier "~96 missing" figure was
//! a whole-archive upper bound dominated by **off-path** BT-coexistence and
//! alternate-OS-adapter code that Wi-Fi init never reaches (0 BT symbols on the
//! reachability path).
//!
//! **Why a runnable Wi-Fi image is still hardware-in-the-loop:** (1) the ROM
//! symbols are **real-silicon addresses** (an emulator without a populated mask
//! ROM cannot execute them); (2) the HiSilicon-toolchain blobs carry **custom
//! relocations** stock `lld` cannot resolve to absolute addresses (the residual
//! probe uses a relocatable link, which defers them). The remaining software
//! work is the data path: the FRW worker thread + HCC transport ([`frw`], [`hcc`]) and
//! the netif→smoltcp bridge ([`netif`]). See `README.md` and `ROADMAP.md`.
//!
//! [`ws63-RF`]: https://github.com/sanchuanhehe/ws63-RF

#![no_std]
#![allow(non_upper_case_globals)] // contract symbols must match the C names exactly

use core::cell::Cell;
use critical_section::Mutex;

pub mod alloc;
pub mod error;
pub mod frw;
pub mod globals;
pub mod hcc;
pub mod libc;
pub mod litos;
pub mod log;
pub mod netif;
/// netif→smoltcp bridge (feature `net`): a Rust TCP/IP stack behind the netif
/// seam. Optional so the bare porting layer stays lean.
#[cfg(feature = "net")]
pub mod netif_smoltcp;
pub mod oal;
pub mod osal;
pub mod osal_adapt;
pub mod osal_ext;
pub mod osal_queue;
pub mod osal_sync;
pub mod osal_wait;
pub mod timer;
pub mod uapi;

// The task scheduler / runtime is an INTERNAL implementation detail: the vendor
// blob reaches it only through the `osal_*` C-ABI symbols (in `osal`), never as
// a Rust API. So `sched` is private (not part of this crate's public surface).
mod sched;
mod selftest;
/// Internal netif→smoltcp bridge self-test (feature `net`). NOT a public API.
#[cfg(feature = "net")]
#[doc(hidden)]
pub use netif_smoltcp::netif_smoltcp_selftest;
/// Internal scheduler self-test hook (used by the `sched_selftest` example;
/// NOT a public API). Hidden from docs.
#[doc(hidden)]
pub use selftest::{frw_hcc_selftest, osal_queue_selftest, sched_selftest, timer_selftest};

// ── Return codes from the ws63-RF OSAL contract (port_osal.h) ──────────────
/// OSAL success (`OSAL_OK`).
pub const OSAL_OK: i32 = 0;
/// OSAL generic failure (`OSAL_NOK`).
pub const OSAL_NOK: i32 = 1;
/// `OSAL_SYS_WAIT_FOREVER`.
pub const OSAL_SYS_WAIT_FOREVER: u32 = 0xFFFF_FFFF;

// ── Log sink ───────────────────────────────────────────────────────────────
/// A log sink receives already-rendered bytes (a NUL-terminated C format
/// string; format specifiers are **not** expanded — see [`log`]).
pub type LogSink = fn(&[u8]);

static LOG_SINK: Mutex<Cell<Option<LogSink>>> = Mutex::new(Cell::new(None));

/// Install the sink that [`osal_printk`](log) / `log_event_wifi_print*` write to
/// (e.g. a UART writer). Without one, log calls are dropped.
pub fn set_log_sink(sink: LogSink) {
    critical_section::with(|cs| LOG_SINK.borrow(cs).set(Some(sink)));
}

/// Emit `bytes` to the installed log sink, if any. Used by [`log`].
pub(crate) fn log_emit(bytes: &[u8]) {
    critical_section::with(|cs| {
        if let Some(sink) = LOG_SINK.borrow(cs).get() {
            sink(bytes);
        }
    });
}
