//! Heap-backed `osal_kmalloc` / `osal_kfree` (ws63-RF `port_osal.h`).
//!
//! A real first-fit heap ([`linked_list_allocator`]) over the SRAM remainder
//! exported by the WS63 runtime linker contract, guarded by a critical section.
//! `osal_kmalloc` returns zero-initialised,
//! 8-byte-aligned memory (the contract's "non-pageable, zero-initialized"
//! semantics); each allocation is prefixed with an 8-byte size + ownership
//! header so `osal_kfree` (which gets only a pointer) can recover and validate
//! the layout before touching the allocator.

use core::alloc::Layout;
use core::cell::RefCell;
use core::ffi::c_void;
use critical_section::Mutex;
use linked_list_allocator::Heap;

#[repr(C, align(8))]
struct AllocationHeader {
    total: u32,
    alignment: u32,
    base_offset: u32,
    magic: u32,
}

const HDR: usize = core::mem::size_of::<AllocationHeader>();
const ALLOC_MAGIC: u32 = 0xA110_CA7E;
const FREED_MAGIC: u32 = 0xF4EE_D000;

unsafe extern "C" {
    static mut __heap_start__: u8;
    static mut __heap_end__: u8;
}

static HEAP: Mutex<RefCell<Heap>> = Mutex::new(RefCell::new(Heap::empty()));

/// Allocate `size` zero-initialised bytes. Returns null on failure / `size==0`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kmalloc(size: usize) -> *mut c_void {
    allocate_aligned(size, HDR)
}

/// Allocate memory with the alignment required by crypto/DMA buffers.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kmalloc_align(size: u32, _flags: u32, boundary: u32) -> *mut c_void {
    let alignment = boundary as usize;
    if alignment < HDR || !alignment.is_power_of_two() {
        return core::ptr::null_mut();
    }
    allocate_aligned(size as usize, alignment)
}

fn allocate_aligned(size: usize, alignment: usize) -> *mut c_void {
    if size == 0 {
        return core::ptr::null_mut();
    }
    let total = match size
        .checked_add(HDR)
        .and_then(|value| value.checked_add(alignment - 1))
    {
        Some(t) => t,
        None => return core::ptr::null_mut(),
    };
    let layout = match Layout::from_size_align(total, alignment) {
        Ok(l) => l,
        Err(_) => return core::ptr::null_mut(),
    };

    critical_section::with(|cs| {
        let mut heap = HEAP.borrow_ref_mut(cs);
        if heap.size() == 0 {
            let start = &raw mut __heap_start__;
            let end = &raw mut __heap_end__;
            let len = end as usize - start as usize;
            // SAFETY: the linker reserves start..end exclusively for the heap;
            // one-time initialization is serialized by this critical section.
            unsafe { heap.init(start, len) };
        }
        match heap.allocate_first_fit(layout) {
            Ok(base) => {
                let base = base.as_ptr();
                let user_addr = (base as usize + HDR + alignment - 1) & !(alignment - 1);
                let user = user_addr as *mut u8;
                let header = unsafe { user.sub(HDR).cast::<AllocationHeader>() };
                // SAFETY: base..base+total is owned by this allocation.
                unsafe {
                    header.write(AllocationHeader {
                        total: total as u32,
                        alignment: alignment as u32,
                        base_offset: header.cast::<u8>().offset_from(base) as u32,
                        magic: ALLOC_MAGIC,
                    });
                    core::ptr::write_bytes(user, 0, size);
                    user as *mut c_void
                }
            }
            Err(_) => core::ptr::null_mut(),
        }
    })
}

/// Free memory returned by [`osal_kmalloc`]. No-op on null.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn osal_kfree(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }

    free_owned(ptr);
}

/// Resize an owned allocation, preserving the common prefix.
pub(crate) fn realloc_owned(ptr: *mut c_void, size: usize) -> *mut c_void {
    if ptr.is_null() {
        return osal_kmalloc(size);
    }
    if size == 0 {
        osal_kfree(ptr);
        return core::ptr::null_mut();
    }
    let old_size = unsafe {
        let header = (ptr as *mut u8).sub(HDR) as *const AllocationHeader;
        if (*header).magic != ALLOC_MAGIC || (*header).total < HDR as u32 {
            return core::ptr::null_mut();
        }
        (*header).total as usize - (*header).base_offset as usize - HDR
    };
    let replacement = osal_kmalloc(size);
    if replacement.is_null() {
        return replacement;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(
            ptr.cast::<u8>(),
            replacement.cast::<u8>(),
            old_size.min(size),
        );
    }
    osal_kfree(ptr);
    replacement
}

fn free_owned(ptr: *mut c_void) {
    let caller = caller_address();

    // A C ABI caller can hand us a foreign, interior, or already-freed pointer.
    // Validate the ownership header before constructing a Layout; passing a
    // fabricated layout into linked_list_allocator would corrupt the heap.
    unsafe {
        let heap_start = (&raw mut __heap_start__) as usize;
        let heap_end = (&raw mut __heap_end__) as usize;
        let user = ptr as usize;
        if user < heap_start.saturating_add(HDR) || user > heap_end || !user.is_multiple_of(HDR) {
            trace_bad_free(user, 0, 0, caller);
            return;
        }

        let header = user.wrapping_sub(HDR) as *mut AllocationHeader;
        let total = (*header).total as usize;
        let base_offset = (*header).base_offset as usize;
        let magic = (*header).magic;
        let base = (header as usize).wrapping_sub(base_offset) as *mut u8;
        let alignment = (*header).alignment as usize;
        let valid = magic == ALLOC_MAGIC
            && total >= HDR
            && total <= heap_end.saturating_sub(base as usize)
            && alignment >= HDR
            && alignment.is_power_of_two()
            && (base as usize).is_multiple_of(alignment);

        if !valid {
            trace_bad_free(ptr as usize, total, magic, caller);
            return;
        }

        (*header).magic = FREED_MAGIC;
        let layout = Layout::from_size_align_unchecked(total, alignment);
        let nn = core::ptr::NonNull::new_unchecked(base);
        critical_section::with(|cs| HEAP.borrow_ref_mut(cs).deallocate(nn, layout));
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

fn trace_bad_free(ptr: usize, total: usize, magic: u32, caller: usize) {
    #[cfg(feature = "rf-init-diag")]
    crate::rf_init_diag::trace_bad_free(ptr as u32, total as u32, magic, caller as u32);

    #[cfg(not(feature = "rf-init-diag"))]
    let _ = (ptr, total, magic, caller);
}
