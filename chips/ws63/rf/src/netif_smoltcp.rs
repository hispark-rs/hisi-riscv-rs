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
const FRAME_PREFIX: usize = 64;
const DIAGNOSTIC_ECHO_IDENTIFIER: [u8; 2] = 0x5753_u16.to_be_bytes();

struct Bridge {
    rx: [[u8; MTU]; RX_DEPTH],
    rx_len: [usize; RX_DEPTH],
    rx_head: usize,
    rx_count: usize,
    rx_dropped: u32,
    rx_high_watermark: usize,
    rx_icmp_echo_replies: u32,
    rx_icmp_sequence_mask: u32,
    rx_dhcp_server_packets: u32,
    last_rx_prefix: [u8; FRAME_PREFIX],
    last_rx_len: usize,
    tx_buf: [u8; MTU],
    tx_len: usize,
    tx_count: u32,
    tx_dhcp_client_packets: u32,
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
    rx_dropped: 0,
    rx_high_watermark: 0,
    rx_icmp_echo_replies: 0,
    rx_icmp_sequence_mask: 0,
    rx_dhcp_server_packets: 0,
    last_rx_prefix: [0; FRAME_PREFIX],
    last_rx_len: 0,
    tx_buf: [0; MTU],
    tx_len: 0,
    tx_count: 0,
    tx_dhcp_client_packets: 0,
    tx_sink: None,
}));

struct ScratchCell(UnsafeCell<[u8; MTU]>);
// SAFETY: each scratch buffer is claimed and released under the single-hart
// critical section before it is accessed outside that section.
unsafe impl Sync for ScratchCell {}

static RX_SCRATCH: ScratchCell = ScratchCell(UnsafeCell::new([0; MTU]));
static TX_SCRATCH: ScratchCell = ScratchCell(UnsafeCell::new([0; MTU]));
static RX_SCRATCH_CLAIMED: cs::Mutex<core::cell::Cell<bool>> =
    cs::Mutex::new(core::cell::Cell::new(false));
static TX_SCRATCH_CLAIMED: cs::Mutex<core::cell::Cell<bool>> =
    cs::Mutex::new(core::cell::Cell::new(false));

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
        let prefix_len = frame.len().min(FRAME_PREFIX);
        b.last_rx_prefix[..prefix_len].copy_from_slice(&frame[..prefix_len]);
        b.last_rx_len = frame.len();
        if let Some(sequence) = diagnostic_echo_reply_sequence(frame) {
            b.rx_icmp_echo_replies = b.rx_icmp_echo_replies.saturating_add(1);
            let sequence = u32::from(sequence);
            if sequence < u32::BITS {
                b.rx_icmp_sequence_mask |= 1 << sequence;
            }
        }
        if has_udp_ports(frame, 67, 68) {
            b.rx_dhcp_server_packets = b.rx_dhcp_server_packets.saturating_add(1);
        }
        if b.rx_count >= RX_DEPTH {
            b.rx_dropped = b.rx_dropped.saturating_add(1);
            return;
        }
        let slot = (b.rx_head + b.rx_count) % RX_DEPTH;
        b.rx[slot][..frame.len()].copy_from_slice(frame);
        b.rx_len[slot] = frame.len();
        b.rx_count += 1;
        b.rx_high_watermark = b.rx_high_watermark.max(b.rx_count);
    });
}

fn has_udp_ports(frame: &[u8], source_port: u16, destination_port: u16) -> bool {
    const ETHERNET_HEADER_LEN: usize = 14;
    const IPV4_MIN_HEADER_LEN: usize = 20;
    const UDP_HEADER_LEN: usize = 8;

    if frame.len() < ETHERNET_HEADER_LEN + IPV4_MIN_HEADER_LEN + UDP_HEADER_LEN
        || frame[12..14] != [0x08, 0x00]
        || frame[14] >> 4 != 4
        || frame[23] != 17
    {
        return false;
    }
    let ipv4_header_len = usize::from(frame[14] & 0x0f) * 4;
    let Some(udp) = ETHERNET_HEADER_LEN.checked_add(ipv4_header_len) else {
        return false;
    };
    frame.len() >= udp + UDP_HEADER_LEN
        && frame[udp..udp + 2] == source_port.to_be_bytes()
        && frame[udp + 2..udp + 4] == destination_port.to_be_bytes()
}

fn diagnostic_echo_reply_sequence(frame: &[u8]) -> Option<u16> {
    const ETHERNET_HEADER_LEN: usize = 14;
    const IPV4_MIN_HEADER_LEN: usize = 20;
    const ICMP_ECHO_HEADER_LEN: usize = 8;

    if frame.len() < ETHERNET_HEADER_LEN + IPV4_MIN_HEADER_LEN + ICMP_ECHO_HEADER_LEN
        || frame[12..14] != [0x08, 0x00]
        || frame[14] >> 4 != 4
        || frame[23] != 1
    {
        return None;
    }

    let ipv4_header_len = usize::from(frame[14] & 0x0f) * 4;
    if ipv4_header_len < IPV4_MIN_HEADER_LEN {
        return None;
    }
    let icmp = ETHERNET_HEADER_LEN.checked_add(ipv4_header_len)?;
    if frame.len() < icmp.checked_add(ICMP_ECHO_HEADER_LEN)?
        || frame[icmp] != 0
        || frame[icmp + 4..icmp + 6] != DIAGNOSTIC_ECHO_IDENTIFIER
    {
        return None;
    }

    Some(u16::from_be_bytes([frame[icmp + 6], frame[icmp + 7]]))
}

/// Snapshot of the bounded RX queue used by the bring-up network path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct RxQueueDiagnostics {
    /// Fixed queue capacity in Ethernet frames.
    pub depth: usize,
    /// Frames waiting for the consumer now.
    pub pending: usize,
    /// Largest number of simultaneously queued frames in this window.
    pub high_watermark: usize,
    /// Frames rejected because all queue slots were occupied in this window.
    pub dropped: u32,
    /// Matching ICMP echo replies observed at the vendor-to-Rust RX seam.
    pub icmp_echo_replies: u32,
    /// Bit `n` records that echo sequence `n` crossed the RX seam.
    pub icmp_sequence_mask: u32,
}

/// Return bounded RX queue occupancy and queue-full loss counters.
#[doc(hidden)]
pub fn rx_queue_diagnostics() -> RxQueueDiagnostics {
    with_bridge(|b| RxQueueDiagnostics {
        depth: RX_DEPTH,
        pending: b.rx_count,
        high_watermark: b.rx_high_watermark,
        dropped: b.rx_dropped,
        icmp_echo_replies: b.rx_icmp_echo_replies,
        icmp_sequence_mask: b.rx_icmp_sequence_mask,
    })
}

/// Start a new RX queue diagnostic window without discarding pending frames.
#[doc(hidden)]
pub fn reset_rx_queue_diagnostics() {
    with_bridge(|b| {
        b.rx_dropped = 0;
        b.rx_high_watermark = b.rx_count;
        b.rx_icmp_echo_replies = 0;
        b.rx_icmp_sequence_mask = 0;
    });
}

/// DHCP packet counts observed at the Rust-visible L2 seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct DhcpDiagnostics {
    /// Client-to-server UDP packets (port 68 to 67).
    pub client_packets: u32,
    /// Server-to-client UDP packets (port 67 to 68).
    pub server_packets: u32,
}

/// Snapshot DHCP traffic without changing the counters.
#[doc(hidden)]
pub fn dhcp_diagnostics() -> DhcpDiagnostics {
    with_bridge(|bridge| DhcpDiagnostics {
        client_packets: bridge.tx_dhcp_client_packets,
        server_packets: bridge.rx_dhcp_server_packets,
    })
}

/// Copy the prefix of the most recently received frame and return its full length.
/// Internal bring-up hook; the snapshot is captured without UART I/O in the RX path.
#[doc(hidden)]
pub fn last_rx(out: &mut [u8]) -> usize {
    with_bridge(|b| {
        let copied = b.last_rx_len.min(FRAME_PREFIX).min(out.len());
        out[..copied].copy_from_slice(&b.last_rx_prefix[..copied]);
        b.last_rx_len
    })
}

fn rx_pop(into: &mut [u8]) -> Option<usize> {
    with_bridge(|b| {
        if b.rx_count == 0 {
            return None;
        }
        let slot = b.rx_head;
        let n = b.rx_len[slot];
        let copied = n.min(into.len());
        into[..copied].copy_from_slice(&b.rx[slot][..copied]);
        b.rx_head = (b.rx_head + 1) % RX_DEPTH;
        b.rx_count -= 1;
        Some(copied)
    })
}

/// Copy and remove the oldest frame received from the vendor data path.
/// Internal bring-up hook used by packet-level HIL checks.
#[doc(hidden)]
pub fn take_received(out: &mut [u8]) -> Option<usize> {
    rx_pop(out)
}

/// IPv4 configuration returned by a DHCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DhcpConfig {
    /// Assigned IPv4 address.
    pub address: [u8; 4],
    /// Prefix length associated with `address`.
    pub prefix_len: u8,
    /// Default router, if supplied by the server.
    pub router: Option<[u8; 4]>,
}

/// Run smoltcp's DHCPv4 client over the real vendor L2 seam.
///
/// This bounded bring-up helper owns a temporary interface/socket set and
/// returns after the first lease or `timeout_ms`.
#[doc(hidden)]
pub fn dhcp_probe(mac: [u8; 6], timeout_ms: u32) -> Option<DhcpConfig> {
    use smoltcp::iface::{Config, Interface, SocketSet, SocketStorage};
    use smoltcp::socket::dhcpv4;
    use smoltcp::wire::{EthernetAddress, HardwareAddress};

    let mut device = Ws63Device;
    let mut config = Config::new(HardwareAddress::Ethernet(EthernetAddress(mac)));
    config.random_seed = 0x5753_3633;
    let mut interface = Interface::new(config, &mut device, Instant::from_millis(0));
    let mut storage = [SocketStorage::EMPTY; 1];
    let mut sockets = SocketSet::new(&mut storage[..]);
    let handle = sockets.add(dhcpv4::Socket::new());

    #[cfg(target_arch = "riscv32")]
    let started_at = crate::uapi::monotonic_ms();
    let mut elapsed = 0_u32;
    loop {
        let now = Instant::from_millis(elapsed as i64);
        let _ = interface.poll(now, &mut device, &mut sockets);
        if let Some(dhcpv4::Event::Configured(config)) =
            sockets.get_mut::<dhcpv4::Socket>(handle).poll()
        {
            return Some(DhcpConfig {
                address: config.address.address().octets(),
                prefix_len: config.address.prefix_len(),
                router: config.router.map(|address| address.octets()),
            });
        }
        if elapsed >= timeout_ms {
            break;
        }
        crate::osal::osal_msleep(10);
        #[cfg(target_arch = "riscv32")]
        {
            elapsed = crate::uapi::monotonic_ms()
                .wrapping_sub(started_at)
                .min(u32::MAX as u64) as u32;
        }
        #[cfg(not(target_arch = "riscv32"))]
        {
            elapsed = elapsed.saturating_add(10);
        }
    }
    None
}

fn tx_emit(frame: &[u8]) {
    let sink = with_bridge(|b| {
        let n = frame.len().min(MTU);
        b.tx_buf[..n].copy_from_slice(&frame[..n]);
        b.tx_len = n;
        b.tx_count = b.tx_count.wrapping_add(1);
        if has_udp_ports(frame, 68, 67) {
            b.tx_dhcp_client_packets = b.tx_dhcp_client_packets.saturating_add(1);
        }
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
        b.tx_len
    })
}

// ── smoltcp Device ───────────────────────────────────────────────────────────

/// A smoltcp [`phy::Device`] backed by the bridge's RX queue + TX sink.
pub struct Ws63Device;

/// RX token carrying one dequeued frame (owns its bytes — no borrow of `Device`).
pub struct RxFrame {
    len: usize,
}

/// TX token: writes into an MTU buffer, then hands it to the bridge TX sink.
pub struct TxBuf;

impl phy::Device for Ws63Device {
    type RxToken<'a> = RxFrame;
    type TxToken<'a> = TxBuf;

    fn receive(&mut self, _t: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let claimed = cs::with(|cs| {
            let claim = RX_SCRATCH_CLAIMED.borrow(cs);
            if claim.get() {
                false
            } else {
                claim.set(true);
                true
            }
        });
        if !claimed {
            return None;
        }
        let tx_claimed = cs::with(|cs| {
            let claim = TX_SCRATCH_CLAIMED.borrow(cs);
            if claim.get() {
                false
            } else {
                claim.set(true);
                true
            }
        });
        if !tx_claimed {
            cs::with(|cs| RX_SCRATCH_CLAIMED.borrow(cs).set(false));
            return None;
        }
        let scratch = unsafe { &mut *RX_SCRATCH.0.get() };
        match rx_pop(scratch) {
            Some(len) => Some((RxFrame { len }, TxBuf)),
            None => {
                cs::with(|cs| {
                    RX_SCRATCH_CLAIMED.borrow(cs).set(false);
                    TX_SCRATCH_CLAIMED.borrow(cs).set(false);
                });
                None
            }
        }
    }

    fn transmit(&mut self, _t: Instant) -> Option<Self::TxToken<'_>> {
        cs::with(|cs| {
            let claim = TX_SCRATCH_CLAIMED.borrow(cs);
            if claim.get() {
                None
            } else {
                claim.set(true);
                Some(TxBuf)
            }
        })
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
        let result = f(&unsafe { &*RX_SCRATCH.0.get() }[..self.len]);
        cs::with(|cs| RX_SCRATCH_CLAIMED.borrow(cs).set(false));
        result
    }
}

impl phy::TxToken for TxBuf {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let n = len.min(MTU);
        let scratch = unsafe { &mut *TX_SCRATCH.0.get() };
        let r = f(&mut scratch[..n]);
        tx_emit(&scratch[..n]);
        cs::with(|cs| TX_SCRATCH_CLAIMED.borrow(cs).set(false));
        r
    }
}

impl Drop for RxFrame {
    fn drop(&mut self) {
        cs::with(|cs| RX_SCRATCH_CLAIMED.borrow(cs).set(false));
    }
}

impl Drop for TxBuf {
    fn drop(&mut self) {
        cs::with(|cs| TX_SCRATCH_CLAIMED.borrow(cs).set(false));
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
        b.rx_dropped = 0;
        b.rx_high_watermark = 0;
        b.rx_icmp_echo_replies = 0;
        b.rx_icmp_sequence_mask = 0;
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

#[cfg(test)]
mod stack_tests {
    use super::{MTU, RX_DEPTH, RxFrame, TxBuf, rx_push, rx_queue_diagnostics};

    #[test]
    fn device_tokens_do_not_embed_mtu_sized_stack_buffers() {
        assert!(core::mem::size_of::<RxFrame>() <= 16);
        assert_eq!(core::mem::size_of::<TxBuf>(), 0);
    }

    #[test]
    fn dhcp_diagnostics_match_only_ipv4_udp_port_direction() {
        let mut frame = [0_u8; 42];
        frame[12..14].copy_from_slice(&[0x08, 0x00]);
        frame[14] = 0x45;
        frame[23] = 17;
        frame[34..36].copy_from_slice(&68_u16.to_be_bytes());
        frame[36..38].copy_from_slice(&67_u16.to_be_bytes());
        assert!(super::has_udp_ports(&frame, 68, 67));
        assert!(!super::has_udp_ports(&frame, 67, 68));

        frame[23] = 1;
        assert!(!super::has_udp_ports(&frame, 68, 67));
        assert!(!super::has_udp_ports(&frame[..20], 68, 67));
    }

    #[test]
    fn full_rx_queue_counts_loss_and_high_watermark() {
        super::with_bridge(|bridge| {
            bridge.rx_head = 0;
            bridge.rx_count = 0;
            bridge.rx_dropped = 0;
            bridge.rx_high_watermark = 0;
        });

        let mut frame = [0_u8; MTU];
        frame[12..14].copy_from_slice(&[0x08, 0x00]);
        frame[14] = 0x45;
        frame[23] = 1;
        frame[34] = 0;
        frame[38..40].copy_from_slice(&super::DIAGNOSTIC_ECHO_IDENTIFIER);
        frame[40..42].copy_from_slice(&3_u16.to_be_bytes());
        rx_push(&frame);
        frame.fill(0);
        for _ in 1..RX_DEPTH {
            rx_push(&frame);
        }
        rx_push(&frame);

        let diagnostics = rx_queue_diagnostics();
        assert_eq!(diagnostics.depth, RX_DEPTH);
        assert_eq!(diagnostics.pending, RX_DEPTH);
        assert_eq!(diagnostics.high_watermark, RX_DEPTH);
        assert_eq!(diagnostics.dropped, 1);
        assert_eq!(diagnostics.icmp_echo_replies, 1);
        assert_eq!(diagnostics.icmp_sequence_mask, 1 << 3);

        super::with_bridge(|bridge| {
            bridge.rx_head = 0;
            bridge.rx_count = 0;
            bridge.rx_dropped = 0;
            bridge.rx_high_watermark = 0;
            bridge.rx_icmp_echo_replies = 0;
            bridge.rx_icmp_sequence_mask = 0;
        });
    }
}
