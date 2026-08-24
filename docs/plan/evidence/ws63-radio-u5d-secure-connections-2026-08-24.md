# WS63 Radio U5D Secure Connections Evidence (2026-08-24)

## Scope

This evidence closes the positive U5D passkey lifecycle on two WS63 boards:
fresh authenticated LE Secure Connections pairing, vendor-managed bond
persistence, and restored-bond reconnect across paired `nRST`. It does not close
the separate rejection or stale-generation negative modes, and it does not
graduate the public BLE API to stable.

## Root Cause And Fix

The failing pairing request and response both negotiated AuthReq `0x0d`
(bonding, MITM, and Secure Connections). The failure was an F6 confirmation
mismatch. Every inspected F6 input except MacKey agreed between the two peers,
which isolated the divergence to ECDH rather than passkey relay, event delivery,
or bond storage.

A ROM-differential census of the vendor multiprecision representation found one
interoperable conversion for the fixed WS63 archive: reverse all 32 bytes of the
private scalar, peer X, peer Y, and shared-secret output. A diagnostic wrapper
that replaced both peers' DHKey with zero appeared to pair successfully; that
result was rejected as a false positive and was never retained in production.

The production fix is split across two release units:

- `hisi-rf-ws63` commit `0d9dbce` supplies the archive key-generation and ECDH
  entry points from the Rust WS63 P-256 backend and its health-checked DRBG. It
  calls the corresponding ROM routines only to preserve the vendor scratch
  allocation/free lifecycle, then overwrites their cryptographic outputs.
- `hisi-rf` commit `8e46c6a` emits both final-link wrap contracts whenever the
  WS63 BLE facade is selected.

The wrappers fail closed. Hardware or DRBG errors increment explicit failure
counters and enter the existing blob-fatal path; they never silently fall back
to software crypto. The final release ELF contains both wrappers and has no
unresolved ECDH reservation symbol.

## Artifacts

| Role | SHA-256 |
|---|---|
| Peripheral | `beb79a513b59d871c2aa28970f0e62a46ea059a9239bad30c1dbe3ee0b8fbbd2` |
| Central | `56c4bc4988d950b72f2fa4935b6406a6b2bb4c58d6b7963b7cb0f02f3def4821` |

Both images were downloaded at 3 MHz with full readback verification. The local
artifact directory is `/private/tmp/ws63-u5d-sc-production-v3-20260824`; it is
an execution record, not a durable release artifact.

## Verification

No-board verification passed:

- `hisi-rf-ws63` radio-stage host contract and clippy with warnings denied;
- `hisi-rf` U2 and U5 fixture contracts and clippy with warnings denied;
- the two U5 RV32 release examples built through the production facade path;
- final-symbol inspection found the Secure Connections wrappers and counters.

The fixed images first passed `3/3` at
`/private/tmp/ws63-u5d-sc-production-v3-3reset-20260824`. Run 1 started from
`BOND_EMPTY`, relayed a redacted passkey, and reached `PAIRED -> AUTH_OK ->
BOND_OBSERVED -> BOND_OK` on both boards. Runs 2 and 3 restored the bond. The
summary reports `contract_pass=true`, `persistence.proven=true`, and two restored
runs per board.

The same unchanged images then passed `20/20` at
`/private/tmp/ws63-u5d-sc-production-v3-20reset-20260824`. All 20 runs restored
the vendor-managed bond on both boards; the summary reports zero failures,
`contract_pass=true`, and `persistence.proven=true`.

## Proof Boundary

This matrix proves the fixed artifacts repeatedly complete the positive U5D
Secure Connections and restored-bond path on the attached boards. It rejects the
earlier zero-DHKey diagnostic as non-evidence and binds the result to the commits
and ELF hashes above. It does not prove all controller archives or peer devices,
all power-loss timings, rejection/cancellation behavior on silicon, or that BLE
traffic can never be lost. The U5D negative reject and stale-generation matrices
remain separate release gates.
