# ----------------------------------------------------------------------
#  Flashboot startup for HiSilicon WS63 (RISC-V RV32IMFC)
#
#  Minimal init before jumping to Rust main():
#  1. Clear PMP (pmpcfg0-3)
#  2. Set mtvec to trap_vector (vectored mode)
#  3. Disable interrupts (mstatus=0, mie=0)
#  4. Enable FPU (FS=0b11)
#  5. Initialize gp (global pointer)
#  6. Initialize sp (stack pointer → __stack_top__)
#  7. Clear BSS
#  8. Call flashboot_main()
#
#  Based on fbb_ws63 flashboot_ws63/startup/riscv_init.S.
# ----------------------------------------------------------------------

.section .text.entry, "ax"
.global _start
.option norvc

_start:
    # ── PMP disable ──────────────────────────────────────────
    li    t0, 0
    csrw  pmpcfg0, t0
    li    t0, 0
    csrw  pmpcfg1, t0
    li    t0, 0
    csrw  pmpcfg2, t0
    li    t0, 0
    csrw  pmpcfg3, t0

    # ── Clear custom WS63 CSR ────────────────────────────────
    li    t0, 0
    csrw  0x7d9, t0

    # ── Save boot flag from a0 ───────────────────────────────
    # (fbb_ws63: saves to __flash_boot_flag_begin__)
    la    t0, __flash_boot_flag
    sw    a0, 0(t0)

    # ── Set trap vector (vectored mode: mtvec[0]=1) ──────────
    la    t0, trap_vector
    addi  t0, t0, 1
    csrw  mtvec, t0

    # ── Disable interrupts during init ───────────────────────
    csrwi mstatus, 0
    csrwi mie, 0

    # ── Enable FPU (FS = 0b11) ───────────────────────────────
    li    t0, 0x00006000
    csrs  mstatus, t0
    csrwi fflags, 0

    # ── Initialize global pointer ────────────────────────────
    .option push
    .option norelax
    la    gp, _gp_
    .option pop

    # ── Initialize stack pointer ─────────────────────────────
    la    sp, __stack_top__

    # ── Zero BSS ─────────────────────────────────────────────
    la    t0, __sbss
    la    t1, __ebss
    bgeu  t0, t1, 2f
1:
    sw    zero, 0(t0)
    addi  t0, t0, 4
    bltu  t0, t1, 1b
2:

    # ── Jump to Rust entry ───────────────────────────────────
    tail  flashboot_main


# ======================================================================
#  Minimal trap vector (all entries → inf loop for flashboot)
# ======================================================================

.section .trap, "ax"
.align 6

.global trap_vector
trap_vector:
    j default_trap_handler      # 0
    j default_trap_handler      # 1
    j default_trap_handler      # 2
    j default_trap_handler      # 3
    j default_trap_handler      # 4
    j default_trap_handler      # 5
    j default_trap_handler      # 6
    j default_trap_handler      # 7
    j default_trap_handler      # 8
    j default_trap_handler      # 9
    j default_trap_handler      # 10
    j default_trap_handler      # 11
    j default_trap_handler      # 12 (NMI)
    j default_trap_handler      # 13
    j default_trap_handler      # 14
    j default_trap_handler      # 15
    j default_trap_handler      # 16
    j default_trap_handler      # 17
    j default_trap_handler      # 18
    j default_trap_handler      # 19
    j default_trap_handler      # 20
    j default_trap_handler      # 21
    j default_trap_handler      # 22
    j default_trap_handler      # 23
    j default_trap_handler      # 24
    j default_trap_handler      # 25
    j default_trap_handler      # 26
    j default_trap_handler      # 27
    j default_trap_handler      # 28
    j default_trap_handler      # 29
    j default_trap_handler      # 30
    j default_trap_handler      # 31
    .rept 60
    j default_trap_handler
    .endr
.option rvc

# ======================================================================
#  Default trap handler — infinite loop
# ======================================================================

.section .text.default_handlers, "ax"
.global default_trap_handler
default_trap_handler:
    j .


# ======================================================================
#  Boot flag storage (written by startup, read by Rust code)
# ======================================================================

.section .data.boot_flag, "aw"
.global __flash_boot_flag
__flash_boot_flag:
    .4byte 0