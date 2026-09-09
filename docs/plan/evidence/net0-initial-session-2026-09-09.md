# NET0 Initial-session Payload Experiment (2026-09-09)

Status: **experimental bring-up, not NET0 acceptance**. New caller-owned L2
queues carried sequence/content-checked ARP/UDP traffic. The attempted 20-round
matrix stopped at round 9; association retries still close this deliberately
one-shot profile. Native producer drainage, reconnect, an Embassy Net profile
and HTTPS remain unaccepted.

## Implementation And Identity

The non-default `standard-l2-initial-session-experiment` requires one successful
bootstrap, one association attempt and a correlated authorized operation. It
opens before publishing `Connected`, refuses duplicate/retry/teardown events,
and never reopens after closure. This is not proof that native hardware or
queues retained across reset were drained. Existing named smoltcp profiles and
the closed `standard-l2` profile are unchanged.

The fixture uses static diagnostic IPv4, ARP and ten 32-byte local UDP echoes
with distinct sequence/content, fixed buffers and no application retransmission.
It then disconnects and checks link-down and the absence of a TX token. Its
10 ms smoltcp polling is a NET0 diagnostic, not the planned NET1 executor model.
It does not exercise DHCP, DNS, TCP, TLS or authenticated time.

| Source | Change | Exact-source CI |
| --- | --- | --- |
| `4a90a8be6007fe4079bdcdc540fd087bd0f44181` | First-session fixture and guards | [23/23](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34322598411) |
| `14a346cfb4472f1340b5ab848c86e681cef84813` | Observe successful association at event publication | [23/23](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34323212544) |
| `30ff74ade1d77526fd3ec06328c603e18507824c` | Read-only route and smoltcp RX classification | [23/23](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34323969213) |

The final source passed 189 offline host tests, RV32 final link and Clippy.
The five new initial-session/production-event tests passed Miri on `14a346c`;
the later commit adds diagnostic snapshots and a fixture observer, not a new
lifecycle transition. CI includes Linux/macOS/Windows contracts and final links.
These commits are **not a new registry release**; version remains alpha.102
with Unreleased changes.

Final STA ELF SHA-256:
`083604380538836bf84af4b554c02db19a3b06f835ef7c10b2eca4c18b22bf64`.
AP ELF SHA-256:
`d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.
AP firmware bytes remained unchanged. Only the public paired-board configuration
was used; no private credential file was read. STA app-only downloads requested
full verify at 3 MHz and took 98.08, 97.68 and 99.33 seconds respectively.
No flashboot or NV was rewritten.

## All Attempts

| Case | Reset policy | Executed / requested | Passed | UDP sent / received |
| --- | --- | --- | --- | --- |
| `unobserved-success` | STA only, AP retained | 1 / 3 | 0 | Not opened |
| `fixed-open-no-reply` | STA only, AP retained | 1 / 3 | 0 | 10 / 0 |
| `rx-boundary-no-reply` | STA only, AP retained | 1 / 1 | 0 | 10 / 0 |
| `ap-once-sta-reset3` | AP reset once, then STA each round | 3 / 3 | 2 | 20 / 20; third round not opened |
| `paired-reset3` | AP then STA reset every round | 3 / 3 | 3 | 30 / 30 |
| `paired-reset20-attempt` | AP then STA reset every round | 9 / 20 | 8 | 80 / 80; ninth round not opened |

Every passing payload round received all ten unique replies with zero invalid
or duplicate payloads. **This is not 20/20, a cold-power-cycle matrix, or a
reconnect test.** The failed attempts are retained rather than replaced by
later successful samples.

Three distinct findings must not be collapsed into one root cause:

1. The first implementation incorrectly attached its successful-association
   observer to a hook invoked only on rejection. `14a346c` fixes that actual
   integration bug and adds a regression through the production event queue.
2. With an already-running AP, the diagnostic STA saw one ARP frame and no
   IPv4 frame at `driverif_input`. That ARP traversed the new queue and smoltcp
   without any route drop. AP counters recorded ten received/submitted echoes,
   which is not proof of on-air TX completion. Resetting only the AP, with
   identical firmware bytes on both boards, restored payload on two rounds.
   This supports an AP/session-state influence but does not identify its native
   TX/RX mechanism or close the retained-AP reliability defect.
3. Later association retries, including round 9 with both boards reset, were
   intentionally rejected by the experimental one-attempt state machine even
   though the control connection eventually succeeded. Do not remove that
   guard merely to obtain a passing matrix; native lifecycle ownership and
   safe retry/reconnect handling remain the implementation gap.

## Evidence And Limits

The [machine record](net0-initial-session-2026-09-09/summary.json) contains every
executed round, reset policy, original UART hash/length, exact source CI,
lock/toolchain, image plan, classifications and public full-line marker excerpts.
[SHA256SUMS](net0-initial-session-2026-09-09/SHA256SUMS) covers all supplied
files. The collector rechecks preserved ELF/raw bytes and reconstructs pass
from complete markers and payload sequence counts. Its positive case and five
tamper cases passed, as did three whitelist-negative cases.

Ambient scan UART is not published. Raw captures and ELF/image bytes remain in
the local experiment directories named by the cases; the downloadable NET0
firmware bundle gate is still open. The included capture script is the final
reproduction tool with paired-reset support, not an assertion that earlier
diagnostic runs used byte-identical tooling. Earlier raw results are reverified
by the collector.

The existing linked-storage checker expects the bootstrap fixture's exported
`NET0_CONTROL`/`NET0_STORAGE_LAYOUT` and rejects this incremental fixture for
missing symbols. Therefore its CI bootstrap report is **not** passed off as a
resource check of this HIL ELF. The machine-readable final-ELF resource gate
must be extended before acceptance. Existing `Flash Init Fail! ret =
0x80001341` warnings are preserved per round, not claimed fixed.

Next work remains the bounded native producer/queue lifecycle and retry
integration described in the [native-fence audit](net0-native-fence-audit-2026-09-09.md),
then retained-AP, reconnect and payload regression on that implementation.
NET0 stays active and NET1 stays queued in the
[single execution plan](../hisi-connectivity-stack.md#net0-net5-https).
