//! IRQ58 notification is separate from transfer validation and memory ownership.
use super::*;
use core::cell::Cell;
use critical_section::Mutex;
use hisi_hal::{
    dma::{self, ChannelId, InterruptEvents},
    interrupt,
};

static OBSERVED: Mutex<Cell<(u32, u32, u32)>> = Mutex::new(Cell::new((0, 0, 0)));
const IDS: [ChannelId; 4] = [
    ChannelId::Channel0,
    ChannelId::Channel1,
    ChannelId::Channel2,
    ChannelId::Channel3,
];
#[unsafe(no_mangle)]
extern "C" fn M_DMA() {
    unsafe {
        interrupt::disable(interrupt::Interrupt::M_DMA);
    }
    let mut complete = 0;
    let mut error = 0;
    for (i, id) in IDS.into_iter().enumerate() {
        let s = dma::interrupt_status(id);
        complete |= u32::from(s.complete) << i;
        error |= u32::from(s.error) << i;
    }
    critical_section::with(|cs| {
        let old = OBSERVED.borrow(cs).get();
        OBSERVED
            .borrow(cs)
            .set((old.0.saturating_add(1), complete, error));
    });
}
fn observed() -> (u32, u32, u32) {
    critical_section::with(|cs| OBSERVED.borrow(cs).get())
}
fn raw() -> u32 {
    unsafe { &*ws53_pac::Mdma::PTR }
        .raw_interrupt_status()
        .read()
        .bits()
}
fn arm() {
    interrupt::clear_pending(interrupt::Interrupt::M_DMA);
    unsafe {
        interrupt::enable(interrupt::Interrupt::M_DMA);
    }
}
pub fn check(
    transfers: &mut [Option<Transfer<'_, Word>>; CHANNELS],
    results: &mut [u32; CHANNELS],
    delay: &mut impl DelayNs,
    uart: &Uart<'_, Uart2<'_>>,
) -> bool {
    assert!(!riscv::register::mstatus::read().mie());
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    arm();
    unsafe {
        riscv::interrupt::enable();
    }
    let mut good = true;
    for i in 0..4 {
        let t = transfers[i].as_mut().unwrap();
        if i == 1 || i == 3 {
            t.listen(InterruptEvents::All).unwrap();
        }
        if i == 2 {
            t.listen(InterruptEvents::Error).unwrap();
        }
        if i == 3 {
            unsafe {
                interrupt::disable(interrupt::Interrupt::M_DMA);
            }
        }
        results[i] = result_code(t.start());
        if results[i] != 0 {
            good = false;
            break;
        }
        delay.delay_ms(2);
        if i == 0 || i == 2 {
            // Completion stays raw while its peripheral source is masked.
            good &= observed().0 == i as u32 && raw() & (1 << i) != 0;
            good &= !dma::interrupt_status(IDS[i]).complete;
            t.listen(InterruptEvents::Completion).unwrap();
        } else if i == 3 {
            good &= observed().0 == 3 && dma::interrupt_status(IDS[i]).complete;
            arm();
        }
        for _ in 0..1000 {
            if observed().0 > i as u32 {
                break;
            }
            delay.delay_us(1);
        }
        let (notifications, complete, errors) = observed();
        good &= notifications == i as u32 + 1 && complete == 1 << i && errors == 0;
        results[i] = result_code(
            t.wait(NonZeroU16::new(1000).unwrap(), delay)
                .and_then(|done| {
                    if done.elements() == 17 {
                        Ok(())
                    } else {
                        Err(Error::CompletionState)
                    }
                }),
        );
        let before = raw();
        if i == 1 {
            // Error W1C must not discard completion or another channel's event.
            t.acknowledge(InterruptEvents::Error).unwrap();
            good &= raw() == before;
        }
        if i != 0 {
            t.acknowledge(InterruptEvents::All).unwrap();
        }
        let after = raw();
        good &= before == [1, 3, 4, 8][i] && after == [1, 1, 0, 0][i];
        super::super::emit(uart, b"WS53 MDMA IRQ ");
        for value in [notifications, complete, errors, before, after] {
            hex(uart, value);
        }
        super::super::emit(uart, b"\r\n");
        if i == 1 {
            transfers[0]
                .as_mut()
                .unwrap()
                .acknowledge(InterruptEvents::Completion)
                .unwrap();
        }
        arm();
        delay.delay_ms(2);
        good &= observed().0 == notifications;
    }
    riscv::interrupt::disable();
    unsafe {
        interrupt::disable(interrupt::Interrupt::M_DMA);
    }
    good && raw() == 0 && observed().0 == 4
}
