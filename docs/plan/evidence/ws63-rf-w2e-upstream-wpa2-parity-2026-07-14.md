# WS63 W2E Upstream WPA2 Parity Evidence

Date: 2026-07-14

Scope: prove that the pinned upstream hostap 2.11 path completes the frozen A4
WPA2 connectivity slice on real WS63 silicon. This evidence does not claim
WPA3 SAE/PMF or transition-mode support.

## Revisions

- Parent: `1028114da` (`fix(rf): stabilize native supplicant event drains`).
- `ws63-radio-sys`: `bd8069b` (`fix(hostap): distinguish EAPOL feed failures`).
- `hisi-rf`: `fac1fe0` (`docs(radio): define runner fairness contract`).
- Examples: `145727f` (`fix(wifi): yield between radio runner batches`).
- ELF SHA-256: `eedf61e4c9245c333125cc2a0744d849ed82d7ebe7af51a6b7cb221852d5a23e`.
- FWPKG SHA-256: `da48bed3e507cb94ba3832a3bb2a6d11d651d03c79dd0f34ac924ad126ccb8d1`.

## Root Cause Closed

The first native protected-connect runs reported `0x57324fff`. Instrumentation
showed that `hisi_wpa_feed_eapol` returned success; the failing value came from
the next WS63 receive ioctl. The delivered SDK disassembly for
`uapi_ioctl_receive_eapol` returns the positive 16-bit sentinel `0xffff` when
the skb queue is empty. The vendor LiteOS `l2_packet_rtos.c` loop treats that
final empty receive as normal batch termination.

The Rust adapter now accepts only `0` as a frame and `0xffff` as end-of-batch.
Every other non-zero value remains an error. This is deliberately narrower than
treating all negative or non-zero statuses as an empty queue. Temporary trap and
FFI instrumentation was removed before the final build.

## Host And Target Gates

```text
cargo test -p ws63-rf-rs --lib --target aarch64-apple-darwin \
  --features upstream-supplicant-port,net
33 passed; 0 failed

cargo test --target aarch64-apple-darwin  # crates/hisi-rf
7 passed; 0 failed

cargo test --workspace --target aarch64-apple-darwin  # ws63-radio-sys
9 passed; 0 failed

uv run scripts/check-native-supplicant-port.py
native supplicant profile: 42 RV32 objects, 15 defines, external ABI locked

cargo clippy -Zbuild-std=core,alloc -p ws63-rf-rs \
  --target riscv32imfc-unknown-none-elf \
  --features upstream-supplicant-port,net -- -D warnings
PASS
```

The guarded two-pass link verified 1157 layout sections, patched 4127
relocations and applied 37 ROM patches.

## Real-Silicon Result

The same cleaned image was flashed once, then booted three times using J-Link
nRST and 115200-baud UART capture. All three runs produced the WPA2 parity
markers. Representative output:

```text
W2D_WPA2_CONNECT_OK freq=0x00000000
A4_RADIO_EVENT kind=connected
RF5A_DHCP_OK addr=192.168.155.8 prefix=0x00000018 router=192.168.155.1
RF5A_ARP_OK mode=smoltcp-neighbor-cache
RF5C_PING_OK target=1.1.1.1 tx=0x00000005 rx=0x00000005 \
  drop=0x00000000 tx_error=0x00000000 loss_pct=0x00000000
RF5C_CONNECTIVITY_SUMMARY gateway_tx=0x00000005 gateway_rx=0x00000000 \
  public_tx=0x00000005 public_rx=0x00000005 rx_queue_drop=0x00000000
A4_DHCP_RENEW_OK client=0x00000001 server=0x00000001
```

The gateway did not answer ICMP, matching the previously recorded reference-host
boundary on this AP; public ICMP, DHCP renewal and zero RX queue drops prove the
data path remained live. `Flash Init Fail! ret = 0x80001341` is an expected ROM
message on this board and was not used as a pass or fail marker.

## Remaining Gate

W2E remains partial. Before W2 can close, the host gate still needs EAPOL/RSNE/
SAE/PMF golden vectors or pcap replay, followed by controlled WPA3-only SAE+PMF
and WPA2/WPA3 transition-mode HIL. The WPA2 test AP and its credentials are not
WPA3 evidence and are not recorded in this artifact.
