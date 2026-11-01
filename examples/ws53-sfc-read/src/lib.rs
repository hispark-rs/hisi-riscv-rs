//! Integration routine only: the caller must first establish every RAM takeover
//! and hardware handoff obligation. This library does not load/reset/flash a board.
#![no_std]
use core::num::{NonZeroU16, NonZeroU32, NonZeroU64};
use hisi_hal::{
    peripherals::{SfcCfg, Tcxo, Unclaimed},
    sfc::{AdmissionError, Budget, Error, FlashAddress, ReadLength, ReadOnly, ReadRegion, StatusRegister},
    tcxo::TcxoDriver,
};

/// Exact raw outputs, without vendor/capacity/protection interpretation.
pub struct Observation {
    pub jedec: [u8; 3],
    pub status: [u8; 3],
    pub data: [u8; 64],
}
#[derive(Debug)]
pub enum ProbeError {
    Admission(AdmissionError),
    Read(Error),
}

/// Execute one RAM-only observation of a caller-specified, verified read range.
///
/// # Safety
/// Uphold ReadOnly::from_configured and both Unclaimed handoff contracts for
/// the entire call and after any failure. In particular, TCXO must be enabled
/// at a known constant rate, with exclusive application snapshot ownership.
/// All code/data (including this routine, helpers and panic handlers) must be
/// independently accessible RAM. There is no board takeover hidden here.
pub unsafe fn observe(
    sfc: Unclaimed<SfcCfg<'static>>,
    tcxo: Unclaimed<Tcxo<'static>>,
    region: ReadRegion,
    address: FlashAddress,
    deadline_ticks: NonZeroU64,
) -> Result<Observation, ProbeError> {
    let clock = TcxoDriver::new(unsafe { tcxo.assume_application_ownership() }, NonZeroU16::new(128).unwrap());
    let token = unsafe { sfc.assume_application_ownership() };
    let budget = Budget::new(deadline_ticks, NonZeroU32::new(4096).unwrap());
    let mut reader =
        unsafe { ReadOnly::from_configured(token, region, budget, clock) }.map_err(ProbeError::Admission)?;
    let jedec = reader.jedec_id().map_err(ProbeError::Read)?;
    let status = [
        reader.status(StatusRegister::Status1).map_err(ProbeError::Read)?,
        reader.status(StatusRegister::Status2).map_err(ProbeError::Read)?,
        reader.status(StatusRegister::Status3).map_err(ProbeError::Read)?,
    ];
    let mut data = [0; 64];
    reader.read(address, ReadLength::try_new(64).unwrap(), &mut data).map_err(ProbeError::Read)?;
    Ok(Observation { jedec, status, data })
}
