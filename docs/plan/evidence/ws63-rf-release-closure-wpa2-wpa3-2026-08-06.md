# WS63 RF Release-Closure WPA2/WPA3 Evidence (2026-08-06)

## Scope

This evidence closes the profile-specific two-board HIL gate for the bounded
`hisi-rf` composition. WPA2 and pure-WPA3 are separate immutable ELF files built
from the same pinned parent closure; they are not two runtime modes of one image.

The tested repository closure was:

```text
hisi-rf-ws63=f41d62de4e6d4128967c0f5abce319dba73e5fb3 (0.1.0-alpha.71)
hisi-rf=3353b4edb41d2391fd1eadecc46e30b9d520e273 (0.1.0-alpha.83)
hisi-rf-core=50c650a2a44eda1d1d716cd873720559ccfcd8db
hisi-rf-rtos-driver=916b6f3c3b95db0d738ba02db18b95ee3ec1414d
hisi-rtos=ee20e150ad2b5d9fb00047691fd7539fa3354124 (0.1.0-alpha.23)
ws63-radio-sys=82b09c87748c118b7eaee5828e0686a80f2363e6
ws63-examples=34289bb32cc427a1aeb837ccb9b9a4cead037900
hisi-hal=43cf75520daee6daa5c7093d5b11694371491e75 (0.7.0-alpha.7 release-prep)
ws63-pac=37104c8481be979b3b3f7bd1d5dd11bb844393ee (0.4.4)
```

At test time the HAL entry was the exact release-prep commit and the PAC entry
was the commit later tagged as `v0.4.4`; neither dependency was resolved through
crates.io for this HIL build. The final HAL release adds only commit `ff706af`
to lock `ws63-pac 0.4.4`, so it does not change the tested target code. The
result proves the pinned parent release candidate; crates.io publication is
reported separately below.

## Artifacts And Flashing

Both profiles passed final-link inspection with 37 ROM patches, zero remaining
vendor relocations, and the upstream native supplicant reachable. Their
identities were:

| Profile | Role | ELF SHA-256 |
|---|---|---|
| pure-WPA3 | SoftAP | `41b9eec84a9f713c193ad6fbffd35215d1adcd996b252cc9f1f29b6c5ca753bf` |
| pure-WPA3 | STA | `d4af5ea61f600145dbc1a917fdcd7253f4a9b9f8cebc85860be5d8de0dda1515` |
| WPA2 | SoftAP | `9cefc2459daf4df4329d540ec44ec942019a6f3c90ad1095684f5d740bb8be14` |
| WPA2 | STA | `0ef7fa36f1367075b915182930598fac30e6e0c32906b8799ba5e915ab398bca` |

Each role was flashed once as a `hisi-fwpkg` planned binary image and verified
by full readback before the reset matrix. Pure-WPA3 completed at 3 MHz in 91.16
seconds (AP) and 99.00 seconds (STA). The first WPA2 AP attempt at 3 MHz failed
readback verification and the automatic 500 kHz retry then lost the debug
transport. J-Link nRST restored the board; verified 1 MHz retries completed in
140.03 seconds (AP) and 154.53 seconds (STA). This is a probe/download
reliability observation and was excluded from the connectivity matrix.

## Two-Board Matrices

After flashing, each matrix used paired J-Link nRST only; no firmware was
rewritten between runs.

| Contract | Result | STA echo | Auth response-2 timeout | Queue/backend errors |
|---|---:|---:|---:|---:|
| pure-WPA3 SAE+PMF | 20/20 | 200/200 | 0 | 0 |
| WPA2 Personal | 20/20 | 200/200 | 0 | 0 |

For both profiles:

- every run completed init, scan, association, DHCP, ARP/neighbor resolution,
  ten local UDP echo exchanges, steady-state runner evidence, and lease renew;
- the event queue ended empty, high-water was one, and drops were zero;
- all runner operations reached terminal states and runner errors were zero;
- the STA L2 counters were exactly three DHCP responses plus ten echo replies
  per run;
- no run had a zero-reply local-data-path result.

Per-echo AP UART markers undercounted the final AP counters because long
diagnostic lines can interleave on the serial stream: WPA3 captured 168/200 and
WPA2 164/200 marker pairs. The STA sequence de-duplication and each AP final
network counter proved 10 RX / 10 TX per run, so this is capture observability,
not packet loss.

## RTOS Ownership Causality Boundary

`hisi-rtos` alpha.23 contains the detached pending-target ownership fix and two
diagnostic counters for priority/policy mutation while a pending switch ticket
owns a detached Ready task. Across the available SoftAP scheduler snapshots:

- pure-WPA3: 65 snapshots, both counters always zero;
- WPA2: 56 snapshots, both counters always zero.

The fixed behavior is required and has direct host regressions, but these 40
silicon runs did not exercise that mutation path. The result therefore proves
release-candidate parity and absence of the historical failure, not that
`fac6dd4` was the single cause of old run-04/run-10. The existing recovery and
ownership diagnostics remain in place.

## Release And CI Closure

- `hisi-rf-ws63 0.1.0-alpha.71`: CI/publish runs `31039214045` /
  `31039549364`;
- `hisi-rf 0.1.0-alpha.83`: CI/publish runs `31040616332` / `31041075793`;
- `hisi-rtos 0.1.0-alpha.23`: CI/publish runs `31042032182` / `31042292824`;
- `ws63-pac 0.4.4`: publish run `31046206839`;
- `hisi-hal 0.7.0-alpha.7`: final commit `ff706af`, CI/publish runs
  `31046582802` / `31047012555`;
- `ws63-examples` commit `34289bb`: CI run `31042741510`;
- `hisi-rs-template 0.7.0-alpha.28`: main/tag CI runs `31042921415` /
  `31043936338`; the GitHub prerelease is public.

This closes the bounded facade's profile-specific HIL gate. It does not delete
the one-release migration oracle, declare WPA3 stable, or turn a two-board test
fixture into a product SoftAP support claim.
