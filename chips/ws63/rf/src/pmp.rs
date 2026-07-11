//! WS63 application-owned PMP and memory-attribute setup.
//!
//! Flashboot owns and locks PMP entries 2 through 4. The application fills
//! entries 0, 1, and 5 through 11 before starting the vendor Wi-Fi runtime.
//! The values mirror `fbb_ws63`'s `pmp_cfg.c` default EVB memory layout.

#[cfg(target_arch = "riscv32")]
const PMP_TOR_LOCKED: u8 = 0x88;
#[cfg(target_arch = "riscv32")]
const PMP_R: u8 = 0x01;
#[cfg(target_arch = "riscv32")]
const PMP_W: u8 = 0x02;
#[cfg(target_arch = "riscv32")]
const PMP_X: u8 = 0x04;

/// Configure the PMP entries and memory attributes owned by the WS63 app.
///
/// Flashboot-provided entries 2 through 4 are deliberately left untouched.
/// Call this exactly once after runtime relocation and before invoking any
/// vendor RF entry point.
///
/// # Safety
///
/// This permanently locks PMP entries for the current boot. The linked image
/// must use the bundled WS63 memory layout and must not have configured entries
/// 0, 1, or 5 through 11 for another purpose.
pub unsafe fn prepare_vendor_memory() {
    #[cfg(target_arch = "riscv32")]
    unsafe {
        unsafe extern "C" {
            static __sram_text_begin__: u8;
            static __sram_text_end__: u8;
            static __radar_start: u8;
            static __radar_length: u8;
        }

        const ROX: u8 = PMP_TOR_LOCKED | PMP_R | PMP_X;
        const RW: u8 = PMP_TOR_LOCKED | PMP_R | PMP_W;
        const RWX: u8 = PMP_TOR_LOCKED | PMP_R | PMP_W | PMP_X;

        let sram_text_begin = &raw const __sram_text_begin__ as usize;
        let sram_text_end = (&raw const __sram_text_end__ as usize) & !0x1f;
        let radar_start = &raw const __radar_start as usize;
        let radar_end = radar_start + (&raw const __radar_length as usize);

        configure_tor(0, 0x0018_0000, ROX, 3);
        configure_tor(1, 0x001c_8000, RW, 3);

        configure_tor(5, sram_text_begin, RWX, 7);
        configure_tor(6, sram_text_end, ROX, 7);
        configure_tor(7, radar_start, RWX, 7);
        configure_tor(8, radar_end, RWX, 3);
        configure_tor(9, 0x00a9_8800, RWX, 7);
        configure_tor(10, 0x00c0_1000, ROX, 3);
        configure_tor(11, 0x4a00_1000, RWX, 0);

        core::arch::asm!("fence iorw, iorw", options(nostack));
    }
}

#[cfg(target_arch = "riscv32")]
unsafe fn configure_tor(index: usize, end_addr: usize, config: u8, attr: u8) {
    unsafe {
        write_pmpaddr(index, end_addr >> 2);
        write_memattr(index, attr);

        // Match the vendor HAL ordering: enable TOR and lock only after the
        // address, memory type, and permissions have been installed.
        write_pmpcfg_byte(index, config & 0x07);
        write_pmpcfg_byte(index, config);
    }
}

#[cfg(target_arch = "riscv32")]
unsafe fn write_pmpaddr(index: usize, value: usize) {
    macro_rules! write {
        ($csr:literal) => {
            core::arch::asm!(
                concat!("csrw ", $csr, ", {value}"),
                value = in(reg) value,
                options(nomem, nostack)
            )
        };
    }
    unsafe {
        match index {
            0 => write!("pmpaddr0"),
            1 => write!("pmpaddr1"),
            5 => write!("pmpaddr5"),
            6 => write!("pmpaddr6"),
            7 => write!("pmpaddr7"),
            8 => write!("pmpaddr8"),
            9 => write!("pmpaddr9"),
            10 => write!("pmpaddr10"),
            11 => write!("pmpaddr11"),
            _ => unreachable!(),
        }
    }
}

#[cfg(target_arch = "riscv32")]
unsafe fn write_memattr(index: usize, attr: u8) {
    let shift = if index < 8 {
        index * 4
    } else {
        (index - 8) * 4
    };
    let mut value: usize;
    unsafe {
        if index < 8 {
            core::arch::asm!("csrr {value}, 0x7d8", value = out(reg) value, options(nomem, nostack));
            value = (value & !(0xf << shift)) | ((attr as usize) << shift);
            core::arch::asm!("csrw 0x7d8, {value}", value = in(reg) value, options(nomem, nostack));
        } else {
            core::arch::asm!("csrr {value}, 0x7d9", value = out(reg) value, options(nomem, nostack));
            value = (value & !(0xf << shift)) | ((attr as usize) << shift);
            core::arch::asm!("csrw 0x7d9, {value}", value = in(reg) value, options(nomem, nostack));
        }
    }
}

#[cfg(target_arch = "riscv32")]
unsafe fn write_pmpcfg_byte(index: usize, config: u8) {
    let shift = (index % 4) * 8;
    macro_rules! update {
        ($csr:literal) => {{
            let mut value: usize;
            core::arch::asm!(
                concat!("csrr {value}, ", $csr),
                value = out(reg) value,
                options(nomem, nostack)
            );
            value = (value & !(0xff << shift)) | ((config as usize) << shift);
            core::arch::asm!(
                concat!("csrw ", $csr, ", {value}"),
                value = in(reg) value,
                options(nomem, nostack)
            );
        }};
    }
    unsafe {
        match index / 4 {
            0 => update!("pmpcfg0"),
            1 => update!("pmpcfg1"),
            2 => update!("pmpcfg2"),
            _ => unreachable!(),
        }
    }
}
