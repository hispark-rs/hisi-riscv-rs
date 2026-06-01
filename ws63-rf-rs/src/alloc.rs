//! Heap-backed `osal_kmalloc` / `osal_kfree` (ws63-RF `port_osal.h`).
//!
//! A real first-fit heap ([`linked_list_allocator`]) over a static SRAM pool,
//! guarded by a critical section. `osal_kmalloc` returns zero-initialised,
//! 8-byte-aligned memory (the contract's "non-pageable, zero-initialized"
//! semantics); each allocation is prefixed with an 8-byte size header so
//! `osal_kfree` (which gets only a pointer) can recover the layout.

use core::alloc::Layout;
use core::cell::RefCell;
use core::ffi::c_void;
use critical_section::Mutex;
use linked_list_allocator::Heap;

/// Scaffold heap size. The full Wi-Fi stack needs ~512 KB (the C SDK "local
/// memory pool"); this is sized for the porting-layer smoke test.
/// TODO(phase 4): back this with a reserved SRAM region sized from the C SDK.
const HEAP_SIZE: usize = 64 * 1024;
/// Allocation alignment and size-header width.
const HDR: usize = 8;

#[repr(align(8))]
struct Pool([u8; HEAP_SIZE]);
static mut HEAP_POOL: Pool = Pool([0; HEAP_SIZE]);

static HEAP: Mutex<RefCell<Heap>> = Mutex::new(RefCell::new(Heap::empty()));

/// Allocate `size` zero-initialised bytes. Returns null on failure / `size==0`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kmalloc(size: usize) -> *mut c_void {
    if size == 0 {
        return core::ptr::null_mut();
    }
    let total = match size.checked_add(HDR) {
        Some(t) => t,
        None => return core::ptr::null_mut(),
    };
    let layout = match Layout::from_size_align(total, HDR) {
        Ok(l) => l,
        Err(_) => return core::ptr::null_mut(),
    };

    critical_section::with(|cs| {
        let mut heap = HEAP.borrow_ref_mut(cs);
        if heap.size() == 0 {
            // SAFETY: one-time init of the static pool; single-hart + in a
            // critical section, so no aliasing/race.
            unsafe { heap.init((&raw mut HEAP_POOL.0).cast::<u8>(), HEAP_SIZE) };
        }
        match heap.allocate_first_fit(layout) {
            Ok(base) => {
                let base = base.as_ptr();
                // SAFETY: base..base+total is owned by this allocation.
                unsafe {
                    (base as *mut usize).write(total);
                    let user = base.add(HDR);
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
pub extern "C" fn osal_kfree(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: ptr came from osal_kmalloc, so the 8-byte size header sits just
    // before it and records the original total allocation size.
    unsafe {
        let base = (ptr as *mut u8).sub(HDR);
        let total = (base as *const usize).read();
        let layout = Layout::from_size_align_unchecked(total, HDR);
        let nn = core::ptr::NonNull::new_unchecked(base);
        critical_section::with(|cs| HEAP.borrow_ref_mut(cs).deallocate(nn, layout));
    }
}
