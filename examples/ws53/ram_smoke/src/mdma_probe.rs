//! Bounded RAM observation, NOT a physical DMA drain or buffer handback proof.
//! HIL checks locked PMP region5/noncacheable DTCM, stops old vendor masters and
//! verifies the whole application RAM image. Buffers remain retained forever;
//! only the halted debugger observes bytes, then whole-board reset recovers.
#[cfg(not(feature = "mdma-async-smoke"))]
use core::num::NonZeroU16;
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    dma::{Buffer, Controller, Error, Transfer},
    peripherals::{Mdma, Uart2},
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
};
#[cfg(all(feature = "mdma-width8", feature = "mdma-width16"))]
compile_error!("choose one MDMA element width");
#[cfg(feature = "mdma-width8")]
type Word = u8;
#[cfg(all(feature = "mdma-width16", not(feature = "mdma-width8")))]
type Word = u16;
#[cfg(not(any(feature = "mdma-width8", feature = "mdma-width16")))]
type Word = u32;
#[cfg(all(feature = "mdma-irq-probe", feature = "mdma-direct-probe"))]
compile_error!("MDMA IRQ and legacy direct diagnostic are separate fixtures");
#[cfg(feature = "mdma-async-smoke")]
#[path = "mdma_async.rs"]
mod asynch;
#[cfg(feature = "mdma-irq-probe")]
#[path = "mdma_irq.rs"]
mod irq;
#[cfg(all(
    feature = "mdma-async-smoke",
    any(feature = "mdma-irq-probe", feature = "mdma-direct-probe")
))]
compile_error!("choose one MDMA fixture");
const CHANNELS: usize = if cfg!(feature = "mdma-direct-probe") {
    8
} else {
    4
};
#[unsafe(no_mangle)]
static mut WS53_MDMA_SRC: [[Word; 19]; CHANNELS] = [[0x5a; 19]; CHANNELS];
#[unsafe(no_mangle)]
static mut WS53_MDMA_DST: [[Word; 19]; CHANNELS] = [[0x5a; 19]; CHANNELS];
struct Delay<'a, 'd>(&'a mut TimerDriver<'d>);
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.0.current_value(TimerChannel::Channel1).unwrap();
        for _ in 0..10_000_000 {
            if start.wrapping_sub(self.0.current_value(TimerChannel::Channel1).unwrap()) >= ticks {
                return;
            }
        }
        panic!("independent TIMER stopped");
    }
}
fn hex(uart: &Uart<'_, Uart2<'_>>, v: u32) {
    for shift in (0..8).rev() {
        super::emit(
            uart,
            &[b"0123456789abcdef"[((v >> (shift * 4)) & 15) as usize]],
        );
    }
}
#[cfg(not(feature = "mdma-async-smoke"))]
fn result_code(result: Result<(), Error>) -> u32 {
    match result {
        Ok(()) => 0,
        Err(Error::Bus) => 1,
        Err(Error::Timeout) => 2,
        Err(Error::CompletionState) => 3,
        Err(Error::Configuration) => 4,
        Err(Error::StaleEvent) => 5,
        Err(Error::Occupied) => 6,
        _ => 7,
    }
}
pub fn check(token: Mdma<'_>, timer: &mut TimerDriver<'_>, uart: &Uart<'_, Uart2<'_>>) {
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let mut delay = Delay(timer);
    #[cfg(feature = "mdma-direct-probe")]
    let channels = Controller::new(token).split().ok().unwrap();
    #[cfg(feature = "mdma-direct-probe")]
    let channels = [
        channels.channel0,
        channels.channel1,
        channels.channel2,
        channels.channel3,
        channels.channel4,
        channels.channel5,
        channels.channel6,
        channels.channel7,
    ];
    #[cfg(not(feature = "mdma-direct-probe"))]
    let channels = {
        let c = Controller::new(token).split_four().ok().unwrap();
        [c.channel0, c.channel1, c.channel2, c.channel3]
    };
    let mut transfers: [Option<Transfer<'_, Word>>; CHANNELS] = [const { None }; CHANNELS];
    let mut results = [0u32; CHANNELS];
    for (ch, channel) in channels.into_iter().enumerate() {
        unsafe {
            let src = core::ptr::addr_of_mut!(WS53_MDMA_SRC)
                .cast::<Word>()
                .add(ch * 19 + 1);
            let dst = core::ptr::addr_of_mut!(WS53_MDMA_DST)
                .cast::<Word>()
                .add(ch * 19 + 1);
            for i in 0..17 {
                src.add(i).write((ch * 29 + i * 13 + 7) as Word);
            }
            let source = Buffer::from_coherent_static(core::slice::from_raw_parts_mut(src, 17))
                .ok()
                .unwrap();
            let dest = Buffer::from_coherent_static(core::slice::from_raw_parts_mut(dst, 17))
                .ok()
                .unwrap();
            transfers[ch] = Some(Transfer::new(channel, source, dest).ok().unwrap());
        }
    }
    super::emit(uart, b"WS53 MDMA BEGIN\r\n");
    #[cfg(feature = "mdma-irq-probe")]
    let irq_good = irq::check(&mut transfers, &mut results, &mut delay, uart);
    #[cfg(feature = "mdma-async-fault")]
    let irq_good = asynch::fault_check(&mut transfers, &mut results, &mut delay, uart);
    #[cfg(all(feature = "mdma-async-smoke", not(feature = "mdma-async-fault")))]
    let irq_good = asynch::check(&mut transfers, &mut results, &mut delay);
    #[cfg(not(any(feature = "mdma-irq-probe", feature = "mdma-async-smoke")))]
    let irq_good = true;
    #[cfg(not(any(feature = "mdma-irq-probe", feature = "mdma-async-smoke")))]
    {
        for (i, t) in transfers.iter_mut().enumerate() {
            results[i] = result_code(t.as_mut().unwrap().start());
        }
        for (i, t) in transfers.iter_mut().enumerate() {
            if results[i] == 0 {
                results[i] = result_code(
                    t.as_mut()
                        .unwrap()
                        .wait(NonZeroU16::new(1000).unwrap(), &mut delay)
                        .and_then(|done| {
                            if done.elements() == 17 {
                                Ok(())
                            } else {
                                Err(Error::CompletionState)
                            }
                        }),
                );
            }
        }
    }
    // Never call release_after_external_quiescence: neither completion nor a
    // quiet observation interval proves absence of future posted bus writes.
    core::mem::forget(transfers);
    #[cfg(feature = "mdma-direct-probe")]
    {
        // Diagnostic only: source SDK writes transfer_size before ENABLE,
        // without requiring count readback. Channel0 and its static buffers
        // remain reserved; no allocation is returned to Rust or reused.
        let regs = unsafe { &*ws53_pac::Mdma::PTR };
        let ch = regs.channel(0);
        assert!(
            !ch.configuration().read().enable().bit() && !ch.configuration().read().active().bit()
        );
        assert_eq!(
            ch.source().read().address().bits(),
            core::ptr::addr_of!(WS53_MDMA_SRC) as u32 + core::mem::size_of::<Word>() as u32
        );
        assert_eq!(
            ch.destination().read().address().bits(),
            core::ptr::addr_of!(WS53_MDMA_DST) as u32 + core::mem::size_of::<Word>() as u32
        );
        ch.control()
            .modify(|_, w| unsafe { w.transfer_size().bits(17) });
        unsafe {
            core::arch::asm!("fence iorw, iorw", options(nostack));
        }
        ch.configuration()
            .modify(|_, w| w.halt().clear_bit().enable().set_bit());
        delay.delay_ms(2);
        super::emit(uart, b"WS53 MDMA DIRECT ");
        for v in [
            regs.raw_interrupt_status().read().bits(),
            ch.control().read().bits(),
            ch.configuration().read().bits(),
            ch.source().read().bits(),
            ch.destination().read().bits(),
        ] {
            hex(uart, v);
        }
        super::emit(uart, b"\r\n");
        ch.configuration()
            .modify(|_, w| w.halt().set_bit().enable().clear_bit());
    }
    timer.disable(TimerChannel::Channel1);
    super::emit(uart, b"WS53 MDMA RESULTS ");
    for code in results {
        hex(uart, code);
    }
    super::emit(uart, b"\r\n");
    if results == [0; CHANNELS] && irq_good {
        #[cfg(all(feature = "mdma-async-smoke", not(feature = "mdma-async-fault")))]
        super::emit(
            uart,
            b"WS53 PASS mdma_async irq=58 channels=4 wakes=3 cancels=2 guards=12 retained=1\r\n",
        );
        #[cfg(feature = "mdma-irq-probe")]
        super::emit(
            uart,
            b"WS53 PASS mdma_irq=58 notifications=4 mask=1 selective_ack=1 retained=1\r\n",
        );
        #[cfg(feature = "mdma-async-fault")]
        super::emit(
            uart,
            b"WS53 PASS mdma_async_fault errors=2 cancels=2 posted=1 retained=1\r\n",
        );
        #[cfg(not(feature = "mdma-async-fault"))]
        super::emit(
            uart,
            if CHANNELS == 4 {
                b"WS53 PASS mdma channels=4 elements=17 retained=1\r\n"
            } else {
                b"WS53 PASS mdma channels=8 elements=17 retained=1\r\n"
            },
        );
    } else {
        super::emit(uart, b"WS53 FAIL mdma observations\r\n");
    }
}
