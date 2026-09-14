//! Safe Rust reimplementation of Google V8's `src/base/bits.h` and `src/base/bits.cc`.
//!
//! All functions preserve 1:1 observable semantics and boundary behaviors as defined in V8.

/// Returns the number of bits set in `value`.
#[inline]
pub const fn count_population_u8(value: u8) -> u32 {
    value.count_ones()
}

#[inline]
pub const fn count_population_u16(value: u16) -> u32 {
    value.count_ones()
}

#[inline]
pub const fn count_population_u32(value: u32) -> u32 {
    value.count_ones()
}

#[inline]
pub const fn count_population_u64(value: u64) -> u32 {
    value.count_ones()
}

/// ReverseBits returns `value` in reverse bit order.
#[inline]
pub const fn reverse_bits_u8(value: u8) -> u8 {
    value.reverse_bits()
}

#[inline]
pub const fn reverse_bits_u16(value: u16) -> u16 {
    value.reverse_bits()
}

#[inline]
pub const fn reverse_bits_u32(value: u32) -> u32 {
    value.reverse_bits()
}

#[inline]
pub const fn reverse_bits_u64(value: u64) -> u64 {
    value.reverse_bits()
}

/// ReverseBytes returns `value` in reverse byte order.
#[inline]
pub const fn reverse_bytes_u8(value: u8) -> u8 {
    value
}

#[inline]
pub const fn reverse_bytes_u16(value: u16) -> u16 {
    value.swap_bytes()
}

#[inline]
pub const fn reverse_bytes_u32(value: u32) -> u32 {
    value.swap_bytes()
}

#[inline]
pub const fn reverse_bytes_u64(value: u64) -> u64 {
    value.swap_bytes()
}

/// CountLeadingZeros returns the number of zero bits following the most
/// significant 1 bit in `value` if `value` is non-zero, otherwise sizeof(T) * 8.
#[inline]
pub const fn count_leading_zeros_u16(value: u16) -> u32 {
    value.leading_zeros()
}

#[inline]
pub const fn count_leading_zeros_u32(value: u32) -> u32 {
    value.leading_zeros()
}

#[inline]
pub const fn count_leading_zeros_u64(value: u64) -> u32 {
    value.leading_zeros()
}

/// CountLeadingSignBits returns the number of leading zeros for a positive number,
/// and the number of leading ones for a negative number.
#[inline]
pub const fn count_leading_sign_bits_i32(value: i32) -> u32 {
    if value < 0 {
        (!value as u32).leading_zeros()
    } else {
        (value as u32).leading_zeros()
    }
}

#[inline]
pub const fn count_leading_sign_bits_i64(value: i64) -> u32 {
    if value < 0 {
        (!value as u64).leading_zeros()
    } else {
        (value as u64).leading_zeros()
    }
}

/// CountTrailingZeros returns the number of zero bits preceding the least
/// significant 1 bit in `value` if non-zero, otherwise sizeof(T) * 8.
#[inline]
pub const fn count_trailing_zeros_u16(value: u16) -> u32 {
    value.trailing_zeros()
}

#[inline]
pub const fn count_trailing_zeros_u32(value: u32) -> u32 {
    value.trailing_zeros()
}

#[inline]
pub const fn count_trailing_zeros_u64(value: u64) -> u32 {
    value.trailing_zeros()
}

#[inline]
pub const fn count_trailing_zeros_i32(value: i32) -> u32 {
    (value as u32).trailing_zeros()
}

#[inline]
pub const fn count_trailing_zeros_i64(value: i64) -> u32 {
    (value as u64).trailing_zeros()
}

/// Precondition: value != 0
#[inline]
pub const fn count_trailing_zeros_non_zero_u32(value: u32) -> u32 {
    debug_assert!(value != 0);
    value.trailing_zeros()
}

#[inline]
pub const fn count_trailing_zeros_non_zero_u64(value: u64) -> u32 {
    debug_assert!(value != 0);
    value.trailing_zeros()
}

/// Returns true iff `value` is a power of 2.
#[inline]
pub const fn is_power_of_two_u32(value: u32) -> bool {
    value > 0 && (value & (value - 1)) == 0
}

#[inline]
pub const fn is_power_of_two_u64(value: u64) -> bool {
    value > 0 && (value & (value - 1)) == 0
}

/// Identical to CountTrailingZeros, but only works for powers of 2.
#[inline]
pub const fn which_power_of_two_u32(value: u32) -> i32 {
    debug_assert!(is_power_of_two_u32(value));
    value.trailing_zeros() as i32
}

#[inline]
pub const fn which_power_of_two_u64(value: u64) -> i32 {
    debug_assert!(is_power_of_two_u64(value));
    value.trailing_zeros() as i32
}

/// RoundUpToPowerOfTwo32 returns the smallest power of two which is >= value.
/// Precondition: value <= 0x80000000u
#[inline]
pub const fn round_up_to_power_of_two_32(value: u32) -> u32 {
    debug_assert!(value <= (1u32 << 31));
    let v = if value > 0 { value - 1 } else { 0 };
    1u32 << (32 - v.leading_zeros())
}

/// RoundUpToPowerOfTwo64 returns the smallest power of two which is >= value.
/// Precondition: value <= 2^63
#[inline]
pub const fn round_up_to_power_of_two_64(value: u64) -> u64 {
    debug_assert!(value <= (1u64 << 63));
    let v = if value > 0 { value - 1 } else { 0 };
    1u64 << (64 - v.leading_zeros())
}

/// RoundDownToPowerOfTwo32 returns greatest power of two which is <= value.
#[inline]
pub const fn round_down_to_power_of_two_32(value: u32) -> u32 {
    if value > 0x80000000u32 {
        return 0x80000000u32;
    }
    let mut result = round_up_to_power_of_two_32(value);
    if result > value {
        result >>= 1;
    }
    result
}

/// Precondition: 0 <= shift < 32
#[inline]
pub const fn rotate_right_32(value: u32, shift: u32) -> u32 {
    (value >> shift) | (value << ((32u32.wrapping_sub(shift)) & 31))
}

/// Precondition: 0 <= shift < 32
#[inline]
pub const fn rotate_left_32(value: u32, shift: u32) -> u32 {
    (value << shift) | (value >> ((32u32.wrapping_sub(shift)) & 31))
}

/// Precondition: 0 <= shift < 64
#[inline]
pub const fn rotate_right_64(value: u64, shift: u64) -> u64 {
    let s = (shift & 63) as u32;
    (value >> s) | (value << ((64u32.wrapping_sub(s)) & 63))
}

/// Precondition: 0 <= shift < 64
#[inline]
pub const fn rotate_left_64(value: u64, shift: u64) -> u64 {
    let s = (shift & 63) as u32;
    (value << s) | (value >> ((64u32.wrapping_sub(s)) & 63))
}

/// Clear the LSB of a value using Brian Kernighan's method.
#[inline]
pub const fn clear_lsb_u32(value: u32) -> u32 {
    value & value.wrapping_sub(1)
}

#[inline]
pub const fn clear_lsb_u64(value: u64) -> u64 {
    value & value.wrapping_sub(1)
}

#[inline]
pub fn signed_add_overflow_32(lhs: i32, rhs: i32, val: &mut i32) -> bool {
    let (res, overflow) = lhs.overflowing_add(rhs);
    *val = res;
    overflow
}

#[inline]
pub fn signed_sub_overflow_32(lhs: i32, rhs: i32, val: &mut i32) -> bool {
    let (res, overflow) = lhs.overflowing_sub(rhs);
    *val = res;
    overflow
}

#[inline]
pub fn signed_mul_overflow_32(lhs: i32, rhs: i32, val: &mut i32) -> bool {
    let (res, overflow) = lhs.overflowing_mul(rhs);
    *val = res;
    overflow
}

#[inline]
pub fn signed_add_overflow_64(lhs: i64, rhs: i64, val: &mut i64) -> bool {
    let (res, overflow) = lhs.overflowing_add(rhs);
    *val = res;
    overflow
}

#[inline]
pub fn signed_sub_overflow_64(lhs: i64, rhs: i64, val: &mut i64) -> bool {
    let (res, overflow) = lhs.overflowing_sub(rhs);
    *val = res;
    overflow
}

#[inline]
pub fn signed_mul_overflow_64(lhs: i64, rhs: i64, val: &mut i64) -> bool {
    let (res, overflow) = lhs.overflowing_mul(rhs);
    *val = res;
    overflow
}

#[inline]
pub const fn signed_mul_high_32(lhs: i32, rhs: i32) -> i32 {
    let value = (lhs as i64) * (rhs as i64);
    ((value as u64) >> 32) as i32
}

#[inline]
pub const fn unsigned_mul_high_32(lhs: u32, rhs: u32) -> u32 {
    let value = (lhs as u64) * (rhs as u64);
    (value >> 32) as u32
}

#[inline]
pub const fn signed_mul_high_64(lhs: i64, rhs: i64) -> i64 {
    let value = (lhs as i128) * (rhs as i128);
    ((value as u128) >> 64) as i64
}

#[inline]
pub const fn unsigned_mul_high_64(lhs: u64, rhs: u64) -> u64 {
    let value = (lhs as u128) * (rhs as u128);
    (value >> 64) as u64
}

#[inline]
pub const fn signed_mul_high_and_add_32(lhs: i32, rhs: i32, acc: i32) -> i32 {
    acc.wrapping_add(signed_mul_high_32(lhs, rhs))
}

#[inline]
pub const fn signed_div_32(lhs: i32, rhs: i32) -> i32 {
    if rhs == 0 {
        0
    } else if rhs == -1 {
        if lhs == i32::MIN {
            lhs
        } else {
            -lhs
        }
    } else {
        lhs / rhs
    }
}

#[inline]
pub const fn signed_div_64(lhs: i64, rhs: i64) -> i64 {
    if rhs == 0 {
        0
    } else if rhs == -1 {
        if lhs == i64::MIN {
            lhs
        } else {
            -lhs
        }
    } else {
        lhs / rhs
    }
}

#[inline]
pub const fn signed_mod_32(lhs: i32, rhs: i32) -> i32 {
    if rhs == 0 || rhs == -1 {
        0
    } else {
        lhs % rhs
    }
}

#[inline]
pub const fn signed_mod_64(lhs: i64, rhs: i64) -> i64 {
    if rhs == 0 || rhs == -1 {
        0
    } else {
        lhs % rhs
    }
}

#[inline]
pub fn unsigned_add_overflow_32(lhs: u32, rhs: u32, val: &mut u32) -> bool {
    let (res, overflow) = lhs.overflowing_add(rhs);
    *val = res;
    overflow
}

#[inline]
pub const fn unsigned_div_32(lhs: u32, rhs: u32) -> u32 {
    if rhs == 0 {
        0
    } else {
        lhs / rhs
    }
}

#[inline]
pub const fn unsigned_div_64(lhs: u64, rhs: u64) -> u64 {
    if rhs == 0 {
        0
    } else {
        lhs / rhs
    }
}

#[inline]
pub const fn unsigned_mod_32(lhs: u32, rhs: u32) -> u32 {
    if rhs == 0 {
        0
    } else {
        lhs % rhs
    }
}

#[inline]
pub const fn unsigned_mod_64(lhs: u64, rhs: u64) -> u64 {
    if rhs == 0 {
        0
    } else {
        lhs % rhs
    }
}

#[inline]
pub const fn wraparound_add_32(lhs: i32, rhs: i32) -> i32 {
    lhs.wrapping_add(rhs)
}

#[inline]
pub const fn wraparound_neg_32(x: i32) -> i32 {
    x.wrapping_neg()
}

#[inline]
pub const fn byte_reverse_16(value: u16) -> u16 {
    value.swap_bytes()
}

#[inline]
pub const fn byte_reverse_32(value: u32) -> u32 {
    value.swap_bytes()
}

#[inline]
pub const fn byte_reverse_64(value: u64) -> u64 {
    value.swap_bytes()
}

#[inline]
pub const fn signed_saturated_add_64(lhs: i64, rhs: i64) -> i64 {
    lhs.saturating_add(rhs)
}

#[inline]
pub const fn signed_saturated_sub_64(lhs: i64, rhs: i64) -> i64 {
    lhs.saturating_sub(rhs)
}

#[inline]
pub const fn bit_width_u32(x: u32) -> i32 {
    (32 - x.leading_zeros()) as i32
}

#[inline]
pub const fn bit_width_u64(x: u64) -> i32 {
    (64 - x.leading_zeros()) as i32
}
