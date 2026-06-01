//! Data globals the ROM-data blob references but no vendor library defines.
//!
//! In `libwifi_rom_data.a`, `g_dmac_algorithm_main` points at `g_dmac_alg_main`
//! and `g_mac_res` points at `g_mac_res_etc`; `nm` confirms neither target is
//! defined in any ws63-RF blob, so the runtime must supply them. Their real
//! content is the DMAC algorithm-main config + extended MAC resources from the
//! C SDK dmac source (not provided in ws63-RF) — SCAFFOLD: zeroed, writable
//! storage so the relocations resolve and the blob has somewhere to read/write.
//! Populating them with correct values is a phase-4 TODO.

use core::cell::UnsafeCell;

/// Writable vendor-data storage exported under a fixed C symbol name.
#[repr(transparent)]
pub struct VendorData<const N: usize>(UnsafeCell<[u8; N]>);
// SAFETY: single-hart; the blob serialises its own access to these globals.
unsafe impl<const N: usize> Sync for VendorData<N> {}
impl<const N: usize> VendorData<N> {
    const fn zeroed() -> Self {
        Self(UnsafeCell::new([0u8; N]))
    }
}

/// DMAC algorithm-main config (referenced by `g_dmac_algorithm_main`).
#[unsafe(no_mangle)]
pub static g_dmac_alg_main: VendorData<64> = VendorData::zeroed();

/// Extended MAC resources (referenced by `g_mac_res`).
#[unsafe(no_mangle)]
pub static g_mac_res_etc: VendorData<64> = VendorData::zeroed();
