//! netif → smoltcp bridge (feature `net`) — replaces the C SDK's lwip behind
//! the netif seam with the Rust [`smoltcp`] TCP/IP stack.
//!
//! Two seams connect the vendor WiFi MAC blob to smoltcp:
//! - **RX**: the driver hands a received Ethernet frame up via
//!   [`driverif_input`](crate::netif::driverif_input); with feature `net` that
//!   pushes the frame bytes into `rx_push` → an internal queue that the
//!   `Ws63Device` `RxToken` drains.
//! - **TX**: smoltcp emits a frame through a `TxToken`, which calls the
//!   registered TX sink (`set_tx_sink`). On hardware that sink invokes the
//!   blob's frame-transmit; standalone it captures the frame for inspection.
//!
//! Mirrors esp-radio's smoltcp `Device`: a frame queue on RX, an MTU buffer on
//! TX. No heap — fixed static ring guarded by a critical section (single hart).
//! Validated by `netif_smoltcp_selftest`: inject an ARP request for our IP, run
//! `Interface::poll`, and confirm smoltcp emits the matching ARP reply.

use core::cell::UnsafeCell;
use critical_section as cs;
use smoltcp::phy::{self, DeviceCapabilities, Medium};
use smoltcp::time::Instant;

/// Max Ethernet frame we buffer (1514 payload + a little slack).
pub const MTU: usize = 1536;
const RX_DEPTH: usize = 4;

struct Bridge {
    rx: [[u8; MTU]; RX_DEPTH],
    rx_len: [usize; RX_DEPTH],
    rx_head: usize,
    rx_count: usize,
    tx_buf: [u8; MTU],
    tx_len: usize,
    tx_count: u32,
    tx_sink: Option<fn(&[u8])>,
}

struct BridgeCell(UnsafeCell<Bridge>);
// SAFETY: only touched inside `cs::with` on a single hart.
unsafe impl Sync for BridgeCell {}

static BRIDGE: BridgeCell = BridgeCell(UnsafeCell::new(Bridge {
    rx: [[0; MTU]; RX_DEPTH],
    rx_len: [0; RX_DEPTH],
    rx_head: 0,
    rx_count: 0,
    tx_buf: [0; MTU],
    tx_len: 0,
    tx_count: 0,
    tx_sink: None,
}));

#[inline]
fn with_bridge<R>(f: impl FnOnce(&mut Bridge) -> R) -> R {
    cs::with(|_| f(unsafe { &mut *BRIDGE.0.get() }))
}

/// Queue a received Ethernet frame for smoltcp (called from `driverif_input`).
/// Drops the frame if it is oversized or the queue is full.
pub fn rx_push(frame: &[u8]) {
    if frame.len() > MTU {
        return;
    }
    with_bridge(|b| {
        if b.rx_count >= RX_DEPTH {
            return;
        }
        let slot = (b.rx_head + b.rx_count) % RX_DEPTH;
        b.rx[slot][..frame.len()].copy_from_slice(frame);
        b.rx_len[slot] = frame.len();
        b.rx_count += 1;
    });
}

fn rx_pop(into: &mut [u8; MTU]) -> Option<usize> {
    with_bridge(|b| {
        if b.rx_count == 0 {
            return None;
        }
        let slot = b.rx_head;
        let n = b.rx_len[slot];
        into[..n].copy_from_slice(&b.rx[slot][..n]);
        b.rx_head = (b.rx_head + 1) % RX_DEPTH;
        b.rx_count -= 1;
        Some(n)
    })
}

fn tx_emit(frame: &[u8]) {
    let sink = with_bridge(|b| {
        let n = frame.len().min(MTU);
        b.tx_buf[..n].copy_from_slice(&frame[..n]);
        b.tx_len = n;
        b.tx_count = b.tx_count.wrapping_add(1);
        b.tx_sink
    });
    // Call the sink OUTSIDE the lock (it may re-enter the bridge / driver).
    if let Some(s) = sink {
        s(frame);
    }
}

/// Install the TX sink invoked for each frame smoltcp transmits (e.g. the blob's
/// frame-send on hardware). Without one, frames are only captured.
pub fn set_tx_sink(sink: fn(&[u8])) {
    with_bridge(|b| b.tx_sink = Some(sink));
}

/// Number of frames smoltcp has transmitted through the bridge (diagnostic).
pub fn tx_count() -> u32 {
    with_bridge(|b| b.tx_count)
}

/// Copy the most recently transmitted frame into `out`; returns its length.
pub fn last_tx(out: &mut [u8]) -> usize {
    with_bridge(|b| {
        let n = b.tx_len.min(out.len());
        out[..n].copy_from_slice(&b.tx_buf[..n]);
        n
    })
}

// ── smoltcp Device ───────────────────────────────────────────────────────────

/// A smoltcp [`phy::Device`] backed by the bridge's RX queue + TX sink.
pub struct Ws63Device;

/// RX token carrying one dequeued frame (owns its bytes — no borrow of `Device`).
pub struct RxFrame {
    buf: [u8; MTU],
    len: usize,
}

/// TX token: writes into an MTU buffer, then hands it to the bridge TX sink.
pub struct TxBuf;

impl phy::Device for Ws63Device {
    type RxToken<'a> = RxFrame;
    type TxToken<'a> = TxBuf;

    fn receive(&mut self, _t: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let mut rx = RxFrame {
            buf: [0; MTU],
            len: 0,
        };
        rx_pop(&mut rx.buf).map(|n| {
            rx.len = n;
            (rx, TxBuf)
        })
    }

    fn transmit(&mut self, _t: Instant) -> Option<Self::TxToken<'_>> {
        Some(TxBuf)
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ethernet;
        caps.max_transmission_unit = 1514;
        caps
    }
}

impl phy::RxToken for RxFrame {
    fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
        f(&self.buf[..self.len])
    }
}

impl phy::TxToken for TxBuf {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let mut buf = [0u8; MTU];
        let n = len.min(MTU);
        let r = f(&mut buf[..n]);
        tx_emit(&buf[..n]);
        r
    }
}

// ── Self-test (ARP round-trip) ───────────────────────────────────────────────

/// Exercise the full bridge end-to-end with no blob: stand up a smoltcp
/// `Interface` over [`Ws63Device`] with MAC `02:00:00:00:00:01` / IP
/// `192.168.1.1`, inject an ARP request ("who-has 192.168.1.1") via the RX seam,
/// run `Interface::poll`, and confirm smoltcp transmits the matching ARP reply
/// ("192.168.1.1 is-at 02:00:00:00:00:01") through the TX seam. Returns
/// `[tx_count, reply_ok, ok]`; a pass is `[1, 1, 1]`. Internal hook.
#[doc(hidden)]
pub fn netif_smoltcp_selftest() -> [u32; 3] {
    use smoltcp::iface::{Config, Interface, SocketSet, SocketStorage};
    use smoltcp::wire::{
        ArpOperation, ArpPacket, ArpRepr, EthernetAddress, EthernetFrame, EthernetProtocol,
        HardwareAddress, IpAddress, IpCidr, Ipv4Address,
    };

    with_bridge(|b| {
        b.tx_count = 0;
        b.tx_len = 0;
        b.rx_count = 0;
        b.rx_head = 0;
    });

    let our_mac = EthernetAddress([0x02, 0, 0, 0, 0, 1]);
    let our_ip = Ipv4Address::new(192, 168, 1, 1);
    let peer_mac = EthernetAddress([0x02, 0, 0, 0, 0, 2]);
    let peer_ip = Ipv4Address::new(192, 168, 1, 2);

    let mut dev = Ws63Device;
    let cfg = Config::new(HardwareAddress::Ethernet(our_mac));
    let mut iface = Interface::new(cfg, &mut dev, Instant::from_millis(0));
    iface.update_ip_addrs(|addrs| {
        let _ = addrs.push(IpCidr::new(IpAddress::Ipv4(our_ip), 24));
    });

    // Build the ARP request frame.
    let req = ArpRepr::EthernetIpv4 {
        operation: ArpOperation::Request,
        source_hardware_addr: peer_mac,
        source_protocol_addr: peer_ip,
        target_hardware_addr: EthernetAddress([0; 6]),
        target_protocol_addr: our_ip,
    };
    let mut frame = [0u8; 64];
    let total = EthernetFrame::<&[u8]>::header_len() + req.buffer_len();
    {
        let mut eth = EthernetFrame::new_unchecked(&mut frame[..]);
        eth.set_src_addr(peer_mac);
        eth.set_dst_addr(EthernetAddress::BROADCAST);
        eth.set_ethertype(EthernetProtocol::Arp);
        let mut arp = ArpPacket::new_unchecked(eth.payload_mut());
        req.emit(&mut arp);
    }
    rx_push(&frame[..total]);

    // Poll: no sockets are needed — ARP is answered at the interface level.
    let mut sock_store = [SocketStorage::EMPTY; 1];
    let mut sockets = SocketSet::new(&mut sock_store[..]);
    iface.poll(Instant::from_millis(1), &mut dev, &mut sockets);

    // Verify the transmitted frame is the expected ARP reply.
    let txc = tx_count();
    let mut out = [0u8; MTU];
    let n = last_tx(&mut out);
    let mut reply_ok = false;
    if let Ok(eth) = EthernetFrame::new_checked(&out[..n])
        && eth.ethertype() == EthernetProtocol::Arp
        && let Ok(pkt) = ArpPacket::new_checked(eth.payload())
        && let Ok(ArpRepr::EthernetIpv4 {
            operation,
            source_hardware_addr,
            source_protocol_addr,
            ..
        }) = ArpRepr::parse(&pkt)
    {
        reply_ok = operation == ArpOperation::Reply
            && source_hardware_addr == our_mac
            && source_protocol_addr == our_ip;
    }
    [txc, reply_ok as u32, (txc >= 1 && reply_ok) as u32]
}
