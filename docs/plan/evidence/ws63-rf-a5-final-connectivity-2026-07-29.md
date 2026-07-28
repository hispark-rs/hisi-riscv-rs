# WS63 A5 Final Connectivity And Response-Bound Evidence

## Scope

This evidence covers the final public-facade `wifi_connectivity` image on real
WS63 silicon. The image used the upstream WPA2 supplicant profile against an AP
advertising WPA2/WPA3 transition mode. It proves the WPA2 connectivity and A5B
response-bound gates; it does **not** establish pure-WPA3 support.

The immutable release closure was:

```text
parent_commit=3781900542e56276a57f03fc1c2f43f376cd1637
hisi_rf_ws63_commit=25c0bad401b746fef6c8545192910174ec0d59cb
hisi_rf_commit=52ba8a223e34402f465686ffdfe391f3e0d5ccc4
hisi_rf_core_commit=76cd41077576e89723afc728a087f85353414cd3
hisi_rtos_commit=e60adadc8bef76c15f161c91cae25f22563b98d1
ws63_radio_sys_commit=aef315a6c8f99961ef5f1dae37c23fe77895f190
```

The final ELF identity was:

```text
profile_id=wifi-connectivity-upstream-wpa2
elf_sha256=1f7f9d3f5ad35b79b1e71be73ea8e6eab5bd44a66886b5796efd4a291b588b36
marker_contract=ws63-connectivity-markers/v1
```

The plain-Cargo final-link checks reported 37 ROM patches, zero vendor
relocations, a reachable upstream supplicant, no reachable legacy supplicant,
and the expected seven-symbol runtime compatibility surface.

## Silicon Result

The image was downloaded once at 3 MHz with full readback verification. The
download completed in 112.16 seconds. The same flash contents were then booted
through J-Link nRST 20 times:

- 20/20 runs passed the ordered image/init/scan/connect/DHCP/neighbor/ping/
  steady-state/lease-renew marker contract;
- all 20 A5B trailers were complete for the long-lived connectivity stage;
- Authentication-response-2 timeouts were 0/20;
- the event queue high-water mark was 1, with zero drops;
- the control queue high-water mark was 1 and ended empty;
- runner errors and blocking scan/poll/sleep/supplicant-poll fallbacks were
  zero;
- runner step maxima were 33--81 ms, below the fail-closed 100 ms limit;
- association ioctl maxima were 28--31 ms;
- initialize took 20--21 ms, scan 1404--1586 ms, and connect 228--5455 ms;
- the gateway returned 100/100 ICMP replies;
- the public target returned 79/100 replies, with at least one reply in every
  run and no TX errors;
- every run reached the DHCP-renew marker.

The public-target loss is an observed network-path characteristic, not an RF
contract violation: the acceptance contract requires a functioning routed path
and reports aggregate loss rather than pretending the Internet target is a
deterministic test fixture. Gateway connectivity, Rust-visible RX accounting,
queue conservation and the per-run public response requirement all passed.

## Parser Consistency Fix

The first machine-readable summary classified all runs as passing but rendered
the aggregate A5B trailer as incomplete because `record_from_log()` parsed it
with the `connect` stage's disconnect requirement. The validator itself already
used the correct stage-aware rule: the long-lived `connectivity` runner has no
terminal disconnect operation.

The parser now uses the same stage-aware rule for validation, per-run records
and aggregate summaries. A regression test proves that a connectivity record
without a disconnect timing is complete, while the `connect` stage still fails
closed when that timing is absent. Offline reanalysis of the immutable 20 UART
captures returned:

```text
counts.pass=20
a5b_metrics.complete_runs=20
a5b_metrics.missing_runs=0
contract_violations=[]
```

Raw UART captures and the machine-readable summaries remain outside the
repository. The one-shot credential file was mode `0600`, consumed before the
build, and absent after the test. This committed evidence contains no SSID,
passphrase, BSSID or key material.

## Remaining Boundary

This closes the final upstream-WPA2 connectivity, shared marker/parser and A5B
response-bound gates for the frozen pre-A5UX API image. Pure-WPA3 SAE+PMF
remains externally blocked on a suitable SAE-only AP. The A5UX public API
convergence must preserve this marker contract and be followed by an equivalent
post-migration silicon run before the blocking adapter can be removed.
