/*
 * Memory layout for WS63 Flashboot (second-stage bootloader).
 *
 * Flashboot runs from the PROGRAM region and uses 32KB of SRAM at 0xA28000.
 * fbb_ws63 equivalent: flashboot uses FLASHBOOT_RAM_ADDR = 0xA28000.
 */

MEMORY
{
    PROGRAM        (rx) : ORIGIN = 0x230300, LENGTH = 0x240000
    FLASHBOOT_RAM (rwx) : ORIGIN = 0xA28000, LENGTH = 0x8000
}

REGION_ALIAS("REGION_TEXT", PROGRAM);
REGION_ALIAS("REGION_RODATA", PROGRAM);
REGION_ALIAS("REGION_DATA", FLASHBOOT_RAM);
REGION_ALIAS("REGION_BSS", FLASHBOOT_RAM);
REGION_ALIAS("REGION_STACK", FLASHBOOT_RAM);

/* Stack (8KB, top of FLASHBOOT_RAM) */
__stack_size = 0x2000;
__stack_start__ = ORIGIN(FLASHBOOT_RAM) + LENGTH(FLASHBOOT_RAM) - __stack_size;
__stack_top__   = ORIGIN(FLASHBOOT_RAM) + LENGTH(FLASHBOOT_RAM);

/* BSS boundaries (for startup.S zeroing) */
__sbss = ADDR(.bss);
__ebss = ADDR(.bss) + SIZEOF(.bss);

/* riscv-rt fallback symbols (not used by flashboot directly, but prevent linker errors) */
PROVIDE(_max_hart_id = 0);
PROVIDE(_hart_stack_size = __stack_size);
PROVIDE(_stack_start = __stack_top__);
PROVIDE(DefaultHandler = default_trap_handler);