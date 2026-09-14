//! Safe Rust reimplementation of Google V8's `src/base/bit-field.h`.
//!
//! Provides compile-time bitfield encoding/decoding templates and array-backed
//! `BitSetComputer` utilities for compact bit-packing with zero overhead.

use std::marker::PhantomData;

/// Helper struct for compile-time bitfield encoding/decoding into integer container `U`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct BitField<T, const SHIFT: usize, const SIZE: usize, U = u32> {
    _phantom: PhantomData<(T, U)>,
}

impl<T: Copy + Into<u64> + TryFrom<u64>, const SHIFT: usize, const SIZE: usize> BitField<T, SHIFT, SIZE, u32> {
    pub const SHIFT: usize = SHIFT;
    pub const SIZE: usize = SIZE;

    pub const MASK: u32 = {
        if SIZE >= 32 {
            !0u32
        } else {
            let ones = (1u64 << SIZE) - 1;
            (ones << SHIFT) as u32
        }
    };

    pub const NUM_VALUES: u64 = if SIZE >= 64 { 0 } else { 1u64 << SIZE };
    pub const MAX: u64 = if SIZE >= 64 { !0u64 } else { (1u64 << SIZE) - 1 };

    /// Tells whether the provided value fits into the bitfield without truncation.
    #[inline]
    pub fn is_valid(value: T) -> bool {
        let val_u64: u64 = value.into();
        (val_u64 & !Self::MAX) == 0
    }

    /// Encodes `value` shifted into position.
    #[inline]
    pub fn encode(value: T) -> u32 {
        debug_assert!(Self::is_valid(value), "BitField: value out of bounds");
        let val_u64: u64 = value.into();
        ((val_u64 & Self::MAX) as u32) << SHIFT
    }

    /// Updates `previous` bit container with `value` placed in this field.
    #[inline]
    pub fn update(previous: u32, value: T) -> u32 {
        (previous & !Self::MASK) | Self::encode(value)
    }

    /// Decodes field value from container `value`.
    #[inline]
    pub fn decode(value: u32) -> T {
        let extracted = ((value & Self::MASK) >> SHIFT) as u64;
        match T::try_from(extracted) {
            Ok(v) => v,
            Err(_) => panic!("BitField::decode conversion failed"),
        }
    }
}

/// 64-bit BitField specialization.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct BitField64<T, const SHIFT: usize, const SIZE: usize> {
    _phantom: PhantomData<T>,
}

impl<T: Copy + Into<u64> + TryFrom<u64>, const SHIFT: usize, const SIZE: usize> BitField64<T, SHIFT, SIZE> {
    pub const SHIFT: usize = SHIFT;
    pub const SIZE: usize = SIZE;

    pub const MASK: u64 = {
        if SIZE >= 64 {
            !0u64
        } else {
            let ones = (1u128 << SIZE) - 1;
            (ones << SHIFT) as u64
        }
    };

    pub const MAX: u64 = if SIZE >= 64 { !0u64 } else { (1u64 << SIZE) - 1 };

    #[inline]
    pub fn is_valid(value: T) -> bool {
        let val: u64 = value.into();
        (val & !Self::MAX) == 0
    }

    #[inline]
    pub fn encode(value: T) -> u64 {
        debug_assert!(Self::is_valid(value), "BitField64: value out of bounds");
        let val: u64 = value.into();
        (val & Self::MAX) << SHIFT
    }

    #[inline]
    pub fn update(previous: u64, value: T) -> u64 {
        (previous & !Self::MASK) | Self::encode(value)
    }

    #[inline]
    pub fn decode(value: u64) -> T {
        let extracted = (value & Self::MASK) >> SHIFT;
        match T::try_from(extracted) {
            Ok(v) => v,
            Err(_) => panic!("BitField64::decode conversion failed"),
        }
    }
}

/// Array-backed computer for encoding/decoding variable number of bit items in integers.
pub struct BitSetComputer<T, const BITS_PER_ITEM: usize, const BITS_PER_WORD: usize, U = u32> {
    _phantom: PhantomData<(T, U)>,
}

impl<T: Copy + Into<u64> + TryFrom<u64>, const BITS_PER_ITEM: usize, const BITS_PER_WORD: usize>
    BitSetComputer<T, BITS_PER_ITEM, BITS_PER_WORD, u32>
{
    pub const ITEMS_PER_WORD: usize = BITS_PER_WORD / BITS_PER_ITEM;
    pub const MASK: u32 = ((1u64 << BITS_PER_ITEM) - 1) as u32;

    #[inline]
    pub const fn word_count(items: usize) -> usize {
        if items == 0 {
            0
        } else {
            (items - 1) / Self::ITEMS_PER_WORD + 1
        }
    }

    #[inline]
    pub const fn index(base_index: usize, item: usize) -> usize {
        base_index + item / Self::ITEMS_PER_WORD
    }

    #[inline]
    pub const fn shift(item: usize) -> usize {
        (item % Self::ITEMS_PER_WORD) * BITS_PER_ITEM
    }

    #[inline]
    pub fn decode(data: u32, item: usize) -> T {
        let extracted = ((data >> Self::shift(item)) & Self::MASK) as u64;
        match T::try_from(extracted) {
            Ok(v) => v,
            Err(_) => panic!("BitSetComputer::decode conversion failed"),
        }
    }

    #[inline]
    pub fn encode(previous: u32, item: usize, value: T) -> u32 {
        let shift_val = Self::shift(item);
        let val_u64: u64 = value.into();
        let set_bits = ((val_u64 as u32) & Self::MASK) << shift_val;
        (previous & !(Self::MASK << shift_val)) | set_bits
    }
}
