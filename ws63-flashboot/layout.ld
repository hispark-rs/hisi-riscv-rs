/*
 * Minimal linker layout for WS63 flashboot.
 *
 * Places .text.entry first (startup.S), then .text, .rodata, .data, .bss.
 * All code runs from PROGRAM (XIP), data/BSS in FLASHBOOT_RAM.
 */

SECTIONS
{
    .text.entry : ALIGN(8) {
        KEEP(*(.text.entry))
        . = ALIGN(4);
    } > PROGRAM

    .text : ALIGN(8) {
        *(.text .text.*)
        . = ALIGN(4);
    } > PROGRAM

    .rodata : ALIGN(8) {
        *(.rodata .rodata.*)
        . = ALIGN(4);
    } > PROGRAM

    .trap : ALIGN(64) {
        KEEP(*(.trap))
        . = ALIGN(4);
    } > PROGRAM

    .data : ALIGN(8) {
        *(.data .data.*)
        *(.data.boot_flag)
        . = ALIGN(4);
        _gp_ = . + 0x800;
    } > FLASHBOOT_RAM AT > PROGRAM

    .bss (NOLOAD) : ALIGN(8) {
        *(.bss .bss.*)
        . = ALIGN(4);
    } > FLASHBOOT_RAM
}
