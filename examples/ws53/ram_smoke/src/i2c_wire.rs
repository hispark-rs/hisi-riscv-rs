//! Controlled RAM, no external responder: actual master address/NACK/STOP.
//! Source 163d9150 pinctrl comments: bus0 SGPIO2(MGPIO21)/MGPIO22 mode3;
//! bus1 MGPIO8/9 mode3. Only analyzer inputs are connected. No bus scan.
use super::*;
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::{
    delay::DelayNs,
    i2c::{Error as _, ErrorKind, NoAcknowledgeSource, Operation},
};
use hisi_hal::{
    gpio::{GpioDriver, Output},
    i2c::{Address, Config, Error, Failure, I2c, Pins, Timing},
    peripherals::{Gpio0, I2c0, I2c1, Unclaimed},
    pinctrl::{Config as PadConfig, PadControl, Pull},
    soc::ws53::pins::{GpioBank, MuxMode, Pad},
};

#[unsafe(no_mangle)]
pub static mut WS53_I2C_WIRE: [u32; 128] = [0; 128];
#[unsafe(no_mangle)]
pub static mut WS53_I2C_WIRE_BOUNDS: [u32; 16] = [0; 16];
pub fn boundaries() -> [u32; 16] {
    core::array::from_fn(|i| unsafe {
        core::ptr::addr_of!(WS53_I2C_WIRE_BOUNDS)
            .cast::<u32>()
            .add(i)
            .read_volatile()
    })
}
#[unsafe(no_mangle)]
pub static WS53_I2C_WIRE_BUS: u32 = cfg!(feature = "i2c-wire-bus1") as u32;
fn put(i: usize, v: u32) {
    unsafe {
        core::ptr::addr_of_mut!(WS53_I2C_WIRE)
            .cast::<u32>()
            .add(i)
            .write_volatile(v)
    }
}
fn now(cell: &RefCell<&mut TimerDriver<'_>>) -> u32 {
    cell.borrow_mut()
        .current_value(TimerChannel::Channel1)
        .unwrap()
}
#[derive(Clone, Copy)]
struct Delay<'a, 'b, 'd>(&'a RefCell<&'b mut TimerDriver<'d>>);
impl DelayNs for Delay<'_, '_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = now(self.0);
        for _ in 0..1_000_000 {
            if start.wrapping_sub(now(self.0)) >= ticks {
                return;
            }
        }
        panic!("I2C delay TIMER stopped")
    }
}
fn input(pad: Pad, value: Option<bool>) -> bool {
    let line = pad.gpio_line().unwrap();
    let mask = line.mask();
    macro_rules! update {
        ($pac:ident) => {{
            let r = unsafe { &*ws53_pac::$pac::PTR };
            let before = r.gpio_sw_oen().read().pins().bits() as u32;
            r.gpio_sw_oen().modify(|old, w| unsafe {
                let n = old.pins().bits() as u32;
                w.pins().bits(
                    (if value.unwrap_or(true) {
                        n | mask
                    } else {
                        n & !mask
                    }) as _,
                )
            });
            before & mask != 0
        }};
    }
    match line.bank() {
        GpioBank::Bank0 => update!(Gpio0),
        GpioBank::Bank2 => update!(Gpio2),
        _ => panic!("unselected I2C bank"),
    }
}
fn prepare(pad: Pad, mode: u8) -> (PadControl<'static>, PadConfig, bool) {
    let direction = input(pad, None);
    let mut owner = unsafe { PadControl::claim(pad) }.unwrap();
    let old = owner.configuration().unwrap();
    owner
        .configure(
            PadConfig::new(
                pad.select_mode(MuxMode::from_code(mode).unwrap()).unwrap(),
                pad.drive_setting(0).unwrap(),
                Pull::Up,
                true,
                old.schmitt_enabled(),
            )
            .unwrap(),
        )
        .unwrap();
    (owner, old, direction)
}
fn restore(mut pad: PadControl<'_>, saved: PadConfig, direction: bool) {
    pad.configure(saved).unwrap();
    assert_eq!(pad.configuration().unwrap(), saved);
    input(pad.pad(), Some(direction));
}
#[derive(Clone, Copy)]
enum Transfer {
    Write7,
    Read7,
    Write10,
    Empty,
}
fn run<T, D: DelayNs>(
    bus: &mut I2c<'_, T, D>,
    cell: &RefCell<&mut TimerDriver<'_>>,
    marker: &mut Output<'_, '_, '_>,
    phase: usize,
    hz: u32,
    transfer: Transfer,
) {
    let read = matches!(transfer, Transfer::Read7);
    let ten = matches!(transfer, Transfer::Write10);
    let empty = matches!(transfer, Transfer::Empty);
    if phase == 6 {
        bus.recover().unwrap();
    }
    let address = if ten {
        Address::ten(0x2aa).unwrap()
    } else {
        Address::seven(0x55).unwrap()
    };
    let mut byte = [0xcc];
    let before = now(cell);
    marker.set_high();
    riscv::asm::fence();
    let post_high = now(cell);
    let result = if empty {
        bus.transaction(address, &mut [])
    } else if read {
        bus.transaction(address, &mut [Operation::Read(&mut byte)])
    } else {
        bus.transaction(address, &mut [Operation::Write(&[0xa5])])
    };
    let (outcome, source, detail) = match result {
        Ok(()) => (0, 0, 0),
        Err(e) => {
            let code = if e.kind() == ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address) {
                1
            } else {
                2
            };
            match e {
                Error::Failure(Failure::Abort { source, detail }) => (code, source, detail),
                _ => (3, 0, 0),
            }
        }
    };
    if empty {
        Delay(cell).delay_us(100);
    }
    let pre_low = now(cell);
    riscv::asm::fence();
    marker.set_low();
    riscv::asm::fence();
    let after = now(cell);
    // Diagnostic stores occur after the last timestamp, outside the bracket.
    unsafe {
        let bounds = core::ptr::addr_of_mut!(WS53_I2C_WIRE_BOUNDS).cast::<u32>();
        bounds.add(phase * 2).write_volatile(post_high);
        bounds.add(phase * 2 + 1).write_volatile(pre_low);
    }
    for (j, v) in [
        phase as u32,
        hz,
        if ten { 0x2aa } else { 0x55 },
        u32::from(read),
        u32::from(ten),
        outcome,
        source,
        detail,
        u32::from(bus.is_poisoned()),
        before,
        after,
        before.wrapping_sub(after),
    ]
    .into_iter()
    .enumerate()
    {
        put(32 + phase * 12 + j, v)
    }
    put(1, phase as u32 + 1);
    if outcome != u32::from(!empty) || bus.is_poisoned() || byte != [0xcc] {
        put(2, 20 + phase as u32);
        panic!("I2C physical result differs")
    }
    // A fresh empty operation must be accepted after each automatic cleanup.
    bus.transaction(address, &mut []).unwrap();
    Delay(cell).delay_ms(2);
}
pub fn check(
    i0: Unclaimed<I2c0<'static>>,
    i1: Unclaimed<I2c1<'static>>,
    gpio: Unclaimed<Gpio0<'static>>,
    timer: &mut TimerDriver<'_>,
    uart: &Uart<'_, Uart2<'_>>,
) {
    // Preserve the RAM ELF's read-only mode witness. Flash uses the same
    // implementation with explicit typed instance tokens for both controllers.
    let bus = unsafe { core::ptr::addr_of!(WS53_I2C_WIRE_BUS).read_volatile() };
    assert_eq!(bus, u32::from(cfg!(feature = "i2c-wire-bus1")));
    let selected = match bus {
        0 => Selection::I2c0(i0),
        1 => Selection::I2c1(i1),
        _ => panic!("I2C RAM mode witness"),
    };
    let mut bank = GpioDriver::new_gpio0(unsafe { gpio.assume_application_ownership() });
    let _ = check_selected(selected, &mut bank, timer, uart);
}

pub enum Selection {
    I2c0(Unclaimed<I2c0<'static>>),
    I2c1(Unclaimed<I2c1<'static>>),
}

pub fn check_selected(
    selected: Selection,
    bank: &mut GpioDriver<'static>,
    timer: &mut TimerDriver<'_>,
    uart: &Uart<'_, Uart2<'_>>,
) -> [u32; 128] {
    // A second invocation is a new observation, not continuation of the first
    // mailbox. Only diagnostic RAM is written here; no MMIO or old context.
    for i in 0..128 {
        put(i, 0);
    }
    for i in 0..16 {
        unsafe {
            core::ptr::addr_of_mut!(WS53_I2C_WIRE_BOUNDS)
                .cast::<u32>()
                .add(i)
                .write_volatile(0)
        }
    }
    let bus = match &selected {
        Selection::I2c0(_) => 0,
        Selection::I2c1(_) => 1,
    };
    put(0, 0x53c30001);
    put(3, bus);
    assert!(!riscv::register::mstatus::read().mie());
    let crg = unsafe { &*ws53_pac::MCrg::PTR };
    let old = crg.m_clken1().read().bits();
    put(4, u32::from(old));
    let g0 = unsafe { &*ws53_pac::Gpio0::PTR };
    let g2 = unsafe { &*ws53_pac::Gpio2::PTR };
    let d0 = g0.gpio_sw_oen().read().bits();
    let d2 = g2.gpio_sw_oen().read().bits();
    put(10, d0);
    put(12, d2);
    assert_ne!(d0 & (1 << 17), 0);
    assert_eq!(g0.gpio_int_en().read().bits(), 0);
    assert_eq!(g0.gpio_sw_ctl().read().bits(), 0);
    assert!(!interrupt::is_enabled(
        hisi_hal::interrupt::Interrupt::GPIO_0
    ));
    if old & 16 != 0 {
        assert!(
            unsafe { &*ws53_pac::Uart1::PTR }
                .line_status()
                .read()
                .tx_empty_s()
                .bit()
        );
    }
    let mut marker_owner = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    let marker_saved = marker_owner.configuration().unwrap();
    marker_owner
        .configure(
            PadConfig::new(
                Pad::Mgpio17
                    .select_mode(MuxMode::from_code(0).unwrap())
                    .unwrap(),
                Pad::Mgpio17.drive_setting(0).unwrap(),
                Pull::Down,
                true,
                marker_saved.schmitt_enabled(),
            )
            .unwrap(),
        )
        .unwrap();
    let marker_pin = marker_owner
        .into_gpio()
        .unwrap_or_else(|_| panic!("marker route"));
    let (scl, sda) = if bus == 0 {
        (Pad::Sgpio2, Pad::Mgpio22)
    } else {
        (Pad::Mgpio8, Pad::Mgpio9)
    };
    let (scl, scl_saved, scl_dir) = prepare(scl, 3);
    let (sda, sda_saved, sda_dir) = prepare(sda, 3);
    let mut marker = bank
        .init_output(marker_pin, false)
        .unwrap_or_else(|_| panic!("marker ownership"));
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let cell = RefCell::new(timer);
    Delay(&cell).delay_ms(2);
    macro_rules! exercise {
        ($reservation:ident,$new:ident,$pac:ident,$irq:ident,$gate:ident,$bit:expr) => {{
            let irq = hisi_hal::interrupt::Interrupt::$irq;
            assert!(!interrupt::is_enabled(irq) && !interrupt::is_pending(irq));
            // The I2C-specific handoff is directly into its clock-enabling ctor;
            // no controller MMIO occurs before that gate is enabled.
            let mut token = unsafe { $reservation.assume_application_ownership() };
            let mut pins = unsafe { Pins::from_configured(scl, sda) };
            for (iter, hz) in [10_000, 100_000].into_iter().enumerate() {
                let config =
                    Config::new(Timing::from_hz(hz).unwrap(), NonZeroU16::new(5000).unwrap());
                if iter == 0 {
                    put(5, u32::from(crg.m_clken1().read().bits()));
                }
                let mut driver = I2c::$new(token, pins, config, Delay(&cell))
                    .unwrap_or_else(|_| panic!("I2C construction failed"));
                if iter == 0 {
                    put(6, u32::from(crg.m_clken1().read().bits()));
                }
                Delay(&cell).delay_ms(2);
                for case in 0..3 {
                    run(
                        &mut driver,
                        &cell,
                        &mut marker,
                        iter * 3 + case,
                        hz,
                        match case {
                            0 => Transfer::Write7,
                            1 => Transfer::Read7,
                            _ => Transfer::Write10,
                        },
                    );
                }
                if iter == 1 {
                    run(&mut driver, &cell, &mut marker, 6, hz, Transfer::Write7);
                    run(&mut driver, &cell, &mut marker, 7, hz, Transfer::Empty);
                    let r = unsafe { &*ws53_pac::$pac::PTR };
                    put(14, r.ic_intr_mask().read().bits());
                    put(15, r.ic_dma_cr().read().bits());
                    put(16, u32::from(r.ic_enable_status().read().ic_en().bit()));
                    put(17, r.ic_raw_intr_stat().read().bits());
                    put(
                        18,
                        u32::from(interrupt::is_enabled(irq) || interrupt::is_pending(irq)),
                    );
                }
                (token, pins) = driver
                    .try_free()
                    .unwrap_or_else(|_| panic!("I2C shutdown failed"));
                assert_eq!(crg.m_clken1().read().bits(), old & !(1 << $bit));
            }
            put(7, u32::from(crg.m_clken1().read().bits()));
            crg.m_clken1()
                .modify(|_, w| w.$gate().bit(old & (1 << $bit) != 0));
            let (scl, sda) = pins.into_pads();
            restore(scl, scl_saved, scl_dir);
            restore(sda, sda_saved, sda_dir);
            let _ = token;
        }};
    }
    match selected {
        Selection::I2c0(i0) => exercise!(i0, new_i2c0, I2c0, I2C_0, i2c0_enable, 6),
        Selection::I2c1(i1) => exercise!(i1, new_i2c1, I2c1, I2C_1, i2c1_enable, 7),
    }
    {
        let _released = marker.free();
    }
    let mut owner = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    owner.configure(marker_saved).unwrap();
    assert_eq!(owner.configuration().unwrap(), marker_saved);
    put(9, 1);
    put(11, g0.gpio_sw_oen().read().bits());
    put(13, g2.gpio_sw_oen().read().bits());
    assert_eq!(g0.gpio_sw_oen().read().bits(), d0);
    assert_eq!(g2.gpio_sw_oen().read().bits(), d2);
    put(8, u32::from(crg.m_clken1().read().bits()));
    assert_eq!(crg.m_clken1().read().bits(), old);
    cell.borrow_mut().disable(TimerChannel::Channel1);
    cell.borrow_mut().clear_interrupt(TimerChannel::Channel1);
    interrupt::clear_pending(TimerChannel::Channel1.interrupt());
    put(20, 1);
    put(1, 9);
    emit(
        uart,
        b"WS53 I2C WIRE OBSERVED phases=8 nacks=7 restored=1\r\n",
    );
    core::array::from_fn(|i| unsafe {
        core::ptr::addr_of!(WS53_I2C_WIRE)
            .cast::<u32>()
            .add(i)
            .read_volatile()
    })
}
