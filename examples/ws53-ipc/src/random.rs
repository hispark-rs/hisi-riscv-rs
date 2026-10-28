//! Model-only IPC18 + actual PAC/TRNG service. Never load on a physical board.
//! The registered address is metadata; the image is ordinary application RAM.
#![no_std]
#![no_main]
use core::{num::NonZeroU16, ptr, ptr::NonNull};
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    ipc::{
        Channel, MAX_MESSAGE,
        mailbox::{MailboxImage, MailboxSize, Route},
        spsc::{CoherentMemory, Receive},
    },
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    trng::{Error, TrngDriver},
};
use hisi_riscv_rt::entry;
use hisi_ws53_coex::random_buffer::{
    BufferRegion, RandomBufferImage, RequestError, ServiceError, UpdateRequest,
};

#[repr(C, align(4))]
pub struct Inbox([u8; 256]);
#[repr(C, align(4))]
pub struct Destination([u8; 132]);
#[unsafe(no_mangle)]
pub static mut WS53_RANDOM_INBOX: Inbox = Inbox([0; 256]);
#[unsafe(no_mangle)]
pub static mut WS53_RANDOM_DESTINATION: Destination = Destination([0xa5; 132]);
// ready,panic,completed,result,selected-offset,selected-length,staged-prefix,
// reserved,reserved,reserved,reserved,magic
#[unsafe(no_mangle)]
pub static mut WS53_RANDOM_REPORT: [u32; 12] = [0; 12];
// Host only changes controls between completed requests, before publishing W.
// total attempts, staging capacity, registered length
#[unsafe(no_mangle)]
pub static mut WS53_RANDOM_CONTROL: [u32; 3] = [512, 128, 128];

fn put(index: usize, value: u32) {
    unsafe {
        ptr::addr_of_mut!(WS53_RANDOM_REPORT)
            .cast::<u32>()
            .add(index)
            .write_volatile(value)
    };
}
fn control(index: usize) -> u32 {
    unsafe {
        ptr::addr_of!(WS53_RANDOM_CONTROL)
            .cast::<u32>()
            .add(index)
            .read_volatile()
    }
}
fn request_error(error: RequestError) -> u32 {
    match error {
        RequestError::WrongMessage => 10,
        RequestError::PayloadLength => 11,
        RequestError::UnknownUpdate => 12,
        RequestError::InvalidExtent => 13,
        RequestError::UnregisteredRegion => 14,
        RequestError::UnevenHalves => 15,
    }
}
struct Delay<'d>(TimerDriver<'d>);
impl DelayNs for Delay<'_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.0.current_value(TimerChannel::Channel1).unwrap();
        for _ in 0..10_000_000 {
            if start.wrapping_sub(self.0.current_value(TimerChannel::Channel1).unwrap()) >= ticks {
                return;
            }
        }
        panic!();
    }
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let mut delay = Delay(timer);
    let size = MailboxSize::try_from_total_bytes(256).unwrap();
    unsafe {
        let bytes = &mut (*ptr::addr_of_mut!(WS53_RANDOM_INBOX)).0;
        MailboxImage::initialize(bytes, size, Route::ControlToApplication(Channel::Channel1))
            .unwrap();
        bytes[3] = 0;
        bytes[4..6].copy_from_slice(&243u16.to_le_bytes());
    }
    // SAFETY: explicit model coherent RAM, complementary host writes, no real
    // vendor core/DMA/cache. Host waits for ready and uses SDK guarded cursors.
    let memory = unsafe {
        CoherentMemory::from_mapping(
            NonNull::new(ptr::addr_of_mut!(WS53_RANDOM_INBOX).cast()).unwrap(),
            size,
        )
    }
    .unwrap();
    let mut rx = unsafe { Receive::from_prepared(memory, Channel::Channel1) }.unwrap();
    // Model producer is configured by the host. Fault cases intentionally test
    // a source failing after this synthetic handoff, not a real entropy claim.
    let mut random = unsafe { TrngDriver::from_configured(p.TRNG.assume_application_ownership()) };
    put(11, 0x524e4431);
    put(0, 1);
    let mut frame = [0; MAX_MESSAGE];
    let mut scratch = [0x33; 128];
    let mut completed = 0u32;
    loop {
        let Some(message) = rx.try_pop_into(&mut frame).unwrap() else {
            continue;
        };
        // Convert the legacy HAL frame through the audited SDK encoding until
        // the queue transport also moves to the protocol component.
        let mut encoded = [0u8; MAX_MESSAGE];
        let used = message.encode(&mut encoded);
        let message = hisi_ws53_coex::Message::decode(&encoded[..used]).unwrap();
        let length = control(2);
        assert!((1..=128).contains(&length));
        let region = BufferRegion::try_from_control_bounds(0xc1000, 0xc1000 + length).unwrap();
        put(4, 0);
        put(5, 0);
        put(6, 0);
        let result = match UpdateRequest::decode(message, region) {
            Err(error) => request_error(error),
            Ok(request) => {
                put(4, request.offset() as u32);
                put(5, request.byte_len() as u32);
                let attempts = u16::try_from(control(0))
                    .ok()
                    .and_then(NonZeroU16::new)
                    .unwrap();
                let capacity = control(1) as usize;
                assert!(capacity <= 128);
                // Exclusive model image only. Host does not touch destination
                // from publication of W through the completed release marker.
                // The received address is NEVER converted into a pointer.
                let destination = unsafe { &mut (*ptr::addr_of_mut!(WS53_RANDOM_DESTINATION)).0 };
                let mut image =
                    RandomBufferImage::attach(region, &mut destination[1..1 + length as usize])
                        .unwrap();
                match image.update(request, &mut scratch[..capacity], |bytes| {
                    random.fill_bytes(bytes, attempts, &mut delay)
                }) {
                    Ok(()) => 0,
                    Err(ServiceError::Request(error)) => request_error(error),
                    Err(ServiceError::ScratchTooSmall) => 20,
                    Err(ServiceError::Random(error)) => {
                        put(6, error.written() as u32);
                        match error.error() {
                            Error::RingDisabled => 30,
                            Error::Timeout => 31,
                        }
                    }
                }
            }
        };
        put(3, result);
        completed += 1;
        // Publish result only after the copy (or error) and image borrow ends.
        unsafe { core::arch::asm!("fence rw,rw", options(nostack)) };
        put(2, completed);
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    put(1, 1);
    loop {
        core::hint::spin_loop()
    }
}
