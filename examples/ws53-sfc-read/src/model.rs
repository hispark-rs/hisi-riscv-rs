//! Explicit synthetic-model fixture, not a deployable board image.
#![no_std]
#![no_main]
use core::num::{NonZeroU16, NonZeroU32, NonZeroU64};
use hisi_hal::{
    Peripherals,
    sfc::{
        AdmissionError, Budget, Error, FlashAddress, ReadLength, ReadOnly, ReadRegion, Stage, StatusRegister,
        TickSource,
    },
    tcxo::TcxoDriver,
};
use hisi_riscv_rt::entry;

// Ordinary Rust RAM, only published after data is final. Model runner reads it
// externally; no UART, host acknowledgement or register preinitialization is used.
#[repr(C)]
pub struct Report {
    phase: u32,
    error: u32,
    jedec: [u8; 4],
    status: [u8; 4],
    data: [u8; 64],
    first_error: u32,
    followup_error: u32,
    followup_read_error: u32,
    followup_buffer_intact: u32,
}
#[unsafe(no_mangle)]
pub static mut WS53_SFC_READ_MODEL: Report = Report {
    phase: 0,
    error: 0,
    jedec: [0; 4],
    status: [0; 4],
    data: [0; 64],
    first_error: 0,
    followup_error: 0,
    followup_read_error: 0,
    followup_buffer_intact: 0,
};

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let mut clock = TcxoDriver::new(unsafe { p.TCXO.assume_application_ownership() }, NonZeroU16::new(128).unwrap());
    // The isolated model owns this independent counter and starts it here;
    // the host does not preload a timebase or SFC registers.
    if clock.enable().is_err() {
        finish(5);
    }
    // This explicit feature is only for the synthetic configured SFC profile.
    // Its external NOR is supplied by the model; it does not prove a board's
    // capacity, clocking, cache or DMA/XIP handoff. No Flash addresses are loaded.
    let region = ReadRegion::try_new(FlashAddress::try_new(0).unwrap(), NonZeroU32::new(64).unwrap()).unwrap();
    let budget = Budget::new(NonZeroU64::new(320_000).unwrap(), NonZeroU32::new(4096).unwrap());
    let token = unsafe { p.SFC.assume_application_ownership() };
    let result = unsafe { ReadOnly::from_configured(token, region, budget, clock) };
    let mut reader = match result {
        Ok(r) => r,
        Err(error) => {
            unsafe {
                WS53_SFC_READ_MODEL.first_error = admission_code(error);
            }
            finish(1)
        }
    };
    let id = match reader.jedec_id() {
        Ok(v) => v,
        Err(error) => failed(&mut reader, error, 2),
    };
    let mut statuses = [0; 3];
    for (output, register) in
        statuses.iter_mut().zip([StatusRegister::Status1, StatusRegister::Status2, StatusRegister::Status3])
    {
        *output = match reader.status(register) {
            Ok(v) => v,
            Err(error) => failed(&mut reader, error, 3),
        };
    }
    let mut data = [0x5a; 64];
    if let Err(error) = reader.read(FlashAddress::try_new(0).unwrap(), ReadLength::try_new(64).unwrap(), &mut data) {
        // Failed reads must leave the caller's sentinel untouched.
        if data != [0x5a; 64] {
            finish(0xfffe);
        }
        failed(&mut reader, error, 4);
    }
    unsafe {
        WS53_SFC_READ_MODEL.jedec = [id[0], id[1], id[2], 0];
        WS53_SFC_READ_MODEL.status = [statuses[0], statuses[1], statuses[2], 0];
        WS53_SFC_READ_MODEL.data = data;
    }
    finish(0)
}
fn admission_code(error: AdmissionError) -> u32 {
    0x100
        + match error {
            AdmissionError::CommandBusy => 1,
            AdmissionError::DmaBusy => 2,
            AdmissionError::ClockDisabled => 3,
            AdmissionError::ClockSource => 4,
            AdmissionError::SpiMode => 5,
            AdmissionError::AddressMode => 6,
            AdmissionError::ConfigurationChanged => 7,
        }
}
fn error_code(error: Error) -> u32 {
    match error {
        Error::Poisoned => 1,
        Error::BufferLength => 2,
        Error::OutsideRegion => 3,
        Error::Admission(error) => admission_code(error),
        Error::TimeSource => 5,
        Error::Deadline(Stage::NorReady) => 6,
        Error::Deadline(Stage::Command) => 7,
        Error::PollLimit(Stage::NorReady) => 8,
        Error::PollLimit(Stage::Command) => 9,
    }
}
fn failed(reader: &mut ReadOnly<'_, impl TickSource>, error: Error, stage: u32) -> ! {
    let followup = match reader.status(StatusRegister::Status2) {
        Ok(_) => 0,
        Err(error) => error_code(error),
    };
    let mut buffer = [0x5a; 7];
    let followup_read =
        match reader.read(FlashAddress::try_new(0).unwrap(), ReadLength::try_new(7).unwrap(), &mut buffer) {
            Ok(_) => 0,
            Err(error) => error_code(error),
        };
    unsafe {
        WS53_SFC_READ_MODEL.first_error = error_code(error);
        WS53_SFC_READ_MODEL.followup_error = followup;
        WS53_SFC_READ_MODEL.followup_read_error = followup_read;
        WS53_SFC_READ_MODEL.followup_buffer_intact = u32::from(buffer == [0x5a; 7]);
    }
    finish(stage)
}
fn finish(error: u32) -> ! {
    unsafe {
        WS53_SFC_READ_MODEL.error = error;
        core::arch::asm!("fence rw, rw", options(nostack));
        WS53_SFC_READ_MODEL.phase = if error == 0 { 2 } else { 3 };
    }
    loop {
        core::hint::spin_loop();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    finish(0xffff)
}
