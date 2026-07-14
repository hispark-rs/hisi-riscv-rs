# WS63 RF A3 IRQ Epilogue Preemption Evidence (2026-07-14)

## Scope

This evidence covers immediate single-hart priority preemption after an ISR
wakes a higher-priority task. It does not claim timer-driven time slicing,
budget enforcement, priority inheritance, nested-IRQ/FP stress coverage, or
Embassy integration.

## Runtime Contract

- The WS63 startup paths call `__hisi_irq_epilogue` on the IRQ stack and pass
  the interrupted task's saved frame pointer. The epilogue returns the frame
  that the runtime must restore.
- DIRECT, dedicated MIE, and local interrupt exits share the contract. NMI and
  exception exits do not invoke it.
- `hisi-riscv-rt` supplies a linker `PROVIDE` alias to a no-op implementation.
  An RTOS supplies a strong symbol without colliding with `global_asm!` under
  LTO.
- `hisi-rtos` switches only when the scheduler is started in Priority mode,
  interrupt nesting depth is zero, the current task is unlocked, and a strictly
  higher-priority task is ready. Scheduler metadata changes occur inside a short
  critical section; the runtime then restores the selected task frame and exits
  through `mret`.

The final RF ELF resolved the strong and fallback symbols separately:

```text
__hisi_irq_epilogue         T 0x0023fdce
__hisi_irq_epilogue_default T 0x002312bc
```

A non-RTOS `uart_hello` link resolved both names to the default address, proving
the optional integration preserves ordinary applications.

## Host And Build Evidence

- `hisi-rtos`: host clippy with `-D warnings`, 11/11 unit tests, and RV32IMFC
  `-Zbuild-std=core,alloc` check passed.
- `hisi-riscv-rt`: default WS63 and `riscv-rt-start-experiment` feature paths
  both passed RV32IMFC checks.
- Guarded RF link verified 1,485 final layout sections, patched 5,334 vendor
  relocations, and generated 37 mask-ROM patches.
- Image `code_area_hash`:
  `8e74425e8cb6c1b00dd93938a095e77f4d6fc68232e9f4f2ffc0cda29c951d85`.

| Artifact | SHA-256 |
| --- | --- |
| `wifi_init_smoke` ELF | `e5e178d903314071257b2a6d44a97712f2bb42454c58a8308faa4df6b60ebc14` |
| canonical `.hisi.img` | `d5ebc2a22aae30bc9e7f7024c9d9ddfeab6201ea6a1ddb0e70ddbdf31e02dfe0` |
| FlashPlan JSON | `5d056a4865012151a42e75f6cc5a8f182cfb8c81bb169752661d99be6dcbde2b` |

## Silicon Evidence

The image was downloaded through the FlashPlan raw-bin path, followed by a
physical J-Link nRST and 115200-baud UART capture. The connected WS63 EVB
completed the connectivity path:

```text
RF1_IMAGE_OK
RF2_INIT_OK
RF3_SCAN_OK count=0x00000015
RF5B_WPA_CONNECT_OK
RF5A_DHCP_OK addr=192.168.155.2 prefix=0x00000018 router=192.168.155.1
RF5A_ARP_OK rx=0x00000003
RF5C_PING_OK rx=0x00000005
RFDBG_RTOS switches=0x0000279a irq_preemptions=0x00000289
run-hw: done.
```

The WPA2 credential was injected through the build environment and is not
stored in the repository or evidence record.

## Superseding Evidence

The later unified 272-byte context and timer-preemption implementation supersedes
the original partial trap-frame mechanism. See
[A3 unified task-context preemption](ws63-rf-a3-unified-context-2026-07-14.md).

## Remaining A3 Gates

- priority inheritance;
- nested-IRQ, timeout, and scheduler stress HIL;
- Embassy thread-mode executor and unique time-driver integration.
