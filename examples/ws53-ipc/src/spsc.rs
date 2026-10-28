//! Concurrent ordinary-RAM peer fixture only. Never load on a physical board.
#![no_std]
#![no_main]
use core::ptr::{self, NonNull};
use hisi_riscv_rt::entry;
use hisi_ws53_coex::{
    Channel, MAX_MESSAGE, Message, MessageId, Priority,
    layout::{ControlSramRange, SdkMailboxLayout},
    mailbox::{MailboxError, MailboxImage, MailboxSize, Route},
    spsc::{CoherentMemory, Error, Receive, Transmit},
};

const COUNT: u32 = 512;
#[repr(C, align(4))]
pub struct Image([u8; 256]);
#[unsafe(no_mangle)]
pub static mut WS53_SPSC_TX: Image = Image([0; 256]);
#[unsafe(no_mangle)]
pub static mut WS53_SPSC_RX: Image = Image([0; 256]);
// input gate,ready,panic,tx,rx,full,empty,txhash,rxhash,target,magic,unused,
// valid-done,tx-reclaimed,rx-reclaimed,unused,unused,bad-error,bad-output-kept,
// layout-ok,layout-app-base,layout-total,layout-refusals,layout-app-end.
#[unsafe(no_mangle)]
pub static mut WS53_SPSC_REPORT: [u32; 24] = [0; 24];
fn put(index: usize, value: u32) {
    unsafe {
        ptr::addr_of_mut!(WS53_SPSC_REPORT)
            .cast::<u32>()
            .add(index)
            .write_volatile(value)
    };
}
fn get(index: usize) -> u32 {
    unsafe {
        ptr::addr_of!(WS53_SPSC_REPORT)
            .cast::<u32>()
            .add(index)
            .read_volatile()
    }
}
fn length(n: u32) -> usize {
    match n {
        0..=2 => 60,
        3 => 47,
        _ => (n % 61) as usize,
    }
}
fn message(n: u32, payload: &[u8; 60]) -> Message<'_> {
    Message::new(
        MessageId::try_from_raw((n % 19) as u8).unwrap(),
        if n & 1 == 0 {
            Priority::High
        } else {
            Priority::Low
        },
        &payload[..length(n)],
    )
    .unwrap()
}
fn payload(n: u32) -> [u8; 60] {
    core::array::from_fn(|i| (n * 17 + i as u32 * 37) as u8)
}
fn hash(previous: u32, m: Message<'_>) -> u32 {
    let mut frame = [0; MAX_MESSAGE];
    let used = m.encode(&mut frame);
    frame[..used]
        .iter()
        .fold(previous, |h, b| (h ^ u32::from(*b)).wrapping_mul(16777619))
}

fn check_layout() {
    // Ordinary copied bytes only. This reproduces the observed header field;
    // it does not read or claim ownership of a real Ccore/shared-memory window.
    let mut header = [0u8; 28];
    header[24..].copy_from_slice(&0x1e300u32.to_le_bytes());
    let sram = ControlSramRange::try_from_bounds(0x10000, 0x1ff00).unwrap();
    let layout = SdkMailboxLayout::from_boot_header(&header, sram).unwrap();
    let expected = [
        (
            Route::ControlToApplication(Channel::Channel0),
            0x1e300,
            1268,
        ),
        (Route::ControlToApplication(Channel::Channel1), 0x1e800, 244),
        (
            Route::ApplicationToControl(Channel::Channel0),
            0x1e900,
            1268,
        ),
        (Route::ApplicationToControl(Channel::Channel1), 0x1ee00, 244),
    ];
    let mut total = 0;
    for (route, address, capacity) in expected {
        let placement = layout.for_route(route);
        assert_eq!(placement.control_address().raw(), address);
        assert_eq!(placement.size().capacity(), capacity);
        total += placement.size().total_bytes() as u32;
    }
    let app_base = layout.placements()[0]
        .control_address()
        .application_view()
        .raw();
    assert_eq!(app_base, 0x1001e300);
    let mut refused = 0;
    for base in [0x10000u32, 0x1e301, 0x1f304, 0x1001e300, u32::MAX - 3] {
        header[24..].copy_from_slice(&base.to_le_bytes());
        assert!(SdkMailboxLayout::from_boot_header(&header, sram).is_err());
        refused += 1;
    }
    put(19, 1);
    put(20, app_base);
    put(21, total);
    put(22, refused);
    put(23, app_base + total);
}
#[entry]
fn main() -> ! {
    check_layout();
    let size = MailboxSize::try_from_total_bytes(256).unwrap();
    // Exclusive pre-start initialization only. Host waits for ready before
    // accessing these regions; no Rust reference to a live queue is retained.
    unsafe {
        MailboxImage::initialize(
            &mut (*ptr::addr_of_mut!(WS53_SPSC_TX)).0,
            size,
            Route::ApplicationToControl(Channel::Channel0),
        )
        .unwrap();
        let b = &mut (*ptr::addr_of_mut!(WS53_SPSC_RX)).0;
        MailboxImage::initialize(b, size, Route::ControlToApplication(Channel::Channel1)).unwrap();
        b[3] = 0;
        b[4..6].copy_from_slice(&243u16.to_le_bytes());
    }
    // SAFETY: explicit coherent model RAM with static extent/alignment; the host
    // peer uses the qualified algorithms and complementary fields. It starts
    // only after ready. No Ccore, cached alias, MMIO, old ISR or DMA exists here.
    // These model facts do not establish any physical platform handoff.
    let tx_memory = unsafe {
        CoherentMemory::from_mapping(
            NonNull::new(ptr::addr_of_mut!(WS53_SPSC_TX).cast()).unwrap(),
            size,
        )
    }
    .unwrap();
    let rx_memory = unsafe {
        CoherentMemory::from_mapping(
            NonNull::new(ptr::addr_of_mut!(WS53_SPSC_RX).cast()).unwrap(),
            size,
        )
    }
    .unwrap();
    let mut tx = Some(unsafe { Transmit::from_prepared(tx_memory, Channel::Channel0) }.unwrap());
    let mut rx = Some(unsafe { Receive::from_prepared(rx_memory, Channel::Channel1) }.unwrap());
    put(7, 2166136261);
    put(8, 2166136261);
    put(9, COUNT);
    put(10, 0x53505343);
    put(1, 1);
    let mut output = [0xa5; MAX_MESSAGE];
    loop {
        let gate = get(0);
        let sent = get(3);
        let received = get(4);
        if gate & 1 != 0 && sent < COUNT {
            let p = payload(sent);
            let input = message(sent, &p);
            // SAFETY: model peer only compares inline bytes, never executes a
            // service or dereferences payload pointers. Real permissions differ.
            match unsafe { tx.as_mut().unwrap().try_push(input) } {
                Ok(()) => {
                    put(7, hash(get(7), input));
                    put(3, sent + 1);
                }
                Err(Error::Mailbox(MailboxError::Full)) => put(5, get(5).wrapping_add(1)),
                Err(_) => panic!(),
            }
        }
        if gate & 2 != 0 && received < COUNT {
            match rx.as_mut().unwrap().try_pop_into(&mut output).unwrap() {
                Some(actual) => {
                    let p = payload(received);
                    assert_eq!(actual, message(received, &p));
                    put(8, hash(get(8), actual));
                    put(4, received + 1);
                }
                None => put(6, get(6).wrapping_add(1)),
            }
        }
        if get(3) == COUNT && get(4) == COUNT {
            put(12, 1);
        }
        if gate & 8 != 0 && get(17) == 0 {
            let mut bad_output = [0xa5; MAX_MESSAGE];
            match rx.as_mut().unwrap().try_pop_into(&mut bad_output) {
                Err(Error::Mailbox(MailboxError::Message(_))) => {
                    put(18, u32::from(bad_output.iter().all(|b| *b == 0xa5)));
                    put(17, 27);
                }
                Ok(None) => {}
                _ => panic!(),
            }
        }
        if gate == 4 && get(13) == 0 && get(14) == 0 {
            // SAFETY: host explicitly stops its peer after all valid frames and
            // fault publication finish, then signals4. Synchronous QTest writes
            // are complete; no subsequent queue access until system reset.
            let _tx_mapping = unsafe { tx.take().unwrap().reclaim_after_peer_quiescent() };
            put(13, 1);
            let _rx_mapping = unsafe { rx.take().unwrap().reclaim_after_peer_quiescent() };
            put(14, 1);
        }
        core::hint::spin_loop();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    put(2, 1);
    loop {
        core::hint::spin_loop();
    }
}
