//! CPU/IRQ fixture for an explicit synthetic peer only. Never load on a board.
#![no_std]
#![no_main]
use core::{cell::Cell, ptr};
use critical_section::Mutex;
use hisi_hal::ipc::mailbox::{MailboxError, MailboxImage, MailboxSize, Route};
use hisi_hal::{
    Peripherals, interrupt,
    ipc::{Channel, Doorbell, MAX_MESSAGE, Message, MessageId, Priority, ReceiveEnableState},
};
use hisi_riscv_rt::entry;

// command,sequence,ready,channel,value,result,rx0,rx1,tx0,tx1,irq0,irq1,panic,last-cause,unused,codec-hash
#[unsafe(no_mangle)]
pub static mut WS53_IPC_MODEL: [u32; 16] = [0; 16];
// Explicit serialized fixture: the host may access this image ONLY while the
// CPU is stopped and no Rust borrow exists. The foreground borrows it only for
// commands7..10 and drops the borrow before publishing completion. No ISR uses
// it. This does not emulate coherent concurrent access by a control core.
#[unsafe(no_mangle)]
pub static mut WS53_IPC_MAILBOX_MODEL: [u8; 256] = [0; 256];
// The PAC reference intentionally makes Doorbell !Send/!Sync. This one-hart
// model moves its sole owner through short IRQ-disabled metadata operations;
// do not weaken the HAL's cross-thread properties for an ISR fixture.
static mut OWNER: Option<Doorbell<'static>> = None;
static COUNTS: Mutex<Cell<[u32; 2]>> = Mutex::new(Cell::new([0; 2]));
fn take_owner() -> Doorbell<'static> {
    critical_section::with(|_| {
        // SAFETY: one application hart; all OWNER accesses use this section.
        unsafe { (&mut *ptr::addr_of_mut!(OWNER)).take().unwrap() }
    })
}
fn return_owner(owner: Doorbell<'static>) {
    critical_section::with(|_| {
        // SAFETY: move back the unique owner without a shared reference.
        unsafe { *ptr::addr_of_mut!(OWNER) = Some(owner) };
    });
}
fn put(i: usize, v: u32) {
    unsafe {
        ptr::addr_of_mut!(WS53_IPC_MODEL)
            .cast::<u32>()
            .add(i)
            .write_volatile(v)
    };
}
fn get(i: usize) -> u32 {
    unsafe {
        ptr::addr_of!(WS53_IPC_MODEL)
            .cast::<u32>()
            .add(i)
            .read_volatile()
    }
}
fn selected() -> Option<Channel> {
    match get(3) {
        0 => Some(Channel::Channel0),
        1 => Some(Channel::Channel1),
        _ => None,
    }
}
fn counts() {
    let n = critical_section::with(|cs| COUNTS.borrow(cs).get());
    put(10, n[0]);
    put(11, n[1]);
}
fn with_owner(f: impl FnOnce(&mut Doorbell<'static>)) {
    let channels = [Channel::Channel0, Channel::Channel1];
    let enabled = channels.map(|c| interrupt::is_enabled(c.interrupt()));
    // Short CSR masking only. No hardware transaction/callback under the
    // metadata critical section; a unique token is moved out before MMIO.
    for c in channels {
        unsafe { interrupt::disable(c.interrupt()) };
    }
    let mut owner = take_owner();
    f(&mut owner);
    return_owner(owner);
    for (c, on) in channels.into_iter().zip(enabled) {
        if on {
            unsafe { interrupt::enable(c.interrupt()) };
        }
    }
}
fn on_irq(channel: Channel, index: usize) {
    let mut owner = take_owner();
    owner.acknowledge(channel);
    if owner.receive_pending(channel) {
        // Bounded fault path; no repeated ISR/poll while an ACK is held.
        owner.set_receive_enabled(channel, false);
        unsafe { interrupt::disable(channel.interrupt()) };
        put(5, 4);
    }
    interrupt::clear_pending(channel.interrupt());
    critical_section::with(|cs| {
        let mut n = COUNTS.borrow(cs).get();
        n[index] += 1;
        COUNTS.borrow(cs).set(n);
    });
    return_owner(owner);
    let cause: u32;
    unsafe { core::arch::asm!("csrr {}, mcause",out(reg)cause,options(nostack)) };
    put(13, cause);
}
#[unsafe(no_mangle)]
extern "C" fn APP_INT0() {
    on_irq(Channel::Channel0, 0);
}
#[unsafe(no_mangle)]
extern "C" fn APP_INT1() {
    on_irq(Channel::Channel1, 1);
}
fn codec_hash() -> u32 {
    let payload: [u8; 60] = core::array::from_fn(|i| (i * 37 + 11) as u8);
    let mut output = [0; MAX_MESSAGE];
    let mut hash = 2166136261u32;
    for id in 0..19 {
        for priority in [Priority::High, Priority::Low] {
            for size in 0..=60 {
                let m = Message::new(
                    MessageId::try_from_raw(id).unwrap(),
                    priority,
                    &payload[..size],
                )
                .unwrap();
                let used = m.encode(&mut output);
                if Message::decode(&output[..used]).unwrap() != m {
                    return 0;
                }
                for b in &output[..used] {
                    hash = (hash ^ u32::from(*b)).wrapping_mul(16777619);
                }
            }
        }
    }
    hash
}
fn frame_hash(message: Message<'_>) -> u32 {
    let mut frame = [0; MAX_MESSAGE];
    let used = message.encode(&mut frame);
    frame[..used].iter().fold(2166136261, |h, b| {
        (h ^ u32::from(*b)).wrapping_mul(16777619)
    })
}
fn mailbox_error(error: MailboxError) -> u32 {
    match error {
        MailboxError::StorageSize => 20,
        MailboxError::Route => 21,
        MailboxError::Capacity => 22,
        MailboxError::Cursor => 23,
        MailboxError::EmptyFlag => 24,
        MailboxError::Full => 25,
        MailboxError::Incomplete => 26,
        MailboxError::Message(_) => 27,
    }
}
fn mailbox_command(cmd: u32, channel: Channel, value: u32) -> Result<(), u32> {
    let reversed = if cmd == 8 { value >> 24 } else { value };
    if reversed > 1 {
        return Err(28);
    }
    let route = if reversed == 0 {
        Route::ApplicationToControl(channel)
    } else {
        Route::ControlToApplication(channel)
    };
    let size = MailboxSize::try_from_total_bytes(256).unwrap();
    // SAFETY: this model harness enforces the exclusive stop/command contract
    // described on the static; no host, ISR or other CPU accesses the image
    // during this call. The reference does not escape command completion.
    let bytes = unsafe { &mut *ptr::addr_of_mut!(WS53_IPC_MAILBOX_MODEL) };
    if cmd == 7 {
        MailboxImage::initialize(bytes, size, route).map_err(mailbox_error)?;
        return Ok(());
    }
    let mut image = MailboxImage::attach(bytes, size, route).map_err(mailbox_error)?;
    match cmd {
        8 => {
            let length = (value & 255) as usize;
            let id = MessageId::try_from_raw(((value >> 8) & 255) as u8).ok_or(28u32)?;
            let priority = match (value >> 16) & 255 {
                0 => Priority::High,
                1 => Priority::Low,
                _ => return Err(28),
            };
            let payload: [u8; 60] = core::array::from_fn(|i| (i * 37 + 11) as u8);
            if length > payload.len() {
                return Err(28);
            }
            let input = Message::new(id, priority, &payload[..length]).map_err(|_| 28u32)?;
            image.try_push(input).map_err(mailbox_error)?;
        }
        9 => {
            let mut output = [0xa5; MAX_MESSAGE];
            let message = image
                .try_pop_into(&mut output)
                .map_err(mailbox_error)?
                .ok_or(10u32)?;
            put(14, frame_hash(message));
        }
        10 => put(14, image.queued_bytes().map_err(mailbox_error)? as u32),
        _ => return Err(28),
    }
    Ok(())
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // Model-only handoff: no real peer/shared memory exists or is accessed.
    // The explicit QEMU profile grants accessible, released doorbell registers;
    // these assumptions cannot authorize this fixture on hardware.
    let ipc = unsafe { p.IPC.assume_application_ownership() };
    let owner = unsafe { Doorbell::from_configured(ipc, ReceiveEnableState::Disabled) };
    return_owner(owner);
    unsafe { interrupt::enable_global() };
    put(2, 1);
    loop {
        counts();
        let cmd = get(0);
        if cmd == 0 {
            core::hint::spin_loop();
            continue;
        }
        put(5, 0);
        if cmd == 6 {
            put(15, codec_hash());
        } else if let Some(channel) = selected() {
            match cmd {
                1 => with_owner(|d| d.set_receive_enabled(channel, get(4) != 0)),
                2 => with_owner(|d| {
                    // The virtual peer has no ring parser or remote pointers. This
                    // does not satisfy or test any real firmware publication duty.
                    if unsafe { d.notify(channel) }.is_err() {
                        put(5, 1);
                    }
                }),
                3 => with_owner(|d| d.acknowledge(channel)),
                4 => unsafe {
                    if get(4) != 0 {
                        interrupt::enable(channel.interrupt());
                    } else {
                        interrupt::disable(channel.interrupt());
                    }
                },
                5 => with_owner(|d| {
                    put(6, u32::from(d.receive_pending(Channel::Channel0)));
                    put(7, u32::from(d.receive_pending(Channel::Channel1)));
                    put(8, u32::from(d.send_pending(Channel::Channel0)));
                    put(9, u32::from(d.send_pending(Channel::Channel1)));
                }),
                7..=10 => {
                    if let Err(error) = mailbox_command(cmd, channel, get(4)) {
                        put(5, error);
                    }
                }
                _ => put(5, 2),
            }
        } else {
            put(5, 3);
        }
        counts();
        put(0, 0);
        put(1, get(1) + 1);
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    put(12, 1);
    unsafe { interrupt::disable_global() };
    loop {
        core::hint::spin_loop();
    }
}
