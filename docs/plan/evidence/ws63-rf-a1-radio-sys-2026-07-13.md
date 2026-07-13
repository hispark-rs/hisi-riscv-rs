# WS63 RF A1 Radio Sys And Link Migration - 2026-07-13

## Scope

This evidence records the extraction of the language-neutral vendor payload,
archive profile, ABI paths and post-link transforms from the transitional RF
crate/examples into the independent `ws63-radio-sys` repository. It does not
claim that A1 is complete: the WS63 crypto hardware adapter still needs its own
release unit.

## Ownership

- `ws63-radio-sys` commit `5cf66089c7bd7f8b3e1c4af67f450ab974853b5d`
  nests `ws63-RF` commit `d01274cdd1bd42aeb417ade97eee108c1f7b9440`.
- `ws63-radio-sys` owns archive order, whole-archive policy, root symbols and
  Cargo `links` metadata for blob/include/ROM paths.
- `hisi-rf-link` owns the versioned relocation, layout verification and mask-ROM
  patch implementations. The parent retains only build orchestration.
- `hisi-rom-sys` remains the generated ROM-symbol fact crate; `hisi-fwpkg`
  remains the image/header/hash fact source.
- The old `chips/ws63/rf/ws63-RF` gitlink and duplicate parent Python tools were
  removed. Consumers no longer enumerate vendor archives or reach laterally
  into the transitional RF crate.

## Build And Link Evidence

- Independent sys/link tests, Clippy and package preflight pass; relocation KATs
  run in the new repository CI.
- `wifi_blob_link`, `ws63-rf-rs` and `wifi_init_smoke` build through the exported
  Cargo metadata contract.
- WPA mode is fail-closed unless an explicit WPA2 profile archive is supplied;
  it cannot silently fall back to the full mbedTLS-bearing supplicant archive.
- Guarded stock-rust-lld flow verified 1,486 final layout sections, patched 5,335
  participating vendor relocations, rejected unresolved self-call placeholders,
  generated 37 mask-ROM patches and resolved `frw_rom_cb_register` to `0x128d4a`.

| Artifact | SHA-256 |
| --- | --- |
| WPA2-Personal archive | `891c195279d768ce5664e16fecac95f353fd2217c4e3590315baa4cb6e7f25a2` |
| final ELF | `cde854338e66017bb914740757bedf63b6124a771a75786d456176110455d663` |
| final map | `4b896b685926a310188c6a1fb784e226a4410d1a4abc9610890d8a2885974ed4` |
| relocation manifest | `611cf554ed7279fbdf23ca5f4e06e3f6efee64d56b6c500279076a0ffe5a5a4c` |
| ROM patch report | `46a0884cfb0c41859009c94477e3e1f7b6d81f1e3e000faf1dc8cb8b06bce71d` |
| canonical image | `e989cfe296c1e4b3153529a51478ca26d6286154d0e5db936eeceb2456484cb6` |
| FlashPlan JSON | `de6c9cd14403e74591174985d9ddcfd6819edd5c4075a433b0967451a0eeac3a` |
| app-only FWPKG | `bc61d7a06588d72360591ab1eb4f6af233e1f092424016b0e3ab20a13157b7e4` |

## Silicon Parity

The WPA2 credential was injected only at build time. The canonical image was
transferred at 115200 baud with explicit loaderboot; UART was opened before a
physical J-Link nRST pulse.

```text
RF1_IMAGE_OK
RF2_INIT_OK ifname=wlan0
RF3_SCAN_OK count=0x0000001f
RF5B_WPA_CONNECT_OK freq=0x0000096c
RF5A_DHCP_OK addr=192.168.155.2 prefix=0x00000018 router=192.168.155.1
RF5A_ARP_OK rx=0x00000004
RF5C_PING_OK rx=0x00000005
```

This proves that both the new archive-selection contract and the packaged
post-link implementation ran in the final silicon image.

