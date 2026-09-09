# NET0 Checked Native User Cleanup (2026-09-09)

Status: **HMAC cleanup failure propagation implemented and negative HIL
passed; NET0 native producer drainage and reconnect remain open**. The
successful one-shot traffic samples must not be described as complete NET0,
Embassy Net or HTTPS acceptance.

## Implementation And Boundary

Backend `94478fcdbd212d11ac814a704df4bfd33753abd1` wraps actual HMAC
user-delete and exported MAC resource-free calls. A bounded tracker correlates
opaque user identity, a non-reused ticket and exactly one nested free result.
It does not dereference private user fields or encode firmware RAM addresses.
Native calls execute outside the tracker critical section. Delete entry closes
L2 admission before native progress, and a first cleanup failure stays sticky
for this boot. Missing, duplicate, stale and capacity-exhausted observations
fail closed.

The pinned vendor call graph has two lossy returns: user-delete can discard a
failed final free, and kick-user can discard user-delete's status. Checking
only the outer WAL return therefore misses a real error. Both queued and
inline Rust deauthentication paths now check the correlated result. The
local `hmac_user_free_etc` symbol cannot be interposed with GNU `--wrap`; the
implementation instead wraps the exported `hmac_res_free_mac_user_etc` called
by that local function. The final-ELF gate decodes six actual AUIPC/JALR call
edges, rather than accepting symbol presence. Removing any one call while
retaining its symbols fails the six mutation tests.

This closes checked **host-user cleanup status**, not the separate DMAC final
free status, queued TX/EAPOL, RX message 595, pre-allocation hardware work, or
the native reference wait's missing local deadline. The current link wrappers
are delivered to the integration repository's explicit fixtures; transitive
external-Cargo delivery remains a gate before a supported standard-L2 profile.
The [native fence audit](net0-native-fence-audit-2026-09-09.md) remains open.

## Exact Firmware And Attempts

The AP was unchanged, SHA-256
`d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.
All rounds reset AP, wait for its ready marker, then reset STA. The public
paired-board configuration was used, not private AP credentials. Only STA app
images were flashed, at 3 MHz with full verification; no flashboot/NV writes
or persistent runner/service installation occurred.

| Fixture | Source | STA ELF SHA-256 | Result |
|---|---|---|---|
| Healthy | `94478fc` | `7580ba102a1d476ef7db5b899f50755316893beed564dfde79828b8d55835c5c` | 3/3 preflight, then 13/14 of requested 20 |
| Incorrect negative assertion | `02b1951` | `78808ae7a5fe4d17b0cef59215d34c5b7e37fc68f8cadae2bf227a111f2bee0e` | 0/1 of requested 3; assertion panic retained |
| Corrected negative assertion | `c3b1448` | `f70d21ace16372f9e8a58471ea6927b9ecf955b56e4ddf7678e21044bd3cba1a` | 3/3 negative-test passes |

The healthy preflight received 30/30 unique UDP replies. Its same-image
follow-up received 130/130 in the first 13 rounds. Round 14 completed control
connection only after a second association attempt; the existing one-shot
guard emitted `initial-rejected` and kept L2 closed. No payload or cleanup
was attempted in that round. The original 40-second failure capture remains
retained; the outcome is not 20/20 and is not attributed to this cleanup fix.
Every successful healthy round observed delete entered/completed=1/1,
active=0, scoped free completed=1, and inner/outer/checked status=0.

The explicit, non-default negative feature calls real resource release once,
then changes only a successful scoped return to 100. It does not corrupt the
native allocator or claim to induce a real hardware free failure. The first
fixture incorrectly expected public `backend_code=100`: HIL instead observed
the existing encoded diagnostic `0x5732d064` and lossless
`hostap_status=100`, then the fixture panicked. This failed run is preserved.

The corrected fixture checks both values, the Disconnect stage, completed
cleanup conservation, device Down and no TX token. All three rounds first
received 10/10 UDP replies and then correctly rejected cleanup. No
`DISCONNECT_OK`, `INITIAL_SESSION_CLOSED` success marker or
`CONNECT_PROFILE_OK` is accepted for this negative case. These are rejection
tests, not three additional normal connectivity successes. STA was left in
the fixture's closed error state after the third round.

## Verification And Delivery

Local integration tests pass 203/203, including the new encoded/raw status
mapping test. The nine tracker tests previously passed Miri. RV32 final link,
Clippy with warnings denied, format, six resolved-call mutations and eight
resource-descriptor mutations pass. The actual healthy and corrected negative
ELFs each report a 21,440-byte control object containing 12,560-byte L2 storage,
101,888-byte RF arena and 197,120-byte RTOS arena. This descriptor is not a
claim that all firmware BSS or future TLS memory is accounted for.

The [machine record](net0-user-cleanup-2026-09-09/summary.json) preserves all
four matrices, full-line whitelisted UART excerpts, original capture
hashes/lengths, lock/toolchain, three distinct FlashPlans, call reports and
actual-HIL storage reports. Its collector revalidates complete markers,
cleanup counters, encoded status and raw trace; tests reject six altered
negative observations and retain the real assertion failure.

CI and downloaded report verification are recorded in the
[download receipt](net0-user-cleanup-2026-09-09/download-verification.json).
It compares ZIP SHA-256 against GitHub artifact digests and checks the three
platform reports and the independent source copy. Resource layouts and resolved
source/callee graphs agree; ROM-call trampoline addresses and ELF hashes differ
between hosts. Original addresses and report-byte hashes are retained, not
normalized away as evidence of byte-identical firmware. These artifacts are reports,
not CI firmware binaries, and Actions retention is finite. Local ELF/image
bytes are not advertised as downloadable release artifacts. No new crate
version or support/stability claim was published for this diagnostic work.

The [active NET plan](../hisi-connectivity-stack.md#net0-net5-https) remains at
NET0. The next functional gate is bounded native producer closure/drainage
and same-device reconnect, not another closed-route or one-shot success claim.
