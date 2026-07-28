# A5UX Caller-Owned Radio Storage Evidence

## Scope

This evidence freezes the public ownership shape for WS63 radio control state
and the shared RF arena. It is API, link-layout, package, and cross-host build
evidence; it does not replace Wi-Fi connectivity HIL.

## Frozen Contract

- An application declares one `RadioStorage<Profile, EVENTS>` through
  `declare_radio_storage!`.
- `install()` is one-shot and returns an installed capability.
- `into_init_parts()` exposes the typed control storage and arena capability
  required by the composition root without creating a second owner.
- Control state remains in ordinary writable memory. The large arena backing
  remains in `.hisi.shared-arena` as NOLOAD. The split is a physical linker
  requirement hidden behind one logical owner.
- RTOS allocation hooks use `InstalledRadioStorage`; an uninstalled or duplicate
  owner cannot become the allocator source.

## WS63 Target Model

The versioned `hisi-rf-resource-report/v6` report uses explicit WS63 RV32 layout
constants guarded by target-side compile-time size assertions. It does not use
the pointer width of the macOS, Linux, or Windows host generating the report.

For `wifi-wpa2-smoltcp` with eight event slots:

| Item | Bytes |
| --- | ---: |
| control storage | 8,544 |
| radio state | 2,168 |
| crypto DMA | 4,384 |
| aligned arena backing | 303,168 |
| usable shared RF arena | 303,104 |
| immutable composition handle | 0 |
| total caller-owned storage | 311,712 |

The 64-byte difference between backing and usable arena is explicit alignment
and claim metadata, not hidden heap use.

## Release And Verification

- `hisi-rf-ws63 0.1.0-alpha.43`, commit `7c1c6a6`, was published after CI run
  `30396521380` passed host tests, RV32 checks, package checks, public API gates,
  and Linux/macOS/Windows final-link consumers.
- `hisi-rf 0.1.0-alpha.52`, commits `4cdac81` and `6fd60b4`, was published by run
  `30398107514` after CI run `30397686357` passed the facade API gate and
  crates.io-only WPA2/WPA3 consumers on Linux, macOS, and Windows.
- The WS63 `wifi_connectivity` example builds both WPA2 and WPA3 release images
  against the published facade.
- Template commit `48d88ee` generates one storage composition. A generated
  project in a path containing spaces and non-ASCII characters completed a
  release build and emitted the target-model values above. Template CI run
  `30398671812` passed WS63/BS21 generation plus Linux, macOS, and Windows
  resource-report jobs before tag `v0.7.0-alpha.13` was created.

No network credential or credential-bearing firmware is part of this evidence.
