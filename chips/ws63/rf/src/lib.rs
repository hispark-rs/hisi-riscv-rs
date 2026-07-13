//! # ws63-rf-rs — Rust porting layer for the closed-source WS63 RF blobs
//!
//! The WS63 Wi-Fi/BLE/SLE radio ships as closed-source vendor static libraries
//! in the [`ws63-RF`] delivery (`libwifi_driver_dmac.a`, `libbg_common.a`, …)
//! plus the **runtime-agnostic porting contract** in `ws63-radio-sys/ws63-RF/include/port/`:
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
//! ## Status — Wi-Fi init + active scan verified on real WS63
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
//! - **Timers** — a real ms software-timer service ([`timer`]):
//!   `osal_adapt_timer_*` / `frw_dmac_timer_*`, fired from the FRW worker loop.
//! - **FRW/HCC data path** ([`frw`], [`hcc`]) — a real message-node pool, the
//!   WiFi worker thread (on `sched`) and the host↔device message FIFO; the
//!   blob's protocol half drives them. Validated by `frw_hcc_selftest`.
//! - **netif → smoltcp** (`netif_smoltcp`, feature `net`) — a real
//!   `smoltcp::phy::Device` behind the netif seam: `driverif_input` feeds the RX
//!   queue, `TxToken` calls the TX sink. Validated by `netif_smoltcp_selftest`
//!   (an ARP request round-trips driver→smoltcp→driver).
//! - **Logging / securec** — `osal_printk`, `log_event_*`, `memset_s`/`memcpy_s`
//!   ([`log`]); string/time leaves ([`osal_ext`]).
//! - **Adaptation** — the full `osal_adapt_*` shim ([`osal_adapt`]).
//! - **ROM state** — `g_dmac_alg_main` / `g_mac_res_etc` resolve to their
//!   mask-ROM BSS addresses from `ws63_acore_rom.lds`; Rust must not shadow
//!   these fixed objects with guessed storage.
//!
//! Remaining post-scan work:
//! - **netif pbuf/TX contract** ([`netif`]) — the verified 80-byte zero-copy
//!   reserve supports scan RX, while the remaining build-specific offsets need
//!   generated assertions and the TX sink needs the blob transmit adapter.
//! - **eFuse/TRNG** ([`uapi`]) — scaffold values; a HW run needs real ones.
//!   NV reads already use the read-only ACPU flash KV parser with page/key/CRC
//!   validation; encrypted records are deliberately rejected.
//!
//! **What "symbol closure" means here.** The vendor blobs
//! (`libwifi_driver_{hmac,dmac,tcm}.a`, `libbg_common.a`, `libwifi_alg_*.a`,
//! `libwifi_rom_data.a`) link as one relocatable object against this crate, the
//! WS63 mask-ROM symbol table (`ws63-radio-sys/ws63-RF/rom/ws63_acore_rom.lds`) and compiler-rt
//! with **zero duplicate symbols**, and a `--gc-sections` link rooted at
//! `uapi_wifi_init` leaves a **residual of just two symbols**
//! (`__wifi_pkt_ram_begin__`/`__wifi_pkt_ram_end__` — firmware linker region
//! bounds, supplied by hisi-riscv-rt or an equivalent downstream layout). Reproduce with
//! `ws63-rf-rs/tools/mac-link-residual.sh`. The earlier "~96 missing" figure was
//! a whole-archive upper bound dominated by **off-path** BT-coexistence and
//! alternate-OS-adapter code that Wi-Fi init never reaches (0 BT symbols on the
//! reachability path).
//!
//! **Why a runnable Wi-Fi image is still hardware-in-the-loop:** (1) the ROM
//! symbols are **real-silicon addresses** (an emulator without a populated mask
//! ROM cannot execute them); (2) the HiSilicon-toolchain blobs carry **custom
//! relocations** that stock `lld` does not recognize directly. The guarded
//! two-pass build keeps `rust-lld` as the layout owner, patches those relocations,
//! and fails if the final section layout differs. The runtime + data-path
//! plumbing (scheduler, OSAL, FRW/HCC, timers, netif→smoltcp) is implemented and
//! self-tested standalone. Real silicon now reaches `RF2_INIT_OK` and
//! `RF3_SCAN_OK`; what remains is TX/RX closure, association and IP connectivity.
//! See `README.md`, `ROADMAP.md`, and `docs/plan/hisi-connectivity-stack.md`.
//!
//! [`ws63-RF`]: https://github.com/hispark-rs/ws63-RF

#![no_std]
#![allow(non_upper_case_globals)] // contract symbols must match the C names exactly

#[cfg(all(test, not(target_arch = "riscv32")))]
mod host_test_support {
    struct HostCriticalSection;

    critical_section::set_impl!(HostCriticalSection);

    unsafe impl critical_section::Impl for HostCriticalSection {
        unsafe fn acquire() -> critical_section::RawRestoreState {}

        unsafe fn release(_: critical_section::RawRestoreState) {}
    }
}

use core::cell::Cell;
use critical_section::Mutex;

pub mod alloc;
mod compiler_rt;
#[cfg(feature = "wifi-wpa2-personal")]
mod crypto;
pub mod error;
pub mod frw;
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
mod pmp;
#[cfg(feature = "rf-init-diag")]
mod rf_init_diag;
pub mod timer;
pub mod uapi;
pub mod wifi;
#[cfg(feature = "wifi-wpa2-personal")]
mod wpa_compat;

pub use pmp::prepare_vendor_memory;

/// Terminal target for a mask-ROM callback not supplied by the current port.
///
/// This is part of the callback-table safety contract, not optional tracing:
/// every fixed-address veneer must point at executable code even in a minimal
/// full-init build.
#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub extern "C" fn __ws63_missing_rom_callback() -> ! {
    let caller: u32;
    unsafe {
        core::arch::asm!("mv {caller}, ra", caller = out(reg) caller, options(nomem, nostack));
    }
    let mut hex = [0_u8; 8];
    for (index, byte) in hex.iter_mut().enumerate() {
        let nibble = ((caller >> ((7 - index) * 4)) & 0xf) as u8;
        *byte = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + nibble - 10
        };
    }
    log_emit(b"RFDBG_MISSING_ROM_CALLBACK ra=0x");
    log_emit(&hex);
    log_emit(b"\r\n");
    loop {
        core::hint::spin_loop();
    }
}

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
    let sink = critical_section::with(|cs| LOG_SINK.borrow(cs).get());
    if let Some(sink) = sink {
        sink(bytes);
    }
}

/// Force the C porting contract objects into the final link.
///
/// This is normally unnecessary with `rust-lld`, but it is useful for the RF
/// vendor-link lane: GNU ld scans static archives left-to-right, while rustc's
/// Rust rlibs can appear before the vendor Wi-Fi `.a` files that reference the
/// C ABI symbols. A binary that calls this function makes those symbols
/// live from Rust's side, so the linker does not depend on archive rescans.
#[doc(hidden)]
#[inline(never)]
pub fn force_link_contract() {
    macro_rules! keep {
        ($symbol:path as $ty:ty) => {
            let _ = core::hint::black_box($symbol as $ty);
        };
    }

    use core::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};

    keep!(alloc::osal_kmalloc as extern "C" fn(usize) -> *mut c_void);
    keep!(alloc::osal_kfree as extern "C" fn(*mut c_void));

    keep!(log::log_event_wifi_print0 as extern "C" fn(c_uint) -> c_int);
    keep!(log::log_event_wifi_print1 as extern "C" fn(c_uint, c_uint) -> c_int);
    keep!(log::log_event_wifi_print2 as extern "C" fn(c_uint, c_uint, c_uint) -> c_int);
    keep!(log::log_event_wifi_print3 as extern "C" fn(c_uint, c_uint, c_uint, c_uint) -> c_int);
    keep!(
        log::log_event_wifi_print4
            as extern "C" fn(c_uint, c_uint, c_uint, c_uint, c_uint) -> c_int
    );
    keep!(log::log_event_print0 as extern "C" fn() -> c_int);
    keep!(log::log_event_print1 as extern "C" fn() -> c_int);
    keep!(log::log_event_print2 as extern "C" fn() -> c_int);
    keep!(log::log_event_print3 as extern "C" fn() -> c_int);
    keep!(log::log_event_print4 as extern "C" fn() -> c_int);
    keep!(log::osal_printk as extern "C" fn(*const c_char) -> c_int);
    keep!(
        log::snprintf_s
            as extern "C" fn(
                *mut c_char,
                usize,
                usize,
                *const c_char,
                c_uint,
                c_uint,
                c_uint,
                c_uint,
            ) -> c_int
    );
    keep!(log::memset_s as extern "C" fn(*mut c_void, usize, c_int, usize) -> c_int);
    keep!(log::memcpy_s as extern "C" fn(*mut c_void, usize, *const c_void, usize) -> c_int);

    keep!(libc::malloc as extern "C" fn(c_ulong) -> *mut c_void);
    keep!(libc::free as extern "C" fn(*mut c_void));
    keep!(libc::memalign as extern "C" fn(c_ulong, c_ulong) -> *mut c_void);
    keep!(libc::strcmp as extern "C" fn(*const c_char, *const c_char) -> c_int);
    keep!(libc::strtol as extern "C" fn(*const c_char, *mut *mut c_char, c_uint) -> c_long);
    keep!(libc::atoi as extern "C" fn(*const c_char) -> c_int);
    keep!(libc::strstr as extern "C" fn(*const c_char, *const c_char) -> *mut c_char);
    keep!(libc::tolower as extern "C" fn(c_int) -> c_int);
    keep!(libc::gettimeofday as extern "C" fn(*mut osal_ext::OsalTimeval, *mut c_void) -> c_int);
    keep!(libc::print_str as extern "C" fn(*const c_char));

    keep!(osal::osal_irq_lock as extern "C" fn() -> c_ulong);
    keep!(osal::osal_irq_restore as extern "C" fn(c_ulong));
    keep!(osal::osal_udelay as extern "C" fn(u32));
    keep!(osal::osal_flush_cache as extern "C" fn());
    keep!(
        osal::osal_irq_request
            as extern "C" fn(
                u32,
                Option<unsafe extern "C" fn(u32, *mut c_void)>,
                Option<unsafe extern "C" fn(u32, *mut c_void)>,
                *const c_char,
                *mut c_void,
            ) -> c_int
    );
    keep!(osal::osal_irq_enable as extern "C" fn(u32) -> c_int);
    keep!(osal::osal_irq_disable as extern "C" fn(u32) -> c_int);
    keep!(osal::osal_irq_clear as extern "C" fn(u32) -> c_int);
    keep!(osal::osal_msleep as extern "C" fn(u32));
    keep!(osal::osal_get_current_pid as extern "C" fn() -> c_int);
    keep!(osal::osal_get_current_tid as extern "C" fn() -> c_int);
    keep!(
        osal::osal_kthread_create
            as extern "C" fn(
                Option<extern "C" fn(*mut c_void) -> *mut c_void>,
                *mut c_void,
                *const c_char,
                usize,
            ) -> *mut c_void
    );

    keep!(osal_adapt::osal_adapt_atomic_set as extern "C" fn(*mut osal_sync::OsalAtomic, c_int));
    keep!(osal_adapt::osal_adapt_get_jiffies as extern "C" fn() -> u64);
    keep!(osal_adapt::osal_adapt_irq_lock as extern "C" fn() -> c_uint);
    keep!(osal_adapt::osal_adapt_irq_restore as extern "C" fn(c_uint));
    keep!(
        osal_adapt::osal_adapt_kthread_create
            as extern "C" fn(
                Option<extern "C" fn(*mut c_void) -> *mut c_void>,
                *mut c_void,
                *const c_char,
                c_uint,
            ) -> *mut c_void
    );

    keep!(osal_ext::osal_vmalloc as extern "C" fn(c_ulong) -> *mut c_void);
    keep!(osal_ext::osal_vfree as extern "C" fn(*mut c_void));
    keep!(osal_ext::osal_strlen as extern "C" fn(*const c_char) -> c_uint);
    keep!(osal_ext::osal_strcmp as extern "C" fn(*const c_char, *const c_char) -> c_int);
    keep!(
        osal_ext::osal_adapt_strncmp
            as extern "C" fn(*const c_char, *const c_char, c_uint) -> c_int
    );
    keep!(osal_ext::osal_memcmp as extern "C" fn(*const c_void, *const c_void, c_int) -> c_int);
    keep!(
        osal_ext::osal_strtol as extern "C" fn(*const c_char, *mut *mut c_char, c_uint) -> c_long
    );
    keep!(osal_ext::osal_get_jiffies as extern "C" fn() -> u64);
    keep!(osal_ext::osal_gettimeofday as extern "C" fn(*mut osal_ext::OsalTimeval));
    keep!(
        osal_ext::osal_copy_to_user
            as extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_ulong
    );

    keep!(netif::pbuf_alloc as extern "C" fn(c_int, u16, c_int) -> *mut c_void);
    keep!(netif::pbuf_free as extern "C" fn(*mut c_void) -> u8);
    keep!(netif::pbuf_ref as extern "C" fn(*mut c_void));
    keep!(netif::pbuf_header as extern "C" fn(*mut c_void, i16) -> u8);
    keep!(netif::driverif_input as extern "C" fn(*mut c_void, *mut c_void));
    keep!(
        netif::netifapi_netif_add
            as extern "C" fn(*mut c_void, *const u32, *const u32, *const u32) -> c_int
    );
    keep!(netif::netifapi_netif_remove as extern "C" fn(*mut c_void) -> c_int);
    keep!(netif::netifapi_netif_find_by_name as extern "C" fn(*const u8) -> *mut c_void);
    keep!(
        netif::netifapi_netif_get_addr
            as extern "C" fn(*mut c_void, *mut u32, *mut u32, *mut u32) -> c_int
    );
    keep!(
        netif::netifapi_netif_add_ext_callback as extern "C" fn(*mut c_void, *mut c_void) -> c_int
    );
    keep!(netif::netifapi_set_ip6_autoconfig_disabled as extern "C" fn(*mut c_void) -> c_int);
    keep!(
        netif::netifapi_netif_add_ip6_linklocal_address as extern "C" fn(*mut c_void, u8) -> c_int
    );
    keep!(netif::netifapi_netif_set_up as extern "C" fn(*mut c_void) -> c_int);
    keep!(netif::netifapi_netif_set_down as extern "C" fn(*mut c_void) -> c_int);
    keep!(netif::netifapi_netif_set_link_up as extern "C" fn(*mut c_void) -> c_int);
    keep!(netif::netifapi_netif_set_default as extern "C" fn(*mut c_void) -> c_int);
    keep!(netif::netif_set_link_up_interface as extern "C" fn(*mut c_void));
    keep!(netif::netif_set_link_down_interface as extern "C" fn(*mut c_void));
    keep!(netif::tcpip_callback as extern "C" fn(*mut c_void, *mut c_void) -> c_int);

    keep!(uapi::uapi_tsensor_get_current_temp as extern "C" fn(*mut i8) -> u32);
    keep!(uapi::uapi_nv_read as extern "C" fn(u16, u16, *mut u16, *mut u8) -> u32);
    keep!(uapi::uapi_nv_write as extern "C" fn(u16, *const u8, u16) -> u32);
    keep!(uapi::uapi_efuse_read_bit as extern "C" fn(*mut u8, u32, u8) -> u32);
    keep!(uapi::uapi_efuse_read_buffer as extern "C" fn(*mut u8, u32, u16) -> u32);
    keep!(uapi::uapi_drv_cipher_trng_get_random_bytes as extern "C" fn(*mut u8, u32) -> u32);
    keep!(uapi::get_dev_addr as extern "C" fn(*mut u8, u8, u8) -> u32);
    keep!(uapi::get_tcxo_freq as extern "C" fn() -> u32);
    #[cfg(not(feature = "wifi-wpa2-personal"))]
    {
        keep!(uapi::uapi_wifi_softap_stop as extern "C" fn() -> i32);
        keep!(uapi::uapi_wifi_sta_stop as extern "C" fn() -> i32);
    }
}
