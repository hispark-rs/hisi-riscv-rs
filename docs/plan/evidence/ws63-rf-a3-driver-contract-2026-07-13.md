# WS63 RF A3 Driver Contract Evidence (2026-07-13)

## Scope

This evidence covers the first A3 vertical slice: introducing the independent,
chip-neutral `hisi-rf-rtos-driver` contract and routing the WS63 vendor task
create, sleep and current-task operations through it. The cooperative scheduler
is still a transitional backend inside `ws63-rf-rs`; this evidence does not
claim that scheduler ownership has moved to `hisi-rtos` or that preemption is
complete.

## Contract Result

- `hisi-rf-rtos-driver 0.1.0-alpha.1` defines fallible task and semaphore
  capabilities without depending on WS63, RF, a scheduler, an allocator or a
  network stack.
- Exactly one static `Runtime` can be installed. Reinstalling the same runtime
  is idempotent; selecting a different implementation fails explicitly.
- Task stack size is a validated non-zero value, waits distinguish no-wait,
  bounded and forever, and semaphore wake is documented as bounded/ISR-safe.
- The transitional cooperative runtime implements the contract. The actual
  vendor `osal_kthread_create`, `osal_msleep` and current-task ABI now dispatch
  through `hisi-rf-rtos-driver`, rather than merely compiling an unused facade.

## Host And Link Evidence

- Driver crate: fmt, clippy, unit test and `cargo package --locked` pass.
- RF crate: 15/15 host library tests pass with `net,wifi-wpa2-personal`.
- Parent workspace check, CI-shape clippy and default release build pass.
- Guarded RF link verifies 1,486 layout sections, 5,335 vendor relocations and
  37 mask-ROM patches.
- Final ELF SHA-256:
  `976273574176f4cd9ef5cb8450809a8b3e97717fbb3f858adc1e8bcbbb064036`.
- Planned image SHA-256:
  `90e7bc55222f835703cf150c7791d1cc35de36f10fab91cc9cd435ce469810f2`.

## WS63 HIL

The planned image was downloaded through the probe-rs raw-bin path, booted by
J-Link nRST and captured from UART0 at 115200 baud. The same firmware produced:

```text
RF1_IMAGE_OK
RF2_INIT_OK ifname=wlan0 ... mac=<redacted>
RF3_SCAN_OK count=0x00000019
RF5B_WPA_CONNECT_OK freq=0x0000096c
RF5A_DHCP_OK addr=192.168.155.2 prefix=0x00000018 router=192.168.155.1
RF5A_ARP_OK rx=0x00000004
RF5C_PING_OK rx=0x00000005
```

The test passphrase was injected at build time and is not committed or included
in this evidence file.

## Remaining A3 Work

- Route semaphore/wait/event operations through the driver contract.
- Extract scheduler, context switch, task stacks and semaphore implementation to
  the independent `hisi-rtos` release unit.
- Add priority ordering, timer/software-interrupt preemption, deferred stack
  reclamation, Embassy executor/time integration and FP-context stress HIL.
