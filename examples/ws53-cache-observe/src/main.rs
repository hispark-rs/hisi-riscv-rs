//! Bounded RAM-only observation. Never writes cache/PMP/maintenance CSRs.
#![no_std]
#![no_main]
#[cfg(not(any(feature = "hil-controlled", feature = "model-only")))]
compile_error!("select a controlled HIL or model execution environment");
#[cfg(all(feature = "hil-controlled", feature = "model-only"))]
compile_error!("execution environments are mutually exclusive");

use core::ptr::{addr_of, addr_of_mut};
use hisi_hal::{
    Peripherals,
    gpio::{GpioBank, GpioDriver, Pad},
    interrupt,
    peripherals::Uart2,
    pinctrl::{Config, PadControl, Pull},
    soc::ws53::pins::MuxMode,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
};
use hisi_riscv_rt::{ExceptionContext, entry};

#[used]
#[unsafe(no_mangle)]
static mut WS53_CACHE_OBSERVE: [u32; 64] = [0; 64];
#[used]
#[unsafe(no_mangle)]
static WS53_CACHE_OBSERVE_ENV: u32 = if cfg!(feature = "hil-controlled") {
    0x484f4253
} else {
    0x4d4f4253
};
static mut DATA: u32 = 0x53cc0123;
static mut BSS: [u32; 4] = [0; 4];
const NONCE: u32 = 0x53cc0061;

core::arch::global_asm!(
    r#"
    .section .text.ws53_observe_csr,"ax"
    .option push
    .option norvc
    .macro OBSERVE_CSR name, number
    .balign 4
    .global \name
    .type \name,@function
\name:
    li a0, 0xfeedf00d
    .global \name\()_site
\name\()_site:
    csrr a0, \number
    ret
    .size \name, .-\name
    .endm
    OBSERVE_CSR obs_icctl, 0x7c0
    OBSERVE_CSR obs_aprefi, 0x7c6
    OBSERVE_CSR obs_dcctl, 0x7c1
    OBSERVE_CSR obs_inhibit, 0x320
    OBSERVE_CSR obs_cycle, 0xb00
    OBSERVE_CSR obs_cycleh, 0xb80
    OBSERVE_CSR obs_status, 0x300
    .option pop
"#
);
unsafe extern "C" {
    fn obs_icctl() -> u32;
    fn obs_aprefi() -> u32;
    fn obs_dcctl() -> u32;
    fn obs_inhibit() -> u32;
    fn obs_cycle() -> u32;
    fn obs_cycleh() -> u32;
    fn obs_status() -> u32;
    fn obs_icctl_site();
    fn obs_aprefi_site();
    fn obs_dcctl_site();
    fn obs_inhibit_site();
    fn obs_cycle_site();
    fn obs_cycleh_site();
    fn obs_status_site();
}
fn put(index: usize, value: u32) {
    unsafe {
        addr_of_mut!(WS53_CACHE_OBSERVE)
            .cast::<u32>()
            .add(index)
            .write_volatile(value)
    }
}
fn get(index: usize) -> u32 {
    unsafe {
        addr_of!(WS53_CACHE_OBSERVE)
            .cast::<u32>()
            .add(index)
            .read_volatile()
    }
}
fn fail(code: u32) -> ! {
    put(2, code);
    put(1, 0xee);
    loop {
        core::hint::spin_loop();
    }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn WS53_EXCEPTION(ctx: &mut ExceptionContext) {
    let sites = [
        obs_icctl_site as *const () as u32,
        obs_aprefi_site as *const () as u32,
        obs_dcctl_site as *const () as u32,
        obs_inhibit_site as *const () as u32,
        obs_cycle_site as *const () as u32,
        obs_cycleh_site as *const () as u32,
        obs_status_site as *const () as u32,
    ];
    let Some(index) = sites.iter().position(|&pc| pc == ctx.program_counter()) else {
        fail(90)
    };
    if ctx.cause() != 2 {
        fail(91)
    }
    put(4, get(4) | (1 << index));
    put(5, get(5) + 1);
    put(55, ctx.program_counter());
    put(56, ctx.trap_value());
    // Only these seven fixed, noncompressed CSRR sites are skippable. The
    // caller receives the sentinel and must consult the separate valid mask.
    unsafe { ctx.set_program_counter(ctx.program_counter() + 4) };
}
fn sample_csrs(start: usize) {
    let readers = [
        obs_icctl,
        obs_aprefi,
        obs_dcctl,
        obs_inhibit,
        obs_cycle,
        obs_cycleh,
        obs_status,
    ];
    for (index, read) in readers.iter().enumerate() {
        put(start + index, unsafe { read() });
    }
}
#[inline(never)]
fn busy(iterations: u32) -> u32 {
    let remaining: u32;
    unsafe {
        core::arch::asm!(
            ".option push", ".option norvc",
            "2: addi {n}, {n}, -1", "bnez {n}, 2b", ".option pop",
            n = inout(reg) iterations => remaining, options(nomem, nostack)
        )
    }
    remaining
}
fn emit(uart: &Uart<'_, Uart2<'_>>, data: &[u8]) {
    for &byte in data {
        let mut budget = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            if budget == 0 {
                fail(20)
            }
            budget -= 1;
        }
    }
}
fn hex(uart: &Uart<'_, Uart2<'_>>, value: u32) {
    for shift in (0..8).rev() {
        emit(
            uart,
            &[b"0123456789abcdef"[((value >> (shift * 4)) & 15) as usize]],
        );
    }
}
#[entry]
fn main() -> ! {
    if riscv::register::mstatus::read().mie() {
        fail(1)
    }
    put(0, 0x53c10001);
    put(3, unsafe {
        addr_of!(WS53_CACHE_OBSERVE_ENV).read_volatile()
    });
    put(48, unsafe { addr_of!(DATA).read_volatile() });
    put(49, unsafe { addr_of!(BSS).cast::<u32>().read_volatile() });
    let sp: u32;
    unsafe {
        core::arch::asm!("mv {sp}, sp", sp=out(reg)sp, options(nostack));
    }
    put(50, sp);
    if get(48) != 0x53cc0123 || get(49) != 0 {
        fail(2)
    }
    put(1, 1);
    while get(7) == 0 {
        core::hint::spin_loop();
    }
    if get(7) != NONCE {
        fail(3)
    }
    sample_csrs(8);
    let p = Peripherals::take().unwrap();
    let uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    let channel = TimerChannel::Channel1;
    if interrupt::is_enabled(channel.interrupt()) || interrupt::is_pending(channel.interrupt()) {
        fail(4)
    }
    let mut pad = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    let original = pad.configuration().unwrap();
    let old_mode = original.mode().mode().code();
    put(51, u32::from(old_mode));
    put(53, original.pull() as u32);
    put(54, u32::from(original.input_enabled()));
    if !matches!(old_mode, 0 | 2) || interrupt::is_enabled(GpioBank::Bank0.interrupt()) {
        fail(5)
    }
    if old_mode == 2
        && unsafe { &*ws53_pac::MCrg::PTR }
            .uart_soft_reset()
            .read()
            .uart_h0_reset_n()
            .bit()
    {
        fail(12)
    }
    pad.configure(
        Config::new(
            Pad::Mgpio17
                .select_mode(MuxMode::from_code(0).unwrap())
                .unwrap(),
            Pad::Mgpio17.drive_setting(0).unwrap(),
            Pull::Down,
            true,
            original.schmitt_enabled(),
        )
        .unwrap(),
    )
    .unwrap();
    let pin = pad.into_gpio().unwrap_or_else(|_| fail(6));
    let mut bank = GpioDriver::new_gpio0(unsafe { p.GPIO0.assume_application_ownership() });
    let mut output = bank.init_output(pin, false).unwrap_or_else(|_| fail(7));
    timer.configure(
        channel,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(channel);
    put(22, 32_000_000);
    put(23, 3);
    for (index, iterations) in [4096, 16384, 65536].into_iter().enumerate() {
        let base = 24 + 8 * index;
        put(base, iterations);
        // Separate variable-time diagnostic reads from the electrical busy
        // bracket. Cycle/timer deltas include their known call/read overhead.
        let high_before = unsafe { obs_cycleh() };
        let cycle_before = unsafe { obs_cycle() };
        let timer_before = timer.current_value(channel).unwrap();
        output.set_high();
        riscv::asm::fence();
        let remaining = busy(iterations);
        riscv::asm::fence();
        output.set_low();
        let timer_after = timer.current_value(channel).unwrap();
        let cycle_after = unsafe { obs_cycle() };
        let high_after = unsafe { obs_cycleh() };
        for (offset, value) in [
            timer_before,
            timer_after,
            cycle_before,
            cycle_after,
            high_before,
            high_after,
            remaining,
        ]
        .into_iter()
        .enumerate()
        {
            put(base + 1 + offset, value);
        }
        if remaining != 0 {
            fail(8)
        }
        let start = timer.current_value(channel).unwrap();
        for tries in 0..1_000_000 {
            if start.wrapping_sub(timer.current_value(channel).unwrap()) >= 32_000 {
                break;
            }
            if tries == 999_999 {
                fail(9)
            }
        }
    }
    timer.disable(channel);
    timer.clear_interrupt(channel);
    interrupt::clear_pending(channel.interrupt());
    let _pin = output.free();
    let mut restored = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    restored.configure(original).unwrap();
    if restored.configuration().unwrap() != original {
        fail(10)
    }
    put(52, 1);
    sample_csrs(15);
    for index in 0..4 {
        if get(8 + index) != get(15 + index) {
            fail(11)
        }
    }
    put(1, 2);
    emit(&uart, b"WS53 CACHE OBSERVE BEGIN\r\n");
    for row in 0..4 {
        emit(&uart, b"WS53 CACHE WORDS row=");
        hex(&uart, row);
        for index in 0..16 {
            emit(&uart, b" ");
            hex(&uart, get(16 * row as usize + index));
        }
        emit(&uart, b"\r\n");
    }
    emit(&uart, b"WS53 CACHE OBSERVE PASS readonly=1 markers=3\r\n");
    let mut budget = 1_000_000;
    while !uart.tx_flushed() {
        if budget == 0 {
            fail(21)
        }
        budget -= 1;
    }
    loop {
        core::hint::spin_loop();
    }
}
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    fail(99)
}
