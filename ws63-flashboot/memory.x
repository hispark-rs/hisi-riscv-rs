/*
 * Memory layout for WS63 Flashboot (second-stage bootloader).
 *
 * Flashboot runs from the PROGRAM region in SPI NOR flash and uses
 * a dedicated SRAM area (0xA28000) for stack and data.
 *
 *   PROGRAM:  0x230300 - 0x4F0300  (~2.75MB app code in flash)
 *   SRAM:     0xA00000 - 0xA90000  (576K main system RAM)
 *   FLASHBOOT_RAM: 0xA28000        (dedicated 32KB for flashboot)
 */

MEMORY
{
    /* Application program region in SPI flash */
    PROGRAM  (rx) : ORIGIN = 0x230300, LENGTH = 0x240000

    /* Flashboot dedicated RAM (32KB at 0xA28000) */
    FLASHBOOT_RAM (rwx) : ORIGIN = 0xA28000, LENGTH = 0x8000
}

REGION_ALIAS("REGION_TEXT", PROGRAM);
REGION_ALIAS("REGION_RODATA", PROGRAM);
REGION_ALIAS("REGION_DATA", FLASHBOOT_RAM);
REGION_ALIAS("REGION_BSS", FLASHBOOT_RAM);
REGION_ALIAS("REGION_STACK", FLASHBOOT_RAM);
REGION_ALIAS("REGION_HEAP", FLASHBOOT_RAM);

/* Stack size for flashboot */
__stack_size = 0x2000;  /* 8KB */

/* riscv-rt required symbols */
PROVIDE(_max_hart_id = 0);
PROVIDE(_hart_stack_size = __stack_size);
PROVIDE(_stack_start = ORIGIN(FLASHBOOT_RAM) + LENGTH(FLASHBOOT_RAM));
