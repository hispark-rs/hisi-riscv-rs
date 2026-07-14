# WS63 W2D Native Runner And RX Bridge Evidence

Date: 2026-07-14

Scope: prove that the pinned upstream hostap context is owned and advanced by
the native `hisi-rf` / `RadioRunner` path on real WS63 silicon. This evidence
does not claim protected-network association or WPA parity.

Status: historical intermediate evidence. The remaining protected-connect gate
was later closed by [W2E upstream WPA2 parity](ws63-rf-w2e-upstream-wpa2-parity-2026-07-14.md).

## Revisions

- Parent: `7e67f145d` (`feat(rf): bridge native supplicant RX to runner`).
- Examples: `0b8d3dc` (`feat: exercise native supplicant runner on silicon`).
- `hisi-crypto-ws63`: `b8d11db` (`feat: gate WS63 crypto capabilities`).
- `ws63-radio-sys`: `59d5ce0` (opaque context size/alignment ABI).
- `hisi-rf`: `b357aff` (backend polling from `RadioRunner`).

## Contracts Closed

- `NativeSupplicant` owns exact-size/alignment C storage and releases it in
  `destroy -> free` order.
- WS63 management RX callbacks deep-copy at most 768 bytes into an eight-slot
  FIFO, then wake the runner. Queue overflow is observable, not silent.
- EAPOL callbacks only set a pending flag and wake the runner. The runner uses
  WS63 commands 6/7/8 to enable, drain and disable the bounded receive path.
- Only `RadioRunner::run_once` feeds management/EAPOL frames into hostap.
- The upstream profile explicitly selects RustCrypto PBKDF2 and WS63 TRNG;
  vendor WPA archives are not linked and no failure-triggered fallback exists.

## Host And Link Gates

```text
cargo test -p ws63-rf-rs --lib --target aarch64-apple-darwin \
  --features upstream-supplicant-port,net
27 passed; 0 failed

cargo clippy -Zbuild-std=core,alloc -p ws63-rf-rs \
  --target riscv32imfc-unknown-none-elf \
  --features upstream-supplicant-port,net -- -D warnings
PASS

WS63_RF_FEATURES=full-init,upstream-supplicant \
  chips/ws63/rf/tools/rf-build-full-init-lld-layout-patch.sh
verified layout sections: 1157
patched relocations: 4127
ROM patches: 37
```

## Real-Silicon Result

The planned bin image was downloaded through probe-rs, followed by J-Link nRST
and 115200-baud UART capture. Download completed in 23.19 seconds.

```text
RF1_IMAGE_OK
RF2_INIT_BEGIN
RF2_INIT_OK ifname=hisi-rf
A4_RADIO_EVENT kind=initialized
RF3_SCAN_BEGIN
RF3_SCAN_OK count=0x00000011 truncated=0x00000001
A4_RADIO_EVENT kind=scan-completed
W2D_NATIVE_RUNNER_RX_READY
```

The scan included both `HUAWEI-HLJ_Guest` and `HUAWEI-HLJ`. No native context,
EAPOL-enable, runner-poll, exception or queue-overflow error was reported.

## Remaining Gate

W2D remains incomplete. `driver_ws63` still needs auth/assoc request mapping,
corresponding vendor event translation and protected-network connect. W2E then
requires upstream-native WPA2 connect, DHCP, ARP, repeated ping and lease-renew
parity before WPA3-only SAE+PMF or transition-mode evidence is accepted.
