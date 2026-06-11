/*
 * Memory layout for HiSilicon BS20 / BS2X (RV32IMFC, SparkLink/NearLink).
 *
 *   ROM:      0x000000 - 0x080000  (mask ROM; 32K MPU window, symbols to ~0x40000)
 *   ITCM:     0x080000 - 0x100000  (512K instruction TCM)
 *   L2RAM:    0x100000 - 0x120000  (128K main RAM — BS20; BS21E/BS22 are 160K)
 *   FLASH:    0x10000000           (1M XIP NOR flash)
 *
 * Milestone-1 layout (QEMU `-kernel`): code in flash (XIP), data/bss/stack in
 * L2RAM. Matches the WS63 PROGRAM=flash / SRAM=ram split. Values from the
 * fbb_bs2x SDK (platform_core.h); see docs/bs21-recon.md.
 */

MEMORY
{
    /* Mask ROM (secure-libc / printf / timing / watchdog live here — not used by
       a standard RV32IMFC Rust app; region exists only for the PROVIDE symbols). */
    BOOTROM  (rx) : ORIGIN = 0x00000000, LENGTH = 0x8000
    ROM      (rx) : ORIGIN = 0x00008000, LENGTH = 0x78000

    /* Instruction TCM (512K) */
    ITCM     (rwx): ORIGIN = 0x00080000, LENGTH = 0x70000

    /* Data TCM — carved from the top of the TCM window */
    DTCM     (rw) : ORIGIN = 0x000F0000, LENGTH = 0x10000

    /* XIP NOR flash (1M) + the program region within it */
    FLASH    (rx) : ORIGIN = 0x10000000, LENGTH = 0x100000
    PROGRAM  (rx) : ORIGIN = 0x10000000, LENGTH = 0x100000

    /* Main system RAM (L2RAM, 128K) */
    SRAM     (rwx): ORIGIN = 0x00100000, LENGTH = 0x20000

    /* Preserved region (256 bytes at the top of L2RAM for boot state) */
    PRESERVE (rw) : ORIGIN = 0x00120000 - 0x100, LENGTH = 0x100
}

/* Memory regions exported as symbols for runtime relocation (same set hisi-riscv-rt's
   layout.ld / startup.S expect). */
PROVIDE(__rom_start = ORIGIN(ROM));
PROVIDE(__rom_length = LENGTH(ROM));
PROVIDE(__itcm_start = ORIGIN(ITCM));
PROVIDE(__itcm_length = LENGTH(ITCM));
PROVIDE(__dtcm_start = ORIGIN(DTCM));
PROVIDE(__dtcm_length = LENGTH(DTCM));
PROVIDE(__sram_start = ORIGIN(SRAM));
PROVIDE(__sram_length = LENGTH(SRAM));
PROVIDE(__flash_start = ORIGIN(FLASH));
PROVIDE(__flash_length = LENGTH(FLASH));
PROVIDE(__program_start = ORIGIN(PROGRAM));
PROVIDE(__program_length = LENGTH(PROGRAM));

/* Stack sizes (overridable). */
__stack_size     = DEFINED(__stack_size)     ? __stack_size     : 0x2000;
__irq_stack_size = DEFINED(__irq_stack_size) ? __irq_stack_size : 0x800;
__exc_stack_size = DEFINED(__exc_stack_size) ? __exc_stack_size : 0x800;
__nmi_stack_size = DEFINED(__nmi_stack_size) ? __nmi_stack_size : 0x400;

/* riscv-rt v0.14 required symbols. Stack top = top of L2RAM. */
PROVIDE(_stack_start = ORIGIN(SRAM) + LENGTH(SRAM));
PROVIDE(_max_hart_id = 0);
PROVIDE(_hart_stack_size = 0x2000);

PROVIDE(__sidata = 0);
PROVIDE(__sdata = 0);
PROVIDE(__edata = 0);
PROVIDE(__sbss = 0);
PROVIDE(__ebss = 0);

/* riscv-rt v0.14 region aliases (same mapping as WS63: text/rodata in flash,
   data/bss/stack/heap in RAM). */
REGION_ALIAS("REGION_TEXT", PROGRAM);
REGION_ALIAS("REGION_RODATA", PROGRAM);
REGION_ALIAS("REGION_DATA", SRAM);
REGION_ALIAS("REGION_BSS", SRAM);
REGION_ALIAS("REGION_STACK", SRAM);
REGION_ALIAS("REGION_HEAP", SRAM);
