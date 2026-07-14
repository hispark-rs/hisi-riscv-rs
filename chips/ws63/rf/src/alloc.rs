//! WS63 RF C-allocation adapter backed by [`hisi_alloc::CHeap`].
//!
//! This module owns only the vendor `osal_kmalloc` ABI, linker-arena selection,
//! and RF diagnostics. Allocation mechanics live in `hisi-alloc`.

use core::ffi::c_void;

use hisi_alloc::{CHeap, FreeError};
use portable_atomic::{AtomicU32, AtomicUsize, Ordering};

const DEFAULT_ALIGNMENT: usize = 16;

static HEAP: CHeap = CHeap::empty();

const FREE_TRACE_CAPACITY: usize = 16;
const ALLOCATION_TRACE_CAPACITY: usize = 16;

struct FreeTraceSlot {
    sequence: AtomicU32,
    pointer: AtomicUsize,
    caller: AtomicUsize,
}

impl FreeTraceSlot {
    const fn new() -> Self {
        Self {
            sequence: AtomicU32::new(0),
            pointer: AtomicUsize::new(0),
            caller: AtomicUsize::new(0),
        }
    }
}

static FREE_TRACE_SEQUENCE: AtomicU32 = AtomicU32::new(0);
static FREE_TRACE: [FreeTraceSlot; FREE_TRACE_CAPACITY] =
    [const { FreeTraceSlot::new() }; FREE_TRACE_CAPACITY];

struct AllocationTraceSlot {
    sequence: AtomicU32,
    pointer: AtomicUsize,
    size: AtomicUsize,
    caller: AtomicUsize,
}

impl AllocationTraceSlot {
    const fn new() -> Self {
        Self {
            sequence: AtomicU32::new(0),
            pointer: AtomicUsize::new(0),
            size: AtomicUsize::new(0),
            caller: AtomicUsize::new(0),
        }
    }
}

static ALLOCATION_TRACE_SEQUENCE: AtomicU32 = AtomicU32::new(0);
static ALLOCATION_TRACE: [AllocationTraceSlot; ALLOCATION_TRACE_CAPACITY] =
    [const { AllocationTraceSlot::new() }; ALLOCATION_TRACE_CAPACITY];

/// One allocator free event captured without synchronous logging.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FreeTraceRecord {
    pub sequence: u32,
    pub pointer: usize,
    pub caller: usize,
}

/// One allocator allocation event captured without synchronous logging.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AllocationTraceRecord {
    pub sequence: u32,
    pub pointer: usize,
    pub size: usize,
    pub caller: usize,
}

/// Copy recent free events into `output` for post-fault diagnostics.
#[doc(hidden)]
pub fn free_trace_snapshot(output: &mut [FreeTraceRecord]) -> usize {
    let count = output.len().min(FREE_TRACE_CAPACITY);
    for (output, slot) in output.iter_mut().zip(FREE_TRACE.iter()).take(count) {
        loop {
            let before = slot.sequence.load(Ordering::Acquire);
            let pointer = slot.pointer.load(Ordering::Relaxed);
            let caller = slot.caller.load(Ordering::Relaxed);
            let after = slot.sequence.load(Ordering::Acquire);
            if before == after {
                *output = FreeTraceRecord {
                    sequence: after,
                    pointer,
                    caller,
                };
                break;
            }
        }
    }
    count
}

/// Copy recent allocation events into `output` for post-fault diagnostics.
#[doc(hidden)]
pub fn allocation_trace_snapshot(output: &mut [AllocationTraceRecord]) -> usize {
    let count = output.len().min(ALLOCATION_TRACE_CAPACITY);
    for (output, slot) in output.iter_mut().zip(ALLOCATION_TRACE.iter()).take(count) {
        loop {
            let before = slot.sequence.load(Ordering::Acquire);
            let pointer = slot.pointer.load(Ordering::Relaxed);
            let size = slot.size.load(Ordering::Relaxed);
            let caller = slot.caller.load(Ordering::Relaxed);
            let after = slot.sequence.load(Ordering::Acquire);
            if before == after {
                *output = AllocationTraceRecord {
                    sequence: after,
                    pointer,
                    size,
                    caller,
                };
                break;
            }
        }
    }
    count
}

fn record_free(pointer: usize, caller: usize) {
    let sequence = FREE_TRACE_SEQUENCE
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1);
    let slot = &FREE_TRACE[(sequence.wrapping_sub(1) as usize) % FREE_TRACE_CAPACITY];
    slot.sequence.store(0, Ordering::Relaxed);
    slot.pointer.store(pointer, Ordering::Relaxed);
    slot.caller.store(caller, Ordering::Relaxed);
    slot.sequence.store(sequence, Ordering::Release);
}

fn record_allocation(pointer: usize, size: usize, caller: usize) {
    let sequence = ALLOCATION_TRACE_SEQUENCE
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1);
    let slot = &ALLOCATION_TRACE[(sequence.wrapping_sub(1) as usize) % ALLOCATION_TRACE_CAPACITY];
    slot.sequence.store(0, Ordering::Relaxed);
    slot.pointer.store(pointer, Ordering::Relaxed);
    slot.size.store(size, Ordering::Relaxed);
    slot.caller.store(caller, Ordering::Relaxed);
    slot.sequence.store(sequence, Ordering::Release);
}

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
    allocate_zeroed(size, DEFAULT_ALIGNMENT)
}

pub(crate) fn allocate_zeroed(size: usize, alignment: usize) -> *mut c_void {
    let caller = caller_address();
    if alignment < core::mem::align_of::<usize>() || !alignment.is_power_of_two() || !ensure_heap()
    {
        return core::ptr::null_mut();
    }
    let pointer = HEAP.allocate_zeroed(size, alignment);
    record_allocation(pointer as usize, size, caller);
    pointer.cast()
}

/// Allocate memory with the alignment required by crypto/DMA buffers.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kmalloc_align(size: u32, _flags: u32, boundary: u32) -> *mut c_void {
    let alignment = boundary as usize;
    if alignment < DEFAULT_ALIGNMENT {
        return core::ptr::null_mut();
    }
    allocate_zeroed(size as usize, alignment)
}

/// Free memory returned by [`osal_kmalloc`]. Null is a no-op.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn osal_kfree(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    let caller = caller_address();
    record_free(ptr as usize, caller);
    if !ensure_heap() {
        trace_bad_free(ptr as usize, FreeError::Uninitialized, caller);
        return;
    }
    // SAFETY: this C boundary cannot express provenance. `CHeap` validates the
    // complete ownership header and arena bounds before touching its free list.
    if let Err(error) = unsafe { HEAP.deallocate(ptr.cast()) } {
        trace_bad_free(ptr as usize, error, caller);
    }
}

/// Resize an owned allocation, preserving the common prefix.
pub(crate) fn realloc_owned(ptr: *mut c_void, size: usize) -> *mut c_void {
    // SAFETY: this compatibility helper is used only with CHeap-owned pointers.
    unsafe { reallocate_zeroed(ptr, size, DEFAULT_ALIGNMENT) }
}

/// Resize an owned allocation using an explicit power-of-two alignment.
pub(crate) unsafe fn reallocate_zeroed(
    ptr: *mut c_void,
    size: usize,
    alignment: usize,
) -> *mut c_void {
    if alignment < core::mem::align_of::<usize>() || !alignment.is_power_of_two() {
        return core::ptr::null_mut();
    }
    if !ensure_heap() {
        return core::ptr::null_mut();
    }
    // SAFETY: the compatibility callers use only pointers returned by the
    // allocator. Foreign pointers fail validation and produce null.
    unsafe { HEAP.reallocate_zeroed(ptr.cast(), size, alignment).cast() }
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
