//! Hardware-only PAC diagnostic under the runner's application/control-core
//! takeover. Sends only JEDEC ID, status and ordinary read commands to CS1.
//! No WREN, program, erase, protection, FAPC or remap configuration writes.
use hisi_hal::{peripherals::Uart2, uart::Uart};
use ws53_pac::sfc_cfg::RegisterBlock;

fn hex(uart: &Uart<'_, Uart2<'_>>, value: u32, digits: u32) {
    for i in (0..digits).rev() {
        super::emit(
            uart,
            &[b"0123456789abcdef"[((value >> (4 * i)) & 15) as usize]],
        );
    }
}

#[derive(Clone, Copy)]
enum Command {
    Identity,
    Status1,
    Status2,
    Status3,
    Read(u32),
}
fn read(regs: &RegisterBlock, command: Command) -> [u8; 64] {
    assert!(
        regs.cmd_config().read().start().bit_is_clear(),
        "SFC initially busy"
    );
    let (instruction, address, length) = match command {
        Command::Identity => (0x9f, None, 3),
        Command::Status1 => (0x05, None, 1),
        Command::Status2 => (0x35, None, 1),
        Command::Status3 => (0x15, None, 1),
        Command::Read(offset) => {
            assert!(offset <= 0x400000 - 64);
            (0x03, Some(offset), 64)
        }
    };
    regs.cmd_ins()
        .modify(|_, w| unsafe { w.reg_ins().bits(instruction) });
    if let Some(offset) = address {
        unsafe {
            regs.cmd_addr()
                .write_with_zero(|w| w.cmd_addr().bits(offset))
        };
    }
    // SDK hal_sfc_regs_set_opt + set_opt_attr: standard SPI, no dummy bytes,
    // CS1, read direction, data length-1, optional 24-bit address, start.
    unsafe {
        regs.cmd_config().write_with_zero(|w| {
            w.sel_cs()
                .set_bit()
                .addr_en()
                .bit(address.is_some())
                .data_en()
                .set_bit()
                .rw()
                .set_bit()
                .data_cnt()
                .bits(length - 1)
                .start()
                .set_bit()
        })
    };
    let mut complete = false;
    for _ in 0..1_000_000 {
        if regs.cmd_config().read().start().bit_is_clear() {
            complete = true;
            break;
        }
    }
    assert!(complete, "SFC command timeout; reset required");
    // These are ordinary data-buffer slots, not FIFO pop registers.
    let words = [
        regs.cmd_databuf_0().read().bits(),
        regs.cmd_databuf_1().read().bits(),
        regs.cmd_databuf_2().read().bits(),
        regs.cmd_databuf_3().read().bits(),
        regs.cmd_databuf_4().read().bits(),
        regs.cmd_databuf_5().read().bits(),
        regs.cmd_databuf_6().read().bits(),
        regs.cmd_databuf_7().read().bits(),
        regs.cmd_databuf_8().read().bits(),
        regs.cmd_databuf_9().read().bits(),
        regs.cmd_databuf_10().read().bits(),
        regs.cmd_databuf_11().read().bits(),
        regs.cmd_databuf_12().read().bits(),
        regs.cmd_databuf_13().read().bits(),
        regs.cmd_databuf_14().read().bits(),
        regs.cmd_databuf_15().read().bits(),
    ];
    let mut bytes = [0; 64];
    for (dest, word) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(words) {
        dest.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

pub fn check(uart: &Uart<'_, Uart2<'_>>) {
    let regs = unsafe { &*ws53_pac::SfcCfg::PTR };
    assert_eq!(regs.bus_dma_ctrl().read().bits() & 1, 0, "SFC DMA active");
    assert!(
        regs.global_config().read().flash_addr_mode().bit_is_clear(),
        "unexpected four-byte mode"
    );
    let mut idle = false;
    for _ in 0..1000 {
        if read(regs, Command::Status1)[0] & 1 == 0 {
            idle = true;
            break;
        }
    }
    assert!(idle, "Flash busy");
    let id = read(regs, Command::Identity);
    assert!(
        id[..3] != [0, 0, 0] && id[..3] != [255, 255, 255],
        "invalid JEDEC ID"
    );
    super::emit(uart, b"WS53 SFC ID ");
    for byte in &id[..3] {
        hex(uart, u32::from(*byte), 2);
    }
    super::emit(uart, b" SR ");
    for command in [Command::Status1, Command::Status2, Command::Status3] {
        hex(uart, u32::from(read(regs, command)[0]), 2);
    }
    super::emit(uart, b"\r\n");
    for offset in [
        0, 0x20000, 0x30000, 0x30300, 0x5d000, 0x5d300, 0x80000, 0x3fc000,
    ] {
        let bytes = read(regs, Command::Read(offset));
        super::emit(uart, b"WS53 SFC READ ");
        hex(uart, offset, 8);
        super::emit(uart, b" ");
        for byte in bytes {
            hex(uart, u32::from(byte), 2);
        }
        super::emit(uart, b"\r\n");
    }
    super::emit(uart, b"WS53 SFC READ DONE\r\n");
}
