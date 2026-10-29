//! Controlled TRUE synchronous faults; never fake trap CSR entry.
#![no_std]
#![no_main]
#[cfg(not(any(feature = "qemu-only", feature = "hil-controlled")))]
compile_error!("select qemu-only or hil-controlled with the RAM handoff coordinator");
#[cfg(all(feature = "qemu-only", feature = "hil-controlled"))]
compile_error!("model-only and hardware-controlled profiles are mutually exclusive");
#[cfg(all(
    feature = "hil-controlled",
    any(
        feature = "legacy-hooks",
        feature = "named-stop",
        feature = "default-stop",
        feature = "default-halt"
    )
))]
compile_error!("initial controlled HIL admits only typed and IRQ profiles");
#[cfg(feature = "flash-autonomous")]
compile_error!("Flash uses the independent ws53-flash-exceptions workspace");
mod exercise;
use hisi_riscv_rt::entry;
#[entry]
fn main() -> ! {
    exercise::run_probes();
    #[cfg(feature = "irq-exception")]
    {
        let p = hisi_hal::Peripherals::take().unwrap();
        let mut timer =
            hisi_hal::timer::TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
        exercise::irq_probe(&mut timer);
    }
    exercise::finish();
    exercise::park()
}
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    exercise::fail()
}
