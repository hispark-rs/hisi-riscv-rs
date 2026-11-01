//! Single-hart foreground/ISR access to one owned HAL GPIO output.
//! No raw GPIO access aliases the output token. The fixed bank storage is
//! initialized exactly once while IRQs are disabled and never accessed again
//! except through Output's exclusive borrow, serialized by critical sections.
use core::{
    cell::{Cell, RefCell},
    mem::MaybeUninit,
};
use critical_section::Mutex;
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    gpio::{GpioBank, GpioDriver, Output, Pad},
    interrupt,
    peripherals::Gpio0,
    pinctrl::{Config, PadControl, Pull},
    timer::TimerDriver,
};

static OUTPUT: Mutex<RefCell<Option<Output<'static, 'static, 'static>>>> =
    Mutex::new(RefCell::new(None));
static ORIGINAL: Mutex<Cell<Option<Config>>> = Mutex::new(Cell::new(None));
static INITIALIZED: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static mut BANK: MaybeUninit<GpioDriver<'static>> = MaybeUninit::uninit();

pub fn level(high: bool) {
    critical_section::with(|cs| {
        let mut slot = OUTPUT.borrow(cs).borrow_mut();
        let pin = slot.as_mut().expect("trace output not owned");
        if high {
            pin.set_high();
        } else {
            pin.set_low();
        }
        riscv::asm::fence();
        assert_eq!(pin.is_set_high(), high);
    });
}

fn pulse(timer: &mut TimerDriver<'_>, ms: u32) {
    level(true);
    super::Delay(timer).delay_ms(ms);
    level(false);
    super::Delay(timer).delay_ms(1);
}

pub fn initialize(token: Gpio0<'static>, timer: &mut TimerDriver<'_>) {
    assert!(!riscv::register::mstatus::read().mie());
    assert!(!interrupt::is_enabled(GpioBank::Bank0.interrupt()));
    assert!(!interrupt::is_pending(GpioBank::Bank0.interrupt()));
    critical_section::with(|cs| {
        assert!(
            !INITIALIZED.borrow(cs).replace(true),
            "trace initialized twice"
        );
        let mut pad = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
        let original = pad.configuration().unwrap();
        assert_eq!(
            original.mode().mode().code(),
            0,
            "trace pad used by a peripheral"
        );
        let regs = unsafe { &*ws53_pac::Gpio0::PTR };
        assert_eq!(regs.gpio_int_en().read().bits(), 0, "GPIO0 interrupt owner");
        assert_ne!(
            regs.gpio_sw_oen().read().bits() & (1 << 17),
            0,
            "trace old output active"
        );
        // Source selector is opaque. Only accept the same observed source word
        // as the prior measured GPIO17 fixture, without writing/interpreting it.
        assert_eq!(
            regs.gpio_sw_ctl().read().bits(),
            0,
            "trace source observation differs"
        );
        let cfg = Config::new(
            original.mode(),
            Pad::Mgpio17.drive_setting(0).unwrap(),
            Pull::Down,
            true,
            original.schmitt_enabled(),
        )
        .unwrap();
        pad.configure(cfg).unwrap();
        let pin = pad
            .into_gpio()
            .unwrap_or_else(|_| panic!("trace GPIO preparation failed"));
        // SAFETY: entry is single-shot with global MIE off. BANK's address is
        // stable for the boot. Its only mutable reference moves into Output;
        // neither this module nor any caller touches BANK after this point.
        let bank: &'static mut GpioDriver<'static> = unsafe {
            let pointer = core::ptr::addr_of_mut!(BANK).cast::<GpioDriver<'static>>();
            pointer.write(GpioDriver::new_gpio0(token));
            &mut *pointer
        };
        let output = bank
            .init_output(pin, false)
            .unwrap_or_else(|_| panic!("trace wrong GPIO bank"));
        ORIGINAL.borrow(cs).set(Some(original));
        *OUTPUT.borrow(cs).borrow_mut() = Some(output);
    });
    super::Delay(timer).delay_ms(2);
    for ms in [1, 2, 4] {
        pulse(timer, ms);
    }
}

pub fn finish(timer: &mut TimerDriver<'_>) {
    assert!(!riscv::register::mstatus::read().mie());
    assert!(!interrupt::is_enabled(
        hisi_hal::uart::UartPort::Uart2.interrupt()
    ));
    pulse(timer, 8);
    critical_section::with(|cs| {
        let output = OUTPUT.borrow(cs).borrow_mut().take().unwrap();
        assert!(!output.is_set_high());
        {
            let _returned_pin = output.free();
        }
        // Ownership has returned from GPIO. No live AnyPin/Output aliases it.
        let mut owner = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
        let original = ORIGINAL.borrow(cs).take().unwrap();
        owner.configure(original).unwrap();
        assert_eq!(owner.configuration().unwrap(), original);
    });
}
