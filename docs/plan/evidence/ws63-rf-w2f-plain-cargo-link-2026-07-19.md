# WS63 W2F Plain Cargo Link Evidence (2026-07-19)

## Scope

This evidence closes the build/distribution and first-silicon-execution part of
W2F. It proves that the upstream hostap firmware can consume Cargo-delivered,
normalized radio artifacts and complete one stock-`rust-lld` link without the
guarded two-pass script, a vendor SDK checkout, or an external RISC-V C toolchain
on the consumer machine.

It does **not** claim a Personal-mode association. The silicon gate deliberately
uses public fixture credentials and stops after init/scan, so transition-mode
connect parity and the controlled pure-WPA3 SAE+PMF matrix remain separate gates.

## Release Unit

`ws63-radio-sys v0.1.0-alpha.2` is one versioned release unit containing:

- `hisi-rf-link`;
- `ws63-radio-blob`;
- `ws63-radio-sys`.

Tag `v0.1.0-alpha.2` triggered GitHub Actions run `29684445921`, which packaged
and published the crates in dependency order, waiting for each exact version to
become visible on crates.io before publishing the next. All three versions are
available from crates.io.

The subsequent release-hardening commit `b9e9686` added a CI gate that rebuilds
all normalized vendor archives from the pinned `ws63-RF` submodule and compares
bytes, SHA-256, size and relocation counts with the Cargo payload. Child run
`29685140772` passed. The two pinned upstream hostap target archives are compiled
from the complete source profiles and checked for ABI/symbol drift; making their
archive bytes reproducible under one downloadable, exactly pinned C compiler is
still release hardening work, not a consumer dependency.

## Cross-OS Consumer Build

Parent commit `074143838` passed GitHub Actions run `29685710936` with the same
ordinary Cargo build and final-ELF verifier on:

- Ubuntu x86_64: upstream WPA2 and WPA3 profiles;
- macOS arm64: upstream WPA3 profile;
- Windows x86_64, native PowerShell runner: upstream WPA3 profile.

The verifier established:

- `.patch` contains 37 ROM patch entries;
- `.wifi_pkt_ram` is the expected `NOBITS` region;
- relocation types 58, 59 and 61 are absent from the final ELF;
- upstream hostap object markers are reachable;
- vendor supplicant, vendor mbedTLS/LiteOS libc and legacy provider symbols are
  not reachable.

No matrix job invoked Bash, Python, GCC or binutils from the Cargo build path.
The uv verifier runs after linking and is a CI assertion, not a build dependency.

## Silicon Init/Scan Gate

The executable contract is:

```console
PORT=<uart> PROBE_SPEED=3000 \
WS63_CONNECTIVITY_PROFILE=upstream-wpa3 \
WS63_CONNECTIVITY_EXPECT=init-scan \
bash hil/ws63-connectivity-smoke.sh
```

`init-scan` supplies only public fixture values and preserves the same clean
temporary target, final-ELF checks, `hisi-fwpkg` FlashPlan, complete probe-rs
verify, J-Link nRST and UART capture used by the full connectivity gate.

The formal run completed with:

- clean Cargo build: 22.62 s;
- final ELF: 37 ROM patches, zero vendor relocations, upstream supplicant present;
- 3 MHz planned-bin download with full verify: 93.89 s, first attempt;
- `RF1_IMAGE_OK`;
- `RF2_INIT_OK ifname=hisi-rf` and initialized event;
- `RF3_SCAN_OK count=0x0000000d truncated=0x00000001` and scan-completed event;
- `W2D_NATIVE_RUNNER_RX_READY`;
- no fatal connectivity marker;
- `WS63 RADIO INIT/SCAN SMOKE: PASS` and process exit 0.

An immediately preceding clean run also completed full verify at 3 MHz in
129.11 s and reached init, a nine-AP scan and native-runner readiness. Its full
connect assertion failed by construction because the fixture SSID does not
exist; that result is not counted as a WPA failure.

The flashboot `Flash Init Fail! ret = 0x80001341` line remains an expected board
boot observation and did not prevent the verified application image from
starting.

## Remaining W2F Gates

- Run the same plain-Cargo lane through the complete transition-mode
  connect/DHCP/ARP/ping/renew contract with credentials supplied outside logs.
- Run and stabilize the controlled pure-WPA3 SAE+required-PMF reset matrix.
- Only after those parity gates may the vendor guarded oracle and
  `wpa_compat.rs` leave the default migration window.
- Pin a downloadable maintainer C toolchain and reproduce the two hostap target
  archive bytes in release CI; consumer builds must remain toolchain-free.
