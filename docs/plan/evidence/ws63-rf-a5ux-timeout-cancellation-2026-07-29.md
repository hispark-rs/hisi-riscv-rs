# A5UX timeout and cancellation contract evidence

Status: host and release closure complete; final transition-profile HIL pending.

## Contract

The release train distinguishes three independent bounds:

| Boundary | Public shape | Stable result |
|---|---|---|
| Protocol operation | `OperationTimeout` | `operation.timeout` in `hisi-rf-error/v3` |
| Backend lifecycle | `BackendTimeout` | `backend.timeout` in `hisi-rf-error/v3` |
| Application wait | application-owned deadline | application-owned, secret-free marker |

Dropping a Future after the backend has accepted its operation requests
cancellation through an RAII guard. `Drop` performs no vendor, transport, or
hardware work. The runner advances cancellation and cleanup in normal task
context, and stale completions cannot complete a later operation generation.

The generated application uses `ApplicationWaitDeadline` and
`hisi-rf-application-wait/v1`; that marker is deliberately outside the
`hisi-rf-error/v3` backend schema.

## Released units

| Unit | Commit | Release | Publish workflow |
|---|---|---|---|
| `hisi-rf-core` | `c762481` | `v0.1.0-alpha.17` | `30408159872` |
| `hisi-rf-ws63` | `c82eab9` | `v0.1.0-alpha.44` | `30408770547` |
| `hisi-rf` | `c14a8c4` | `v0.1.0-alpha.54` | `30409208054` |
| `hisi-rs-template` | `284825d` | `v0.7.0-alpha.15` | pending at evidence creation |

The WS63 examples consume facade `alpha.54` at commit `4bb1d08`. The facade's
external consumer fixture was advanced on main in commit `189457d`.

## Verification

- `hisi-rf-core`: 50 host tests, formatting, public API snapshot, clippy, RV32
  check, and standalone package.
- `hisi-rf-ws63`: WPA2 blocking 69 tests, WPA3 blocking 74 tests, WPA2
  incremental 89 tests, WPA3 incremental 94 tests, plus clippy, RV32,
  public-API snapshots, and standalone package.
- `hisi-rf`: host tests, clippy, all RV32 facade profiles, WPA2/WPA3 public API
  parity, dependency-boundary checks, standalone package, and crates.io-only
  WPA2/WPA3 consumers.
- `wifi_connectivity`: WPA2 and WPA3 RV32 release builds using a shared
  application configuration module.
- generated WS63 Wi-Fi project: check, release build, and deterministic resource
  report with control storage 8576 bytes and radio state 2212 bytes.

These checks establish the type, error, cancellation, packaging, and consumer
contracts. They do not replace the final real-silicon transition-profile matrix.
Pure-WPA3 remains an external blocked gate because the available AP does not
provide a pure WPA3 mode.
