//! Minimal UART driver for flashboot debug output.
//!
//! Standalone implementation — no HAL dependency. Only TX (write-only).
//! Based on fbb_ws63 `hiburn_uart_init()` / `uart_port_set_clock_value()`.

// ── UART0 register addresses ─────────────────────────────────────

const UART0_BASE: u32 = 0x4401_0000;
const UART_CTL: *mut u16 = (UART0_BASE + 0x00) as *mut u16;
const UART_DIV_L: *mut u16 = (UART0_BASE + 0x04) as *mut u16;
const UART_DIV_H: *mut u16 = (UART0_BASE + 0x08) as *mut u16;
const UART_DIV_FRA: *mut u16 = (UART0_BASE + 0x0C) as *mut u16;
const UART_FIFO_CTL: *mut u16 = (UART0_BASE + 0x10) as *mut u16;
const UART_FIFO_STATUS: *const u16 = (UART0_BASE + 0x14) as *const u16;
const UART_DATA: *mut u16 = (UART0_BASE + 0x18) as *mut u16;

/// Initialize UART0 at 115200 baud for boot-time debug output.
///
/// Uses PCLK = 160 MHz (PLL-derived, switched before calling).
/// Baud rate = PCLK / (16 * div)
/// For 115200: div = 160_000_000 / (16 * 115200) ≈ 87
pub fn init(pclk: u32, baud: u32) {
    unsafe {
        // Disable UART during config
        UART_CTL.write_volatile(0);

        // Enable divider access
        UART_CTL.write_volatile(1 << 7); // DIV_EN

        // Set baud rate divider
        let div = pclk / (16 * baud);
        UART_DIV_L.write_volatile((div & 0xFF) as u16);
        UART_DIV_H.write_volatile(((div >> 8) & 0xFF) as u16);
        UART_DIV_FRA.write_volatile(0);

        // 8N1: data_bits=8 (3 << 2), no parity, 1 stop, UART_EN + DIV_EN
        UART_CTL.write_volatile(1 | (3 << 2) | (1 << 7)); // UART_EN=1, DIV_EN=1

        // Enable FIFO and clear
        UART_FIFO_CTL.write_volatile(0x07);
    }
}

/// Write a single byte to UART0 (blocking).
pub fn putc(c: u8) {
    unsafe {
        while UART_FIFO_STATUS.read_volatile() & (1 << 5) != 0 {} // tx_fifo_full
        UART_DATA.write_volatile(c as u16);
    }
}

/// Write a null-terminated string to UART0.
pub fn puts(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            putc(b'\r');
        }
        putc(b);
    }
}

/// Write a hex u32 value to UART0.
pub fn puthex32(val: u32) {
    putc(b'0');
    putc(b'x');
    for i in (0..8).rev() {
        let nibble = ((val >> (i * 4)) & 0xF) as u8;
        let c = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + nibble - 10
        };
        putc(c);
    }
}
