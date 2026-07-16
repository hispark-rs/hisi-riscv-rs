# WS63 W2E-H SPACC AES Evidence - 2026-07-16

## Scope

This evidence closes the AES key-wrap/CMAC step of W2E-H for the upstream-native
WPA path. It does not claim that PKE P-256/SAE is hardware-backed.

The implementation keeps protocol logic in pinned upstream hostap. Its
`aes-unwrap.c` and `aes-omac1.c` continue to call the narrow `aes_encrypt` /
`aes_decrypt` ABI; the WS63 shim now routes those calls through
`hisi_crypto::TryBlockCipher` to `hisi-crypto-ws63`.

## Hardware Boundary

- SPACC symmetric channel 1 uses two-entry input/output DMA rings.
- AES keys are loaded through KM/KLAD into one explicitly locked MCipher keyslot.
- AES-128, AES-192, and AES-256 key-length encodings are `1`, `2`, and `3`.
- Every channel, KLAD, keyslot, and operation wait has a bounded poll contract.
- Hardware failure is returned to hostap; there is no RustCrypto fallback.
- DMA buffers and KLAD staging registers are cleared after the engine is proven
  stopped. A channel-clear failure leaves channel/keyslot ownership fail-closed.

The sequence was checked against the Apache-2.0 WS63 `security_unified` driver.
The independent Rust structure and attribution are recorded in the
`hisi-crypto-ws63` `NOTICE` file.

## Register Diagnosis

The first HIL attempt failed with backend code `0xffff1309` at the KLAD lock
gate. Read-only inspection showed:

```text
KL_COM_LOCK_INFO   = 0x0000002a
KL_COM_LOCK_STATUS = 0x000000aa
```

The owner status proved that the lock succeeded. The two-bit fail fields use a
redundant encoding: the vendor driver rejects the exact value `1`, rather than
all non-zero values. Matching that documented behavior fixed the false timeout.

## Verification

- CMSIS-SVD schema validation passed for WS63 and BS2X.
- Deterministic `ws63-pac` regeneration, build, and clippy passed.
- `hisi-crypto-ws63` host tests: 10 passed.
- RV32 check and clippy with diagnostics passed.
- Guarded upstream WPA2 and WPA3 link profiles both passed: 1,157 layout
  sections, 4,127 patched relocations, and 37 ROM patches.
- The on-silicon diagnostic runs NIST AES-128/192/256 encrypt/decrypt vectors
  plus a bounded lock-timeout recovery check before radio initialization.

The final upstream WPA2 image completed association, two EAPOL RX/TX exchanges,
DHCP, ARP, and 5/5 public ICMP replies. Its cipher diagnostics were:

```text
requests=36 failures=0 max_ms=1 recovery_tests=1 recovery_failures=0
```

The unchanged image then ran 20 J-Link nRST connect trials:

- association: 20/20;
- `WLAN_AUTH_RSP2_TIMEOUT`: 0;
- AES requests: 720/720 successful;
- recovery self-tests: 20/20 successful;
- per-operation observed maximum: 1 ms.

After tightening timeout cleanup to clear the channel before scrubbing DMA
storage, a newly linked and fully verified image again completed association,
EAPOL, DHCP, lease renewal, and public ICMP. Cipher diagnostics remained 36/36
successful with the recovery test passing and a 1 ms observed maximum.

The committed implementation also passed the controlled upstream WPA3
transition smoke using ephemeral credential injection and a temporary target
directory. SAE completed with required PMF, EAPOL completed, DHCP renewed, and
both gateway and public ICMP returned 5/5 replies. SPACC AES handled 91/91 block
operations with zero failures and a 1 ms observed maximum. The script removed
the credential file and temporary build tree after the run.

The matrix artifacts were written to
`/private/tmp/ws63-spacc-aes-wpa2-matrix`; this path is local evidence, not a
checked-in artifact or long-term HIL runner.

## Remaining Gates

- add true cross-owner contention injection before stabilizing the backend;
- replace internal DMA scratch with caller-provided static storage;
- move WPA3 SAE P-256/Dragonfly to the PKE backend with separate vectors and HIL;
- publish the SVD/PAC version before publishing a dependent crypto crate.
