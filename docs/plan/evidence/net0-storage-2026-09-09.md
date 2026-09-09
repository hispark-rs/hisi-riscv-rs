# NET0 Caller-Owned Storage Evidence (2026-09-09)

Status: **physical storage and bootstrap admission passed**. Native RX/TX
quiescence, connected traffic, reconnect, Embassy Net and HTTPS are not covered.
NET0 remains active in the [connectivity plan](../hisi-connectivity-stack.md#net0-net5-https).

## Source And CI

- Backend source: `e117f585b80d72779fc3c6fb26b8bbd9564a918f`, not a new crate release.
- Storage implementation: `aecb495`; feature-off assertion fix: `e117f58`.
- Exact-source [CI run 34301630873](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34301630873)
  passed all 23 jobs, including old profiles, Miri and macOS/Linux/Windows RF links.
- The preceding [failed run](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34301317112)
  is retained: an assertion became `usize >= 0` with NET0 disabled and Clippy
  rejected it. The fix scopes the assertion to its feature; no lint was disabled.
- [Machine-readable evidence](net0-storage-2026-09-09/summary.json) binds downloaded
  CI reports, source, local ELF/image/lock/tool hashes and each UART capture.

## Physical Contract

| RV32 allocation | Bytes |
|---|---:|
| Caller-owned control object | 21,376 |
| L2 storage within control | 12,560 |
| L2 packet payload, 4 RX + 4 TX, MTU 1,514 | 12,112 |
| L2 metadata/padding | 448 |
| Combined RF/RTOS arena backing | 299,008 |
| Main stack | 32,768 |
| Wi-Fi packet RAM | 49,152 |

The L2 offset is 2,296 bytes within the control object. All three CI hosts and
the independently built local ELF agree on these fields. L2 storage is included
once in control storage, not added again to caller-owned totals. Vendor task
stacks remain 24 KiB each. The fixture now supplies the separately declared
RTOS arena rather than allocating its RTOS stacks from the RF heap.

The standalone fixture uses the existing ecosystem size-oriented release
profile (`opt-level=s`, LTO, one codegen unit). No arena or stack was reduced to
fit. A default opt-level=3/no-LTO trial exceeded SRAM and is not a supported
fixture configuration.

## Hardware Observations

The same local ELF (`ed798e7ee4c45ef898467de26b7f81d537fb3cc26b375da78aa239a8baca2a63`)
was written to the app partition only through `hisi-fwpkg plan` and probe-rs bin
download with full verify at 3 MHz. Probe-rs reported 113.53 s and 114.69 s for
the two downloads. Flashboot and NV were not replaced.

- J-Link 23121310 / UART `wchusbserial1130`: 3/3 nRST bootstrap/admission passes.
- J-Link 24060504 / UART `wchusbserial1140`: 3/3 nRST bootstrap/admission passes.
- Each raw capture contains `RFDBG_BOOTSTRAP_PROFILE_OK` and
  `A5U_TASK_STACK_ADMISSION_OK`; no bootstrap-error or panic marker was observed.
- The pre-application `Flash Init Fail! ret = 0x80001341` warning remains in the
  raw logs. Its cause is not established or waived by this narrowly scoped gate.

The [capture tool](net0-storage-2026-09-09/capture-tool.py) uses the repository's
J-Link reset helper, opens UART before reset and stops on the first failed
sample. It used no AP credentials and installed no persistent runner/service.

## Limits

Downloaded CI reports contain the hashes their target checker calculated;
the CI ELF files were not retained in those artifacts, so this is not downloaded
CI-binary acceptance. The local HIL ELF/image remain local until the complete
NET0 evidence publication. Raw logs, report bytes and the capture tool are
preserved here. Six reset samples do not establish a universal reliability rate.

The v14 parent report assembler now requires the experimental target layout
descriptor and rejects host-size reports, missing descriptors, inconsistent
packet accounting and double counting. Generic NET0 profile/worker integration
and native teardown fencing remain separate unfinished work.
