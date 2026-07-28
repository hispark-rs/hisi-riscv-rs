# WS63 A5 Marker And Artifact Contract Evidence

## Scope

This evidence covers the shared fail-closed connectivity marker parser and the
artifact identity used by the WS63 single-run smoke and unchanged-image reset
matrix. It proves the credential-free `init-scan` stage on real silicon. It does
not prove association, WPA2/WPA3, DHCP, lease renewal, or ping.

The tested release closure included:

- `hisi-rf-ws63 0.1.0-alpha.36`;
- `ws63-radio-sys 0.1.0-alpha.8`;
- `hisi-rf-core 0.1.0-alpha.16`;
- `hisi-rf-rtos-driver 0.1.0-alpha.17`;
- `hisi-rtos 0.1.0-alpha.13`.

## Executable Contract

`hil/ws63-connectivity-reset-matrix.py` now owns the shared contract for
single-run HIL, multi-reset HIL and offline reanalysis:

- `init-scan`, `connect` and `connectivity` stages;
- ordered image/init/event/scan/connect/DHCP/neighbor/ping/steady/renew markers;
- fatal marker rejection;
- non-zero queue drop, TX error, backend error and blocking fallback rejection;
- optional A5B runner-step budget enforcement;
- deterministic `hisi-connectivity-artifact/v1` identity containing profile,
  marker-contract revision and final ELF SHA-256.

The parser verifies the identity against the final ELF before classifying UART
captures. Its JSON result includes the identity-manifest hash and per-run
contract violations, so a missing marker or mismatched ELF/profile cannot be
reported as a pass.

The host regression suite passed 18 tests. It includes missing-renew,
non-zero-drop, backend-error, runner-budget, ELF mutation and profile-drift
negative cases. The repository uv contract and credential-loader tests also
passed.

## QEMU Contract-Only Result

The `connectivity_contract_fixture` target image was built with the same
release closure and executed under `hisi-riscv-qemu 0.4.9`. Its identity was:

```text
profile_id=upstream-wpa2
elf_sha256=614a79ca37a22b9d207047f704b42d40fa1149251b7646e0926e2b0b4aadcd55
```

The image emitted
`RFDBG_CONNECTIVITY_CONTRACT_FIXTURE scope=contract-only` and the complete
ordered marker transcript. The shared parser accepted it with zero violations
and labelled the evidence `contract-only`. The ordinary HIL path rejects that
fixture marker, so QEMU output cannot be misreported as RF silicon evidence.
The parent CI runs the same fixture with a pinned QEMU release and checksum.

## Silicon Result

The credential-free upstream-WPA2 `init-scan` image was built through the
plain-Cargo normalized-archive lane. Final-link checks reported 37 ROM patches,
zero vendor relocations and a reachable upstream supplicant. The image identity
was:

```text
profile_id=upstream-wpa2
elf_sha256=02e62bf5a7f91f2a05f029517c7813fd8c19fdec575f8a45f7e4a5929b6c3b90
manifest_sha256=d71e1b96e391ad154b200553062dab8dcdd09ba45f1858ae2f9d20507dd1f62b
```

Probe-rs used 3 MHz and retained full readback verification. Download and
verification completed successfully in 107.34 seconds. J-Link nRST then reached
the image/init/initialized-event/non-empty-scan/scan-completed-event and native
runner markers. The strict parser returned one pass with zero contract
violations.

Raw UART and machine-readable artifacts were retained outside the repository.
They contain no passphrase; the temporary build directory and ELF were deleted
on script exit.

## Remaining Boundary

This closes the HIL implementation and credential-free silicon proof of the
shared marker/identity contract. It does not close the A5 acceptance item yet:

- the final release-train connectivity image must pass the full
  connect/DHCP/neighbor/repeated-ping/lease-renew contract;
- the A5B final image still needs the 20-reset response-bound matrix;
- pure-WPA3 remains an external AP gate.
