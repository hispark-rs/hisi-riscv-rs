//! # ws63-rf-rs — Rust porting layer for the closed-source WS63 RF blobs
//!
//! The WS63 Wi-Fi/BLE/SLE radio ships as closed-source vendor static libraries
//! in the [`ws63-RF`] delivery (`libwifi_driver_dmac.a`, `libbg_common.a`, …)
//! plus the **runtime-agnostic porting contract** in `ws63-RF/include/port/`:
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
//! ## Status — this is a porting-layer scaffold, NOT a working Wi-Fi stack
//!
//! Implemented for real (usable today):
//! - **Memory** — `osal_kmalloc`/`osal_kfree` over a real heap ([`alloc`]).
//! - **Logging** — `osal_printk`, `log_event_wifi_print{0,1,2,4}` to a settable
//!   sink ([`set_log_sink`]); `memset_s`/`memcpy_s` (real, bounds-checked).
//! - **Time** — `uapi_systick_get_ms`, `osal_udelay` via [`ws63_hal`] timers.
//! - **IRQ** — `osal_irq_enable/disable/lock/restore` via [`ws63_hal::interrupt`].
//! - **OAL pool config** — `oal_mem_*` over the 48 KB Wi-Fi packet RAM.
//! - **Globals** — `g_dmac_alg_main` / `g_mac_res_etc` (referenced by the ROM
//!   data blob; not defined by any vendor lib — provided here).
//!
//! Typed, documented **stubs** (return error / `OSAL_NOK`, log "unimplemented"):
//! - Threads/wait (`osal_kthread_*`, `osal_wait_*`) — need a scheduler/RTOS.
//! - Framework + IPC (`frw_*`, `hcc_*`, `wlan_*`) — need the host↔device-MAC
//!   message framework (single-core shared-memory IPC) and the descriptor rings.
//!
//! **Why connectivity does NOT yet work** (but it does NOT need radio
//! reverse-engineering): `libwifi_driver_dmac.a` has ~1080 undefined symbols,
//! of which ~422 are WS63 **mask-ROM** functions (`fe_*`/`hal_machw_*`/… —
//! addresses in `ws63-RF/rom/ws63_acore_rom.lds`) and ~618 are defined by other
//! vendor Wi-Fi `.a` libs the ws63-RF extraction omitted (`libwifi_driver_hmac`
//! /`_tcm`/`_btcoex`/`_alg_*`/`libwpa_supplicant` — see `ws63-RF/LIB_EXTRACT.md`).
//! With those, the surface closes to ~40 symbols — the porting contract THIS
//! crate implements + compiler-rt. Still genuinely needed: a task scheduler
//! (FRW worker thread / `osal_kthread_*`), a real `.wifi_pkt_ram` region, and
//! vendoring the omitted libs. See `README.md` and `ROADMAP.md` phase 4.
//!
//! [`ws63-RF`]: https://github.com/sanchuanhehe/ws63-RF

#![no_std]
#![allow(non_upper_case_globals)] // contract symbols must match the C names exactly

use core::cell::Cell;
use critical_section::Mutex;

pub mod alloc;
pub mod error;
pub mod globals;
pub mod ipc;
pub mod log;
pub mod oal;
pub mod osal;
pub mod osal_sync;
pub mod uapi;

// The task scheduler / runtime is an INTERNAL implementation detail: the vendor
// blob reaches it only through the `osal_*` C-ABI symbols (in `osal`), never as
// a Rust API. So `sched` is private (not part of this crate's public surface).
mod sched;
mod selftest;
/// Internal scheduler self-test hook (used by the `sched_selftest` example;
/// NOT a public API). Hidden from docs.
#[doc(hidden)]
pub use selftest::sched_selftest;

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
