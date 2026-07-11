//! Bridges mask-ROM arithmetic veneers to Rust's compiler-builtins runtime.
//!
//! The WS63 ROM calls selected compiler helpers through its ordered callback
//! table. Keep the ROM implementation as the caller and expose distinct bridge
//! names so the bridge body can use normal Rust arithmetic without recursively
//! entering the same ROM veneer.

/// Unsigned 64-bit division callback used by the ROM systick implementation.
#[unsafe(no_mangle)]
pub extern "C" fn __ws63_udivdi3(dividend: u64, divisor: u64) -> u64 {
    dividend / divisor
}

/// Unsigned 64-bit remainder callback paired with [`__ws63_udivdi3`].
#[unsafe(no_mangle)]
pub extern "C" fn __ws63_umoddi3(dividend: u64, divisor: u64) -> u64 {
    dividend % divisor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_unsigned_64_bit_arithmetic() {
        assert_eq!(__ws63_udivdi3(10_001, 100), 100);
        assert_eq!(__ws63_umoddi3(10_001, 100), 1);
    }
}
