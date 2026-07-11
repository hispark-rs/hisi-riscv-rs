//! Bridges mask-ROM arithmetic veneers to Rust's compiler-builtins runtime.
//!
//! The WS63 ROM calls selected compiler helpers through its ordered callback
//! table. Keep the ROM implementation as the caller and expose distinct bridge
//! names so the bridge body can use normal Rust arithmetic without recursively
//! entering the same ROM veneer.

/// 64-bit logical left shift callback used by ROM TX completion.
///
/// Implement this from 32-bit halves so RV32 codegen cannot lower the bridge
/// itself back into `__ashldi3` and recurse through the same ROM veneer.
#[unsafe(no_mangle)]
pub extern "C" fn __ws63_ashldi3(value: u64, shift: u32) -> u64 {
    let shift = shift & 63;
    if shift == 0 {
        return value;
    }
    let low = value as u32;
    let high = (value >> 32) as u32;
    let (new_low, new_high) = if shift < 32 {
        (low << shift, (high << shift) | (low >> (32 - shift)))
    } else {
        (0, low << (shift - 32))
    };
    ((new_high as u64) << 32) | new_low as u64
}

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

    #[test]
    fn bridges_64_bit_left_shift_without_recursive_builtin() {
        let value = 0x0123_4567_89ab_cdef;
        for shift in 0..64 {
            assert_eq!(__ws63_ashldi3(value, shift), value.wrapping_shl(shift));
        }
    }
}
