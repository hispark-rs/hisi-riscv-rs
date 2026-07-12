//! WS63 RF C-allocation adapter backed by [`hisi_alloc::CHeap`].
//!
//! This module owns only the vendor `osal_kmalloc` ABI, linker-arena selection,
//! and RF diagnostics. Allocation mechanics live in `hisi-alloc`.

use core::ffi::c_void;

use hisi_alloc::{CHeap, FreeError};

const DEFAULT_ALIGNMENT: usize = 16;

static HEAP: CHeap = CHeap::empty();

#[cfg(target_arch = "riscv32")]
unsafe extern "C" {
    static mut __heap_start__: u8;
    static mut __heap_end__: u8;
}

#[cfg(all(test, not(target_arch = "riscv32")))]
#[repr(align(64))]
struct HostArena([u8; 64 * 1024]);

#[cfg(all(test, not(target_arch = "riscv32")))]
static mut HOST_ARENA: HostArena = HostArena([0; 64 * 1024]);

fn ensure_heap() -> bool {
    #[cfg(target_arch = "riscv32")]
    let (start, len) = {
        let start = &raw mut __heap_start__;
        let end = &raw mut __heap_end__;
        (start, end as usize - start as usize)
    };

    #[cfg(all(test, not(target_arch = "riscv32")))]
    let (start, len) = {
        // SAFETY: this is the only address acquisition for the static host
        // arena; all subsequent access is serialized through `HEAP`.
        let start = unsafe { (&raw mut HOST_ARENA.0).cast::<u8>() };
        (start, 64 * 1024)
    };

    #[cfg(all(not(test), not(target_arch = "riscv32")))]
    let (start, len) = (core::ptr::null_mut(), 0);

    // SAFETY: each selected region is static, exclusively owned by this heap,
    // and remains valid for the entire firmware or host-test process.
    unsafe { HEAP.init(start, len).is_ok() }
}

/// Allocate `size` zero-initialized bytes. Returns null on failure or zero size.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kmalloc(size: usize) -> *mut c_void {
    if !ensure_heap() {
        return core::ptr::null_mut();
    }
    HEAP.allocate_zeroed(size, DEFAULT_ALIGNMENT).cast()
}

/// Allocate memory with the alignment required by crypto/DMA buffers.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kmalloc_align(size: u32, _flags: u32, boundary: u32) -> *mut c_void {
    let alignment = boundary as usize;
    if alignment < DEFAULT_ALIGNMENT || !alignment.is_power_of_two() || !ensure_heap() {
        return core::ptr::null_mut();
    }
    HEAP.allocate_zeroed(size as usize, alignment).cast()
}

/// Free memory returned by [`osal_kmalloc`]. Null is a no-op.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn osal_kfree(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    if !ensure_heap() {
        trace_bad_free(ptr as usize, FreeError::Uninitialized, caller_address());
        return;
    }
    // SAFETY: this C boundary cannot express provenance. `CHeap` validates the
    // complete ownership header and arena bounds before touching its free list.
    if let Err(error) = unsafe { HEAP.deallocate(ptr.cast()) } {
        trace_bad_free(ptr as usize, error, caller_address());
    }
}

/// Resize an owned allocation, preserving the common prefix.
pub(crate) fn realloc_owned(ptr: *mut c_void, size: usize) -> *mut c_void {
    if !ensure_heap() {
        return core::ptr::null_mut();
    }
    // SAFETY: the compatibility callers use only pointers returned by the
    // allocator. Foreign pointers fail validation and produce null.
    unsafe {
        HEAP.reallocate_zeroed(ptr.cast(), size, DEFAULT_ALIGNMENT)
            .cast()
    }
}

#[inline(always)]
fn caller_address() -> usize {
    #[cfg(target_arch = "riscv32")]
    {
        let caller: usize;
        // SAFETY: reading `ra` has no memory or stack side effects.
        unsafe {
            core::arch::asm!("mv {caller}, ra", caller = out(reg) caller, options(nomem, nostack));
        }
        caller
    }

    #[cfg(not(target_arch = "riscv32"))]
    {
        0
    }
}

fn trace_bad_free(ptr: usize, error: FreeError, caller: usize) {
    let code = match error {
        FreeError::Uninitialized => 1,
        FreeError::OutOfBounds => 2,
        FreeError::Misaligned => 3,
        FreeError::InvalidHeader => 4,
    };

    #[cfg(feature = "rf-init-diag")]
    crate::rf_init_diag::trace_bad_free(ptr as u32, 0, code, caller as u32);

    #[cfg(not(feature = "rf-init-diag"))]
    let _ = (ptr, code, caller);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_adapter_allocates_zeroed_and_reallocates() {
        let ptr = osal_kmalloc(32).cast::<u8>();
        assert!(!ptr.is_null());
        // SAFETY: adapter returned a live 32-byte allocation.
        assert!(
            unsafe { core::slice::from_raw_parts(ptr, 32) }
                .iter()
                .all(|byte| *byte == 0)
        );
        // SAFETY: ptr uniquely owns an adapter allocation.
        unsafe { core::ptr::write_bytes(ptr, 0xA5, 32) };
        let grown = realloc_owned(ptr.cast(), 96).cast::<u8>();
        assert!(!grown.is_null());
        // SAFETY: grown is live for 96 bytes.
        let bytes = unsafe { core::slice::from_raw_parts(grown, 96) };
        assert!(bytes[..32].iter().all(|byte| *byte == 0xA5));
        assert!(bytes[32..].iter().all(|byte| *byte == 0));
        osal_kfree(grown.cast());
    }
}
