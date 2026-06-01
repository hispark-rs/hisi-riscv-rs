//! Cooperative task scheduler — the runtime under the WS63 OSAL contract.
//!
//! Modeled on esp-rtos's scheduler (TCB + context switch + ready queue +
//! blocking primitives), adapted for the WS63 app core (single-hart
//! `rv32imfc`). Key difference from esp32c3 (`rv32imc`): WS63 has the **F**
//! extension, so the context switch must also save/restore the callee-saved FP
//! registers `fs0..fs11` (the WiFi blob does floating-point RF math).
//!
//! This is a **cooperative** scheduler: a task runs until it calls
//! [`yield_now`], blocks on a [`Semaphore`], or [`sleep_ms`]s. That matches the
//! vendor WiFi worker-thread model (it waits on semaphores / sleeps). Preemptive
//! time-slicing (a timer ISR driving the switch) is a follow-on; the cooperative
//! core is what the blob's `osal_kthread_*` / `osal_sem_*` / `osal_wait_*` /
//! `osal_msleep` need.
//!
//! Layering: this module depends only on `core` + the crate's heap; nothing
//! above (no net stack, no wifi) — the dependency is strictly downward.

use crate::alloc::{osal_kfree, osal_kmalloc};
use core::cell::{RefCell, UnsafeCell};
use core::ffi::c_void;
use critical_section::Mutex;

/// Max concurrent tasks (slot table; the WiFi stack uses only a few).
const MAX_TASKS: usize = 16;
/// Minimum task stack (bytes), 16-byte aligned.
const MIN_STACK: usize = 4096;
/// Rough cycles/ms for sleep deadlines (uncalibrated; see `crate::osal`).
const CYCLES_PER_MS: u64 = 240_000;
/// Sentinel "no task" index for intrusive list links.
const NIL: usize = usize::MAX;

// ── Saved CPU context (offsets MUST match `context_switch` asm) ──────────────
#[repr(C)]
#[derive(Clone, Copy)]
struct Ctx {
    ra: usize,      // 0
    sp: usize,      // 4
    s: [usize; 12], // 8..56  (s0..s11)
    fs: [u32; 12],  // 56..104 (fs0..fs11, FLEN=32)
}
impl Ctx {
    const fn zero() -> Self {
        Ctx {
            ra: 0,
            sp: 0,
            s: [0; 12],
            fs: [0; 12],
        }
    }
}

/// Cooperative context switch: save callee-saved regs of the current task to
/// `*old`, restore `*new`, return into the new task. Caller-saved regs are
/// spilled by the compiler around this normal call, so only callee-saved
/// (ra, sp, s0-s11, fs0-fs11) need saving.
#[unsafe(naked)]
unsafe extern "C" fn context_switch(old: *mut Ctx, new: *const Ctx) {
    core::arch::naked_asm!(
        // Enable the F extension for the fsw/flw below (rv32imfc has it, but the
        // inline-asm assembler context defaults to a baseline without F).
        ".option arch, +f",
        // save current -> *old (a0)
        "sw  ra,  0(a0)",
        "sw  sp,  4(a0)",
        "sw  s0,  8(a0)",
        "sw  s1, 12(a0)",
        "sw  s2, 16(a0)",
        "sw  s3, 20(a0)",
        "sw  s4, 24(a0)",
        "sw  s5, 28(a0)",
        "sw  s6, 32(a0)",
        "sw  s7, 36(a0)",
        "sw  s8, 40(a0)",
        "sw  s9, 44(a0)",
        "sw  s10,48(a0)",
        "sw  s11,52(a0)",
        "fsw fs0, 56(a0)",
        "fsw fs1, 60(a0)",
        "fsw fs2, 64(a0)",
        "fsw fs3, 68(a0)",
        "fsw fs4, 72(a0)",
        "fsw fs5, 76(a0)",
        "fsw fs6, 80(a0)",
        "fsw fs7, 84(a0)",
        "fsw fs8, 88(a0)",
        "fsw fs9, 92(a0)",
        "fsw fs10,96(a0)",
        "fsw fs11,100(a0)",
        // restore *new (a1) -> current
        "lw  ra,  0(a1)",
        "lw  sp,  4(a1)",
        "lw  s0,  8(a1)",
        "lw  s1, 12(a1)",
        "lw  s2, 16(a1)",
        "lw  s3, 20(a1)",
        "lw  s4, 24(a1)",
        "lw  s5, 28(a1)",
        "lw  s6, 32(a1)",
        "lw  s7, 36(a1)",
        "lw  s8, 40(a1)",
        "lw  s9, 44(a1)",
        "lw  s10,48(a1)",
        "lw  s11,52(a1)",
        "flw fs0, 56(a1)",
        "flw fs1, 60(a1)",
        "flw fs2, 64(a1)",
        "flw fs3, 68(a1)",
        "flw fs4, 72(a1)",
        "flw fs5, 76(a1)",
        "flw fs6, 80(a1)",
        "flw fs7, 84(a1)",
        "flw fs8, 88(a1)",
        "flw fs9, 92(a1)",
        "flw fs10,96(a1)",
        "flw fs11,100(a1)",
        "ret",
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Free,
    Ready,
    Running,
    Blocked,
    Sleeping,
}

/// Task entry signature (matches the OSAL `osal_kthread_func`).
pub type TaskFn = extern "C" fn(*mut c_void) -> *mut c_void;

struct Tcb {
    ctx: Ctx,
    state: State,
    stack: usize, // heap allocation addr to free on exit (0 for the main task)
    entry: Option<TaskFn>,
    arg: usize,   // task argument (*mut c_void stored as usize so Tcb is Send)
    next: usize,  // intrusive link: ready queue OR one wait queue
    wake_at: u64, // mcycle deadline when Sleeping
}
impl Tcb {
    const fn empty() -> Self {
        Tcb {
            ctx: Ctx::zero(),
            state: State::Free,
            stack: 0,
            entry: None,
            arg: 0,
            next: NIL,
            wake_at: 0,
        }
    }
}

struct Sched {
    tasks: [Tcb; MAX_TASKS],
    current: usize,
    ready_head: usize,
    ready_tail: usize,
    started: bool,
}
impl Sched {
    const fn new() -> Self {
        const E: Tcb = Tcb::empty();
        Sched {
            tasks: [E; MAX_TASKS],
            current: 0,
            ready_head: NIL,
            ready_tail: NIL,
            started: false,
        }
    }
    fn ready_push(&mut self, i: usize) {
        self.tasks[i].next = NIL;
        if self.ready_tail == NIL {
            self.ready_head = i;
        } else {
            self.tasks[self.ready_tail].next = i;
        }
        self.ready_tail = i;
    }
    fn ready_pop(&mut self) -> usize {
        let i = self.ready_head;
        if i != NIL {
            self.ready_head = self.tasks[i].next;
            if self.ready_head == NIL {
                self.ready_tail = NIL;
            }
            self.tasks[i].next = NIL;
        }
        i
    }
    fn wake_sleepers(&mut self, now: u64) {
        for i in 0..MAX_TASKS {
            if self.tasks[i].state == State::Sleeping && now >= self.tasks[i].wake_at {
                self.tasks[i].state = State::Ready;
                self.ready_push(i);
            }
        }
    }
    fn alloc_slot(&mut self) -> Option<usize> {
        (0..MAX_TASKS).find(|&i| self.tasks[i].state == State::Free)
    }
}

static SCHED: Mutex<RefCell<Sched>> = Mutex::new(RefCell::new(Sched::new()));

fn now_cycles() -> u64 {
    #[cfg(target_arch = "riscv32")]
    {
        loop {
            let (hi1, lo, hi2): (u32, u32, u32);
            unsafe {
                core::arch::asm!("csrr {0}, mcycleh", out(reg) hi1, options(nomem, nostack));
                core::arch::asm!("csrr {0}, mcycle",  out(reg) lo,  options(nomem, nostack));
                core::arch::asm!("csrr {0}, mcycleh", out(reg) hi2, options(nomem, nostack));
            }
            if hi1 == hi2 {
                return ((hi1 as u64) << 32) | lo as u64;
            }
        }
    }
    #[cfg(not(target_arch = "riscv32"))]
    0
}

/// First-run trampoline: a freshly switched-to task lands here (its `ctx.ra`),
/// runs its entry, then exits. Reads its own entry/arg from the current TCB.
extern "C" fn trampoline() -> ! {
    let (entry, arg) = critical_section::with(|cs| {
        let s = SCHED.borrow_ref(cs);
        let t = &s.tasks[s.current];
        (t.entry, t.arg)
    });
    if let Some(f) = entry {
        f(arg as *mut c_void);
    }
    task_exit();
}

/// Initialize the scheduler, adopting the current execution as the main task
/// (slot 0). Idempotent.
pub fn init() {
    critical_section::with(|cs| {
        let s = &mut *SCHED.borrow_ref_mut(cs);
        if s.started {
            return;
        }
        s.tasks[0].state = State::Running;
        s.current = 0;
        s.started = true;
    });
}

/// Spawn a task. Returns its slot index, or `None` if the table/stack is full.
pub fn spawn(entry: TaskFn, arg: *mut c_void, stack_size: usize) -> Option<usize> {
    init();
    let size = stack_size.max(MIN_STACK);
    let stack = osal_kmalloc(size);
    if stack.is_null() {
        return None;
    }
    // 16-byte aligned stack top.
    let top = (stack as usize + size) & !0xf;
    critical_section::with(|cs| {
        let s = &mut *SCHED.borrow_ref_mut(cs);
        let i = match s.alloc_slot() {
            Some(i) => i,
            None => {
                osal_kfree(stack);
                return None;
            }
        };
        let t = &mut s.tasks[i];
        t.ctx = Ctx::zero();
        // Cast through a fn pointer (not a direct fn-item->int cast).
        let tramp: extern "C" fn() -> ! = trampoline;
        t.ctx.ra = tramp as usize;
        t.ctx.sp = top;
        t.state = State::Ready;
        t.stack = stack as usize;
        t.entry = Some(entry);
        t.arg = arg as usize;
        t.wake_at = 0;
        s.ready_push(i);
        Some(i)
    })
}

/// Switch away from `prev` to the next ready task, busy-idling (waking sleepers)
/// until one is runnable. `prev`'s state must already be set by the caller
/// (Ready+queued for yield, Blocked for a wait, Free for exit).
fn switch_away(prev: usize) {
    loop {
        let next = critical_section::with(|cs| {
            let s = &mut *SCHED.borrow_ref_mut(cs);
            s.wake_sleepers(now_cycles());
            s.ready_pop()
        });
        if next == NIL {
            core::hint::spin_loop();
            continue;
        }
        if next == prev {
            // Only runnable task is ourselves: keep running (re-mark Running).
            critical_section::with(|cs| {
                SCHED.borrow_ref_mut(cs).tasks[next].state = State::Running;
            });
            return;
        }
        let (op, np) = critical_section::with(|cs| {
            let s = &mut *SCHED.borrow_ref_mut(cs);
            s.tasks[next].state = State::Running;
            s.current = next;
            (
                core::ptr::addr_of_mut!(s.tasks[prev].ctx),
                core::ptr::addr_of!(s.tasks[next].ctx),
            )
        });
        // SAFETY: ctx live in the static SCHED (stable address); single-hart, the
        // lock is released so the resumed task can re-enter the scheduler.
        unsafe { context_switch(op, np) };
        return;
    }
}

/// Yield the CPU: requeue the current task and run the next ready one.
pub fn yield_now() {
    let prev = critical_section::with(|cs| {
        let s = &mut *SCHED.borrow_ref_mut(cs);
        let cur = s.current;
        s.tasks[cur].state = State::Ready;
        s.ready_push(cur);
        cur
    });
    switch_away(prev);
}

/// Sleep the current task for `ms` milliseconds (cooperative; wakes when a later
/// schedule sees the deadline pass).
pub fn sleep_ms(ms: u32) {
    if ms == 0 {
        yield_now();
        return;
    }
    let prev = critical_section::with(|cs| {
        let s = &mut *SCHED.borrow_ref_mut(cs);
        let cur = s.current;
        s.tasks[cur].state = State::Sleeping;
        s.tasks[cur].wake_at = now_cycles() + ms as u64 * CYCLES_PER_MS;
        cur
    });
    switch_away(prev);
}

/// Current task slot index (its "pid"/"tid").
pub fn current_id() -> usize {
    critical_section::with(|cs| SCHED.borrow_ref(cs).current)
}

fn task_exit() -> ! {
    // Mark the slot free and switch away forever. The stack is intentionally
    // leaked: we are still executing on it until `switch_away` transfers control,
    // and a single hart can't safely free the stack it is running on.
    // TODO: defer-free exited stacks from another task. The WiFi worker model
    // rarely exits tasks, so leaking here is acceptable for now.
    let prev = critical_section::with(|cs| {
        let s = &mut *SCHED.borrow_ref_mut(cs);
        let cur = s.current;
        s.tasks[cur] = Tcb::empty(); // -> Free (stack ptr dropped == leaked)
        cur
    });
    switch_away(prev);
    unreachable!()
}

// ── Counting semaphore (blocks via the scheduler) ───────────────────────────

/// A counting semaphore. Tasks block in [`Semaphore::down`] when the count is 0
/// and are woken by [`Semaphore::up`]. Backs `osal_sem_*` / `osal_wait_*` /
/// `osal_mutex_*`.
///
/// `&self` methods + interior mutability so it can be a `static` or heap object
/// shared across tasks; all state is touched only inside the scheduler critical
/// section (single-hart exclusive). Waiters are queued on the per-task `next`
/// link (a task is on at most one queue — ready OR one wait queue — at a time).
pub struct Semaphore {
    inner: UnsafeCell<SemState>,
}
struct SemState {
    count: i32,
    wait_head: usize,
    wait_tail: usize,
}
// SAFETY: `inner` is only accessed inside `critical_section::with` on a single
// hart, which serialises every access.
unsafe impl Sync for Semaphore {}

impl Semaphore {
    /// Create a semaphore with initial `count`.
    pub const fn new(count: i32) -> Self {
        Semaphore {
            inner: UnsafeCell::new(SemState {
                count,
                wait_head: NIL,
                wait_tail: NIL,
            }),
        }
    }

    /// Acquire (P). Consumes a count if available, else blocks until [`up`] hands
    /// one off. Direct-handoff semantics: being woken == being granted, so there
    /// is no re-check loop (the only thing that unblocks a waiter is `up`).
    ///
    /// [`up`]: Semaphore::up
    pub fn down(&self) {
        let block = critical_section::with(|cs| {
            let s = &mut *SCHED.borrow_ref_mut(cs);
            // SAFETY: exclusive under the critical section (single hart).
            let st = unsafe { &mut *self.inner.get() };
            if st.count > 0 {
                st.count -= 1;
                false
            } else {
                let cur = s.current;
                s.tasks[cur].state = State::Blocked;
                s.tasks[cur].next = NIL;
                if st.wait_tail == NIL {
                    st.wait_head = cur;
                } else {
                    s.tasks[st.wait_tail].next = cur;
                }
                st.wait_tail = cur;
                true
            }
        });
        if block {
            // Parked on this sem's wait queue; `up` will move us back to Ready
            // (== the grant). When we resume here, we already hold the count.
            switch_away(current_id());
        }
    }

    /// Acquire with a timeout (ms). Returns `true` if a count was obtained,
    /// `false` if the deadline passed first. `u32::MAX` (wait-forever) blocks
    /// like [`down`](Semaphore::down).
    ///
    /// Cooperative sleep-poll: re-checks `try_down`, then parks for 1 ms so the
    /// `up`-ing task (and the rest of the system) runs, until granted or expired.
    /// `mcycle` (the time base) advances in real time regardless, so the deadline
    /// is honoured even while parked.
    pub fn down_timeout(&self, timeout_ms: u32) -> bool {
        if timeout_ms == u32::MAX {
            self.down();
            return true;
        }
        let deadline = now_cycles() + timeout_ms as u64 * CYCLES_PER_MS;
        loop {
            if self.try_down() {
                return true;
            }
            if now_cycles() >= deadline {
                return false;
            }
            sleep_ms(1);
        }
    }

    /// Try to acquire without blocking. Returns true on success.
    pub fn try_down(&self) -> bool {
        critical_section::with(|_cs| {
            // SAFETY: exclusive under the critical section.
            let st = unsafe { &mut *self.inner.get() };
            if st.count > 0 {
                st.count -= 1;
                true
            } else {
                false
            }
        })
    }

    /// Release (V). Wakes one waiter if any, else increments the count.
    pub fn up(&self) {
        critical_section::with(|cs| {
            let s = &mut *SCHED.borrow_ref_mut(cs);
            // SAFETY: exclusive under the critical section.
            let st = unsafe { &mut *self.inner.get() };
            let w = st.wait_head;
            if w != NIL {
                st.wait_head = s.tasks[w].next;
                if st.wait_head == NIL {
                    st.wait_tail = NIL;
                }
                s.tasks[w].next = NIL;
                s.tasks[w].state = State::Ready;
                s.ready_push(w);
            } else {
                st.count += 1;
            }
        });
    }
}
