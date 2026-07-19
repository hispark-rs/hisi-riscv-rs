# W2E Upstream WPA Personal Host Protocol Vectors

Date: 2026-07-15

## Scope

This evidence closes the host-side protocol-vector part of W2E. It does not claim
that WPA3 runs on WS63 silicon, that the target crypto backend is hardware
accelerated, or that a full four-way handshake transcript has been replayed.

The source oracle is the pinned upstream hostap 2.11 tree:

- tag: `hostap_2_11`
- commit: `d945ddd368085f255e68328f2d3b020ceea359af`
- repository owner: `ws63-radio-sys/third-party/hostap`

## Executable Contract

`ws63-radio-sys/scripts/check-upstream-personal-vectors.py` builds the exact pinned
hostap SAE implementation against OpenSSL, then runs:

- WPA2 PMK-to-PTK known-answer checks for KCK, KEK and TK;
- an exact EAPOL message 2 MIC known-answer check;
- WPA2-Personal, WPA3-SAE and transition-mode RSNE parsing with PMF capability checks;
- SAE group 19 hunting-and-pecking and hash-to-element two-party commit/process/confirm
  roundtrips;
- all five upstream hostap SAE fuzzer corpus fixtures with pinned parse outcomes.

The CI workflow runs the same script. The script deletes its owned out-of-tree
hostap build directory before compiling so dependency files from macOS and Linux
cannot contaminate one another.

## Results

The following command passed on both macOS arm64 with Clang/OpenSSL and the OrbStack
Linux development VM with GCC/OpenSSL:

```console
uv run scripts/check-upstream-personal-vectors.py
upstream personal vectors: WPA2 PTK/MIC, RSNE/PMF, SAE HnP/H2E roundtrips, 5 SAE corpus fixtures
```

Existing native-port checks remained green:

```console
uv run scripts/check-native-supplicant-port.py
native supplicant profile: 42 RV32 objects, 15 defines, external ABI locked

cargo test --workspace --target aarch64-apple-darwin
test result: ok
```

## Remaining W2E Gates

- pure WPA3-Personal SAE + required PMF on a controlled WPA3-only AP;
- WPA2/WPA3 transition-mode HIL;
- W2E-H hardware migration evidence before any stable WPA3 acceleration claim.

The existing `HUAWEI-HLJ_Guest` WPA2 AP is only a parity fixture and cannot satisfy
the pure WPA3 gate.
