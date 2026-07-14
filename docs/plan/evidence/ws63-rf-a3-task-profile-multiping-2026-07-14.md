# WS63 RF A3 Task Profile And Multi-Ping Evidence (2026-07-14)

## Scope

This evidence closes Q3 archive-bound task classification and records the Q4
quota decision for the current WS63 radio payload. It also replaces the earlier
single-ping observation with a 20-reset, five-ping-per-target matrix.

It does **not** close A3 as a whole. Public ICMP still loses packets, gateway
ICMP cannot yet be compared with a valid same-L2 reference host, and the current
RX ring silently drops queue-full frames without counting them. Those are
data-path risks, not authentication regressions.

## Frozen Inputs

| Input | Revision or SHA-256 |
| --- | --- |
| parent repository | `6618933d579a9aa16babef161ac25716465a64fe` |
| `ws63-radio-sys` | `be9092196d67a0bd86cd1060ef1763d87fdebdc6` |
| `hisi-rtos` | `71577058f7feaa20b08110981de2a9f92af70f0a` |
| WS63 examples | `0981c58e166248fa49a0bbf6674aedaf016f589b` |
| scheduling profile revision | `ws63-scheduling-2026-07-14` |
| scheduling profile file | `81c5907032f53cbf6c04adb8f9edbd4c3c24c649c2195f99fac4eb37c9ef6bf5` |
| radio payload revision | `d01274cdd1bd42aeb417ade97eee108c1f7b9440` |
| WPA2-Personal archive | `891c195279d768ce5664e16fecac95f353fd2217c4e3590315baa4cb6e7f25a2` |
| final `wifi_init_smoke` ELF | `61f6af60f82f33ec27b8fcab0fc39732e5da066fd522e54692ebc5b6bd7df7d6` |
| canonical `.hisi.img` | `904ad4b7feff4d6f0a63e2948d1f44916db0ea7b013c20dff9871996126c7d07` |
| FlashPlan JSON | `d4198b9fbdbc83ba1a42f97ab2d44a44a5f8285261a4508572d0ac2859567b8e` |

The machine-readable reset summary is stored beside this document as
`ws63-rf-a3-task-profile-multiping-2026-07-14.summary.json`; its SHA-256 is
`6719eddaa3185f2de4dc890cf34f4421c253f634de36552960e543423d2ac5f1`.
The raw UART logs and per-run task-profile reports remain in the ignored local
`target/hil/ws63-connectivity-multiping-20x-20260714-rerun/` directory because
they contain nearby AP identifiers.

## Silicon Reset Matrix

The already-flashed image was booted through 20 physical J-Link nRST cycles.
The UART was opened before each reset, and every run continued for 1.5 seconds
after the connectivity terminal marker so that RTOS diagnostics were captured.

| Observation | Result |
| --- | ---: |
| image/init/scan completed | 20/20 |
| WPA2 association completed | 20/20 |
| DHCP completed | 20/20 |
| gateway ARP completed | 20/20 |
| `WLAN_AUTH_RSP2_TIMEOUT` (`0x1451`) | 0/20 |
| exception or `0xdeadbeef` poison fault | 0/20 |
| ICMP TX errors | 0/200 |
| gateway `192.168.155.1` replies | 0/100 |
| public `1.1.1.1` replies | 88/100 |
| runs with all five public replies | 12/20 |
| runs with one or more public replies | 20/20 |

Public reply RTT was 254--740 ms, with a 266 ms median and 310.8 ms mean.
Timeouts were spread across sequence numbers 1--5 (`1, 2, 1, 3, 5`), so this
is not a fixed-sequence construction bug. The host running the test was on
`192.168.3.0/24`; its route to `192.168.155.1` passed through a tunnel rather
than the AP's L2 network. Consequently, gateway `0/100` is not proof of an AP
ICMP policy and remains unattributed.

All failures happened after association, DHCP and ARP. The matrix therefore
separates the remaining 12% loss from the previously closed `0x1451`
authentication-response timeout.

## Q3 Archive-Bound Task Profile

`hisi-rf-link task-profile` parsed the final ELF and each UART log. All 20
reports used the same profile/payload/ELF hashes. The four observed vendor
tasks all matched exact entry symbols or the mask-ROM address; application
main, internal idle and the Rust timer worker intentionally remained
`unknown` because they are not archive-owned tasks.

| Task | Role | Vendor priority | Samples | CPU ms/run | Dispatches/run | Max run | Max ready | Max lock |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| task 2 `frw_task_process` | worker | 4 | 20 | 307--477 | 625--1079 | 5 ms | 8 ms | 5 ms |
| task 3 `frw_task_process` | worker | 4 | 20 | 17--30 | 77--80 | 2 ms | 8 ms | 1 ms |
| task 5 `frw_task_thread` | critical | 5 | 20 | 238--412 | 780--1612 | 37 ms | 38 ms | 1 ms |
| task 6 `wpa_supplicant_main_task` | critical | 4 | 20 | 37--45 | 27--28 | 8 ms | 5 ms | 1 ms |

Every task used policy value 0 (`Cooperative`) in every run. The already
verified stale-switch recovery was exercised 372 times in total, 9--35 times
per reset, with no exception or multiple-`Running` corruption observed.

## Q4 Decision

No current vendor task is changed to `Budgeted`, and no aggregate radio group
quota or Reservation is introduced:

- the current archive has no observed runaway vendor task; maximum continuous
  vendor execution was 37 ms across the matrix;
- the 12% ICMP loss does not establish excessive vendor CPU consumption, and a
  periodic CPU cap would not by itself guarantee RX service;
- the RX ring's queue-full path is currently unobservable, so changing policy
  before measuring that path would confound the data-plane diagnosis;
- no measured minimum-service requirement exists, so Reservation fails its G0
  admission gate.

This is an archive- and evidence-bound decision, not a permanent claim that RF
tasks should always be Cooperative. A new payload hash, a changed task set, or
new Q2 latency evidence must reopen Q3/Q4.

## Remaining A3 Gate

Q3 and Q4 are complete for the frozen payload. A3 remains active until the RX
queue-full path is counted, the multi-ping matrix is repeated with that
instrumentation, and the public loss/gateway behavior is either fixed or given
a quantified environmental boundary. A4 must still replace the bring-up-only
DHCP/ARP/ping helper with a long-lived network runner; this evidence does not
claim sustained-IP readiness.
