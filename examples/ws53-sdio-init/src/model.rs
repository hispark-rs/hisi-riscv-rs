//! Host-controlled RAM fixture for the actual HAL. Never load on a board.
#![no_std]
#![no_main]
use core::{num::NonZeroU16, ptr};
use hisi_hal::{
    Peripherals,
    sdio::{EventSnapshot, InitError, InitProgress, Initialization},
};
use hisi_riscv_rt::entry;

// All words are ordinary application RAM. The model runner supplies commands
// only here, and supplies host clock/events via the virtual host inputs.
// It does not configure SDIO MMIO, set CPU registers or bypass the Rust HAL.
// [command, response_seq, admission, progress, error, global, fn0, fn1,
//  message_present, message, callback_count, delay_count, panic]
#[unsafe(no_mangle)]
pub static mut WS53_SDIO_INIT_MODEL: [u32; 13] = [0; 13];
fn put(index: usize, value: u32) {
    unsafe {
        ptr::addr_of_mut!(WS53_SDIO_INIT_MODEL)
            .cast::<u32>()
            .add(index)
            .write_volatile(value)
    }
}
fn get(index: usize) -> u32 {
    unsafe {
        ptr::addr_of!(WS53_SDIO_INIT_MODEL)
            .cast::<u32>()
            .add(index)
            .read_volatile()
    }
}
fn snapshot(event: EventSnapshot) {
    put(5, event.global);
    put(6, event.function0);
    put(7, event.function1.bits());
    put(8, u32::from(event.host_message.is_some()));
    put(9, event.host_message.unwrap_or(0));
}
fn progress(p: InitProgress) -> u32 {
    match p {
        InitProgress::WaitingHostClock => 1,
        InitProgress::WaitingFunctionEnable => 2,
        InitProgress::FunctionReady => 3,
    }
}
fn error(e: InitError) -> u32 {
    match e {
        InitError::ClockGateReadback => 1,
        InitError::DirtyStart { events, .. } => {
            snapshot(events);
            2
        }
        InitError::HostClockLost => 3,
        InitError::UnsupportedEvents(events) => {
            snapshot(events);
            4
        }
        InitError::ReadyReadback(events) => {
            snapshot(events);
            5
        }
        InitError::Timeout(p) => {
            put(3, progress(p));
            6
        }
        InitError::Cancelled => 7,
        InitError::Poisoned => 8,
    }
}
struct TestDelay;
impl embedded_hal::delay::DelayNs for TestDelay {
    fn delay_ns(&mut self, _: u32) {
        put(11, get(11) + 1);
    }
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // Test-only ownership: no competing core, local IRQ, active DMA or host
    // protocol exists in this explicit synthetic initialization profile.
    let token = unsafe { p.SDIO.assume_application_ownership() };
    let mut owner = match unsafe { Initialization::begin(token) } {
        Ok(v) => {
            put(2, 1);
            v
        }
        Err((e, v)) => {
            put(4, error(e));
            put(2, 2);
            v
        }
    };
    loop {
        let command = get(0);
        if command == 0 {
            core::hint::spin_loop();
            continue;
        }
        put(4, 0);
        match command {
            1 => match owner.poll() {
                Ok(result) => {
                    snapshot(result.events);
                    put(3, progress(result.progress));
                }
                Err(e) => put(4, error(e)),
            },
            2 => {
                if let Err(e) =
                    owner.wait_bounded(NonZeroU16::new(7).unwrap(), &mut TestDelay, |e| {
                        snapshot(e);
                        put(10, get(10) + 1);
                    })
                {
                    put(4, error(e));
                } else {
                    put(3, 3);
                }
            }
            3 => owner.cancel(),
            _ => put(4, 0xfffe),
        }
        put(0, 0);
        unsafe {
            core::arch::asm!("fence rw, rw", options(nostack));
        }
        put(1, get(1) + 1);
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    put(12, 1);
    loop {
        core::hint::spin_loop();
    }
}
