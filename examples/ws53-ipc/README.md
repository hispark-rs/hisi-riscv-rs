# WS53 IPC CPU/IRQ model fixture

This isolated workspace executes the production `hisi-hal::ipc` doorbells and
bounded message codec on QEMU's explicitly assumed two-channel virtual peer.
Enable `model-handoff` to build. Never load this fixture on a physical board:
there is no initialized vendor shared ring, coordinated peer or resource lease.
The virtual peer only sets/consumes notification bits and never dereferences a
remote pointer. A successful notify is not evidence of safe hardware publication.

The source-bound build/execute entry points are:

```sh
uv run --script scripts/ws53_ipc_build.py --output target/ipc-firmware
uv run --script scripts/test-ws53-ipc-model.py \
  --qemu /path/to/source-bound/qemu-system-riscv32 \
  --model-receipt /path/to/source-bound/receipt.json \
  --model-source /path/to/hisi-riscv-qemu-ipc \
  --elf target/ipc-firmware/firmware.elf --receipt target/ipc-firmware/receipt.json \
  --output target/ipc-run
```

Run these from the parent integration checkout. The fixed model builder is
`scripts/ws53_ci_qemu_ipc_mask_build.py`. Normal Cargo builds do not need Python,
SDK, a network or register generation. Every workspace commits its own lockfile.

Three boots exercise actual IRQ26/27 handlers and returns, masked notification
retention/reenable, independent send/ACK, busy-send preservation, bounded held
ACK and rearm, plus2318 codec frames per boot against an independent host hash.
The host writes only three declared ordinary-RAM command words. No MMIO setup,
handler result, pending bit or CPU register is preinitialized by the host.
IRQ-owner movement uses short single-hart metadata critical sections, never
holding one over a driver operation or user callback. The PAC-backed Doorbell
remains !Send/!Sync; the fixture does not weaken it for static interrupt use.

This is not a ring transport, DMA/cache handoff, control-core firmware, TRNG
service, low-power protocol, timing/electrical proof or hardware qualification.

The fixture adopts `ReceiveEnableState::Disabled` from the explicit digital
profile. Receive-mask reads are rejected by the corrected model; the HAL must
only write its shadow. QOM receive-mask inspection is a debug observation, never
an invented readable MMIO register or a silicon readback qualification.

The same firmware also exposes `WS53_IPC_MAILBOX_MODEL`, a256-byte local image.
Commands7..10 initialize, push, pop and inspect the production HAL's
`MailboxImage`. The independent runner is `scripts/test-ws53-ipc-mailbox-model.py`
with the same six arguments as the doorbell runner above. It tests three real
CPU boots, exact-full/empty cases, wrapped headers/payloads, both routes, malformed
metadata/messages with no consumption, independent frame hashes and BSS reset.
The host is allowed to write only the declared command inputs and image, never
MMIO, CPU registers or result words. CPU/peer image access is **serialized**:
the host may access it only after stopping the CPU, and every Rust borrow ends
before command completion. IRQ handlers never borrow it. This fixture does not
model a concurrently running vendor core, coherent caches, cross-core locking,
doorbell drain/rearm or production receive-buffer leases. Do not put it on a board.

The bounded ROM interleaving check is separately reproducible with
`scripts/check-ws53-ipc-ring-interleavings.py --rom <private-prior-read.bin>
--output <new-report.json>`. It requires the exact prior ROM hash/instructions,
does not read hardware, and does not publish the vendor ROM. A counterexample is
not evidence that the same race occurred on silicon. See the integration
`docs/plan/ws53-ipc-ring.md` for the concurrency boundary and remaining work.

`ipc-spsc-model` is a second, strictly model-only binary. Build it through
`scripts/ws53_ipc_spsc_build.py` and execute
`scripts/test-ws53-ipc-spsc-model.py` with the same six arguments. It uses the
production volatile ordinary-RAM backend and TX-gap/RX-biased-cursor profile,
with explicitly coherent static mappings. A running virtual peer independently
uses the original SDK consumer/producer algorithms. It writes only the peer's
cursor/flag/data fields and an input lifecycle gate, never MMIO, CPU registers
or result words. The CPU is not stopped for transfers; three resets each test
512 messages per direction, a full243-byte data occupancy, wrapping, malformed
ID rejection without consumption, unchanged error output and quiescent reclaim.

This does not qualify installed Ccore firmware, service callbacks, coherent
physical mappings, cache maintenance, doorbell drain/rearm or hardware timing.
Source-bound builders use separate binary/schema profiles; cross-profile
receipts must be rejected. Both builds still require `model-handoff` and neither
binary may be loaded on a physical board.
