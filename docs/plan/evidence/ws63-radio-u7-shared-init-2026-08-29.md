# WS63 Radio U7 Shared Initialization Evidence (2026-08-29)

## Scope

This evidence closes the U7 shared-initialization sub-gate for the named
Wi-Fi+BLE and Wi-Fi+SLE compositions on two WS63 boards. It proves that the
fixed images can admit the combined resources, start the RF/RTOS platform, and
initialize the selected BGLE role repeatedly. It does not prove concurrent
Wi-Fi traffic with BLE/SLE over the air, coexistence latency, heap watermark,
or stable `coex` API readiness.

## Implementation And Release Identity

The implementation sequence in `hisi-rf-ws63` is:

- `42df675`: link the WS63 Wi-Fi and BGLE archive closures;
- `69c13ce`: compose checked Wi-Fi+BLE/SLE resource plans;
- `83ebccc`: remove double-counting from Wi-Fi vendor-task admission;
- `277eac2`: map SLE coexistence reservations to the correct task group;
- `2dca6a6`: exercise the Wi-Fi admission contract across host profiles.

These changes are contained in `hisi-rf-ws63 0.1.0-alpha.86` at commit
`f9736680393f884bba360841991cd9591ca02884`. Main CI run `33241607107` and
publish run `33241766142` passed. The public facade consumes that release in
`hisi-rf 0.1.0-alpha.101`; its release commit is
`227bff411dac9ca39197366425ee320b1a480c82`, main CI run `33241960841` passed,
and publish run `33242159753` succeeded. Post-release fixture commit `07ca5e5`
then passed CI run `33242276157`, rebuilding the exact released facade and RTOS
combination from crates.io on Linux, macOS, and Windows, including offline
rebuilds with read-only registry sources.

## Artifacts

| Composition | Artifact | SHA-256 |
|---|---|---|
| Wi-Fi + BLE | ELF | `6b84d85869e09868aa5026111d78cdb1c178001727159e20f3bd89f9bf9cbafe` |
| Wi-Fi + BLE | FlashPlan image | `98838cadb29eea20570bb47b9ad8f488419f0b195cdc203193b738d2445ee6a8` |
| Wi-Fi + SLE | ELF | `3ef7dd92f5cd950be90158ea102bbf7a43be09a670a15c9375d794fac904911c` |
| Wi-Fi + SLE | FlashPlan image | `9fdb9c9fde2e8f06e2a274b87b4f693401c4fb6b3e506cea5a0a4422d1921b76` |

Both images were downloaded through the `hisi-fwpkg` FlashPlan binary path at
3 MHz with full readback verification. Wi-Fi+BLE took 134.45 seconds and
Wi-Fi+SLE took 141.98 seconds. The exact HIL artifacts are local execution
records rather than durable release assets.

## Verification

The repository-owned `hil/ws63-coexistence-reset-matrix.py` runner is a PEP 723
`uv` script. It resets and captures both boards, records one UART log per role
and run, hashes the input ELF files, and fails closed on missing stage markers
or fatal markers.

The fixed artifacts passed:

- `3/3`: `/private/tmp/ws63-coexistence-init-3reset-277eac2-20260829`;
- `20/20`: `/private/tmp/ws63-coexistence-init-20reset-277eac2-20260829`.

Each board emitted `RFDBG_COEX_INIT_BEGIN`, `RFDBG_COEX_RF_POWER_OK`,
`RFDBG_COEX_RTOS_OK`, its role-specific `RFDBG_*_SHARED_PLATFORM_OK`, and
`RFDBG_COEX_INIT_OK`. The 20-run contract summary reports 20 passes, zero
failures, and no panic, missing-ROM-callback, coexistence-init error, or
scheduler-contract marker.

Two earlier failed matrices remain useful counter-evidence. The first failed
all three runs with resource status `0x703`; after the aggregate admission fix,
the next failed at SLE task spawn index 3. The task-group offset fix in
`277eac2` closed that second defect. These failures are retained as diagnosis
history and are not rewritten as successful evidence.

## Proof Boundary

This is integration and repeated-reset evidence for shared resource ownership
and initialization of the exact artifacts above. It does not show simultaneous
Wi-Fi packet traffic with BLE advertising/connection or SLE announce/link/data,
nor does it measure IRQ latency, allocator/heap peaks, event conservation under
traffic, or RF coexistence quality. Those remain the U7/X0 concurrent-traffic
acceptance gate; `coex` must remain unavailable as a stable public promise until
that gate passes.
