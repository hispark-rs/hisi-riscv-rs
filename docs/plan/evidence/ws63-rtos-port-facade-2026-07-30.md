# WS63 RTOS port facade evidence

## Scope

This evidence closes the first user-facing WS63 port facade increment. It does
not close the deferred caller-owned `SchedulerStorage<N>` migration or the full
Cooperative/Budgeted/Preemptive/Embassy policy matrix.

## Release units

- `hisi-rtos 0.1.0-alpha.15` (`ca46c677acef5871629d6094b5387567a65ccdb0`)
- `ws63-examples` (`76f0a57fd7bec9d5e23cc0ada27d911755b2607e`)
- `hisi-rs-template v0.7.0-alpha.18`

The RTOS standalone package resolved `hisi-hal 0.7.0-alpha.6` from crates.io.
Its isolated release checks passed host tests, RV32 clippy with `chip-ws63`,
RV32 build with `chip-ws63,embassy`, and `cargo package --locked`.

## Public shape

The application now:

1. binds TIMER_INT0 and SOFT_INT0 with `hisi_rtos::bind_interrupts!`;
2. passes TIMER and SYS_CTL1 singleton tokens to
   `hisi_rtos::ws63::start`;
3. retains a typed WS63 runtime capability for ported policy operations.

The application no longer defines interrupt handler bodies, constructs
`SchedulerPort`, supplies the scheduler monotonic clock, or manually sequences
global interrupt enablement. The still-deferred storage work is explicit:
dynamic task stacks continue to use caller-provided allocation callbacks.

## Software verification

- hisi-rtos CI run `30492759785`: host tests, RV32 default and WS63 feature
  rows, Kani, and TLA+ all passed.
- hisi-rtos publish run `30492943437`: crates.io publish passed.
- A generated Wi-Fi project using only published dependencies completed an
  RV32 release build outside the parent workspace.
- hisi-rs-template CI run `30493223527`: WS63 Wi-Fi generation, check, release
  build, image plan, and Linux/macOS/Windows resource-report jobs passed.
- ws63-examples fmt run `30493826468` passed.

## Silicon verification

The final ELF identity was
`8be4402639d1c746c38151b2ad744b7de36aea21a3e8d9dbd9f4be7a54d0abca`.
It contained 37 ROM patches, zero vendor relocations, and the upstream native
supplicant.

The first 3 MHz probe-rs attempt failed while programming the first page at
`0x00230000`, then the retry could not reconnect to DMI. This is classified as
a probe transport failure, not firmware evidence. The board was restored with
the checked-in official full FWPKG through hisiflash.

The same ELF was then downloaded at 1 MHz with complete readback verification
in 142.62 seconds. J-Link nRST and UART capture produced:

- RF image, init, and scan markers;
- WPA2 connect;
- DHCP lease;
- direct gateway ARP request/reply;
- valid UDP DNS response from the primary public resolver;
- lease renewal and repeated runner-alive markers;
- zero event drop, DNS validation error, DNS transmit error, or backend error.

The repository connectivity contract reported `{"pass": 1}` and
`WS63 CONNECTIVITY SMOKE: PASS`.

No credential values or credential-file paths are part of this evidence.
