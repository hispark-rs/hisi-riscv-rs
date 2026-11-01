//! Autonomous Flash-start relocation consumer. No host gate or Flash commands.
//! QEMU-only at present; no UART/pad ownership or physical qualification.
#![no_std]
#![no_main]

use core::ptr::{addr_of, addr_of_mut};
use hisi_riscv_rt::entry;

#[used]
#[unsafe(no_mangle)]
static mut WS53_RAM_FUNCTIONS_REPORT: [u32; 12] = [0; 12];
static mut SEED: u32 = 0x53a019;

#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".ws53.ramrodata.table")]
static WS53_RAM_TABLE: [u32; 4] = [0x12, 0x34, 0x56, 0x78];

#[cfg(feature = "flash-data")]
#[used]
static FLASH_TABLE: [u32; 4] = [0x12, 0x34, 0x56, 0x78];

#[cfg(feature = "overflow")]
core::arch::global_asm!(".section .ws53.ramrodata.overflow,\"a\"\n.space 0x7004\n");

// The annotation is a placement promise only. Every callee/constant that must
// survive Flash unavailability needs explicit placement and an ELF audit.
#[inline(never)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".ws53.ramtext.leaf")]
unsafe extern "C" fn ws53_ram_leaf(seed: u32) -> u32 {
    #[cfg(not(feature = "flash-data"))]
    let table = addr_of!(WS53_RAM_TABLE).cast::<u32>();
    #[cfg(feature = "flash-data")]
    let table = addr_of!(FLASH_TABLE).cast::<u32>();
    let word = unsafe { table.add((seed & 3) as usize).read_volatile() };
    seed.wrapping_mul(33).wrapping_add(word)
}

#[cfg(feature = "flash-call")]
#[inline(never)]
#[unsafe(no_mangle)]
extern "C" fn ws53_flash_helper(seed: u32) -> u32 {
    core::hint::black_box(seed).wrapping_add(1)
}

#[inline(never)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".ws53.ramtext.root")]
unsafe extern "C" fn ws53_ram_root(seed: u32) -> u32 {
    let value = unsafe { ws53_ram_leaf(seed) };
    #[cfg(feature = "flash-call")]
    let value = ws53_flash_helper(value);
    value.rotate_left(7) ^ seed
}

#[inline(never)]
#[unsafe(no_mangle)]
extern "C" fn ws53_ram_functions_ready() {
    // Model execution breakpoint: no host gate or patched guest instruction.
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}

#[inline(never)]
#[unsafe(no_mangle)]
extern "C" fn ws53_ram_functions_done() {
    unsafe { core::arch::asm!("nop", options(nomem, nostack)) }
}

fn put(index: usize, value: u32) {
    unsafe {
        addr_of_mut!(WS53_RAM_FUNCTIONS_REPORT)
            .cast::<u32>()
            .add(index)
            .write_volatile(value)
    }
}

#[cfg(feature = "ram-functions")]
unsafe extern "C" {
    static __ws53_ramtext_load: u8;
    static __ws53_ramtext_begin: u8;
    static __ws53_ramtext_end: u8;
}

#[cfg(feature = "ram-functions")]
#[entry]
fn main() -> ! {
    // This exclusive FlashBoot fixture inherits the WDT and keeps Ccore reset.
    // Bound the unlock handshake; no claim of production shared ownership.
    let wdt = unsafe { &*ws53_pac::Wdt::PTR };
    unsafe {
        wdt.lock_command()
            .write_with_zero(|w| w.key().bits(0x5a5a5a5a))
    };
    let mut budget = 10_000;
    while wdt.lock_status().read().locked().bit_is_set() {
        if budget == 0 {
            put(1, 0xef);
            loop {
                core::hint::spin_loop();
            }
        }
        budget -= 1;
    }
    let _ = wdt.wdt_eoi().read();
    wdt.wdt_cr().modify(|_, w| w.wdt_en().clear_bit());
    put(0, 0x53535246);
    put(1, 1);
    let source = addr_of!(__ws53_ramtext_load);
    let dest = addr_of!(__ws53_ramtext_begin);
    let count = addr_of!(__ws53_ramtext_end) as usize - dest as usize;
    let copied =
        (0..count).all(|i| unsafe { source.add(i).read_volatile() == dest.add(i).read_volatile() });
    put(2, u32::from(copied));
    put(3, ws53_ram_root as *const () as u32);
    put(4, ws53_ram_leaf as *const () as u32);
    put(5, addr_of!(WS53_RAM_TABLE) as u32);
    put(6, count as u32);
    ws53_ram_functions_ready();
    let seed = unsafe { addr_of!(SEED).read_volatile() };
    let result = unsafe { ws53_ram_root(seed) };
    let expected = seed
        .wrapping_mul(33)
        .wrapping_add([0x12, 0x34, 0x56, 0x78][(seed & 3) as usize])
        .rotate_left(7)
        ^ seed;
    put(7, result);
    put(8, expected);
    put(9, seed);
    put(
        1,
        if copied && result == expected {
            5
        } else {
            0xee
        },
    );
    ws53_ram_functions_done();
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(feature = "ram-functions"))]
#[entry]
fn main() -> ! {
    // Keep annotations live to test the linker's absent-feature rejection,
    // without introducing missing relocation-bound symbols as an earlier error.
    put(7, unsafe { ws53_ram_root(addr_of!(SEED).read_volatile()) });
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    put(1, 0xef);
    loop {
        core::hint::spin_loop();
    }
}
