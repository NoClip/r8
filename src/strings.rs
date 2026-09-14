//! Safe Rust reimplementation of Google V8's `src/base/strings.h`.
//!
//! Provides string utilities, hex conversion routines, bounded string copies,
//! and safe null-terminating formatted print helpers matching V8 semantics.

pub type Uc16 = u16;
pub type Uc32 = u32;
pub const K_UC16_SIZE: usize = std::mem::size_of::<Uc16>();

/// Returns the value (0..15) of a hexadecimal character `c`.
/// If `c` is not a legal hexadecimal character, returns -1.
#[inline]
pub fn hex_value(c: Uc32) -> i32 {
    let c0 = c.wrapping_sub('0' as u32);
    if c0 <= 9 {
        return c0 as i32;
    }
    let ca = (c | 0x20).wrapping_sub('a' as u32);
    if ca <= 5 {
        return (ca + 10) as i32;
    }
    -1
}

/// Returns the uppercase hexadecimal ASCII character for `value` (0..15).
#[inline]
pub fn hex_char_of_value(value: i32) -> u8 {
    debug_assert!((0..=16).contains(&value));
    if value < 10 {
        b'0' + (value as u8)
    } else {
        b'A' + ((value - 10) as u8)
    }
}

/// Copies up to `n` characters from `src` to `dest`.
/// Zero-pads `dest` if `src` is shorter than `n`.
pub fn str_n_cpy(dest: &mut [u8], src: &[u8], n: usize) {
    let limit = dest.len().min(n);
    let mut i = 0;
    while i < limit && i < src.len() && src[i] != 0 {
        dest[i] = src[i];
        i += 1;
    }
    while i < limit {
        dest[i] = 0;
        i += 1;
    }
}

/// Safe string formatting helper.
/// Ensures that `dest` is always null-terminated.
/// Returns the number of characters written, or -1 if output was truncated.
pub fn safe_snprintf(dest: &mut [u8], formatted_str: &str) -> i32 {
    if dest.is_empty() {
        return -1;
    }
    let bytes = formatted_str.as_bytes();
    if bytes.len() >= dest.len() {
        let copy_len = dest.len() - 1;
        dest[..copy_len].copy_from_slice(&bytes[..copy_len]);
        dest[copy_len] = 0;
        -1
    } else {
        dest[..bytes.len()].copy_from_slice(bytes);
        dest[bytes.len()] = 0;
        bytes.len() as i32
    }
}
