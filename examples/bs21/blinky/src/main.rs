//! HiSilicon BS21 LED Blinky example.
//!
//! The BS21 analogue of `ws63-examples/blinky`: it toggles a GPIO using the same
//! chip-neutral HAL GPIO path ([`OutputConfig`] + the type-erased [`Output`]
//! driver), built with `--features chip-bs21` so the peripheral addresses come
//! from `bs2x-pac` and the clocks/counts from `soc/bs21.rs`.
//!
//! Milestone-1 target: boot end-to-end on the `bs21` QEMU machine. The busy-wait
//! is sized for the BS21 app core (64 MHz) rather than WS63's 240 MHz.

#![no_std]
#![no_main]

use hisi_riscv_hal::gpio::{AnyPin, OutputConfig};
use hisi_riscv_rt::entry;

/// Approximate busy-wait delay (~64 cycles ≈ 1 µs at the 64 MHz BS21 CPU clock).
fn delay_ms(ms: u32) {
    for _ in 0..ms {
        for _ in 0..64_000 {
            core::hint::spin_loop();
        }
    }
}

#[entry]
fn main() -> ! {
    // GPIO0 as a push-pull output starting low, built via the OutputConfig builder.
    // SAFETY: GPIO0 is a valid BS21 pin (0..32) and this example owns it exclusively.
    let mut led = unsafe { AnyPin::steal(0) }.init_output(OutputConfig::new().with_initial(false));

    loop {
        led.set_high();
        delay_ms(500);
        led.set_low();
        delay_ms(500);
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
