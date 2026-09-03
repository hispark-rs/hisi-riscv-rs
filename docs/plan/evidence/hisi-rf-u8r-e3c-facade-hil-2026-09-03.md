# hisi-rf U8R-E3c Facade HIL Evidence (2026-09-03)

## Scope

This evidence closes the fixed-image silicon parity gate for the opaque Wi-Fi
device and diagnostics facade released in `hisi-rf 0.1.0-alpha.114`. It proves
the WPA2 SoftAP/STA integration built from one clean parent commit and its exact
recorded submodules. It does not reverse the U8 no-go graduation decision,
stabilize coexistence, or prove that an external RF environment cannot lose
traffic.

## Source Closure And Artifacts

The final build used a clean detached worktree at parent commit
`b70186139c8f45bd7908b58e3942642be8de8528`. Its relevant pinned inputs were:

```text
hisi-rf=c6812fc8c1115db6cb10b4c8d01b08a71251c35a (0.1.0-alpha.114)
hisi-rf-ws63=a8005de80b5b61ac9d50a5d6a4bf79ada15a2e90
hisi-rf-core=b5db5e27cf55484d8c9f4df390b81efa2771cc88
hisi-rf-rtos-driver=f36280b51d93f8cb4e00962e3a450f6b8322e8bd
hisi-rtos=3c49a1e5f61d931a1604e41d37edf5a7e889cd72 (0.1.0-alpha.25)
ws63-radio-sys=176774a466dac7ecc37d0d64d87ccae18ec0ca14
hisi-hal=996eaa08858f855f9779354e0b4183862dd61844
ws63-examples=9649c1d6061f6c5dc803587baccfda0ade7d899f
```

The immutable artifact identities were:

| Role | Profile | ELF SHA-256 |
|---|---|---|
| SoftAP | `facade-alpha114-wpa2-softap` | `f1ccf42b169df42cc996e5af3a19dc78860d5a051267ebaddaa681e583728c38` |
| STA | `facade-alpha114-wpa2-sta` | `7fefd18bc0c2d59eb3592e41c8ff81c50c62d67e05953ef1d50bf2d85e032c3d` |

Both ELF files were converted through the canonical `hisi-fwpkg` FlashPlan bin
path and downloaded with probe-rs at 3 MHz with full readback verification on
the first attempt. SoftAP took 83.13 seconds and STA took 91.18 seconds.

## Silicon Matrix

After flashing once, the same images passed a strict paired-board 3-reset shape
gate and a fresh 20-reset matrix. Every generation reset the SoftAP first,
waited for peer readiness, then reset and measured the STA. The 20-run result
was:

- 20/20 complete contract passes and 20/20 peer-ready generations;
- zero authentication-response-2 timeouts;
- exactly 200/200 unique local UDP echo replies at the STA, with 13 IPv4 RX
  packets per run: three DHCP responses plus ten echo replies;
- complete A5B metrics in 20/20 runs, zero event drops, zero runner errors, and
  a maximum runner step of 38 ms against the 100 ms gate;
- complete resource contracts in 20/20 runs, zero RTOS/RF allocation failures,
  16,192 bytes of runtime headroom and at least 53,540 bytes of RF headroom;
- IRQ45 enabled with no terminal pending state in every run;
- `A5R_READY_OWNERSHIP_OK`, with ready-owner, duplicate-membership,
  wrong-bucket, invalid-link, detached-priority and detached-policy counters all
  zero in every run.

The resource report still records `calibrated=0`; this matrix proves the
explicit arena/headroom contract and absence of violations, not a wider claim
that every profile resource constant has graduated to a calibrated stable API.

The SoftAP per-packet UART markers captured only 102/200 observed/submitted
sequence pairs because long peer diagnostics interleaved and truncated lines.
All STA sequences were recovered, `submitted_without_sta_receive` was zero, and
the complete SoftAP final counters that survived capture reported `echo_rx=10`
and `echo_tx=10`. This is a UART observability limit, not evidence of 98 missing
wire packets.

Local artifacts are retained at:

```text
/private/tmp/ws63-e3c-clean-b70186139-20260903/reset3
/private/tmp/ws63-e3c-clean-b70186139-20260903/reset20
```

## Decision

U8R-E3c and the U8R remediation window are complete. The public snapshots,
host and package checks, three-OS crates.io-only consumers, examples, template,
and now fixed-image Wi-Fi HIL all agree on the alpha.114 opaque facade boundary.
The U8 no-go decision remains in force: a future stable graduation requires a
new product/API review and its own evidence gate.
