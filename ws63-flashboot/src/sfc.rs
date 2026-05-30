//! SPI Flash Controller (SFC) initialization for WS63 flashboot.
//!
//! Based on fbb_ws63 `sfc_init()` in flashboot main.c.
//! Configures the SFC for quad-SPI fast read from external NOR flash.

// ── SFC register addresses ───────────────────────────────────────

/// SFC global config register
const SFC_GLOBAL_CONFIG: *mut u32 = 0x4800_0100 as *mut u32;
/// SFC bus config register 1
const SFC_BUS_CONFIG1: *mut u32 = 0x4800_0200 as *mut u32;
/// SFC timing register
const SFC_TIMING: *mut u32 = 0x4800_0110 as *mut u32;
/// SFC soft reset mask
const SFC_SOFT_RST_MASK: *mut u32 = 0x4800_0130 as *mut u32;
/// SFC interrupt clear
const SFC_INT_CLEAR: *mut u32 = 0x4800_012C as *mut u32;
/// SFC command instruction register
const SFC_CMD_INS: *mut u32 = 0x4800_0308 as *mut u32;
/// SFC command address register
const SFC_CMD_ADDR: *mut u32 = 0x4800_030C as *mut u32;
/// SFC command config register
const SFC_CMD_CONFIG: *mut u32 = 0x4800_0300 as *mut u32;
/// SFC command data buffer (16 words)
const SFC_CMD_DATABUF: *mut u32 = 0x4800_0400 as *mut u32;
/// SFC interrupt status
const SFC_INT_STATUS: *const u32 = 0x4800_0124 as *const u32;

// ── Image header structure ───────────────────────────────────────

/// Key area (0x100 bytes): signature + public key
#[repr(C)]
pub struct KeyArea {
    pub key_id: u32,            // +0x00
    pub key_type: u32,          // +0x04
    pub key_length: u32,        // +0x08
    pub sig_length: u32,        // +0x0C
    pub sig_scheme: u32,        // +0x10
    _reserved: [u8; 0xF0],     // +0x14..0x100
}

/// Code info area (0x200 bytes): image metadata
#[repr(C)]
pub struct CodeInfo {
    pub image_id: u32,          // +0x100
    pub structure_version: u32, // +0x104
    pub structure_length: u32,  // +0x108
    pub signature_length: u32,  // +0x10C
    pub image_version: u32,     // +0x110 (params_version_ext)
    pub image_length: u32,      // +0x114 (offset in struct)
    pub load_addr: u32,         // +0x118
    _reserved: [u8; 0x1E4],    // +0x11C..0x300
}

/// Combined image header (key_area + code_info = 0x300 bytes).
#[repr(C)]
pub struct ImageHeader {
    pub key_area: KeyArea,
    pub code_info: CodeInfo,
}

impl ImageHeader {
    /// Read an ImageHeader from flash at `addr` via SFC command.
    pub fn read(flash_addr: u32) -> Self {
        // All-zero header is fine — invalid fields will be caught by image::validate()
        let mut header: ImageHeader = unsafe { core::mem::zeroed() };
        let buf = &mut header as *mut ImageHeader as *mut u32;
        sfc_read_data(flash_addr, buf, core::mem::size_of::<ImageHeader>() as u32 / 4);
        header
    }
}

// ── SFC initialization ───────────────────────────────────────────

/// Initialize the SFC for quad-SPI read from flash.
///
/// Returns `true` on success, `false` on failure.
pub fn sfc_init(_tcxo_hz: u32) -> bool {
    unsafe {
        // 1. Release SFC from soft reset
        SFC_SOFT_RST_MASK.write_volatile(0x01);

        // 2. Clear all interrupts
        SFC_INT_CLEAR.write_volatile(0x03);

        // 3. Configure timing (from fbb_ws63 defaults)
        // tshsl=5, tcss=1, tcsh=1
        // timing = tshsl | (tcss << 8) | (tcsh << 12) = 5 | (1<<8) | (1<<12) = 0x1105
        SFC_TIMING.write_volatile(0x1105);

        // 4. Configure bus for quad-SPI fast read
        // rd_mem_if_type = 4 (Quad I/O), rd_dummy = 4, rd_ins = 0xEB (Quad I/O Fast Read)
        // wr_mem_if_type = 2 (Dual I/O), wr_ins = 0x02 (Page Program)
        let bus_cfg1: u32 =
            (4 << 0)   // rd_mem_if_type: Quad I/O
            | (4 << 3)  // rd_dummy_bytes: 4
            | (0xEB << 8)  // rd_ins: Quad I/O Fast Read
            | (2 << 16)    // wr_mem_if_type: Dual I/O
            | (0x02 << 22); // wr_ins: Page Program
        SFC_BUS_CONFIG1.write_volatile(bus_cfg1);

        // 5. Configure global settings
        // SPI mode 0, no write protect, 3-byte address, rd_delay = 0
        SFC_GLOBAL_CONFIG.write_volatile(0x00);

        true
    }
}

/// Read image header from flash at the given address.
pub fn read_image_header(flash_addr: u32) -> ImageHeader {
    ImageHeader::read(flash_addr)
}

/// Read data from flash using SFC command.
fn sfc_read_data(addr: u32, dst: *mut u32, words: u32) {
    unsafe {
        // Set instruction (standard SPI read: 0x03, or quad read: 0xEB)
        SFC_CMD_INS.write_volatile(0x03);
        // Set address
        SFC_CMD_ADDR.write_volatile(addr);

        // Build command config
        let data_len = words * 4; // convert words to bytes
        let cmd_cfg: u32 =
            (1 << 0)    // start
            | (1 << 2)   // addr_en
            | (1 << 7)   // data_en
            | (1 << 8);  // rw = read

        // For small reads (< 64 bytes), use 1-word data buffer
        SFC_CMD_CONFIG.write_volatile(cmd_cfg | (((data_len - 1) & 0x3F) << 9));

        // Read data from buffer register
        for i in 0..words {
            while SFC_INT_STATUS.read_volatile() & 0x01 == 0 {} // wait for cmd done
            let word = SFC_CMD_DATABUF.read_volatile();
            dst.add(i as usize).write_volatile(word);
            if i < words - 1 {
                SFC_INT_CLEAR.write_volatile(0x01);
            }
        }
        SFC_INT_CLEAR.write_volatile(0x01);
    }
}
