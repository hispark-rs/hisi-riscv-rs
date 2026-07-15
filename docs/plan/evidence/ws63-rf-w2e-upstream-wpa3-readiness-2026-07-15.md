# W2E Upstream WPA3 Readiness

Date: 2026-07-15

## Scope

This evidence covers the build, ABI, and regression gates needed before a
controlled WPA3 HIL run. It does not claim a successful SAE exchange on silicon:
the available `HUAWEI-HLJ_Guest` fixture advertised WPA2-Personal to the WS63 scan
path, so the typed configuration rejected it before authentication.

## Source And ABI Closure

The source remains the pinned upstream hostap 2.11 tree at commit
`d945ddd368085f255e68328f2d3b020ceea359af`. The new
`personal-wpa3` profile adds only the group-19 SAE/Dragonfly objects and compile
definitions required by WPA3-Personal. `check-native-supplicant-port.py` verified:

```console
native supplicant profile personal: 42 RV32 objects, 15 defines, external ABI locked
native supplicant profile personal-wpa3: 46 RV32 objects, 18 defines, external ABI locked
```

The target adapter supplies the 41 bignum/P-256 symbols referenced by that exact
profile through the narrow `hisi-crypto` SAE capabilities. It accepts only IKE
group 19, obtains caller-visible entropy from the WS63 TRNG capability, and keeps
SHA/HMAC/AES/PBKDF2 on the explicitly selected RustCrypto profile. This is a mixed
backend, not a complete hardware-acceleration claim.

## Final Image Closure

The guarded two-pass rust-lld flow built
`wifi_init_smoke --features full-init,upstream-supplicant,upstream-wpa3` with an
ephemeral target directory. The final checks reported:

- 1157 oracle/final RF layout sections matched;
- no unresolved self-call placeholders remained;
- 37 mask-ROM patches were generated;
- the complete image hash was patched successfully;
- the real WS63 TRNG/UAPI path linked without a vendor WPA archive.

Host and target checks also passed: 33 `ws63-rf-rs` host library tests, the two
native profile ABI checks, RV32 clippy with warnings denied, workflow lint, and
shell syntax checks.

## Silicon Results

The `upstream-wpa3` transition-profile HIL booted, initialized RF, and completed
an 11-BSS scan. The selected Guest BSS was not reported as WPA3 or transition, so
the firmware emitted:

```text
W2D_NATIVE_RUNNER_RX_READY
W2E_WPA3_CONFIG_ERR:scan-security
```

No SAE Authentication frame was sent. This is a controlled fail-closed result,
not an SAE failure and not WPA3 HIL evidence.

The same board and AP then passed the `upstream-wpa2` regression: association,
DHCP, ARP neighbor discovery, 5/5 public ICMP replies, zero RX queue drops, steady
network runner, and DHCP renewal all produced their expected markers.

## Remaining Gate

- provide a controlled WPA3-only AP and run SAE + required-PMF HIL;
- provide a confirmed WPA2/WPA3 transition BSS and run the transition HIL;
- retain the W2E-H per-capability hardware migration gate before declaring stable
  WPA3-SAE or complete WPA hardware acceleration.
