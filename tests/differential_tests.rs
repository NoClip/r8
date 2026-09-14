//! Comprehensive differential test suite porting all tests from V8's
//! `test/unittests/base/bits-unittest.cc` and adding differential validation
//! against V8 C++ reference algorithms.

use v8_base_bits::*;

// Reference algorithms from Hacker's Delight / V8 C++ bits.h fallback implementations

fn reference_popcount_u32(mut value: u32) -> u32 {
    let mask = [0x55555555u32, 0x33333333u32, 0x0f0f0f0fu32];
    value = ((value >> 1) & mask[0]) + (value & mask[0]);
    value = ((value >> 2) & mask[1]) + (value & mask[1]);
    value = ((value >> 4) & mask[2]) + (value & mask[2]);
    value = (value >> 8) + value;
    value = (value >> 16) + value;
    value & 0xff
}

fn reference_clz_u32(value: u32) -> u32 {
    if value == 0 {
        return 32;
    }
    let mut n = 0u32;
    let mut x = value;
    if x <= 0x0000FFFF { n += 16; x <<= 16; }
    if x <= 0x00FFFFFF { n += 8; x <<= 8; }
    if x <= 0x0FFFFFFF { n += 4; x <<= 4; }
    if x <= 0x3FFFFFFF { n += 2; x <<= 2; }
    if x <= 0x7FFFFFFF { n += 1; }
    n
}

fn reference_clp2_u32(mut x: u32) -> u32 {
    if x != 0 {
        x -= 1;
    }
    x |= x >> 1;
    x |= x >> 2;
    x |= x >> 4;
    x |= x >> 8;
    x |= x >> 16;
    x + 1
}

#[test]
fn test_count_population() {
    // 8-bit
    assert_eq!(0, count_population_u8(0));
    assert_eq!(1, count_population_u8(1));
    assert_eq!(2, count_population_u8(0x11));
    assert_eq!(4, count_population_u8(0x0F));
    assert_eq!(6, count_population_u8(0x3F));
    assert_eq!(8, count_population_u8(0xFF));

    // 16-bit
    assert_eq!(0, count_population_u16(0));
    assert_eq!(1, count_population_u16(1));
    assert_eq!(4, count_population_u16(0x1111));
    assert_eq!(8, count_population_u16(0xF0F0));
    assert_eq!(12, count_population_u16(0xF0FF));
    assert_eq!(16, count_population_u16(0xFFFF));

    // 32-bit
    assert_eq!(0, count_population_u32(0));
    assert_eq!(1, count_population_u32(1));
    assert_eq!(8, count_population_u32(0x11111111));
    assert_eq!(16, count_population_u32(0xF0F0F0F0));
    assert_eq!(24, count_population_u32(0xFFF0F0FF));
    assert_eq!(32, count_population_u32(0xFFFFFFFF));

    // 64-bit
    assert_eq!(0, count_population_u64(0));
    assert_eq!(1, count_population_u64(1));
    assert_eq!(2, count_population_u64(0x8000000000000001));
    assert_eq!(8, count_population_u64(0x11111111));
    assert_eq!(16, count_population_u64(0xF0F0F0F0));
    assert_eq!(24, count_population_u64(0xFFF0F0FF));
    assert_eq!(32, count_population_u64(0xFFFFFFFF));
    assert_eq!(16, count_population_u64(0x1111111111111111));
    assert_eq!(32, count_population_u64(0xF0F0F0F0F0F0F0F0));
    assert_eq!(48, count_population_u64(0xFFF0F0FFFFF0F0FF));
    assert_eq!(64, count_population_u64(0xFFFFFFFFFFFFFFFF));
}

#[test]
fn test_count_leading_zeros() {
    assert_eq!(16, count_leading_zeros_u16(0));
    assert_eq!(15, count_leading_zeros_u16(1));
    for shift in 0..16 {
        assert_eq!(15 - shift, count_leading_zeros_u16(1u16 << shift));
    }
    assert_eq!(4, count_leading_zeros_u16(0x0F0F));

    assert_eq!(32, count_leading_zeros_u32(0));
    assert_eq!(31, count_leading_zeros_u32(1));
    for shift in 0..32 {
        assert_eq!(31 - shift, count_leading_zeros_u32(1u32 << shift));
    }
    assert_eq!(4, count_leading_zeros_u32(0x0F0F0F0F));

    assert_eq!(64, count_leading_zeros_u64(0));
    assert_eq!(63, count_leading_zeros_u64(1));
    for shift in 0..64 {
        assert_eq!(63 - shift, count_leading_zeros_u64(1u64 << shift));
    }
    assert_eq!(36, count_leading_zeros_u64(0x0F0F0F0F));
    assert_eq!(4, count_leading_zeros_u64(0x0F0F0F0F00000000));
}

#[test]
fn test_count_trailing_zeros() {
    assert_eq!(16, count_trailing_zeros_u16(0));
    assert_eq!(15, count_trailing_zeros_u16(0x8000));
    for shift in 0..16 {
        assert_eq!(shift, count_trailing_zeros_u16(1u16 << shift));
    }
    assert_eq!(4, count_trailing_zeros_u16(0xF0F0));

    assert_eq!(32, count_trailing_zeros_u32(0));
    assert_eq!(31, count_trailing_zeros_u32(0x80000000));
    for shift in 0..32 {
        assert_eq!(shift, count_trailing_zeros_u32(1u32 << shift));
    }
    assert_eq!(4, count_trailing_zeros_u32(0xF0F0F0F0));

    assert_eq!(32, count_trailing_zeros_i32(0));
    for shift in 0..31 {
        assert_eq!(shift, count_trailing_zeros_i32(1i32 << shift));
    }
    assert_eq!(4, count_trailing_zeros_i32(0x70F0F0F0));
    assert_eq!(2, count_trailing_zeros_i32(-4));
    assert_eq!(0, count_trailing_zeros_i32(-1));

    assert_eq!(64, count_trailing_zeros_u64(0));
    assert_eq!(63, count_trailing_zeros_u64(0x8000000000000000));
    for shift in 0..64 {
        assert_eq!(shift, count_trailing_zeros_u64(1u64 << shift));
    }
    assert_eq!(4, count_trailing_zeros_u64(0xF0F0F0F0));
    assert_eq!(36, count_trailing_zeros_u64(0xF0F0F0F000000000));
}

#[test]
fn test_power_of_two() {
    assert!(!is_power_of_two_u32(0));
    for shift in 0..32 {
        assert!(is_power_of_two_u32(1u32 << shift));
        assert!(!is_power_of_two_u32((1u32 << shift).wrapping_add(5)));
        assert!(!is_power_of_two_u32(!(1u32 << shift)));
    }
    for shift in 2..32 {
        assert!(!is_power_of_two_u32((1u32 << shift) - 1));
    }
    assert!(!is_power_of_two_u32(0xFFFFFFFF));

    assert!(!is_power_of_two_u64(0));
    for shift in 0..64 {
        assert!(is_power_of_two_u64(1u64 << shift));
        assert!(!is_power_of_two_u64((1u64 << shift).wrapping_add(5)));
        assert!(!is_power_of_two_u64(!(1u64 << shift)));
    }
    for shift in 2..64 {
        assert!(!is_power_of_two_u64((1u64 << shift) - 1));
    }
    assert!(!is_power_of_two_u64(0xFFFFFFFFFFFFFFFF));
}

#[test]
fn test_which_power_of_two() {
    for shift in 0..31 {
        assert_eq!(shift as i32, which_power_of_two_u32(1u32 << shift));
    }
    for shift in 0..63 {
        assert_eq!(shift as i32, which_power_of_two_u64(1u64 << shift));
    }
}

#[test]
fn test_round_up_to_power_of_two() {
    for shift in 0..31 {
        assert_eq!(1u32 << shift, round_up_to_power_of_two_32(1u32 << shift));
    }
    assert_eq!(1, round_up_to_power_of_two_32(0));
    assert_eq!(1, round_up_to_power_of_two_32(1));
    assert_eq!(4, round_up_to_power_of_two_32(3));
    assert_eq!(0x80000000, round_up_to_power_of_two_32(0x7FFFFFFF));

    for shift in 0..63 {
        let value = 1u64 << shift;
        assert_eq!(value, round_up_to_power_of_two_64(value));
    }
    assert_eq!(1, round_up_to_power_of_two_64(0));
    assert_eq!(1, round_up_to_power_of_two_64(1));
    assert_eq!(4, round_up_to_power_of_two_64(3));
    assert_eq!(1u64 << 63, round_up_to_power_of_two_64((1u64 << 63) - 1));
    assert_eq!(1u64 << 63, round_up_to_power_of_two_64(1u64 << 63));
}

#[test]
fn test_round_down_to_power_of_two() {
    for shift in 0..31 {
        assert_eq!(1u32 << shift, round_down_to_power_of_two_32(1u32 << shift));
    }
    assert_eq!(0, round_down_to_power_of_two_32(0));
    assert_eq!(4, round_down_to_power_of_two_32(5));
    assert_eq!(0x80000000, round_down_to_power_of_two_32(0x80000001));
}

#[test]
fn test_rotates() {
    for shift in 0..32 {
        assert_eq!(0, rotate_right_32(0, shift));
    }
    assert_eq!(1, rotate_right_32(1, 0));
    assert_eq!(1, rotate_right_32(2, 1));
    assert_eq!(0x80000000, rotate_right_32(1, 1));

    for shift in 0..64 {
        assert_eq!(0, rotate_right_64(0, shift));
    }
    assert_eq!(1, rotate_right_64(1, 0));
    assert_eq!(1, rotate_right_64(2, 1));
    assert_eq!(0x8000000000000000, rotate_right_64(1, 1));
}

#[test]
fn test_signed_overflow_32() {
    let mut val = 0i32;
    assert!(!signed_add_overflow_32(0, 0, &mut val));
    assert_eq!(0, val);
    assert!(signed_add_overflow_32(i32::MAX, 1, &mut val));
    assert_eq!(i32::MIN, val);
    assert!(signed_add_overflow_32(i32::MIN, -1, &mut val));
    assert_eq!(i32::MAX, val);
    assert!(signed_add_overflow_32(i32::MAX, i32::MAX, &mut val));
    assert_eq!(-2, val);

    assert!(!signed_sub_overflow_32(0, 0, &mut val));
    assert_eq!(0, val);
    assert!(signed_sub_overflow_32(i32::MIN, 1, &mut val));
    assert_eq!(i32::MAX, val);
    assert!(signed_sub_overflow_32(i32::MAX, -1, &mut val));
    assert_eq!(i32::MIN, val);
}

#[test]
fn test_mul_high_32() {
    assert_eq!(0, signed_mul_high_32(0, 0));
    assert_eq!(-1073741824, signed_mul_high_32(i32::MAX, i32::MIN));
    assert_eq!(-1073741824, signed_mul_high_32(i32::MIN, i32::MAX));
    assert_eq!(1, signed_mul_high_32(1024 * 1024 * 1024, 4));
    assert_eq!(2, signed_mul_high_32(8 * 1024, 1024 * 1024));

    for i in 1..50 {
        assert_eq!(i, signed_mul_high_and_add_32(0, 0, i));
        for j in 1..=i {
            assert_eq!(i, signed_mul_high_and_add_32(j, j, i));
        }
        assert_eq!(i + 1, signed_mul_high_and_add_32(1024 * 1024 * 1024, 4, i));
    }
}

#[test]
fn test_div_and_mod_32() {
    assert_eq!(i32::MIN, signed_div_32(i32::MIN, -1));
    assert_eq!(i32::MAX, signed_div_32(i32::MAX, 1));
    for i in 0..50 {
        assert_eq!(0, signed_div_32(i, 0));
        for j in 1..=i {
            assert_eq!(1, signed_div_32(j, j));
            assert_eq!(i / j, signed_div_32(i, j));
            assert_eq!(-i / j, signed_div_32(i, -j));
        }
    }

    assert_eq!(0, signed_mod_32(i32::MIN, -1));
    assert_eq!(0, signed_mod_32(i32::MAX, 1));
    for i in 0..50 {
        assert_eq!(0, signed_mod_32(i, 0));
        for j in 1..=i {
            assert_eq!(0, signed_mod_32(j, j));
            assert_eq!(i % j, signed_mod_32(i, j));
            assert_eq!(i % j, signed_mod_32(i, -j));
        }
    }
}

#[test]
fn test_unsigned_ops() {
    let mut val = 0u32;
    assert!(!unsigned_add_overflow_32(0, 0, &mut val));
    assert_eq!(0, val);
    assert!(unsigned_add_overflow_32(u32::MAX, 1, &mut val));
    assert_eq!(0, val);
    assert!(unsigned_add_overflow_32(u32::MAX, u32::MAX, &mut val));

    for i in 0..50 {
        assert_eq!(0, unsigned_div_32(i, 0));
        assert_eq!(0, unsigned_mod_32(i, 0));
        for j in (i + 1)..100 {
            assert_eq!(1, unsigned_div_32(j, j));
            assert_eq!(i / j, unsigned_div_32(i, j));
            assert_eq!(0, unsigned_mod_32(j, j));
            assert_eq!(i % j, unsigned_mod_32(i, j));
        }
    }
}

#[test]
fn test_differential_fuzz_against_v8_reference() {
    // Shared pseudorandom corpus covering uniform patterns, edge cases, and powers of two
    let mut corpus: Vec<u32> = vec![
        0, 1, 2, 3, 4, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 255, 256,
        0x7FFFFFFF, 0x80000000, 0xFFFFFFFF,
        0x55555555, 0xAAAAAAAA, 0x0F0F0F0F, 0xF0F0F0F0,
    ];

    let mut seed = 0x12345678u32;
    for _ in 0..10_000 {
        // xorshift32
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        corpus.push(seed);
    }

    for &x in &corpus {
        // Popcount differential
        assert_eq!(count_population_u32(x), reference_popcount_u32(x));
        // CLZ differential
        assert_eq!(count_leading_zeros_u32(x), reference_clz_u32(x));
        // RoundUpToPowerOfTwo differential (for x <= 0x80000000)
        if x <= 0x80000000 {
            assert_eq!(round_up_to_power_of_two_32(x), reference_clp2_u32(x));
        }
        // BitWidth differential
        let expected_bw = if x == 0 { 0 } else { 32 - reference_clz_u32(x) as i32 };
        assert_eq!(bit_width_u32(x), expected_bw);
    }
}

#[test]
fn test_vector_view_basics() {
    let data = [10u32, 20, 30, 40, 50];
    let view = VectorView::from_slice(&data);
    assert_eq!(5, view.size());
    assert!(!view.is_empty());
    assert_eq!(&10, view.first());
    assert_eq!(&50, view.last());
    assert_eq!(&30, view.at(2));
    assert_eq!(30, view[2]);

    let sub = view.sub_vector(1, 4);
    assert_eq!(3, sub.size());
    assert_eq!(&[20u32, 30, 40], sub.as_slice());

    let mut truncated = view;
    truncated.truncate(2);
    assert_eq!(2, truncated.size());
    assert_eq!(&[10u32, 20], truncated.as_slice());
}

#[test]
fn test_vector_view_equality() {
    let a = [1u8, 2, 3];
    let b = [1u8, 2, 3];
    let c = [1u8, 2, 4];
    let view_a = VectorView::from_slice(&a);
    let view_b = VectorView::from_slice(&b);
    let view_c = VectorView::from_slice(&c);

    assert_eq!(view_a, view_b);
    assert_ne!(view_a, view_c);
}

#[test]
fn test_owned_vector() {
    let owned = OwnedVector::<i32>::new_with_value(4, 42);
    assert_eq!(4, owned.size());
    assert_eq!(&[42, 42, 42, 42], &*owned);

    let view = owned.as_vector();
    assert_eq!(4, view.size());
    assert_eq!(&42, view.first());

    let copied = OwnedVector::new_by_copying(&[1, 2, 3]);
    assert_eq!(3, copied.size());
    assert_eq!(&[1, 2, 3], &*copied);
}

#[test]
fn test_vector_view_sub_vector_and_copy() {
    let src = [100u8, 101, 102, 103];
    let mut dst = [0u8; 4];
    dst.copy_from_slice(&src);
    assert_eq!(src, dst);

    let view = VectorView::new(src.as_ptr(), src.len());
    let sub = view.sub_vector(1, 3);
    assert_eq!(2, sub.size());
    assert_eq!(&[101u8, 102], sub.as_slice());

    let dst_view = VectorView::new(dst.as_ptr(), dst.len());
    assert_eq!(view, dst_view);
}

#[test]
fn test_small_vector_inline() {
    let mut sv: SmallVector<i32, 4> = SmallVector::new();
    assert_eq!(0, sv.len());
    assert!(sv.is_empty());
    assert_eq!(4, sv.capacity());
    assert!(!sv.is_big());

    sv.push(10);
    sv.push(20);
    sv.push(30);
    assert_eq!(3, sv.len());
    assert_eq!(10, sv[0]);
    assert_eq!(20, sv[1]);
    assert_eq!(30, sv[2]);
    assert!(!sv.is_big());

    assert_eq!(Some(30), sv.pop());
    assert_eq!(2, sv.len());
    assert_eq!(&[10, 20], sv.as_slice());
}

#[test]
fn test_small_vector_spill_to_heap() {
    let mut sv: SmallVector<i32, 4> = SmallVector::new();
    for i in 0..4 {
        sv.push(i * 10);
    }
    assert_eq!(4, sv.len());
    assert!(!sv.is_big());

    // 5th push must trigger spill to heap with V8 growth formula: RoundUpToPowerOfTwo(max(5, 2 * 4)) = 8
    sv.push(40);
    assert_eq!(5, sv.len());
    assert!(sv.is_big());
    assert_eq!(8, sv.capacity());

    // Verify values preserved across spill
    for i in 0..5 {
        assert_eq!(i as i32 * 10, sv[i]);
    }

    // Growth calculation check
    assert_eq!(8, calc_growth(4, 5));
    assert_eq!(16, calc_growth(8, 9));
    assert_eq!(64, calc_growth(8, 50));

    // Insert at index 2
    sv.insert(2, 999);
    assert_eq!(6, sv.len());
    assert_eq!(999, sv[2]);
    assert_eq!(20, sv[3]);

    // Remove at index 2
    let removed = sv.remove(2);
    assert_eq!(999, removed);
    assert_eq!(5, sv.len());
    assert_eq!(20, sv[2]);

    // Pop back n
    sv.pop_back_n(2);
    assert_eq!(3, sv.len());
    assert_eq!(&[0, 10, 20], sv.as_slice());

    // Resize
    sv.resize(6, 77);
    assert_eq!(6, sv.len());
    assert_eq!(77, sv[3]);
    assert_eq!(77, sv[4]);
    assert_eq!(77, sv[5]);

    // Clone
    let cloned = sv.clone();
    assert_eq!(sv.as_slice(), cloned.as_slice());

    // Clear
    sv.clear();
    assert_eq!(0, sv.len());
    assert!(sv.is_empty());
}

#[test]
fn test_strings_hex() {
    // Digits '0'..'9'
    for c in b'0'..=b'9' {
        let val = hex_value(c as u32);
        assert_eq!((c - b'0') as i32, val);
        assert_eq!(c, hex_char_of_value(val));
    }

    // Uppercase 'A'..'F'
    for c in b'A'..=b'F' {
        let val = hex_value(c as u32);
        assert_eq!((c - b'A' + 10) as i32, val);
        assert_eq!(c, hex_char_of_value(val));
    }

    // Lowercase 'a'..'f'
    for c in b'a'..=b'f' {
        let val = hex_value(c as u32);
        assert_eq!((c - b'a' + 10) as i32, val);
    }

    // Illegal characters
    assert_eq!(-1, hex_value(b'/' as u32));
    assert_eq!(-1, hex_value(b':' as u32));
    assert_eq!(-1, hex_value(b'@' as u32));
    assert_eq!(-1, hex_value(b'G' as u32));
    assert_eq!(-1, hex_value(b'g' as u32));
    assert_eq!(-1, hex_value(0x1234));
    assert_eq!(-1, hex_value(0xFFFFFFFF));
}

#[test]
fn test_strings_str_n_cpy() {
    let mut dest = [0xFFu8; 8];
    let src = b"hello\0world";

    str_n_cpy(&mut dest, src, 6);
    assert_eq!(&dest[..5], b"hello");
    assert_eq!(dest[5], 0);

    // Padding with zeroes
    let mut dest_padded = [0xAAu8; 10];
    str_n_cpy(&mut dest_padded, b"hi\0", 5);
    assert_eq!(&dest_padded[..2], b"hi");
    assert_eq!(&dest_padded[2..5], &[0, 0, 0]);
    assert_eq!(dest_padded[5], 0xAA);

    let mut dest_short = [0xFFu8; 6];
    str_n_cpy(&mut dest_short, b"v8\0", 4);
    assert_eq!(&dest_short[..2], b"v8");
    assert_eq!(&dest_short[2..4], &[0, 0]);
}

#[test]
fn test_strings_safe_snprintf() {
    let mut buf = [0u8; 8];

    // Fits completely: 5 characters + null terminator in 8-byte buffer
    let res = safe_snprintf(&mut buf, "hello");
    assert_eq!(5, res);
    assert_eq!(&buf[..5], b"hello");
    assert_eq!(0, buf[5]);

    // Truncates: 10 characters in 8-byte buffer -> writes 7 chars, null-terminator at index 7, returns -1
    let res = safe_snprintf(&mut buf, "0123456789");
    assert_eq!(-1, res);
    assert_eq!(&buf[..7], b"0123456");
    assert_eq!(0, buf[7]);

    let mut buf_trunc = [0u8; 6];
    let res = safe_snprintf(&mut buf_trunc, "abcdefgh");
    assert_eq!(-1, res);
    assert_eq!(&buf_trunc[..5], b"abcde");
    assert_eq!(0, buf_trunc[5]);
}

#[test]
fn test_hashmap_insert_lookup() {
    let mut map: V8HashMap<u32, u32> = V8HashMap::with_capacity(8);
    assert_eq!(0, map.occupancy());
    assert_eq!(8, map.capacity());

    map.insert_new(10, 100, 1000);
    map.insert_new(20, 200, 2000);

    let e1 = map.lookup(&10, 100).expect("Should find key 10");
    assert_eq!(10, e1.key);
    assert_eq!(1000, e1.value);
    assert_eq!(100, e1.hash());

    let e2 = map.lookup(&20, 200).expect("Should find key 20");
    assert_eq!(20, e2.key);
    assert_eq!(2000, e2.value);

    assert!(map.lookup(&30, 300).is_none());
}

#[test]
fn test_hashmap_resize_at_80_percent() {
    let mut map: V8HashMap<u32, u32> = V8HashMap::with_capacity(8);
    assert_eq!(8, map.capacity());

    // Insert 6 elements: occupancy = 6, 6 + 6/4 = 7 < 8 -> no resize
    for i in 1..=6 {
        map.insert_new(i, i * 11, i * 111);
    }
    assert_eq!(6, map.occupancy());
    assert_eq!(8, map.capacity());

    // 7th element: occupancy becomes 7, 7 + 7/4 = 8 >= 8 -> triggers resize to 16
    map.insert_new(7, 7 * 11, 7 * 111);
    assert_eq!(7, map.occupancy());
    assert_eq!(16, map.capacity());

    // All elements must still be present after rehashing
    for i in 1..=7 {
        let e = map.lookup(&i, i * 11).expect("Element must be present after resize");
        assert_eq!(i * 111, e.value);
    }
}

#[test]
fn test_hashmap_backward_shift_deletion() {
    let mut map: V8HashMap<u32, u32> = V8HashMap::with_capacity(8);

    // Insert elements that all collide on the same hash bucket: bucket 3 (hash = 3)
    map.insert_new(1, 3, 101); // lands at index 3
    map.insert_new(2, 3, 102); // lands at index 4
    map.insert_new(3, 3, 103); // lands at index 5

    assert_eq!(3, map.occupancy());

    // Remove the middle element in the collision chain (key 2)
    let removed = map.remove(&2, 3);
    assert_eq!(Some(102), removed);
    assert_eq!(2, map.occupancy());

    // Deletion algorithm must backward-shift key 3 into index 4 so probe chain is intact!
    assert!(map.lookup(&2, 3).is_none());
    let e3 = map.lookup(&3, 3).expect("Key 3 must still be found despite key 2 deletion");
    assert_eq!(103, e3.value);

    // Remove head of collision chain (key 1)
    let removed1 = map.remove(&1, 3);
    assert_eq!(Some(101), removed1);
    assert_eq!(1, map.occupancy());

    let e3_again = map.lookup(&3, 3).expect("Key 3 must still be found");
    assert_eq!(103, e3_again.value);
}

#[test]
fn test_hashmap_basic_operations() {
    let mut map: V8HashMap<usize, usize> = V8HashMap::with_capacity(8);
    assert_eq!(0, map.occupancy());
    assert_eq!(8, map.capacity());

    let entry = map.lookup_or_insert(0x1000, 42, 0x2000);
    assert_eq!(0x2000, entry.value);
    assert_eq!(1, map.occupancy());

    let looked_up = map.lookup(&0x1000, 42);
    assert!(looked_up.is_some());
    assert_eq!(0x2000, looked_up.unwrap().value);

    let removed = map.remove(&0x1000, 42);
    assert_eq!(Some(0x2000), removed);
    assert_eq!(0, map.occupancy());
}

#[test]
fn test_bitfield_basic() {
    type FieldA = BitField<u32, 0, 3>; // values 0..7
    type FieldB = BitField<u32, 3, 5>; // values 0..31
    type FieldC = BitField<u32, 8, 1>; // values 0..1 (bool)

    assert_eq!(0b111, FieldA::MASK);
    assert_eq!(0b11111000, FieldB::MASK);
    assert_eq!(0b100000000, FieldC::MASK);

    assert_eq!(7, FieldA::MAX);
    assert_eq!(31, FieldB::MAX);
    assert_eq!(1, FieldC::MAX);

    assert!(FieldA::is_valid(5));
    assert!(!FieldA::is_valid(8));

    // Pack into a single u32 container
    let mut container = 0u32;
    container = FieldA::update(container, 5);
    container = FieldB::update(container, 25);
    container = FieldC::update(container, 1);

    assert_eq!(5, FieldA::decode(container));
    assert_eq!(25, FieldB::decode(container));
    assert_eq!(1, FieldC::decode(container));

    // Update FieldB independently
    container = FieldB::update(container, 12);
    assert_eq!(5, FieldA::decode(container));
    assert_eq!(12, FieldB::decode(container));
    assert_eq!(1, FieldC::decode(container));
}

#[test]
fn test_bitfield64() {
    type LowField = BitField64<u64, 0, 32>;
    type HighField = BitField64<u64, 32, 32>;

    assert_eq!(0xFFFFFFFF, LowField::MASK);
    assert_eq!(0xFFFFFFFF00000000, HighField::MASK);

    let mut container = 0u64;
    container = LowField::update(container, 0x12345678);
    container = HighField::update(container, 0xABCDEF01);

    assert_eq!(0x12345678, LowField::decode(container));
    assert_eq!(0xABCDEF01, HighField::decode(container));
    assert_eq!(0xABCDEF0112345678, container);
}

#[test]
fn test_bitfield_operations() {
    type TestField = BitField<u32, 4, 4>;
    assert_eq!(0xF0, TestField::MASK);
    let encoded = TestField::encode(0xA);
    assert_eq!(0xA0, encoded);
    let decoded = TestField::decode(0x1A2);
    assert_eq!(0xA, decoded);
    let updated = TestField::update(0x102, 0xB);
    assert_eq!(0x1B2, updated);
}

#[test]
fn test_bitset_computer() {
    // 4-bit nibbles in 32-bit word -> 8 items per word
    type NibbleSet = BitSetComputer<u32, 4, 32>;

    assert_eq!(8, NibbleSet::ITEMS_PER_WORD);
    assert_eq!(1, NibbleSet::word_count(8));
    assert_eq!(2, NibbleSet::word_count(9));

    let mut word = 0u32;
    for i in 0..8 {
        word = NibbleSet::encode(word, i, (i as u32) + 1);
    }

    for i in 0..8 {
        assert_eq!((i as u32) + 1, NibbleSet::decode(word, i));
    }
}

static LAST_FATAL_MSG: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
static LAST_DCHECK_MSG: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn test_fatal_cb(_file: &str, line: i32, msg: &str) {
    *LAST_FATAL_MSG.lock().unwrap() = Some(format!("{}: {}", line, msg));
}

fn test_dcheck_cb(_file: &str, line: i32, msg: &str) {
    *LAST_DCHECK_MSG.lock().unwrap() = Some(format!("{}: {}", line, msg));
}

#[test]
fn test_logging_hooks() {
    set_fatal_handler(Some(test_fatal_cb));
    set_dcheck_handler(Some(test_dcheck_cb));

    fatal("test.rs", 42, "Simulated fatal error");
    assert_eq!(
        Some("42: Simulated fatal error".to_string()),
        LAST_FATAL_MSG.lock().unwrap().take()
    );

    dcheck("test.rs", 99, "Simulated dcheck error");
    assert_eq!(
        Some("99: Simulated dcheck error".to_string()),
        LAST_DCHECK_MSG.lock().unwrap().take()
    );

    // Reset handlers
    set_fatal_handler(None);
    set_dcheck_handler(None);
}


#[test]
fn test_platform_time() {
    let delta_s = TimeDelta::from_seconds(3);
    assert_eq!(3_000_000, delta_s.in_microseconds());
    assert_eq!(3_000, delta_s.in_milliseconds());
    assert_eq!(3, delta_s.in_seconds());

    let delta_m = TimeDelta::from_minutes(5);
    assert_eq!(300, delta_m.in_seconds());

    let delta_sum = delta_s + delta_m;
    assert_eq!(303, delta_sum.in_seconds());

    // Ticks
    let t1 = TimeTicks::now();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let t2 = TimeTicks::now();
    assert!(t2 >= t1);
    let diff = t2.since(t1);
    assert!(diff.in_microseconds() >= 500);
    assert!(TimeTicks::is_high_resolution());

    // Wall clock
    let wall = Time::now();
    assert!(wall.to_unix_timestamp_micros() > 1_700_000_000_000_000);
}

#[test]
fn test_platform_mutex_and_condvar() {
    use std::sync::Arc;
    use std::thread;

    let mut p_mutex = PlatformMutex::new();

    // Verify basic lock / try_lock / unlock
    assert!(p_mutex.try_lock());
    p_mutex.unlock();

    p_mutex.lock();
    p_mutex.unlock();

    // Recursive mutex
    let mut rec_mutex = PlatformRecursiveMutex::new();
    assert!(rec_mutex.try_lock());
    assert!(rec_mutex.try_lock());
    rec_mutex.lock();
    rec_mutex.unlock();
    rec_mutex.unlock();
    rec_mutex.unlock();

    // Condition variable timeout test
    let notified = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let notified_clone = notified.clone();

    let handle = thread::spawn(move || {
        let mut m = PlatformMutex::new();
        let mut c = PlatformConditionVariable::new();
        m.lock();
        let _ = c.wait_for_millis(&mut m, 5);
        m.unlock();
        notified_clone.store(true, std::sync::atomic::Ordering::Release);
    });

    handle.join().unwrap();
    assert!(notified.load(std::sync::atomic::Ordering::Acquire));
}

#[test]
fn test_parsing_token_properties() {
    // Keywords
    assert!(Token::If.is_keyword());
    assert!(Token::Function.is_keyword());
    assert!(Token::Async.is_keyword());
    assert!(Token::Await.is_keyword());
    assert!(Token::Class.is_keyword());
    assert!(!Token::Identifier.is_keyword());
    assert!(!Token::Add.is_keyword());

    // Operator categories
    assert!(Token::Add.is_binary_op());
    assert!(Token::Mul.is_binary_op());
    assert!(Token::EqStrict.is_binary_op());
    assert!(Token::AssignAdd.is_assignment_op());
    assert!(Token::AssignNullish.is_assignment_op());

    // Precedence hierarchy
    assert_eq!(2, Token::Assign.precedence(true));
    assert_eq!(3, Token::Nullish.precedence(true));
    assert_eq!(4, Token::Or.precedence(true));
    assert_eq!(5, Token::And.precedence(true));
    assert_eq!(9, Token::EqStrict.precedence(true));
    assert_eq!(12, Token::Add.precedence(true));
    assert_eq!(13, Token::Mul.precedence(true));
    assert_eq!(14, Token::Exp.precedence(true));

    // 'in' operator acceptance
    assert_eq!(10, Token::In.precedence(true));
    assert_eq!(0, Token::In.precedence(false));

    // Keyword lookup
    assert_eq!(Some(Token::Function), Token::lookup_keyword("function"));
    assert_eq!(Some(Token::Return), Token::lookup_keyword("return"));
    assert_eq!(Some(Token::Async), Token::lookup_keyword("async"));
    assert_eq!(None, Token::lookup_keyword("customIdentifier"));
}

#[test]
fn test_parsing_scanner_expression() {
    let source = "x + 42 * y;";
    let mut scanner = Scanner::new(source);

    let t1 = scanner.next_token();
    assert_eq!(Token::Identifier, t1.token);
    assert_eq!("x", t1.literal);

    let t2 = scanner.next_token();
    assert_eq!(Token::Add, t2.token);

    let t3 = scanner.next_token();
    assert_eq!(Token::Number, t3.token);
    assert_eq!("42", t3.literal);

    let t4 = scanner.next_token();
    assert_eq!(Token::Mul, t4.token);

    let t5 = scanner.next_token();
    assert_eq!(Token::Identifier, t5.token);
    assert_eq!("y", t5.literal);

    let t6 = scanner.next_token();
    assert_eq!(Token::Semicolon, t6.token);

    let t7 = scanner.next_token();
    assert_eq!(Token::Eos, t7.token);
}

#[test]
fn test_parsing_scanner_function_and_template() {
    let source = r#"
        // Greeting function
        function greet(name) {
            return `Hello, ${name}!`;
        }
    "#;
    let mut scanner = Scanner::new(source);

    let mut tokens = Vec::new();
    loop {
        let tok = scanner.next_token();
        let is_eos = tok.token == Token::Eos;
        tokens.push(tok);
        if is_eos {
            break;
        }
    }

    let token_types: Vec<Token> = tokens.iter().map(|t| t.token).collect();
    assert_eq!(
        vec![
            Token::Function,
            Token::Identifier,
            Token::LeftParen,
            Token::Identifier,
            Token::RightParen,
            Token::LeftBrace,
            Token::Return,
            Token::TemplateSpan,
            Token::Identifier,
            Token::TemplateTail,
            Token::Semicolon,
            Token::RightBrace,
            Token::Eos,
        ],
        token_types
    );

    assert_eq!("greet", tokens[1].literal);
    assert_eq!("name", tokens[3].literal);
    assert_eq!("Hello, ", tokens[7].literal);
    assert_eq!("name", tokens[8].literal);
    assert_eq!("!", tokens[9].literal);
}

#[test]
fn test_parsing_scanner_literals_and_operators() {
    let source = "0x1F 0b1010 0o755 12.34 9007199254740991n \"hello\\nworld\" 'foo\\'bar' === !== >>>= => ??=";
    let mut scanner = Scanner::new(source);

    let mut tokens = Vec::new();
    loop {
        let tok = scanner.next_token();
        let is_eos = tok.token == Token::Eos;
        tokens.push(tok);
        if is_eos {
            break;
        }
    }

    let types: Vec<Token> = tokens.iter().map(|t| t.token).collect();
    assert_eq!(
        vec![
            Token::Number,
            Token::Number,
            Token::Number,
            Token::Number,
            Token::BigInt,
            Token::String,
            Token::String,
            Token::EqStrict,
            Token::NotEqStrict,
            Token::AssignShr,
            Token::Arrow,
            Token::AssignNullish,
            Token::Eos,
        ],
        types
    );

    assert_eq!("hello\nworld", tokens[5].literal);
    assert_eq!("foo'bar", tokens[6].literal);
}

#[test]
fn test_interpreter_bytecode_properties() {
    // Check operand count semantics
    assert_eq!(0, Bytecode::LdaZero.number_of_operands());
    assert_eq!(0, Bytecode::Star0.number_of_operands());
    assert_eq!(0, Bytecode::Return.number_of_operands());
    assert_eq!(1, Bytecode::LdaSmi.number_of_operands());
    assert_eq!(1, Bytecode::Ldar.number_of_operands());
    assert_eq!(1, Bytecode::Star.number_of_operands());
    assert_eq!(1, Bytecode::Jump.number_of_operands());
    assert_eq!(2, Bytecode::Add.number_of_operands());
    assert_eq!(2, Bytecode::Mov.number_of_operands());
    assert_eq!(1, Bytecode::JumpIfTrue.number_of_operands());
    assert_eq!(6, Bytecode::CallRuntime.number_of_operands());

    // Check ShortStar identification
    for i in 0..16 {
        let star_bc = Bytecode::from_short_star_index(i).expect("Must map 0..15");
        assert!(star_bc.is_short_star());
        assert_eq!(Some(i), star_bc.short_star_index());
    }
    assert!(!Bytecode::Star.is_short_star());
    assert!(!Bytecode::Ldar.is_short_star());
    assert!(!Bytecode::Add.is_short_star());

    // Check jump predicates
    assert!(Bytecode::Jump.is_jump());
    assert!(Bytecode::JumpLoop.is_jump());
    assert!(Bytecode::JumpIfTrue.is_jump());
    assert!(Bytecode::JumpIfFalse.is_jump());
    assert!(!Bytecode::Return.is_jump());
    assert!(!Bytecode::Add.is_jump());

    assert!(!Bytecode::Jump.is_conditional_jump());
    assert!(!Bytecode::JumpLoop.is_conditional_jump());
    assert!(Bytecode::JumpIfTrue.is_conditional_jump());
    assert!(Bytecode::JumpIfFalse.is_conditional_jump());

    // Check opcode byte roundtrip and naming
    assert_eq!("Return", Bytecode::Return.name());
    assert_eq!("Star0", Bytecode::Star0.name());
    assert_eq!("LdaSmi", Bytecode::LdaSmi.name());
    assert_eq!(Bytecode::Return, Bytecode::from_byte(Bytecode::Return.to_byte()).unwrap());
    assert_eq!(Bytecode::Star0, Bytecode::from_byte(Bytecode::Star0.to_byte()).unwrap());
}

#[test]
fn test_interpreter_bytecode_metadata() {
    assert_eq!(0, Bytecode::Return.number_of_operands());
    assert_eq!(1, Bytecode::LdaSmi.number_of_operands());
    assert_eq!(2, Bytecode::Add.number_of_operands());

    assert!(Bytecode::Star0.is_short_star());
    assert!(Bytecode::Star15.is_short_star());
    assert!(!Bytecode::Star.is_short_star());
    assert_eq!(Some(0), Bytecode::Star0.short_star_index());
    assert_eq!(Some(15), Bytecode::Star15.short_star_index());
    assert_eq!(None, Bytecode::Star.short_star_index());

    assert!(Bytecode::Jump.is_jump());
    assert!(Bytecode::JumpIfFalse.is_jump());
    assert!(!Bytecode::Add.is_jump());

    assert_eq!("Return", Bytecode::Return.name());
}

#[test]
fn test_interpreter_register_math_and_operands() {
    // Local registers (r0, r1, ...)
    let r0 = Register::new(0);
    let r1 = Register::new(1);
    let r15 = Register::new(15);
    let r16 = Register::new(16);

    assert!(!r0.is_parameter());
    assert!(!r1.is_parameter());
    assert_eq!("r0", r0.to_string_name());
    assert_eq!("r1", r1.to_string_name());

    // Operand involution test: FromOperand(ToOperand(r)) == r
    assert_eq!(r0, Register::from_operand(r0.to_operand()));
    assert_eq!(r1, Register::from_operand(r1.to_operand()));
    assert_eq!(r15, Register::from_operand(r15.to_operand()));
    assert_eq!(r16, Register::from_operand(r16.to_operand()));

    // ShortStar mappings: r0..r15 map to Star0..Star15
    assert_eq!(Some(Bytecode::Star0), r0.try_to_short_star());
    assert_eq!(Some(Bytecode::Star1), r1.try_to_short_star());
    assert_eq!(Some(Bytecode::Star15), r15.try_to_short_star());
    assert_eq!(None, r16.try_to_short_star());

    assert_eq!(r0, Register::from_short_star(Bytecode::Star0));
    assert_eq!(r15, Register::from_short_star(Bytecode::Star15));

    // Parameter registers (a0/receiver, a1, a2, ...)
    let this_reg = Register::receiver();
    assert!(this_reg.is_parameter());
    assert!(this_reg.is_receiver());
    assert_eq!(0, this_reg.to_parameter_index());
    assert_eq!("<this>", this_reg.to_string_name());
    assert_eq!(this_reg, Register::from_operand(this_reg.to_operand()));

    let a1 = Register::from_parameter_index(1);
    assert!(a1.is_parameter());
    assert!(!a1.is_receiver());
    assert_eq!(1, a1.to_parameter_index());
    assert_eq!("a1", a1.to_string_name());
    assert_eq!(a1, Register::from_operand(a1.to_operand()));

    // Special fixed stack frame slots
    assert!(Register::function_closure().is_function_closure());
    assert!(Register::current_context().is_current_context());
    assert!(Register::bytecode_array().is_bytecode_array());
    assert!(Register::bytecode_offset().is_bytecode_offset());
    assert!(Register::feedback_vector().is_feedback_vector());

    // Register methods verification
    assert_eq!(r0, Register::new(0));
    assert_eq!(r0, Register::from_operand(r0.to_operand()));
    assert_eq!(this_reg, Register::receiver());
    assert!(this_reg.is_parameter());
    assert!(this_reg.is_receiver());
    assert_eq!(Some(Bytecode::Star0), r0.try_to_short_star());
    assert_eq!(r0, Register::from_short_star(Bytecode::Star0));
}

#[test]
fn test_interpreter_register_list() {
    let list = RegisterList::from_range(Register::new(2), 4);
    assert_eq!(4, list.register_count());
    assert_eq!(Register::new(2), list.first_register());
    assert_eq!(Register::new(5), list.last_register());
    assert_eq!(Register::new(2), list.get(0));
    assert_eq!(Register::new(3), list.get(1));
    assert_eq!(Register::new(4), list.get(2));
    assert_eq!(Register::new(5), list.get(3));

    let truncated = list.truncate(2);
    assert_eq!(2, truncated.register_count());
    assert_eq!(Register::new(2), truncated.first_register());
    assert_eq!(Register::new(3), truncated.last_register());

    let popped = list.pop_left();
    assert_eq!(3, popped.register_count());
    assert_eq!(Register::new(3), popped.first_register());
    assert_eq!(Register::new(5), popped.last_register());

    let collected: Vec<Register> = list.into_iter().collect();
    assert_eq!(
        vec![
            Register::new(2),
            Register::new(3),
            Register::new(4),
            Register::new(5)
        ],
        collected
    );
}

#[test]
fn test_interpreter_bytecode_array_builder_and_disassembly() {
    // Function: add42(a0) { let r0 = 42; return a0 + r0; }
    let mut builder = BytecodeArrayBuilder::new(1, 1);
    builder.load_smi(42);
    builder.store_accumulator_in_register(Register::new(0)); // Star0 via ShortStar!
    builder.load_accumulator_from_register(Register::receiver()); // Ldar a0
    builder.add(Register::new(0)); // Add r0
    builder.return_value(); // Return

    let array = builder.build();
    assert_eq!(1, array.parameter_count());
    assert_eq!(1, array.register_count());
    assert_eq!(8, array.frame_size());

    let bytes = array.bytecodes();
    // 0: LdaSmi [42]
    assert_eq!(Bytecode::LdaSmi.to_byte(), bytes[0]);
    assert_eq!(42, bytes[1]);
    // 2: Star0 (ShortStar optimization: 1 single byte instead of 2!)
    assert_eq!(Bytecode::Star0.to_byte(), bytes[2]);
    // 3: Ldar a0
    assert_eq!(Bytecode::Ldar.to_byte(), bytes[3]);
    assert_eq!(Register::receiver().to_operand() as i8 as u8, bytes[4]);
    // 5: Add r0, [0]
    assert_eq!(Bytecode::Add.to_byte(), bytes[5]);
    assert_eq!(Register::new(0).to_operand() as i8 as u8, bytes[6]);
    assert_eq!(0, bytes[7]); // feedback slot 0
    // 8: Return
    assert_eq!(Bytecode::Return.to_byte(), bytes[8]);

    let disasm = array.disassemble();
    assert!(disasm.contains("LdaSmi"));
    assert!(disasm.contains("[42]"));
    assert!(disasm.contains("Star0"));
    assert!(disasm.contains("<this>") || disasm.contains("a0"));
    assert!(disasm.contains("Add"));
    assert!(disasm.contains("Return"));
}

#[test]
fn test_interpreter_bytecode_loop_and_branch() {
    // Function with loop and conditional branch:
    // r0 = 0;
    // r1 = 10;
    // loop:
    //   if (r0 >= r1) jump forward
    //   r0++
    //   jump_loop backward
    // return r0
    let mut builder = BytecodeArrayBuilder::new(0, 2);
    builder.load_zero();
    builder.store_accumulator_in_register(Register::new(0)); // Star0
    builder.load_smi(10);
    builder.store_accumulator_in_register(Register::new(1)); // Star1

    builder.load_accumulator_from_register(Register::new(0));
    builder.test_less_than(Register::new(1));
    builder.jump_if_false(6); // jump forward over loop body

    builder.load_accumulator_from_register(Register::new(0));
    builder.inc();
    builder.store_accumulator_in_register(Register::new(0));
    builder.jump_loop(-8); // jump back to loop condition

    builder.load_accumulator_from_register(Register::new(0));
    builder.return_value();

    let array = builder.build();
    assert_eq!(0, array.parameter_count());
    assert_eq!(2, array.register_count());
    assert_eq!(16, array.frame_size());

    let disasm = array.disassemble();
    assert!(disasm.contains("Star0"));
    assert!(disasm.contains("Star1"));
    assert!(disasm.contains("TestLessThan"));
    assert!(disasm.contains("JumpIfFalse"));
    assert!(disasm.contains("JumpLoop"));
    assert!(disasm.contains("Return"));
}

#[test]
fn test_ast_nodes_and_visitor() {
    // Construct an AST program manually
    let program = Program::new(vec![
        Statement::VariableDeclaration {
            name: "x".to_string(),
            init: Some(Expression::Binary {
                op: BinaryOperator::Add,
                left: Box::new(Expression::Literal(LiteralValue::Smi(10))),
                right: Box::new(Expression::Literal(LiteralValue::Smi(20))),
            }),
            is_const: false,
        },
        Statement::FunctionDeclaration {
            name: "square".to_string(),
            params: vec!["n".to_string()],
            body: vec![Statement::Return(Some(Expression::Binary {
                op: BinaryOperator::Mul,
                left: Box::new(Expression::Variable("n".to_string())),
                right: Box::new(Expression::Variable("n".to_string())),
            }))],
            is_async: false,
            is_generator: false,
        },
    ]);

    assert_eq!(2, program.statements.len());

    // Test visitor
    struct AstCounter {
        statement_count: usize,
        expression_count: usize,
    }

    impl AstVisitor for AstCounter {
        fn visit_statement(&mut self, stmt: &Statement) {
            self.statement_count += 1;
            // Call default traversal
            match stmt {
                Statement::Block(stmts) => {
                    for s in stmts { self.visit_statement(s); }
                }
                Statement::Expression(e) => self.visit_expression(e),
                Statement::VariableDeclaration { init, .. } => {
                    if let Some(i) = init { self.visit_expression(i); }
                }
                Statement::UsingDeclaration { init, .. } => {
                    self.visit_expression(init);
                }
                Statement::If { condition, then_branch, else_branch } => {
                    self.visit_expression(condition);
                    self.visit_statement(then_branch);
                    if let Some(e) = else_branch { self.visit_statement(e); }
                }
                Statement::While { condition, body } => {
                    self.visit_expression(condition);
                    self.visit_statement(body);
                }
                Statement::Return(opt) => {
                    if let Some(e) = opt { self.visit_expression(e); }
                }
                Statement::FunctionDeclaration { body, .. } => {
                    for s in body { self.visit_statement(s); }
                }
                Statement::For { init, condition, update, body } => {
                    if let Some(i) = init { self.visit_statement(i); }
                    if let Some(c) = condition { self.visit_expression(c); }
                    if let Some(u) = update { self.visit_expression(u); }
                    self.visit_statement(body);
                }
                Statement::ForIn { object, body, .. } => {
                    self.visit_expression(object);
                    self.visit_statement(body);
                }
                Statement::ForOf { iterable, body, .. } => {
                    self.visit_expression(iterable);
                    self.visit_statement(body);
                }
                Statement::DoWhile { body, condition } => {
                    self.visit_statement(body);
                    self.visit_expression(condition);
                }
                Statement::Switch { discriminant, cases } => {
                    self.visit_expression(discriminant);
                    for c in cases {
                        if let Some(t) = &c.test { self.visit_expression(t); }
                        for s in &c.statements { self.visit_statement(s); }
                    }
                }
                Statement::Break(_) | Statement::Continue(_) => {}
                Statement::TryCatch { try_block, catch_block, finally_block, .. } => {
                    self.visit_statement(try_block);
                    if let Some(c) = catch_block { self.visit_statement(c); }
                    if let Some(f) = finally_block { self.visit_statement(f); }
                }
                Statement::Throw(e) => {
                    self.visit_expression(e);
                }
                Statement::Empty | Statement::Debugger => {}
                Statement::With { object, body } => {
                    self.visit_expression(object);
                    self.visit_statement(body);
                }
                Statement::Labeled { body, .. } => {
                    self.visit_statement(body);
                }
                Statement::ClassDeclaration { methods, constructor, .. } => {
                    if let Some(c) = constructor {
                        for s in &c.body { self.visit_statement(s); }
                    }
                    for m in methods {
                        for s in &m.body { self.visit_statement(s); }
                    }
                }
                Statement::ImportDeclaration { .. } => {}
                Statement::ExportDeclaration { declaration, .. } => {
                    if let Some(d) = declaration { self.visit_statement(d); }
                }
            }
        }

        fn visit_expression(&mut self, expr: &Expression) {
            self.expression_count += 1;
            match expr {
                Expression::Binary { left, right, .. } => {
                    self.visit_expression(left);
                    self.visit_expression(right);
                }
                Expression::Unary { expr, .. } => {
                    self.visit_expression(expr);
                }
                Expression::Assignment { value, .. } => {
                    self.visit_expression(value);
                }
                Expression::Call { callee, arguments } => {
                    self.visit_expression(callee);
                    for a in arguments { self.visit_expression(a); }
                }
                Expression::PropertyAccess { object, .. } => {
                    self.visit_expression(object);
                }
                _ => {}
            }
        }
    }

    let mut counter = AstCounter {
        statement_count: 0,
        expression_count: 0,
    };
    counter.visit_program(&program);

    // 2 top-level stmts (VarDecl + FuncDecl) + 1 nested Return = 3 statements
    assert_eq!(3, counter.statement_count);
    // 1 Add + 2 Smi + 1 Mul + 2 Var("n") = 6 expressions
    assert_eq!(6, counter.expression_count);
}

#[test]
fn test_parser_expression_precedence() {
    // 2 + 3 * 4 should parse as 2 + (3 * 4)
    let source = "2 + 3 * 4;";
    let mut parser = Parser::new(source);
    let stmt = parser.parse_statement().unwrap();

    match stmt {
        Statement::Expression(Expression::Binary { op, left, right }) => {
            assert_eq!(BinaryOperator::Add, op);
            assert_eq!(Expression::Literal(LiteralValue::Smi(2)), *left);
            match *right {
                Expression::Binary { op: mul_op, left: mul_left, right: mul_right } => {
                    assert_eq!(BinaryOperator::Mul, mul_op);
                    assert_eq!(Expression::Literal(LiteralValue::Smi(3)), *mul_left);
                    assert_eq!(Expression::Literal(LiteralValue::Smi(4)), *mul_right);
                }
                other => panic!("Expected Mul binary expression, got {:?}", other),
            }
        }
        other => panic!("Expected Expression statement, got {:?}", other),
    }

    // Parentheses override: (2 + 3) * 4 should parse as ((2 + 3) * 4)
    let paren_source = "(2 + 3) * 4;";
    let mut paren_parser = Parser::new(paren_source);
    let paren_stmt = paren_parser.parse_statement().unwrap();

    match paren_stmt {
        Statement::Expression(Expression::Binary { op, left, right }) => {
            assert_eq!(BinaryOperator::Mul, op);
            assert_eq!(Expression::Literal(LiteralValue::Smi(4)), *right);
            match *left {
                Expression::Binary { op: add_op, left: add_left, right: add_right } => {
                    assert_eq!(BinaryOperator::Add, add_op);
                    assert_eq!(Expression::Literal(LiteralValue::Smi(2)), *add_left);
                    assert_eq!(Expression::Literal(LiteralValue::Smi(3)), *add_right);
                }
                other => panic!("Expected Add binary expression, got {:?}", other),
            }
        }
        other => panic!("Expected Expression statement, got {:?}", other),
    }

    // Left associativity: a - b - c parses as (a - b) - c
    let assoc_source = "a - b - c;";
    let mut assoc_parser = Parser::new(assoc_source);
    let assoc_stmt = assoc_parser.parse_statement().unwrap();

    match assoc_stmt {
        Statement::Expression(Expression::Binary { op, left, right }) => {
            assert_eq!(BinaryOperator::Sub, op);
            assert_eq!(Expression::Variable("c".to_string()), *right);
            match *left {
                Expression::Binary { op: sub1, left: a, right: b } => {
                    assert_eq!(BinaryOperator::Sub, sub1);
                    assert_eq!(Expression::Variable("a".to_string()), *a);
                    assert_eq!(Expression::Variable("b".to_string()), *b);
                }
                other => panic!("Expected (a - b), got {:?}", other),
            }
        }
        other => panic!("Expected Expression statement, got {:?}", other),
    }
}

#[test]
fn test_parser_comparisons_and_logical() {
    let source = "x > 0 && y <= 10;";
    let mut parser = Parser::new(source);
    let stmt = parser.parse_statement().unwrap();

    match stmt {
        Statement::Expression(Expression::Binary { op, left, right }) => {
            assert_eq!(BinaryOperator::LogicalAnd, op);
            match *left {
                Expression::Binary { op: gt, left: x, right: zero } => {
                    assert_eq!(BinaryOperator::GreaterThan, gt);
                    assert_eq!(Expression::Variable("x".to_string()), *x);
                    assert_eq!(Expression::Literal(LiteralValue::Smi(0)), *zero);
                }
                other => panic!("Expected x > 0, got {:?}", other),
            }
            match *right {
                Expression::Binary { op: lte, left: y, right: ten } => {
                    assert_eq!(BinaryOperator::LessThanOrEqual, lte);
                    assert_eq!(Expression::Variable("y".to_string()), *y);
                    assert_eq!(Expression::Literal(LiteralValue::Smi(10)), *ten);
                }
                other => panic!("Expected y <= 10, got {:?}", other),
            }
        }
        other => panic!("Expected LogicalAnd statement, got {:?}", other),
    }
}

#[test]
fn test_parser_function_and_call() {
    let source = "
    function add(a, b) {
        return a + b;
    }
    let result = add(10, 20);
    ";
    let mut parser = Parser::new(source);
    let program = parser.parse_program().unwrap();

    assert_eq!(2, program.statements.len());

    // 1st stmt: function add(a, b)
    match &program.statements[0] {
        Statement::FunctionDeclaration { name, params, body, .. } => {
            assert_eq!("add", name);
            assert_eq!(&vec!["a".to_string(), "b".to_string()], params);
            assert_eq!(1, body.len());
            match &body[0] {
                Statement::Return(Some(Expression::Binary { op, left, right })) => {
                    assert_eq!(&BinaryOperator::Add, op);
                    assert_eq!(&Expression::Variable("a".to_string()), left.as_ref());
                    assert_eq!(&Expression::Variable("b".to_string()), right.as_ref());
                }
                other => panic!("Expected return a + b, got {:?}", other),
            }
        }
        other => panic!("Expected FunctionDeclaration, got {:?}", other),
    }

    // 2nd stmt: let result = add(10, 20);
    match &program.statements[1] {
        Statement::VariableDeclaration { name, init, is_const } => {
            assert_eq!("result", name);
            assert!(!is_const);
            match init.as_ref().unwrap() {
                Expression::Call { callee, arguments } => {
                    assert_eq!(&Expression::Variable("add".to_string()), callee.as_ref());
                    assert_eq!(2, arguments.len());
                    assert_eq!(Expression::Literal(LiteralValue::Smi(10)), arguments[0]);
                    assert_eq!(Expression::Literal(LiteralValue::Smi(20)), arguments[1]);
                }
                other => panic!("Expected Call expression, got {:?}", other),
            }
        }
        other => panic!("Expected VariableDeclaration, got {:?}", other),
    }
}

#[test]
fn test_parser_control_flow_and_asi() {
    let source = "
    let count = 0
    while (count < 5) {
        count = count + 1;
    }
    if (count === 5) {
        return true
    } else {
        return false
    }
    ";
    let mut parser = Parser::new(source);
    let program = parser.parse_program().unwrap();

    assert_eq!(3, program.statements.len());

    // 1st: let count = 0 (tested with ASI)
    match &program.statements[0] {
        Statement::VariableDeclaration { name, init, .. } => {
            assert_eq!("count", name);
            assert_eq!(Some(Expression::Literal(LiteralValue::Smi(0))), *init);
        }
        other => panic!("Expected count declaration, got {:?}", other),
    }

    // 2nd: while (count < 5) { ... }
    match &program.statements[1] {
        Statement::While { condition, body } => {
            match condition {
                Expression::Binary { op, left, right } => {
                    assert_eq!(&BinaryOperator::LessThan, op);
                    assert_eq!(&Expression::Variable("count".to_string()), left.as_ref());
                    assert_eq!(&Expression::Literal(LiteralValue::Smi(5)), right.as_ref());
                }
                other => panic!("Expected condition count < 5, got {:?}", other),
            }
            match body.as_ref() {
                Statement::Block(stmts) => assert_eq!(1, stmts.len()),
                other => panic!("Expected block body, got {:?}", other),
            }
        }
        other => panic!("Expected while loop, got {:?}", other),
    }

    // 3rd: if (count === 5) { return true } else { return false } (tested with ASI)
    match &program.statements[2] {
        Statement::If { condition, then_branch, else_branch } => {
            match condition {
                Expression::Binary { op, .. } => assert_eq!(&BinaryOperator::EqStrict, op),
                other => panic!("Expected count === 5, got {:?}", other),
            }
            assert!(matches!(then_branch.as_ref(), Statement::Block(_)));
            assert!(else_branch.is_some());
        }
        other => panic!("Expected if/else statement, got {:?}", other),
    }
}

#[test]
fn test_parser_property_access() {
    let source = "console.log(\"Hello V8\");";
    let mut parser = Parser::new(source);
    let stmt = parser.parse_statement().unwrap();

    match stmt {
        Statement::Expression(Expression::Call { callee, arguments }) => {
            match *callee {
                Expression::PropertyAccess { object, property } => {
                    assert_eq!(Expression::Variable("console".to_string()), *object);
                    assert_eq!("log", property);
                }
                other => panic!("Expected property access, got {:?}", other),
            }
            assert_eq!(1, arguments.len());
            assert_eq!(Expression::Literal(LiteralValue::String("Hello V8".to_string())), arguments[0]);
        }
        other => panic!("Expected call expression statement, got {:?}", other),
    }
}

#[test]
fn test_bytecode_generator_arithmetic() {
    let source = "let x = 10 + 20 * 3;";
    let mut parser = Parser::new(source);
    let program = parser.parse_program().unwrap();

    let bytecode_array = BytecodeGenerator::compile_program(&program);
    assert!(!bytecode_array.is_empty());

    let disasm = bytecode_array.disassemble();
    assert!(disasm.contains("LdaSmi"));
    assert!(disasm.contains("[10]"));
    assert!(disasm.contains("[20]"));
    assert!(disasm.contains("[3]"));
    assert!(disasm.contains("Mul"));
    assert!(disasm.contains("Add"));
    assert!(disasm.contains("Star"));
    assert!(disasm.contains("Return"));
}

#[test]
fn test_bytecode_generator_function_add() {
    let source = "function add(a, b) { return a + b; }";
    let mut parser = Parser::new(source);
    let program = parser.parse_program().unwrap();

    match &program.statements[0] {
        Statement::FunctionDeclaration { params, body, .. } => {
            let bytecode_array = BytecodeGenerator::compile_function(params, body);
            // 1 receiver `this` + 2 arguments (a, b) = 3 parameter registers
            assert_eq!(3, bytecode_array.parameter_count());

            let disasm = bytecode_array.disassemble();
            // Should load argument a (a1)
            assert!(disasm.contains("a1"));
            // Should load argument b (a2)
            assert!(disasm.contains("a2"));
            // Should add
            assert!(disasm.contains("Add"));
            // Should return
            assert!(disasm.contains("Return"));
        }
        other => panic!("Expected FunctionDeclaration, got {:?}", other),
    }
}

#[test]
fn test_bytecode_generator_if_else_patching() {
    let source = "
    let x = 5;
    if (x > 0) {
        return 1;
    } else {
        return 0;
    }
    ";
    let mut parser = Parser::new(source);
    let program = parser.parse_program().unwrap();

    let bytecode_array = BytecodeGenerator::compile_program(&program);
    let disasm = bytecode_array.disassemble();

    assert!(disasm.contains("TestGreaterThan"));
    assert!(disasm.contains("JumpIfFalse"));
    assert!(disasm.contains("Return"));
}

#[test]
fn test_bytecode_generator_while_loop() {
    let source = "
    let i = 0;
    while (i < 5) {
        i = i + 1;
    }
    return i;
    ";
    let mut parser = Parser::new(source);
    let program = parser.parse_program().unwrap();

    let bytecode_array = BytecodeGenerator::compile_program(&program);
    let disasm = bytecode_array.disassemble();

    assert!(disasm.contains("TestLessThan"));
    assert!(disasm.contains("JumpIfFalse"));
    assert!(disasm.contains("JumpLoop"));
    assert!(disasm.contains("Return"));
}

#[test]
fn test_bytecode_generator_end_to_end_pipeline() {
    // End-to-end test: raw JavaScript string -> Scanner -> Parser -> AST -> BytecodeGenerator -> BytecodeArray
    let js_code = "
    function multiplyByTwo(n) {
        let factor = 2;
        return n * factor;
    }
    ";
    let mut parser = Parser::new(js_code);
    let program = parser.parse_program().unwrap();
    assert_eq!(1, program.statements.len());

    match &program.statements[0] {
        Statement::FunctionDeclaration { name, params, body, .. } => {
            assert_eq!("multiplyByTwo", name);
            let bytecode_array = BytecodeGenerator::compile_function(params, body);
            assert_eq!(2, bytecode_array.parameter_count()); // this + n

            let disasm = bytecode_array.disassemble();
            assert!(disasm.contains("Star"));
            assert!(disasm.contains("Mul"));
            assert!(disasm.contains("Return"));
        }
        other => panic!("Expected FunctionDeclaration, got {:?}", other),
    }
}

#[test]
fn test_interpreter_vm_basic_arithmetic() {
    let res = evaluate_script("10 + 20 * 3;").unwrap();
    assert_eq!(JSValue::Smi(70), res);

    let res_sub = evaluate_script("100 - 45;").unwrap();
    assert_eq!(JSValue::Smi(55), res_sub);

    let res_div = evaluate_script("10 / 2;").unwrap();
    assert_eq!(JSValue::Number(5.0), res_div);

    let res_mod = evaluate_script("17 % 5;").unwrap();
    assert_eq!(JSValue::Smi(2), res_mod);
}

#[test]
fn test_interpreter_vm_variables_and_mutation() {
    let code = "
    let a = 40;
    let b = 2;
    return a + b;
    ";
    let res = evaluate_script(code).unwrap();
    assert_eq!(JSValue::Smi(42), res);

    let mutate_code = "
    let x = 10;
    x = x + 5;
    return x;
    ";
    let mutate_res = evaluate_script(mutate_code).unwrap();
    assert_eq!(JSValue::Smi(15), mutate_res);
}

#[test]
fn test_interpreter_vm_conditionals() {
    let true_code = "
    let x = 10;
    if (x > 5) {
        return 100;
    } else {
        return 200;
    }
    ";
    let true_res = evaluate_script(true_code).unwrap();
    assert_eq!(JSValue::Smi(100), true_res);

    let false_code = "
    let x = 3;
    if (x > 5) {
        return 100;
    } else {
        return 200;
    }
    ";
    let false_res = evaluate_script(false_code).unwrap();
    assert_eq!(JSValue::Smi(200), false_res);
}

#[test]
fn test_interpreter_vm_while_loop_sum() {
    // Gaussian sum 1 + 2 + ... + 10 = 55
    let loop_code = "
    let sum = 0;
    let i = 1;
    while (i <= 10) {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
    ";
    let res = evaluate_script(loop_code).unwrap();
    assert_eq!(JSValue::Smi(55), res);
}

#[test]
fn test_interpreter_vm_strings_and_coercion() {
    let str_code = "\"Hello \" + \"World\";";
    let str_res = evaluate_script(str_code).unwrap();
    assert_eq!(JSValue::String("Hello World".to_string()), str_res);

    let not_code = "!false;";
    let not_res = evaluate_script(not_code).unwrap();
    assert_eq!(JSValue::Boolean(true), not_res);

    let typeof_num = "typeof 42;";
    let typeof_num_res = evaluate_script(typeof_num).unwrap();
    assert_eq!(JSValue::String("number".to_string()), typeof_num_res);

    let typeof_str = "typeof \"abc\";";
    let typeof_str_res = evaluate_script(typeof_str).unwrap();
    assert_eq!(JSValue::String("string".to_string()), typeof_str_res);
}

#[test]
fn test_interpreter_vm_function_execution() {
    let js_code = "
    function multiply(a, b) {
        return a * b;
    }
    ";
    let mut parser = Parser::new(js_code);
    let program = parser.parse_program().unwrap();

    match &program.statements[0] {
        Statement::FunctionDeclaration { params, body, .. } => {
            let bytecode_array = BytecodeGenerator::compile_function(params, body);
            // Execute with arguments [6, 7]
            let result = InterpreterVM::execute(&bytecode_array, &[JSValue::Smi(6), JSValue::Smi(7)]).unwrap();
            assert_eq!(JSValue::Smi(42), result);
        }
        other => panic!("Expected FunctionDeclaration, got {:?}", other),
    }
}

#[test]
fn test_map_transitions_and_sharing() {
    let root_map = Map::root(InstanceType::JSObject, None);
    let obj1 = JSObject::new_with_map(root_map.clone());
    let obj2 = JSObject::new_with_map(root_map);

    // Initial root map must have 0 descriptors
    assert_eq!(0, obj1.borrow().map.borrow().descriptors.len());
    assert_eq!(0, obj2.borrow().map.borrow().descriptors.len());

    // Add property 'x' to obj1 -> Map 0 -> Map 1
    JSObject::set_property(&obj1, "x", JSValue::Smi(10));
    assert_eq!(1, obj1.borrow().map.borrow().descriptors.len());
    assert_eq!(Some(0), obj1.borrow().map.borrow().find_field_index("x"));

    // Add property 'y' to obj1 -> Map 1 -> Map 2
    JSObject::set_property(&obj1, "y", JSValue::Smi(20));
    assert_eq!(2, obj1.borrow().map.borrow().descriptors.len());
    assert_eq!(Some(1), obj1.borrow().map.borrow().find_field_index("y"));

    // Now add 'x' then 'y' to obj2: it must reuse the transition tree and share Map 2!
    JSObject::set_property(&obj2, "x", JSValue::Smi(100));
    JSObject::set_property(&obj2, "y", JSValue::Smi(200));

    assert!(std::rc::Rc::ptr_eq(&obj1.borrow().map, &obj2.borrow().map));
    assert_eq!(JSValue::Smi(10), obj1.borrow().get_property("x"));
    assert_eq!(JSValue::Smi(20), obj1.borrow().get_property("y"));
    assert_eq!(JSValue::Smi(100), obj2.borrow().get_property("x"));
    assert_eq!(JSValue::Smi(200), obj2.borrow().get_property("y"));
}

#[test]
fn test_prototype_inheritance() {
    let proto = JSObject::new_empty(None);
    JSObject::set_property(&proto, "shared", JSValue::String("inherited_val".to_string()));

    let child = JSObject::new_empty(Some(proto.clone()));
    JSObject::set_property(&child, "own", JSValue::Smi(42));

    // Child resolves own property
    assert_eq!(JSValue::Smi(42), child.borrow().get_property("own"));
    // Child resolves inherited property from prototype chain
    assert_eq!(JSValue::String("inherited_val".to_string()), child.borrow().get_property("shared"));
    // Non-existent property returns undefined
    assert_eq!(JSValue::Undefined, child.borrow().get_property("missing"));

    // Child shadows inherited property
    JSObject::set_property(&child, "shared", JSValue::String("shadowed".to_string()));
    assert_eq!(JSValue::String("shadowed".to_string()), child.borrow().get_property("shared"));
    // Prototype remains unchanged
    assert_eq!(JSValue::String("inherited_val".to_string()), proto.borrow().get_property("shared"));
}

#[test]
fn test_fast_vs_dictionary_mode() {
    let obj = JSObject::new_empty(None);
    JSObject::set_property(&obj, "a", JSValue::Smi(1));
    JSObject::set_property(&obj, "b", JSValue::Smi(2));

    assert!(!obj.borrow().map.borrow().is_dictionary_map);

    // Deleting property transitions to dictionary mode
    let deleted = obj.borrow_mut().delete_property("a");
    assert!(deleted);
    assert!(obj.borrow().map.borrow().is_dictionary_map);

    // Property "a" is now gone
    assert_eq!(JSValue::Undefined, obj.borrow().get_property("a"));
    // Property "b" remains accessible in dictionary mode
    assert_eq!(JSValue::Smi(2), obj.borrow().get_property("b"));

    // Dynamic addition in dictionary mode works
    JSObject::set_property(&obj, "c", JSValue::Smi(3));
    assert_eq!(JSValue::Smi(3), obj.borrow().get_property("c"));
}

#[test]
fn test_inline_cache_feedback_vector() {
    let vector = FeedbackVector::new(4);
    let slot = FeedbackSlot(0);

    // Uninitialized slot query returns None
    assert_eq!(None, vector.get_cached_field_index(slot, 0x1000));

    // First access: transitions to Monomorphic
    vector.record_lookup(slot, 0x1000, 2);
    assert_eq!(InlineCacheState::Monomorphic, vector.slots[0].borrow().state);
    assert_eq!(Some(2), vector.get_cached_field_index(slot, 0x1000));
    assert_eq!(None, vector.get_cached_field_index(slot, 0x2000));

    // Second map seen: transitions to Polymorphic
    vector.record_lookup(slot, 0x2000, 5);
    assert_eq!(InlineCacheState::Polymorphic, vector.slots[0].borrow().state);
    assert_eq!(Some(2), vector.get_cached_field_index(slot, 0x1000));
    assert_eq!(Some(5), vector.get_cached_field_index(slot, 0x2000));
}

#[test]
fn test_interpreter_vm_object_literals_and_properties() {
    let code = "
    let pt = { x: 10, y: 20 };
    return pt.x + pt.y;
    ";
    let res = evaluate_script(code).unwrap();
    assert_eq!(JSValue::Smi(30), res);
}

#[test]
fn test_interpreter_vm_object_mutation() {
    let code = "
    let o = { a: 1 };
    o.b = 5;
    o.a = o.a + 10;
    return o.a * o.b;
    ";
    let res = evaluate_script(code).unwrap();
    assert_eq!(JSValue::Smi(55), res);
}

#[test]
fn test_interpreter_vm_array_literals_and_indexing() {
    let code = "
    let arr = [10, 20, 30];
    return arr[0] + arr[1] + arr[2];
    ";
    let res = evaluate_script(code).unwrap();
    assert_eq!(JSValue::Smi(60), res);

    let mut_code = "
    let arr = [1, 2];
    arr[0] = 50;
    return arr[0] + arr[1];
    ";
    let mut_res = evaluate_script(mut_code).unwrap();
    assert_eq!(JSValue::Smi(52), mut_res);

    let len_code = "
    let arr = [10, 20, 30, 40];
    return arr.length;
    ";
    let len_res = evaluate_script(len_code).unwrap();
    assert_eq!(JSValue::Smi(4), len_res);
}

#[test]
fn test_interpreter_vm_object_string_keyed_access() {
    let code = "
    let o = { name: 42 };
    return o[\"name\"];
    ";
    let res = evaluate_script(code).unwrap();
    assert_eq!(JSValue::Smi(42), res);
}

#[test]
fn test_builtins_math() {
    let res_abs = evaluate_script("Math.abs(-15);").unwrap();
    assert_eq!(JSValue::Smi(15), res_abs);

    let res_floor = evaluate_script("Math.floor(3.9);").unwrap();
    assert_eq!(JSValue::Smi(3), res_floor);

    let res_ceil = evaluate_script("Math.ceil(3.1);").unwrap();
    assert_eq!(JSValue::Smi(4), res_ceil);

    let res_sqrt = evaluate_script("Math.sqrt(64);").unwrap();
    assert_eq!(JSValue::Smi(8), res_sqrt);

    let res_pow = evaluate_script("Math.pow(2, 8);").unwrap();
    assert_eq!(JSValue::Smi(256), res_pow);

    let res_max = evaluate_script("Math.max(10, 20, 5);").unwrap();
    assert_eq!(JSValue::Smi(20), res_max);

    let res_min = evaluate_script("Math.min(10, 20, 5);").unwrap();
    assert_eq!(JSValue::Smi(5), res_min);

    let res_trunc = evaluate_script("Math.trunc(-3.7);").unwrap();
    assert_eq!(JSValue::Smi(-3), res_trunc);

    let res_sign = evaluate_script("Math.sign(-42);").unwrap();
    assert_eq!(JSValue::Smi(-1), res_sign);
}

#[test]
fn test_builtins_object() {
    let keys_code = "
    let o = { a: 10, b: 20 };
    let keys = Object.keys(o);
    return keys.length;
    ";
    let res_keys = evaluate_script(keys_code).unwrap();
    assert_eq!(JSValue::Smi(2), res_keys);

    let vals_code = "
    let o = { a: 10, b: 20 };
    let vals = Object.values(o);
    return vals[0] + vals[1];
    ";
    let res_vals = evaluate_script(vals_code).unwrap();
    assert_eq!(JSValue::Smi(30), res_vals);

    let assign_code = "
    let target = { a: 1 };
    Object.assign(target, { b: 2 });
    return target.a + target.b;
    ";
    let res_assign = evaluate_script(assign_code).unwrap();
    assert_eq!(JSValue::Smi(3), res_assign);

    let create_code = "
    let proto = { x: 100 };
    let child = Object.create(proto);
    return child.x;
    ";
    let res_create = evaluate_script(create_code).unwrap();
    assert_eq!(JSValue::Smi(100), res_create);
}

#[test]
fn test_builtins_array() {
    let is_arr_true = evaluate_script("Array.isArray([1, 2]);").unwrap();
    assert_eq!(JSValue::Boolean(true), is_arr_true);

    let is_arr_false = evaluate_script("Array.isArray(42);").unwrap();
    assert_eq!(JSValue::Boolean(false), is_arr_false);

    let push_code = "
    let a = [10, 20];
    a.push(30);
    a.push(40);
    return a.length;
    ";
    let res_push = evaluate_script(push_code).unwrap();
    assert_eq!(JSValue::Smi(4), res_push);

    let pop_code = "
    let a = [10, 20, 30];
    let last = a.pop();
    return last;
    ";
    let res_pop = evaluate_script(pop_code).unwrap();
    assert_eq!(JSValue::Smi(30), res_pop);

    let join_code = "
    let a = [\"Hello\", \"World\"];
    return a.join(\" \");
    ";
    let res_join = evaluate_script(join_code).unwrap();
    assert_eq!(JSValue::String("Hello World".to_string()), res_join);

    let slice_code = "
    let a = [10, 20, 30, 40];
    let s = a.slice(1, 3);
    return s[0] + s[1];
    ";
    let res_slice = evaluate_script(slice_code).unwrap();
    assert_eq!(JSValue::Smi(50), res_slice);

    let index_code = "
    let a = [10, 20, 30];
    return a.indexOf(20);
    ";
    let res_index = evaluate_script(index_code).unwrap();
    assert_eq!(JSValue::Smi(1), res_index);
}

#[test]
fn test_builtins_console() {
    clear_logs();
    evaluate_script("console.log(\"V8\", \"Rust\", 2026);").unwrap();
    assert_eq!(Some("V8 Rust 2026".to_string()), get_last_log());
}

#[test]
fn test_global_variables_and_globalthis() {
    let code = "
    globalThis.myVar = 99;
    return myVar;
    ";
    let res = evaluate_script(code).unwrap();
    assert_eq!(JSValue::Smi(99), res);

    let nan_true = evaluate_script("isNaN(NaN);").unwrap();
    assert_eq!(JSValue::Boolean(true), nan_true);

    let nan_false = evaluate_script("isNaN(42);").unwrap();
    assert_eq!(JSValue::Boolean(false), nan_false);

    let int_res = evaluate_script("parseInt(\"1234\");").unwrap();
    assert_eq!(JSValue::Smi(1234), int_res);

    let float_res = evaluate_script("parseFloat(\"3.14\");").unwrap();
    assert_eq!(JSValue::Number(3.14), float_res);
}

#[test]
fn test_native_function_custom_registration() {
    let mut ctx = Context::new();
    let custom_add = JSFunction::new_native("customAdd", |_this, args| {
        let a = args.get(0).map(|v| v.to_number()).unwrap_or(0.0);
        let b = args.get(1).map(|v| v.to_number()).unwrap_or(0.0);
        Ok(JSValue::Smi((a + b) as i32))
    });

    JSObject::set_property(&ctx.global_object, "customAdd", JSValue::Function(custom_add));

    let res = ctx.eval("customAdd(100, 200);").unwrap();
    assert_eq!(JSValue::Smi(300), res);
}

#[test]
fn test_heap_nursery_bump_allocation() {
    let mut heap = Heap::new(1024, 4096);
    let s1 = heap.allocate(HeapPayload::String("first".to_string()), 32);
    let s2 = heap.allocate(HeapPayload::String("second".to_string()), 32);
    let s3 = heap.allocate(HeapPayload::String("third".to_string()), 32);

    assert_eq!(AllocationSpace::New, s1.space);
    assert_eq!(AllocationSpace::New, s2.space);
    assert_eq!(AllocationSpace::New, s3.space);

    assert_eq!(0, s1.index);
    assert_eq!(1, s2.index);
    assert_eq!(2, s3.index);

    let stats = heap.stats();
    assert_eq!(96, stats.used_heap_size);
    assert_eq!(96, stats.total_allocated_bytes);
}

#[test]
fn test_heap_scavenger_evacuation_and_reclamation() {
    let mut heap = Heap::new(512, 4096);

    // Garbage object: not rooted
    let garbage = heap.allocate(HeapPayload::String("dead".to_string()), 32);

    // Surviving root object: s1 -> s2
    let s2 = heap.allocate(HeapPayload::String("child".to_string()), 32);
    let s1 = heap.allocate(HeapPayload::String("parent".to_string()), 32);

    // Record reference: s1 -> s2
    if let Some(parent) = heap.get_mut(s1) {
        parent.add_reference(s2);
    }

    // Root only s1
    heap.add_global_root(s1);

    // Run Young-Gen Scavenge GC
    let evacuated = heap.collect_garbage(GarbageCollectionType::Scavenge);
    assert_eq!(2, evacuated, "s1 and s2 should be evacuated, garbage discarded");

    // Root pointer should be updated to new forwarded address
    let new_s1 = heap.global_roots[0];
    assert_eq!(AllocationSpace::New, new_s1.space);
    let parent_obj = heap.get(new_s1).expect("s1 must exist in new FromSpace");
    assert_eq!(1, parent_obj.header.age);

    // Outgoing reference in s1 should now point to evacuated s2
    assert_eq!(1, parent_obj.get_references().len());
    let new_s2 = parent_obj.get_references()[0];
    let child_obj = heap.get(new_s2).expect("s2 must exist in new FromSpace");
    assert_eq!(1, child_obj.header.age);

    // Unreachable garbage is gone
    assert!(heap.get(garbage).is_none());
}

#[test]
fn test_heap_generational_tenuring() {
    let mut heap = Heap::new(256, 4096);
    let obj_id = heap.allocate(HeapPayload::String("long_lived".to_string()), 64);
    heap.add_global_root(obj_id);

    // Scavenge 1: object age becomes 1 (remains in NewSpace)
    heap.collect_garbage(GarbageCollectionType::Scavenge);
    let after_gc1 = heap.global_roots[0];
    assert_eq!(AllocationSpace::New, after_gc1.space);
    assert_eq!(1, heap.get(after_gc1).unwrap().header.age);

    // Scavenge 2: object age reaches TENURING_THRESHOLD (2) and is promoted to OldSpace!
    heap.collect_garbage(GarbageCollectionType::Scavenge);
    let after_gc2 = heap.global_roots[0];
    assert_eq!(AllocationSpace::Old, after_gc2.space, "Object must be tenured to OldSpace");
    let tenured = heap.get(after_gc2).expect("Tenured object must exist in OldSpace");
    assert_eq!(2, tenured.header.age);

    if let HeapPayload::String(ref s) = tenured.payload {
        assert_eq!("long_lived", s);
    } else {
        panic!("Wrong payload variant");
    }
}

#[test]
fn test_heap_generational_write_barrier_and_store_buffer() {
    let mut heap = Heap::new(256, 4096);

    // Allocate an object in OldSpace directly
    let old_parent = heap.allocate_old(HeapPayload::String("old_parent".to_string()), 64);

    // Allocate a new object in NewSpace (nursery)
    let young_child = heap.allocate(HeapPayload::String("young_child".to_string()), 32);

    // Generational write barrier: OldSpace -> NewSpace
    heap.write_barrier(old_parent, young_child);
    assert_eq!(1, heap.store_buffer.len());

    // Do NOT add young_child to global roots directly.
    // Scavenger must use StoreBuffer remembered set to preserve young_child!
    heap.collect_garbage(GarbageCollectionType::Scavenge);

    // Verify parent's reference was updated to young_child's forwarded address
    let parent = heap.get(old_parent).unwrap();
    assert_eq!(1, parent.get_references().len());
    let new_child_id = parent.get_references()[0];

    let child = heap.get(new_child_id).expect("young_child must survive via StoreBuffer root");
    if let HeapPayload::String(ref s) = child.payload {
        assert_eq!("young_child", s);
    } else {
        panic!("Wrong child payload");
    }
}

#[test]
fn test_heap_full_mark_sweep_cycle_collection() {
    let mut heap = Heap::new(256, 4096);

    // Allocate cyclic garbage: A -> B -> A in OldSpace
    let a = heap.allocate_old(HeapPayload::String("cycleA".to_string()), 40);
    let b = heap.allocate_old(HeapPayload::String("cycleB".to_string()), 40);
    heap.get_mut(a).unwrap().add_reference(b);
    heap.get_mut(b).unwrap().add_reference(a);

    // Allocate a reachable root object
    let root = heap.allocate_old(HeapPayload::String("reachable".to_string()), 40);
    heap.add_global_root(root);

    let used_before = heap.stats().used_heap_size;
    assert_eq!(120, used_before);

    // Full Mark-Sweep should detect cycle A <-> B is unreachable and sweep both!
    let reclaimed = heap.collect_garbage(GarbageCollectionType::MarkSweep);
    assert_eq!(2, reclaimed, "Both cyclic objects A and B must be reclaimed");

    assert!(heap.get(a).is_none());
    assert!(heap.get(b).is_none());
    assert!(heap.get(root).is_some());

    let used_after = heap.stats().used_heap_size;
    assert_eq!(40, used_after);
}

#[test]
fn test_heap_handlescope_lifetime() {
    let mut heap = Heap::new(256, 4096);

    // Open HandleScope frame
    heap.open_handle_scope();
    let obj_id = heap.allocate(HeapPayload::String("scoped".to_string()), 50);
    let _handle: Handle<String> = heap.create_handle(obj_id);

    // Scavenge while HandleScope is active: object must survive
    heap.collect_garbage(GarbageCollectionType::Scavenge);

    // Handle was updated by Scavenger
    let new_id = heap.handle_scope.roots()[0];
    assert!(heap.get(new_id).is_some());

    // Close HandleScope: unroots the handle
    heap.close_handle_scope();
    assert_eq!(0, heap.handle_scope.roots().len());

    // Mark-Sweep now reclaims the unrooted object
    let reclaimed = heap.collect_garbage(GarbageCollectionType::MarkSweep);
    assert_eq!(1, reclaimed);
    assert!(heap.get(new_id).is_none());
}

#[test]
fn test_heap_factory_and_stats() {
    let mut heap = Heap::new(1024, 4096);

    let obj_handle = Factory::new_object(&mut heap, None);
    let arr_handle = Factory::new_array(&mut heap, vec![JSValue::Smi(1), JSValue::Smi(2)]);
    let str_handle = Factory::new_string(&mut heap, "Hello V8 Heap");
    let map_handle = Factory::new_map(&mut heap, InstanceType::JSObject);
    let func_handle = Factory::new_function(
        &mut heap,
        "testFn",
        FunctionKind::Native(|_this, _args| Ok(JSValue::Smi(42))),
    );

    assert!(heap.get(obj_handle.id()).is_some());
    assert!(heap.get(arr_handle.id()).is_some());
    assert!(heap.get(str_handle.id()).is_some());
    assert!(heap.get(map_handle.id()).is_some());
    assert!(heap.get(func_handle.id()).is_some());

    let stats = heap.stats();
    assert!(stats.used_heap_size > 200);
    assert!(stats.total_allocated_bytes > 200);
}

#[test]
fn test_ic_load_monomorphic_cache_hit() {
    let obj = JSObject::new_empty(None);
    JSObject::set_property(&obj, "alpha", JSValue::Smi(100));
    JSObject::set_property(&obj, "beta", JSValue::Smi(200));

    let feedback = FeedbackVector::new(1);
    let slot = FeedbackSlot(0);

    // Initial access: cache miss (uninitialized -> monomorphic)
    let (val1, hit1) = LoadIC::load(&obj, "alpha", Some((&feedback, slot)));
    assert_eq!(JSValue::Smi(100), val1);
    assert!(!hit1, "First load should be a cache miss");

    // Second access: cache hit (monomorphic fast path)
    let (val2, hit2) = LoadIC::load(&obj, "alpha", Some((&feedback, slot)));
    assert_eq!(JSValue::Smi(100), val2);
    assert!(hit2, "Second load must be a fast-path cache hit");
}

#[test]
fn test_ic_store_monomorphic_fast_path() {
    let obj = JSObject::new_empty(None);
    JSObject::set_property(&obj, "counter", JSValue::Smi(1));

    let feedback = FeedbackVector::new(1);
    let slot = FeedbackSlot(0);

    // Warm up cache via load
    let (initial_val, _) = LoadIC::load(&obj, "counter", Some((&feedback, slot)));
    assert_eq!(JSValue::Smi(1), initial_val);

    // Store through StoreIC fast path
    let hit = StoreIC::store(&obj, "counter", JSValue::Smi(42), Some((&feedback, slot)));
    assert!(hit, "StoreIC should use fast-path monomorphic slot write");

    // Verify written value
    let (updated_val, hit2) = LoadIC::load(&obj, "counter", Some((&feedback, slot)));
    assert_eq!(JSValue::Smi(42), updated_val);
    assert!(hit2);
}

#[test]
fn test_ic_keyed_element_fast_path() {
    let arr = JSArray::new_array(vec![JSValue::Smi(10), JSValue::Smi(20), JSValue::Smi(30)]);
    let arr_val = JSValue::Array(arr);

    // Fast element loading
    let el0 = KeyedIC::load_element(&arr_val, 0);
    let el1 = KeyedIC::load_element(&arr_val, 1);
    let el2 = KeyedIC::load_element(&arr_val, 2);
    assert_eq!(Some(JSValue::Smi(10)), el0);
    assert_eq!(Some(JSValue::Smi(20)), el1);
    assert_eq!(Some(JSValue::Smi(30)), el2);

    // Fast element storing
    let stored = KeyedIC::store_element(&arr_val, 1, JSValue::Smi(999));
    assert!(stored);

    let el1_updated = KeyedIC::load_element(&arr_val, 1);
    assert_eq!(Some(JSValue::Smi(999)), el1_updated);
}

#[test]
fn test_compiler_sea_of_nodes_graph_construction() {
    let code = "let x = 10; let y = 20; return x + y;";
    let mut parser = Parser::new(code);
    let program = parser.parse_program().unwrap();
    let bytecode = BytecodeGenerator::compile_program(&program);

    let graph = BytecodeGraphBuilder::new(&bytecode).build();
    assert!(graph.len() > 3);
    assert!(!graph.returns.is_empty(), "Graph must have a return node");

    let has_add = graph.nodes.iter().any(|n| n.op == NodeOp::Add);
    assert!(has_add, "Graph must contain an Add node");
}

#[test]
fn test_compiler_constant_folding() {
    let mut graph = Graph::new();
    let c1 = graph.add_node(NodeOp::Constant(JSValue::Smi(10)), vec![], None);
    let c2 = graph.add_node(NodeOp::Constant(JSValue::Smi(25)), vec![], None);
    let add_node = graph.add_node(NodeOp::Add, vec![c1, c2], None);
    graph.add_node(NodeOp::Return, vec![add_node], Some(graph.start));

    let folded = Optimizer::constant_folding(&mut graph);
    assert_eq!(1, folded);

    let optimized_node = graph.get(add_node).unwrap();
    assert_eq!(NodeOp::Constant(JSValue::Smi(35)), optimized_node.op);
}

#[test]
fn test_compiler_algebraic_reductions() {
    let mut graph = Graph::new();
    let param = graph.add_node(NodeOp::Parameter(0), vec![], Some(graph.start));
    let zero = graph.add_node(NodeOp::Constant(JSValue::Smi(0)), vec![], None);
    let one = graph.add_node(NodeOp::Constant(JSValue::Smi(1)), vec![], None);

    let add_zero = graph.add_node(NodeOp::Add, vec![param, zero], None);
    let mul_one = graph.add_node(NodeOp::Mul, vec![param, one], None);
    let mul_zero = graph.add_node(NodeOp::Mul, vec![param, zero], None);

    let reductions = Optimizer::algebraic_reductions(&mut graph);
    assert_eq!(3, reductions);

    assert_eq!(NodeOp::Parameter(0), graph.get(add_zero).unwrap().op);
    assert_eq!(NodeOp::Parameter(0), graph.get(mul_one).unwrap().op);
    assert_eq!(NodeOp::Constant(JSValue::Smi(0)), graph.get(mul_zero).unwrap().op);
}

#[test]
fn test_compiler_dead_code_elimination() {
    let mut graph = Graph::new();
    let c1 = graph.add_node(NodeOp::Constant(JSValue::Smi(5)), vec![], None);
    let c2 = graph.add_node(NodeOp::Constant(JSValue::Smi(6)), vec![], None);
    let dead_op = graph.add_node(NodeOp::Add, vec![c1, c2], None); // Unused

    let live_val = graph.add_node(NodeOp::Constant(JSValue::Smi(100)), vec![], None);
    graph.add_node(NodeOp::Return, vec![live_val], Some(graph.start));

    let eliminated = Optimizer::dead_code_elimination(&mut graph);
    assert!(eliminated >= 1);
    assert_eq!(NodeOp::Dead, graph.get(dead_op).unwrap().op);
}

#[test]
fn test_compiler_end_to_end_pipeline_execution() {
    let code = "let a = 15; let b = 25; return (a + b) * 2 - 10;";
    let mut parser = Parser::new(code);
    let program = parser.parse_program().unwrap();
    let bytecode = BytecodeGenerator::compile_program(&program);

    // 1. Interpreter execution
    let vm_result = InterpreterVM::execute(&bytecode, &[]).unwrap();
    assert_eq!(JSValue::Smi(70), vm_result);

    // 2. Optimizing Compiler Pipeline execution
    let (opt_graph, stats) = CompilerPipeline::compile(&bytecode);
    assert!(stats.constants_folded > 0, "Constant folding should optimize expressions");

    let compiler_result = CompilerPipeline::execute(&opt_graph, &[]).unwrap();
    assert_eq!(JSValue::Smi(70), compiler_result, "Compiler and VM results must match differentially");
}

#[test]
fn test_phase13_for_loop() {
    let mut ctx = Context::new();
    let code = "let sum = 0; for (let i = 0; i < 5; i = i + 1) { sum = sum + i; } sum;";
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Smi(10), res);
}

#[test]
fn test_phase13_break_continue() {
    let mut ctx = Context::new();
    let code = r#"
        let sum = 0;
        for (let i = 0; i < 10; i = i + 1) {
            if (i == 3) {
                continue;
            }
            if (i == 7) {
                break;
            }
            sum = sum + i;
        }
        sum;
    "#;
    let res = ctx.eval(code).unwrap();
    // 0 + 1 + 2 + 4 + 5 + 6 = 18
    assert_eq!(JSValue::Smi(18), res);
}

#[test]
fn test_phase13_do_while() {
    let mut ctx = Context::new();
    let code1 = "let count = 0; let i = 0; do { count = count + 1; i = i + 1; } while (i < 5); count;";
    assert_eq!(JSValue::Smi(5), ctx.eval(code1).unwrap());

    let code2 = "let x = 42; do { x = x + 1; } while (false); x;";
    assert_eq!(JSValue::Smi(43), ctx.eval(code2).unwrap());
}

#[test]
fn test_phase13_for_of() {
    let mut ctx = Context::new();
    let code = r#"
        let items = [10, 20, 30, 40];
        let total = 0;
        for (let item of items) {
            if (item == 30) {
                continue;
            }
            total = total + item;
        }
        total;
    "#;
    let res = ctx.eval(code).unwrap();
    // 10 + 20 + 40 = 70
    assert_eq!(JSValue::Smi(70), res);
}

#[test]
fn test_phase13_for_in() {
    let mut ctx = Context::new();
    let code = r#"
        let obj = { x: 10, y: 20, z: 30 };
        let count = 0;
        for (let k in obj) {
            count = count + 1;
        }
        count;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Smi(3), res);
}

#[test]
fn test_phase13_switch_statement() {
    let mut ctx = Context::new();
    let code = r#"
        let val = 2;
        let res = 0;
        switch (val) {
            case 1:
                res = 100;
                break;
            case 2:
                res = 200;
            case 3:
                res = res + 50;
                break;
            default:
                res = -1;
                break;
        }
        res;
    "#;
    // val=2 matches case 2 (res = 200), falls through to case 3 (res = 250), then breaks.
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Smi(250), res);

    let default_code = r#"
        let val = 999;
        let res = 0;
        switch (val) {
            case 1:
                res = 100;
                break;
            default:
                res = -1;
                break;
        }
        res;
    "#;
    let default_res = ctx.eval(default_code).unwrap();
    assert_eq!(JSValue::Smi(-1), default_res);
}

#[test]
fn test_phase13_try_catch() {
    let mut ctx = Context::new();
    let code = r#"
        let caught = "none";
        try {
            throw "custom exception";
        } catch (e) {
            caught = e;
        }
        caught;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::String("custom exception".to_string()), res);
}

#[test]
fn test_phase13_try_catch_finally() {
    let mut ctx = Context::new();
    let code = r#"
        let log = 0;
        try {
            try {
                log = log + 1;
                throw 99;
            } catch (e) {
                log = log + 10;
                throw e;
            } finally {
                log = log + 100;
            }
        } catch (outer) {
            log = log + outer;
        }
        log;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Smi(210), res);
}

#[test]
fn test_phase14_json_parse_and_stringify() {
    let mut ctx = Context::new();
    let code = r#"
        let jsonStr = '{"name":"V8-Rust","version":1,"active":true,"tags":["fast","safe"]}';
        let parsed = JSON.parse(jsonStr);
        let name = parsed.name;
        let ver = parsed.version;
        let is_active = parsed.active;
        let tag0 = parsed.tags[0];
        let tag1 = parsed.tags[1];
        
        let backToStr = JSON.stringify(parsed);
        let prettyStr = JSON.stringify(parsed, null, 2);
        
        name + " v" + ver + " active:" + is_active + " tag0:" + tag0 + " tag1:" + tag1;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::String("V8-Rust v1 active:true tag0:fast tag1:safe".to_string()), res);

    // Test JSON.stringify primitives
    let stringify_res = ctx.eval(r#"JSON.stringify([1, "two", false])"#).unwrap();
    assert_eq!(JSValue::String(r#"[1,"two",false]"#.to_string()), stringify_res);

    // Test syntax error on invalid JSON
    let err_res = ctx.eval(r#"JSON.parse("{ invalid json }")"#);
    assert!(err_res.is_err());
}

#[test]
fn test_phase14_date_constructor_and_prototype() {
    let mut ctx = Context::new();
    let code = r#"
        let nowMs = Date.now();
        let parsedMs = Date.parse("2026-09-12T15:30:00.000Z");
        let d = new Date("2026-09-12T15:30:00.000Z");
        let year = d.getFullYear();
        let month = d.getMonth();
        let date = d.getDate();
        let hours = d.getHours();
        let minutes = d.getMinutes();
        let iso = d.toISOString();
        
        iso;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::String("2026-09-12T15:30:00.000Z".to_string()), res);

    let comp_code = r#"
        let d = new Date("2026-09-12T15:30:45.123Z");
        let y = d.getFullYear();
        let m = d.getMonth();
        let day = d.getDate();
        let h = d.getHours();
        let min = d.getMinutes();
        let s = d.getSeconds();
        let ms = d.getMilliseconds();
        let time = d.getTime();
        
        y + "-" + (m + 1) + "-" + day + " " + h + ":" + min + ":" + s + "." + ms;
    "#;
    let comp_res = ctx.eval(comp_code).unwrap();
    assert_eq!(JSValue::String("2026-9-12 15:30:45.123".to_string()), comp_res);
}

#[test]
fn test_phase14_array_functional_methods() {
    let mut ctx = Context::new();
    let code = r#"
        function double(x) { return x * 2; }
        function isEven(x) { return x % 2 == 0; }
        function add(acc, x) { return acc + x; }
        
        let arr = [1, 2, 3, 4, 5];
        let mapped = arr.map(double);
        let filtered = arr.filter(isEven);
        let sum = arr.reduce(add, 0);
        
        let hasThree = arr.includes(3);
        let hasTen = arr.includes(10);
        let found = arr.find(isEven);
        let foundIdx = arr.findIndex(isEven);
        
        let combined = [1, 2].concat([3, 4]);
        let flattened = [1, [2, [3]]].flat(1);
        
        let filled = [1, 2, 3, 4].fill(9, 1, 3);
        let ofArr = Array.of(10, 20, 30);
        let fromStr = Array.from("abc");
        
        sum;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Smi(15), res);

    // Verify map output
    let map_res = ctx.eval(r#"
        function sq(x) { return x * x; }
        [1, 2, 3, 4].map(sq).join(",");
    "#).unwrap();
    assert_eq!(JSValue::String("1,4,9,16".to_string()), map_res);

    // Verify filter output
    let filter_res = ctx.eval(r#"
        function gtTwo(x) { return x > 2; }
        [1, 2, 3, 4, 5].filter(gtTwo).join("-");
    "#).unwrap();
    assert_eq!(JSValue::String("3-4-5".to_string()), filter_res);

    // Verify includes
    let inc_res = ctx.eval(r#"[10, 20, 30].includes(20)"#).unwrap();
    assert_eq!(JSValue::Boolean(true), inc_res);

    let inc_false_res = ctx.eval(r#"[10, 20, 30].includes(99)"#).unwrap();
    assert_eq!(JSValue::Boolean(false), inc_false_res);
}

#[test]
fn test_phase14_string_prototype_methods() {
    let mut ctx = Context::new();
    let code = r#"
        let s = "  Hello, Safe Rust V8!  ";
        let trimmed = s.trim();
        let upper = trimmed.toUpperCase();
        let lower = trimmed.toLowerCase();
        let sliced = trimmed.slice(7, 16);
        let sub = trimmed.substring(7, 16);
        let idx = trimmed.indexOf("Safe");
        let lastIdx = trimmed.lastIndexOf("e");
        let inc = trimmed.includes("Rust");
        let starts = trimmed.startsWith("Hello");
        let ends = trimmed.endsWith("V8!");
        let rep = "ha".repeat(3);
        let char1 = trimmed.charAt(0);
        let code0 = trimmed.charCodeAt(0);
        let parts = "foo,bar,baz".split(",");
        let replaced = "hello world".replace("world", "Rust");
        let replacedAll = "cat and cat".replaceAll("cat", "dog");
        let padded = "42".padStart(5, "0");
        let fromCode = String.fromCharCode(65, 66, 67);
        let strVal = String(9876);
        
        replaced + " " + replacedAll + " " + padded + " " + fromCode + " " + strVal;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(
        JSValue::String("hello Rust dog and dog 00042 ABC 9876".to_string()),
        res
    );

    // Test case conversions and slicing
    let str_tests = ctx.eval(r#"
        let a = "abc".toUpperCase();
        let b = "DEF".toLowerCase();
        let c = "hello world".slice(0, 5);
        let d = "ha".repeat(4);
        a + "-" + b + "-" + c + "-" + d;
    "#).unwrap();
    assert_eq!(JSValue::String("ABC-def-hello-hahahaha".to_string()), str_tests);

    // Test split
    let split_res = ctx.eval(r#"
        "apple,banana,cherry".split(",").join("|");
    "#).unwrap();
    assert_eq!(JSValue::String("apple|banana|cherry".to_string()), split_res);
}

#[test]
fn test_phase15_microtask_queue_ordering() {
    let mut ctx = Context::new();
    let code = r#"
        let log = [];
        log.push("sync-1");
        queueMicrotask(function() {
            log.push("micro-1");
        });
        queueMicrotask(function() {
            log.push("micro-2");
        });
        log.push("sync-2");
        log;
    "#;
    ctx.eval(code).unwrap();
    let log_str = ctx.eval(r#"log.join(",")"#).unwrap();
    assert_eq!(JSValue::String("sync-1,sync-2,micro-1,micro-2".to_string()), log_str);
}

#[test]
fn test_phase15_promise_resolve_reject_and_chaining() {
    let mut ctx = Context::new();
    let code = r#"
        let result = 0;
        let p = new Promise(function(resolve, reject) {
            resolve(10);
        });
        p.then(function(val) {
            return val * 2;
        }).then(function(val) {
            result = val + 5;
        });
    "#;
    ctx.eval(code).unwrap();
    let final_res = ctx.eval("result").unwrap();
    assert_eq!(JSValue::Smi(25), final_res);

    // Test rejection and catch
    let catch_code = r#"
        let errorMsg = "";
        let p2 = new Promise(function(resolve, reject) {
            reject("Something went wrong");
        });
        p2.catch(function(err) {
            errorMsg = err;
        });
    "#;
    ctx.eval(catch_code).unwrap();
    let err_res = ctx.eval("errorMsg").unwrap();
    assert_eq!(JSValue::String("Something went wrong".to_string()), err_res);

    // Test static Promise.resolve and Promise.reject
    let static_code = r#"
        let r1 = 0;
        Promise.resolve(42).then(function(v) { r1 = v; });
        let r2 = "";
        Promise.reject("static err").catch(function(e) { r2 = e; });
    "#;
    ctx.eval(static_code).unwrap();
    assert_eq!(JSValue::Smi(42), ctx.eval("r1").unwrap());
    assert_eq!(JSValue::String("static err".to_string()), ctx.eval("r2").unwrap());
}

#[test]
fn test_phase15_promise_combinators() {
    let mut ctx = Context::new();

    // Promise.all
    let all_code = r#"
        let allResult = "";
        Promise.all([
            Promise.resolve(10),
            Promise.resolve(20),
            30
        ]).then(function(arr) {
            allResult = arr.join("+");
        });
    "#;
    ctx.eval(all_code).unwrap();
    assert_eq!(JSValue::String("10+20+30".to_string()), ctx.eval("allResult").unwrap());

    // Promise.race
    let race_code = r#"
        let raceResult = "";
        Promise.race([
            Promise.resolve("winner"),
            Promise.resolve("loser")
        ]).then(function(v) {
            raceResult = v;
        });
    "#;
    ctx.eval(race_code).unwrap();
    assert_eq!(JSValue::String("winner".to_string()), ctx.eval("raceResult").unwrap());

    // Promise.allSettled
    let settled_code = r#"
        let count = 0;
        Promise.allSettled([
            Promise.resolve("ok"),
            Promise.reject("fail")
        ]).then(function(arr) {
            count = arr.length;
        });
    "#;
    ctx.eval(settled_code).unwrap();
    assert_eq!(JSValue::Smi(2), ctx.eval("count").unwrap());
}

#[test]
fn test_phase15_async_await() {
    let mut ctx = Context::new();

    // Top-level and async function execution
    let async_code = r#"
        async function fetchValue(x) {
            return x * 3;
        }

        async function compute() {
            let a = await fetchValue(5);
            let b = await Promise.resolve(10);
            return a + b;
        }

        let ans = 0;
        compute().then(function(val) {
            ans = val;
        });
    "#;
    ctx.eval(async_code).unwrap();
    assert_eq!(JSValue::Smi(25), ctx.eval("ans").unwrap());

    // Async function error handling with try/catch
    let error_code = r#"
        async function failing() {
            let caught = "none";
            try {
                let x = await Promise.reject("async boom");
            } catch (e) {
                caught = e;
            }
            return caught;
        }

        let caughtErr = "";
        failing().then(function(v) {
            caughtErr = v;
        });
    "#;
    ctx.eval(error_code).unwrap();
    assert_eq!(JSValue::String("async boom".to_string()), ctx.eval("caughtErr").unwrap());
}

#[test]
fn test_phase16_classes_basic_and_safety() {
    let mut ctx = Context::new();

    let code = r#"
        class Point {
            constructor(x, y) {
                this.x = x;
                this.y = y;
            }

            distanceFromOrigin() {
                return this.x * this.x + this.y * this.y;
            }

            static create(x, y) {
                return new Point(x, y);
            }
        }

        let p1 = new Point(3, 4);
        let dist = p1.distanceFromOrigin();
        let p2 = Point.create(6, 8);
        let dist2 = p2.distanceFromOrigin();

        let callWithoutNewError = "";
        try {
            Point(1, 2);
        } catch (e) {
            callWithoutNewError = e;
        }
    "#;

    ctx.eval(code).unwrap();
    assert_eq!(JSValue::Smi(25), ctx.eval("dist").unwrap());
    assert_eq!(JSValue::Smi(100), ctx.eval("dist2").unwrap());
    assert_eq!(
        JSValue::String("TypeError: Class constructor cannot be invoked without 'new'".to_string()),
        ctx.eval("callWithoutNewError").unwrap()
    );
}

#[test]
fn test_phase16_class_inheritance_and_super() {
    let mut ctx = Context::new();

    let code = r#"
        class Animal {
            constructor(name) {
                this.name = name;
            }

            speak() {
                return this.name + " makes a sound";
            }
        }

        class Dog extends Animal {
            constructor(name, breed) {
                super(name);
                this.breed = breed;
            }

            speak() {
                return super.speak() + " -> " + this.name + " barks!";
            }

            getInfo() {
                return this.name + " is a " + this.breed;
            }
        }

        let d = new Dog("Rex", "Shepherd");
        let sound = d.speak();
        let info = d.getInfo();
    "#;

    ctx.eval(code).unwrap();
    assert_eq!(
        JSValue::String("Rex makes a sound -> Rex barks!".to_string()),
        ctx.eval("sound").unwrap()
    );
    assert_eq!(
        JSValue::String("Rex is a Shepherd".to_string()),
        ctx.eval("info").unwrap()
    );
}

#[test]
fn test_phase16_getters_and_setters() {
    let mut ctx = Context::new();

    let code = r#"
        class Rectangle {
            constructor(width, height) {
                this._width = width;
                this._height = height;
            }

            get area() {
                return this._width * this._height;
            }

            set width(w) {
                this._width = w;
            }

            get width() {
                return this._width;
            }
        }

        let rect = new Rectangle(10, 20);
        let a1 = rect.area;
        rect.width = 15;
        let a2 = rect.area;
        let w = rect.width;
    "#;

    ctx.eval(code).unwrap();
    assert_eq!(JSValue::Smi(200), ctx.eval("a1").unwrap());
    assert_eq!(JSValue::Smi(300), ctx.eval("a2").unwrap());
    assert_eq!(JSValue::Smi(15), ctx.eval("w").unwrap());
}

#[test]
fn test_phase16_instanceof_operator() {
    let mut ctx = Context::new();

    let code = r#"
        class Vehicle {}
        class Car extends Vehicle {}
        class Plane {}

        let myCar = new Car();
        let myPlane = new Plane();

        let carIsCar = myCar instanceof Car;
        let carIsVehicle = myCar instanceof Vehicle;
        let carIsPlane = myCar instanceof Plane;
        let carIsObject = myCar instanceof Object;
        let planeIsVehicle = myPlane instanceof Vehicle;
        let numIsObject = 42 instanceof Object;
    "#;

    ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), ctx.eval("carIsCar").unwrap());
    assert_eq!(JSValue::Boolean(true), ctx.eval("carIsVehicle").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("carIsPlane").unwrap());
    assert_eq!(JSValue::Boolean(true), ctx.eval("carIsObject").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("planeIsVehicle").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("numIsObject").unwrap());
}

#[test]
fn test_phase16_map_and_set_collections() {
    let mut ctx = Context::new();

    let code = r#"
        let m = new Map();
        m.set("alpha", 1).set("beta", 2);
        let mSize1 = m.size;
        let alphaVal = m.get("alpha");
        let hasBeta = m.has("beta");
        let hasGamma = m.has("gamma");
        m.delete("alpha");
        let mSize2 = m.size;
        let hasAlphaAfterDel = m.has("alpha");

        // SameValueZero equality on NaN
        m.set(NaN, "not-a-number");
        let nanVal = m.get(NaN);

        // Set
        let s = new Set();
        s.add(10).add(20).add(10);
        let sSize1 = s.size;
        let has10 = s.has(10);
        let has30 = s.has(30);
        s.delete(10);
        let sSize2 = s.size;
        let has10AfterDel = s.has(10);
        s.clear();
        let sSizeAfterClear = s.size;
    "#;

    ctx.eval(code).unwrap();
    assert_eq!(JSValue::Smi(2), ctx.eval("mSize1").unwrap());
    assert_eq!(JSValue::Smi(1), ctx.eval("alphaVal").unwrap());
    assert_eq!(JSValue::Boolean(true), ctx.eval("hasBeta").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("hasGamma").unwrap());
    assert_eq!(JSValue::Smi(1), ctx.eval("mSize2").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("hasAlphaAfterDel").unwrap());
    assert_eq!(JSValue::String("not-a-number".to_string()), ctx.eval("nanVal").unwrap());

    assert_eq!(JSValue::Smi(2), ctx.eval("sSize1").unwrap());
    assert_eq!(JSValue::Boolean(true), ctx.eval("has10").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("has30").unwrap());
    assert_eq!(JSValue::Smi(1), ctx.eval("sSize2").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("has10AfterDel").unwrap());
    assert_eq!(JSValue::Smi(0), ctx.eval("sSizeAfterClear").unwrap());
}

#[test]
fn test_phase16_weakmap_and_weakset() {
    let mut ctx = Context::new();

    let code = r#"
        class KeyClass {}
        let key1 = new KeyClass();
        let key2 = new KeyClass();

        let wm = new WeakMap();
        wm.set(key1, 100);
        let wmHas1 = wm.has(key1);
        let wmVal1 = wm.get(key1);
        let wmHas2 = wm.has(key2);
        wm.delete(key1);
        let wmHas1AfterDel = wm.has(key1);

        let wmTypeError = "";
        try {
            wm.set("primitive", 42);
        } catch (e) {
            wmTypeError = e;
        }

        let ws = new WeakSet();
        ws.add(key1);
        let wsHas1 = ws.has(key1);
        let wsHas2 = ws.has(key2);
        ws.delete(key1);
        let wsHas1AfterDel = ws.has(key1);

        let wsTypeError = "";
        try {
            ws.add(123);
        } catch (e) {
            wsTypeError = e;
        }
    "#;

    ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), ctx.eval("wmHas1").unwrap());
    assert_eq!(JSValue::Smi(100), ctx.eval("wmVal1").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("wmHas2").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("wmHas1AfterDel").unwrap());
    assert_eq!(
        JSValue::String("TypeError: Invalid value used as weak map key".to_string()),
        ctx.eval("wmTypeError").unwrap()
    );

    assert_eq!(JSValue::Boolean(true), ctx.eval("wsHas1").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("wsHas2").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("wsHas1AfterDel").unwrap());
    assert_eq!(
        JSValue::String("TypeError: Invalid value used in weak set".to_string()),
        ctx.eval("wsTypeError").unwrap()
    );
}

#[test]
fn test_phase16_symbols() {
    let mut ctx = Context::new();

    let code = r#"
        let s1 = Symbol("mySymbol");
        let s2 = Symbol("mySymbol");
        let sSame = (s1 === s2);
        let sType = typeof s1;

        let sFor1 = Symbol.for("app.id");
        let sFor2 = Symbol.for("app.id");
        let sForSame = (sFor1 === sFor2);
        let key = Symbol.keyFor(sFor1);

        let symCtorError = "";
        try {
            new Symbol();
        } catch (e) {
            symCtorError = e;
        }

        let hasIter = typeof Symbol.iterator === "symbol";
        let hasTag = typeof Symbol.toStringTag === "symbol";
    "#;

    ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(false), ctx.eval("sSame").unwrap());
    assert_eq!(JSValue::String("symbol".to_string()), ctx.eval("sType").unwrap());
    assert_eq!(JSValue::Boolean(true), ctx.eval("sForSame").unwrap());
    assert_eq!(JSValue::String("app.id".to_string()), ctx.eval("key").unwrap());
    assert_eq!(
        JSValue::String("TypeError: Symbol is not a constructor".to_string()),
        ctx.eval("symCtorError").unwrap()
    );
    assert_eq!(JSValue::Boolean(true), ctx.eval("hasIter").unwrap());
    assert_eq!(JSValue::Boolean(true), ctx.eval("hasTag").unwrap());
}

#[test]
fn test_phase17_regexp_parser_and_ast() {
    use v8_base_bits::regexp::*;

    // 1. Flag parsing
    let flags = RegExpFlags::parse("gimsuy").unwrap();
    assert!(flags.global);
    assert!(flags.ignore_case);
    assert!(flags.multiline);
    assert!(flags.dot_all);
    assert!(flags.unicode);
    assert!(flags.sticky);
    assert_eq!("gimsuy", flags.to_string_canonical());

    assert!(RegExpFlags::parse("gg").is_err()); // duplicate flag
    assert!(RegExpFlags::parse("x").is_err()); // invalid flag

    // 2. Parser: character class, groups, quantifiers, lookaround
    let mut parser = RegExpParser::new(r"(?<word>\w+)[\s,]+(?=end)", flags);
    let ast = parser.parse().unwrap();
    assert_eq!(1, parser.capture_count);
    assert_eq!(Some(&1), parser.named_groups.get("word"));

    match ast {
        RegExpNode::Sequence(nodes) => {
            assert_eq!(3, nodes.len());
            // Node 0: capture group with name "word"
            match &nodes[0] {
                RegExpNode::Capture { index, name, .. } => {
                    assert_eq!(1, *index);
                    assert_eq!(Some("word".to_string()), *name);
                }
                other => panic!("Expected Capture node, got {:?}", other),
            }
            // Node 1: quantifier on character class [\s,]
            match &nodes[1] {
                RegExpNode::Quantifier { min, max, greedy, .. } => {
                    assert_eq!(1, *min);
                    assert_eq!(None, *max);
                    assert!(*greedy);
                }
                other => panic!("Expected Quantifier node, got {:?}", other),
            }
            // Node 2: positive lookahead (?=end)
            match &nodes[2] {
                RegExpNode::Lookaround { is_positive, is_lookbehind, .. } => {
                    assert!(*is_positive);
                    assert!(!*is_lookbehind);
                }
                other => panic!("Expected Lookaround node, got {:?}", other),
            }
        }
        other => panic!("Expected Sequence node, got {:?}", other),
    }
}

#[test]
fn test_phase17_irregexp_bytecode_compilation() {
    use v8_base_bits::regexp::*;

    let bytecode = RegExpEngine::compile(r"(\d+)-(\w+)", "i").unwrap();
    assert_eq!(2, bytecode.capture_count);
    // 2 * (capture_count + 1) = 6 registers
    assert_eq!(6, bytecode.num_registers);
    assert!(bytecode.flags.ignore_case);
    assert!(!bytecode.instructions.is_empty());

    // Should begin by setting register 0 (match start)
    assert_eq!(
        RegExpOpcode::SetRegisterCurrentPosition { reg: 0 },
        bytecode.instructions[0]
    );
    // Should end with setting register 1 (match end) and Succeed
    let last = &bytecode.instructions[bytecode.instructions.len() - 1];
    assert_eq!(&RegExpOpcode::Succeed, last);
}

#[test]
fn test_phase17_irregexp_interpreter_execution() {
    use v8_base_bits::regexp::*;

    // Test 1: Greedy vs Lazy
    let greedy_bc = RegExpEngine::compile(r"<.*>", "").unwrap();
    let greedy_match = RegExpEngine::exec(&greedy_bc, "<a><b>", 0).unwrap();
    assert_eq!("<a><b>", greedy_match.captures[0].as_ref().unwrap());

    let lazy_bc = RegExpEngine::compile(r"<.*?>", "").unwrap();
    let lazy_match = RegExpEngine::exec(&lazy_bc, "<a><b>", 0).unwrap();
    assert_eq!("<a>", lazy_match.captures[0].as_ref().unwrap());

    // Test 2: Backreference (abc)-\1
    let backref_bc = RegExpEngine::compile(r"([a-z]+)-\1", "").unwrap();
    let br_match = RegExpEngine::exec(&backref_bc, "foo-foo", 0).unwrap();
    assert_eq!("foo-foo", br_match.captures[0].as_ref().unwrap());
    assert_eq!("foo", br_match.captures[1].as_ref().unwrap());
    assert!(RegExpEngine::exec(&backref_bc, "foo-bar", 0).is_none());

    // Test 3: Lookahead and Lookbehind
    let lookahead_bc = RegExpEngine::compile(r"\d+(?=px)", "").unwrap();
    let la_match = RegExpEngine::exec(&lookahead_bc, "font-size: 16px; margin: 20em;", 0).unwrap();
    assert_eq!("16", la_match.captures[0].as_ref().unwrap());
}

#[test]
fn test_phase17_regexp_object_and_methods() {
    let mut ctx = Context::new();

    let code = r#"
        let re = new RegExp("a(b+)c", "i");
        let src = re.source;
        let flags = re.flags;
        let isGlobal = re.global;
        let isIgnoreCase = re.ignoreCase;

        let m = re.exec("test aBBc end");
        let matched = (m !== null);
        let full = m[0];
        let g1 = m[1];
        let idx = m.index;
        let inp = m.input;

        let testTrue = re.test("aBBc");
        let testFalse = re.test("xyz");

        // Stateful global matching
        let reG = new RegExp("\\w+", "g");
        let m1 = reG.exec("hello world")[0];
        let idxAfter1 = reG.lastIndex;
        let m2 = reG.exec("hello world")[0];
        let idxAfter2 = reG.lastIndex;
        let m3 = reG.exec("hello world"); // null
        let idxAfter3 = reG.lastIndex; // 0

        // Sticky matching
        let reY = new RegExp("\\d+", "y");
        reY.lastIndex = 4;
        let mY = reY.exec("abc 123 def");
        let mYVal = "";
        if (mY !== null) {
            mYVal = mY[0];
        }
    "#;

    ctx.eval(code).unwrap();
    assert_eq!(JSValue::String("a(b+)c".to_string()), ctx.eval("src").unwrap());
    assert_eq!(JSValue::String("i".to_string()), ctx.eval("flags").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("isGlobal").unwrap());
    assert_eq!(JSValue::Boolean(true), ctx.eval("isIgnoreCase").unwrap());

    assert_eq!(JSValue::Boolean(true), ctx.eval("matched").unwrap());
    assert_eq!(JSValue::String("aBBc".to_string()), ctx.eval("full").unwrap());
    assert_eq!(JSValue::String("BB".to_string()), ctx.eval("g1").unwrap());
    assert_eq!(JSValue::Smi(5), ctx.eval("idx").unwrap());
    assert_eq!(JSValue::String("test aBBc end".to_string()), ctx.eval("inp").unwrap());

    assert_eq!(JSValue::Boolean(true), ctx.eval("testTrue").unwrap());
    assert_eq!(JSValue::Boolean(false), ctx.eval("testFalse").unwrap());

    // Stateful global assertions
    assert_eq!(JSValue::String("hello".to_string()), ctx.eval("m1").unwrap());
    assert_eq!(JSValue::Smi(5), ctx.eval("idxAfter1").unwrap());
    assert_eq!(JSValue::String("world".to_string()), ctx.eval("m2").unwrap());
    assert_eq!(JSValue::Smi(11), ctx.eval("idxAfter2").unwrap());
    assert_eq!(JSValue::Null, ctx.eval("m3").unwrap());
    assert_eq!(JSValue::Smi(0), ctx.eval("idxAfter3").unwrap());

    // Sticky assertions
    assert_eq!(JSValue::String("123".to_string()), ctx.eval("mYVal").unwrap());
}

#[test]
fn test_phase17_string_prototype_methods() {
    let mut ctx = Context::new();

    let code = r#"
        let s = "The quick brown fox jumps over the lazy dog";
        let reWord = new RegExp("\\b\\w{5}\\b", "g");

        // String.prototype.match
        let matches = s.match(reWord);
        let mLen = matches.length;
        let m0 = matches[0];
        let m1 = matches[1];
        let m2 = matches[2];

        // String.prototype.search
        let searchIdx = s.search(new RegExp("fox"));
        let searchNone = s.search(new RegExp("cat"));

        // String.prototype.replace with pattern and tokens
        let repTokens = "2026-09-12".replace(new RegExp("(\\d{4})-(\\d{2})-(\\d{2})"), "$2/$3/$1");

        // String.prototype.replace with functional replacer
        let repFn = "apple 10 banana 20".replace(new RegExp("\\d+", "g"), function(m) {
            return (parseInt(m) * 2) + "";
        });

        // String.prototype.replaceAll
        let repAll = "foo bar foo baz".replaceAll(new RegExp("foo", "g"), "qux");

        // String.prototype.split with RegExp and captured groups
        let splitParts = "hello 123 world 456 test".split(new RegExp("(\\d+)"));
        let spLen = splitParts.length;
        let sp0 = splitParts[0];
        let sp1 = splitParts[1];
        let sp2 = splitParts[2];
    "#;

    ctx.eval(code).unwrap();
    assert_eq!(JSValue::Smi(3), ctx.eval("mLen").unwrap());
    assert_eq!(JSValue::String("quick".to_string()), ctx.eval("m0").unwrap());
    assert_eq!(JSValue::String("brown".to_string()), ctx.eval("m1").unwrap());
    assert_eq!(JSValue::String("jumps".to_string()), ctx.eval("m2").unwrap());

    assert_eq!(JSValue::Smi(16), ctx.eval("searchIdx").unwrap());
    assert_eq!(JSValue::Smi(-1), ctx.eval("searchNone").unwrap());

    assert_eq!(JSValue::String("09/12/2026".to_string()), ctx.eval("repTokens").unwrap());
    assert_eq!(JSValue::String("apple 20 banana 40".to_string()), ctx.eval("repFn").unwrap());
    assert_eq!(JSValue::String("qux bar qux baz".to_string()), ctx.eval("repAll").unwrap());

    // split with capture group keeps delimiter
    assert_eq!(JSValue::Smi(5), ctx.eval("spLen").unwrap());
    assert_eq!(JSValue::String("hello ".to_string()), ctx.eval("sp0").unwrap());
    assert_eq!(JSValue::String("123".to_string()), ctx.eval("sp1").unwrap());
    assert_eq!(JSValue::String(" world ".to_string()), ctx.eval("sp2").unwrap());
}

#[test]
fn test_phase17_regexp_literals_in_scripts() {
    let mut ctx = Context::new();

    let code = r#"
        let re = /hello\s+(\w+)/i;
        let res = re.exec("Hello World");
        let resWord = res[1];

        let rep = "a1 b2 c3".replace(/\d/g, "X");
        let parts = "apple, orange; banana".split(/[;,]\s*/);
        let p0 = parts[0];
        let p1 = parts[1];
        let p2 = parts[2];
    "#;

    ctx.eval(code).unwrap();
    assert_eq!(JSValue::String("World".to_string()), ctx.eval("resWord").unwrap());
    assert_eq!(JSValue::String("aX bX cX".to_string()), ctx.eval("rep").unwrap());
    assert_eq!(JSValue::String("apple".to_string()), ctx.eval("p0").unwrap());
    assert_eq!(JSValue::String("orange".to_string()), ctx.eval("p1").unwrap());
    assert_eq!(JSValue::String("banana".to_string()), ctx.eval("p2").unwrap());
}

#[test]
fn test_phase18_x64_assembler_arithmetic_and_moves() {
    let mut masm = X64Assembler::new();
    masm.emit_prologue(16);

    // rax = 42
    masm.mov_reg_imm64(X64Register::Rax, 42);
    // rcx = 100
    masm.mov_reg_imm64(X64Register::Rcx, 100);
    // rax += rcx (142)
    masm.add_reg_reg(X64Register::Rax, X64Register::Rcx);
    // rax -= 2 (140)
    masm.sub_reg_imm32(X64Register::Rax, 2);

    masm.emit_epilogue();

    let bytes = masm.finalize();
    assert!(!bytes.is_empty());

    let mut emu = CpuEmulator::new();
    let res = emu.execute_x64(&bytes, 1000).unwrap();
    assert_eq!(140, res.return_value);
}

#[test]
fn test_phase18_x64_assembler_branches_and_labels() {
    let mut masm = X64Assembler::new();
    masm.emit_prologue(16);

    // Sum 1 to 10 in a loop:
    // rax = 0 (accumulator)
    masm.mov_reg_imm64(X64Register::Rax, 0);
    // rcx = 10 (counter)
    masm.mov_reg_imm64(X64Register::Rcx, 10);

    let loop_head = masm.create_label();
    masm.bind(loop_head);

    // rax += rcx
    masm.add_reg_reg(X64Register::Rax, X64Register::Rcx);
    // rcx -= 1
    masm.sub_reg_imm32(X64Register::Rcx, 1);
    // cmp rcx, 0
    masm.cmp_reg_imm32(X64Register::Rcx, 0);
    // jg loop_head
    masm.jcc(Condition::GreaterThan, loop_head);

    masm.emit_epilogue();

    let bytes = masm.finalize();
    let mut emu = CpuEmulator::new();
    let res = emu.execute_x64(&bytes, 1000).unwrap();
    // 10 + 9 + 8 + 7 + 6 + 5 + 4 + 3 + 2 + 1 = 55
    assert_eq!(55, res.return_value);
}

#[test]
fn test_phase18_arm64_assembler_encodings() {
    let mut masm = Arm64Assembler::new();
    masm.emit_prologue(16);

    // add x0, x1, x2
    masm.add(Arm64Register::X0, Arm64Register::X1, Arm64Register::X2);
    // sub x0, x0, x3
    masm.sub(Arm64Register::X0, Arm64Register::X0, Arm64Register::X3);
    // mul x0, x0, x4
    masm.mul(Arm64Register::X0, Arm64Register::X0, Arm64Register::X4);
    // sdiv x0, x0, x5
    masm.sdiv(Arm64Register::X0, Arm64Register::X0, Arm64Register::X5);

    let label = masm.create_label();
    masm.b(label);
    masm.bind(label);

    masm.emit_epilogue();

    let bytes = masm.finalize();
    // ARM64 instructions are strictly 4-byte multiples
    assert_eq!(0, bytes.len() % 4);

    let disasm = disassemble_arm64(&bytes);
    assert!(disasm.contains("add x0, x1, x2"));
    assert!(disasm.contains("sub x0, x0, x3"));
    assert!(disasm.contains("mul x0, x0, x4"));
    assert!(disasm.contains("sdiv x0, x0, x5"));
    assert!(disasm.contains("ret"));
}

#[test]
fn test_phase18_disassembler_x64() {
    let mut masm = X64Assembler::new();
    masm.emit_prologue(16);
    masm.mov_reg_imm64(X64Register::Rax, 12345);
    masm.mov_reg_reg(X64Register::Rdx, X64Register::Rax);
    masm.add_reg_reg(X64Register::Rdx, X64Register::Rcx);
    masm.emit_epilogue();

    let bytes = masm.finalize();
    let disasm = disassemble_x64(&bytes);

    assert!(disasm.contains("push rbp"));
    assert!(disasm.contains("mov rbp, rsp"));
    assert!(disasm.contains("movabs rax, 0x3039") || disasm.contains("12345"));
    assert!(disasm.contains("mov rdx, rax"));
    assert!(disasm.contains("add rdx, rcx"));
    assert!(disasm.contains("ret"));
}

#[test]
fn test_phase18_sea_of_nodes_instruction_selection() {
    // Build a Sea-of-Nodes graph computing: (param0 + param1) * 2
    let mut graph = Graph::new();
    let p0 = graph.add_node(NodeOp::Parameter(0), Vec::new(), None);
    let p1 = graph.add_node(NodeOp::Parameter(1), Vec::new(), None);
    let add_node = graph.add_node(NodeOp::Add, vec![p0, p1], None);
    let c2 = graph.add_node(NodeOp::Constant(JSValue::Smi(2)), Vec::new(), None);
    let mul_node = graph.add_node(NodeOp::Mul, vec![add_node, c2], None);
    graph.add_node(NodeOp::Return, vec![mul_node], None);

    let executable = InstructionSelector::compile_x64(&graph, CallingConvention::WindowsX64);
    assert!(!executable.code_bytes().is_empty());

    let res = executable.execute(&[15, 25]).unwrap();
    // (15 + 25) * 2 = 80
    assert_eq!(80, res.return_value);
}

#[test]
fn test_phase18_jit_tiering_and_promotion() {
    let mut ctx = Context::new();

    let code = r#"
        function compute(a, b) {
            return (a + b) * 3;
        }
        compute(1, 2);
        compute(2, 3);
        compute(3, 4);
        compute(4, 5);
        let resBefore = compute(5, 5); // 5th invocation triggers JIT tier-up
        let resAfter = compute(10, 20); // 6th invocation runs native JIT code
    "#;

    ctx.eval(code).unwrap();

    // (5 + 5) * 3 = 30
    assert_eq!(JSValue::Smi(30), ctx.eval("resBefore").unwrap());
    // (10 + 20) * 3 = 90
    assert_eq!(JSValue::Smi(90), ctx.eval("resAfter").unwrap());

    // Check function object directly
    let compute_val = ctx.global_object.borrow().get_property("compute");
    if let JSValue::Function(ref js_func) = compute_val {
        assert!(js_func.invocation_count.get() >= 5);
        assert!(js_func.is_jit_compiled());
    } else {
        panic!("Expected compute to be JSValue::Function");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// PHASE 19: WebAssembly Baseline Engine (Liftoff)
// ═══════════════════════════════════════════════════════════════════════════════

/// Minimal valid Wasm module with a single function `add(i32, i32) -> i32`.
fn make_add_wasm() -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(b"\0asm");
    m.extend_from_slice(&[1, 0, 0, 0]);

    // Type section (id=1): one functype (i32 i32) -> (i32)
    let type_body: &[u8] = &[
        0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F,
    ];
    m.push(1); m.push(type_body.len() as u8); m.extend_from_slice(type_body);

    // Function section (id=3): func 0 → type 0
    m.push(3); m.push(2); m.push(1); m.push(0);

    // Export section (id=7): "add" → func 0
    let exp: &[u8] = &[0x01, 0x03, b'a', b'd', b'd', 0x00, 0x00];
    m.push(7); m.push(exp.len() as u8); m.extend_from_slice(exp);

    // Code section (id=10):
    let body: &[u8] = &[
        0x00,             // 0 local declarations
        0x20, 0x00,       // local.get 0
        0x20, 0x01,       // local.get 1
        0x6A,             // i32.add
        0x0B,             // end
    ];
    let cs = { let mut v = vec![0x01u8]; v.push(body.len() as u8); v.extend_from_slice(body); v };
    m.push(10); m.push(cs.len() as u8); m.extend_from_slice(&cs);
    m
}

/// Wasm module with `sum(n: i32) -> i32` = sum of 1..=n using a loop.
fn make_sum_wasm() -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(b"\0asm\x01\x00\x00\x00");

    // Type section: (i32) -> (i32)
    let type_body: &[u8] = &[0x01, 0x60, 0x01, 0x7F, 0x01, 0x7F];
    m.push(1); m.push(type_body.len() as u8); m.extend_from_slice(type_body);

    // Function section: func 0 → type 0
    m.push(3); m.push(2); m.push(1); m.push(0);

    // Export section: "sum" → func 0
    let exp: &[u8] = &[0x01, 0x03, b's', b'u', b'm', 0x00, 0x00];
    m.push(7); m.push(exp.len() as u8); m.extend_from_slice(exp);

    // Code section: locals[0]=n (param), locals[1]=acc, locals[2]=i
    let body: &[u8] = &[
        0x02, 0x01, 0x7F, 0x01, 0x7F,  // 2 local groups: 1×i32 (acc), 1×i32 (i)
        0x41, 0x00, 0x21, 0x01,          // acc = 0
        0x41, 0x01, 0x21, 0x02,          // i = 1
        0x03, 0x40,                       // loop (empty type)
          0x20, 0x01, 0x20, 0x02, 0x6A, 0x21, 0x01, // acc = acc + i
          0x20, 0x02, 0x41, 0x01, 0x6A, 0x21, 0x02, // i = i + 1
          0x20, 0x02, 0x20, 0x00, 0x4C,  // i <= n
          0x0D, 0x00,                     // br_if 0 (loop)
        0x0B,                             // end loop
        0x20, 0x01,                       // local.get acc
        0x0B,                             // end function
    ];
    let cs = { let mut v = vec![0x01u8]; v.push(body.len() as u8); v.extend_from_slice(body); v };
    m.push(10); m.push(cs.len() as u8); m.extend_from_slice(&cs);
    m
}

/// Wasm module: `mul_add(a, b, c: i32) -> i32` = (a * b) + c.
fn make_mul_add_wasm() -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(b"\0asm\x01\x00\x00\x00");

    let type_body: &[u8] = &[0x01, 0x60, 0x03, 0x7F, 0x7F, 0x7F, 0x01, 0x7F];
    m.push(1); m.push(type_body.len() as u8); m.extend_from_slice(type_body);
    m.push(3); m.push(2); m.push(1); m.push(0);

    let exp: &[u8] = &[0x01, 0x07, b'm', b'u', b'l', b'_', b'a', b'd', b'd', 0x00, 0x00];
    m.push(7); m.push(exp.len() as u8); m.extend_from_slice(exp);

    let body: &[u8] = &[
        0x00,
        0x20, 0x00, 0x20, 0x01, 0x6C, // a * b
        0x20, 0x02, 0x6A,              // + c
        0x0B,
    ];
    let cs = { let mut v = vec![0x01u8]; v.push(body.len() as u8); v.extend_from_slice(body); v };
    m.push(10); m.push(cs.len() as u8); m.extend_from_slice(&cs);
    m
}

// ─── Test 1: Wasm binary parser ───────────────────────────────────────────────

#[test]
fn test_phase19_wasm_binary_parser() {
    use v8_base_bits::wasm::binary_parser::{parse, ExportDesc};

    let wasm = make_add_wasm();
    let module = parse(&wasm).expect("parse should succeed");

    assert_eq!(1, module.types.len());
    assert_eq!(2, module.types[0].params.len());
    assert_eq!(1, module.types[0].results.len());
    assert_eq!(1, module.functions.len());
    assert_eq!(0, module.functions[0]);
    assert_eq!(1, module.exports.len());
    assert_eq!("add", module.exports[0].name);
    assert!(matches!(module.exports[0].desc, ExportDesc::Func(0)));
    assert_eq!(1, module.code.len());
    assert!(!module.code[0].body.is_empty());

    // Bad magic should fail
    assert!(parse(b"BADD\x01\x00\x00\x00").is_err());
    // Too short should fail
    assert!(parse(b"\0asm").is_err());
}

// ─── Test 2: Wasm interpreter — i32.add ──────────────────────────────────────

#[test]
fn test_phase19_wasm_interpreter_add() {
    use v8_base_bits::wasm::binary_parser::parse;
    use v8_base_bits::wasm::instance::WasmInstance;
    use v8_base_bits::wasm::interpreter::{WasmInterpreter, WasmVal};

    let module = parse(&make_add_wasm()).unwrap();
    let mut inst = WasmInstance::instantiate(&module, vec![]);

    let r = WasmInterpreter::call_export(&module, &mut inst, "add",
        vec![WasmVal::I32(17), WasmVal::I32(25)]).unwrap();
    assert_eq!(WasmVal::I32(42), r[0]);

    let r2 = WasmInterpreter::call_export(&module, &mut inst, "add",
        vec![WasmVal::I32(-5), WasmVal::I32(5)]).unwrap();
    assert_eq!(WasmVal::I32(0), r2[0]);
}

// ─── Test 3: Wasm interpreter — mul_add arithmetic ───────────────────────────

#[test]
fn test_phase19_wasm_interpreter_arithmetic() {
    use v8_base_bits::wasm::binary_parser::parse;
    use v8_base_bits::wasm::instance::WasmInstance;
    use v8_base_bits::wasm::interpreter::{WasmInterpreter, WasmVal};

    let module = parse(&make_mul_add_wasm()).unwrap();
    let mut inst = WasmInstance::instantiate(&module, vec![]);

    // 3*4+5 = 17
    let r = WasmInterpreter::call_export(&module, &mut inst, "mul_add",
        vec![WasmVal::I32(3), WasmVal::I32(4), WasmVal::I32(5)]).unwrap();
    assert_eq!(WasmVal::I32(17), r[0]);

    // 10*10+(-1) = 99
    let r2 = WasmInterpreter::call_export(&module, &mut inst, "mul_add",
        vec![WasmVal::I32(10), WasmVal::I32(10), WasmVal::I32(-1)]).unwrap();
    assert_eq!(WasmVal::I32(99), r2[0]);
}

// ─── Test 4: Wasm interpreter — loop control flow (sum 1..N) ─────────────────

#[test]
fn test_phase19_wasm_interpreter_loop_sum() {
    use v8_base_bits::wasm::binary_parser::parse;
    use v8_base_bits::wasm::instance::WasmInstance;
    use v8_base_bits::wasm::interpreter::{WasmInterpreter, WasmVal};

    let module = parse(&make_sum_wasm()).unwrap();
    let mut inst = WasmInstance::instantiate(&module, vec![]);

    // sum(10) = 55
    let r = WasmInterpreter::call_export(&module, &mut inst, "sum",
        vec![WasmVal::I32(10)]).unwrap();
    assert_eq!(WasmVal::I32(55), r[0]);

    // sum(100) = 5050
    let r2 = WasmInterpreter::call_export(&module, &mut inst, "sum",
        vec![WasmVal::I32(100)]).unwrap();
    assert_eq!(WasmVal::I32(5050), r2[0]);
}

// ─── Test 5: Wasm trap — division by zero ─────────────────────────────────────

#[test]
fn test_phase19_wasm_trap_division_by_zero() {
    use v8_base_bits::wasm::binary_parser::parse;
    use v8_base_bits::wasm::instance::WasmInstance;
    use v8_base_bits::wasm::interpreter::{WasmInterpreter, WasmTrap, WasmVal};

    let mut m = Vec::new();
    m.extend_from_slice(b"\0asm\x01\x00\x00\x00");
    let type_body: &[u8] = &[0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F];
    m.push(1); m.push(type_body.len() as u8); m.extend_from_slice(type_body);
    m.push(3); m.push(2); m.push(1); m.push(0);
    let exp: &[u8] = &[0x01, 0x03, b'd', b'i', b'v', 0x00, 0x00];
    m.push(7); m.push(exp.len() as u8); m.extend_from_slice(exp);
    let body: &[u8] = &[0x00, 0x20, 0x00, 0x20, 0x01, 0x6D, 0x0B]; // a / b (div_s)
    let cs = { let mut v = vec![0x01u8]; v.push(body.len() as u8); v.extend_from_slice(body); v };
    m.push(10); m.push(cs.len() as u8); m.extend_from_slice(&cs);

    let module = parse(&m).unwrap();
    let mut inst = WasmInstance::instantiate(&module, vec![]);

    // 10 / 2 = 5
    let ok = WasmInterpreter::call_export(&module, &mut inst, "div",
        vec![WasmVal::I32(10), WasmVal::I32(2)]).unwrap();
    assert_eq!(WasmVal::I32(5), ok[0]);

    // 10 / 0 → trap
    let err = WasmInterpreter::call_export(&module, &mut inst, "div",
        vec![WasmVal::I32(10), WasmVal::I32(0)]);
    assert_eq!(Err(WasmTrap::DivisionByZero), err);
}

// ─── Test 6: WebAssembly JS API — full round-trip via Context ─────────────────

#[test]
fn test_phase19_webassembly_js_api_roundtrip() {
    let mut ctx = Context::new();

    // WebAssembly global must exist
    let wa_global = ctx.global_object.borrow().get_property("WebAssembly");
    assert!(matches!(wa_global, JSValue::Object(_)), "WebAssembly should be Object");

    if let JSValue::Object(ref wa) = wa_global {
        assert!(matches!(wa.borrow().get_property("validate"),   JSValue::Function(_)));
        assert!(matches!(wa.borrow().get_property("compile"),    JSValue::Function(_)));
        assert!(matches!(wa.borrow().get_property("instantiate"),JSValue::Function(_)));
    }

    // Use WebAssembly.instantiate via JS eval — inline byte array
    let wasm_bytes = make_add_wasm();
    let bytes_js_str: Vec<String> = wasm_bytes.iter().map(|b| b.to_string()).collect();
    let code = format!(
        "var wasmBytes = [{}]; var wasmResult = WebAssembly.instantiate(wasmBytes);",
        bytes_js_str.join(",")
    );
    ctx.eval(&code).expect("WebAssembly.instantiate should succeed");

    // wasmResult = { instance: { exports: { add: [Function] } } }
    let result = ctx.global_object.borrow().get_property("wasmResult");
    assert!(matches!(result, JSValue::Object(_)), "wasmResult should be Object");

    if let JSValue::Object(ref res) = result {
        let instance = res.borrow().get_property("instance");
        assert!(matches!(instance, JSValue::Object(_)));

        if let JSValue::Object(ref inst) = instance {
            let exports = inst.borrow().get_property("exports");
            assert!(matches!(exports, JSValue::Object(_)));

            if let JSValue::Object(ref exp) = exports {
                let add_fn = exp.borrow().get_property("add");
                assert!(matches!(add_fn, JSValue::Function(_)));

                // Call exported Wasm function from Rust: add(10, 32) = 42
                if let JSValue::Function(ref add) = add_fn {
                    let r = add.call(&JSValue::Undefined, &[JSValue::Smi(10), JSValue::Smi(32)]).unwrap();
                    assert_eq!(JSValue::Smi(42), r);
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// PHASE 20: Typed Arrays & ArrayBuffer Subsystem
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_phase20_array_buffer_and_dataview() {
    let mut ctx = Context::new();
    let code = r#"
        let buf = new ArrayBuffer(16);
        let byteLen = buf.byteLength;
        let isViewBuf = ArrayBuffer.isView(buf);

        let sliced = buf.slice(4, 12);
        let slicedLen = sliced.byteLength;

        let dv = new DataView(buf, 0, 16);
        let isViewDv = ArrayBuffer.isView(dv);

        dv.setInt32(0, 123456, true);
        let readInt = dv.getInt32(0, true);

        dv.setFloat64(8, 3.14159, true);
        let readFloat = dv.getFloat64(8, true);

        byteLen == 16 && isViewBuf == false && slicedLen == 8 && isViewDv == true && readInt == 123456;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase20_typed_arrays_all_variants() {
    let mut ctx = Context::new();
    let code = r#"
        let u8 = new Uint8Array(4);
        u8[0] = 10;
        u8[1] = 20;
        u8[2] = 30;
        u8[3] = 40;

        let sub = u8.subarray(1, 3);
        let subSum = sub[0] + sub[1]; // 20 + 30 = 50

        let i32 = new Int32Array(3);
        i32[0] = 100000;
        i32[1] = -50000;
        i32[2] = 200000;
        let i32Sum = i32[0] + i32[1] + i32[2]; // 250000

        let f64 = new Float64Array(2);
        f64[0] = 1.5;
        f64[1] = 2.5;
        let f64Sum = f64[0] + f64[1]; // 4

        subSum == 50 && i32Sum == 250000 && f64Sum == 4;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase20_wasm_linear_memory_typed_array_interop() {
    let mut ctx = Context::new();
    // Instantiate Wasm module with memory export
    let mut m = Vec::new();
    m.extend_from_slice(b"\0asm\x01\x00\x00\x00");
    // Memory section (id=5): 1 memory, min=1 page
    m.extend_from_slice(&[5, 3, 1, 0, 1]);
    // Export section (id=7): "memory" -> mem 0 (kind=2)
    m.extend_from_slice(&[7, 10, 1, 6, b'm', b'e', b'm', b'o', b'r', b'y', 2, 0]);

    let bytes_js: Vec<String> = m.iter().map(|b| b.to_string()).collect();
    let code = format!(
        r#"
        let wasm = WebAssembly.instantiate([{}]);
        let memBuffer = wasm.instance.exports.memory.buffer;
        let u8 = new Uint8Array(memBuffer);
        u8[0] = 42;
        u8[1] = 99;
        u8[0] + u8[1];
        "#,
        bytes_js.join(",")
    );
    let res = ctx.eval(&code).unwrap();
    assert_eq!(JSValue::Smi(141), res);
}

// ═══════════════════════════════════════════════════════════════════════════════
// PHASE 21: Proxy & Reflect Metaprogramming Subsystem
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_phase21_proxy_get_set_has_traps() {
    let mut ctx = Context::new();
    let code = r#"
        let target = { count: 10 };
        let proxy = new Proxy(target, {
            get: function(t, prop) {
                if (prop == "intercepted") {
                    return 999;
                }
                return t[prop];
            },
            set: function(t, prop, val) {
                t[prop] = val * 2;
                return true;
            },
            has: function(t, prop) {
                if (prop == "secret") { return true; }
                return prop in t;
            }
        });

        let v1 = proxy.count;        // 10
        let v2 = proxy.intercepted;  // 999
        proxy.count = 25;
        let v3 = proxy.count;        // 50
        let hasSecret = "secret" in proxy; // true

        v1 == 10 && v2 == 999 && v3 == 50 && hasSecret == true;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase21_proxy_apply_construct_traps() {
    let mut ctx = Context::new();
    let code = r#"
        function sum(a, b) {
            return a + b;
        }

        let proxyFn = new Proxy(sum, {
            apply: function(target, thisArg, args) {
                return target(args[0], args[1]) * 10;
            }
        });

        let r = proxyFn(3, 4); // (3 + 4) * 10 = 70
        r;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Smi(70), res);
}

#[test]
fn test_phase21_reflect_builtins() {
    let mut ctx = Context::new();
    let code = r#"
        let obj = { x: 10, y: 20 };
        let v1 = Reflect.get(obj, "x");
        Reflect.set(obj, "z", 30);
        let hasZ = Reflect.has(obj, "z");
        let v3 = Reflect.get(obj, "z");
        Reflect.deleteProperty(obj, "y");
        let hasY = Reflect.has(obj, "y");

        v1 == 10 && hasZ == true && v3 == 30 && hasY == false;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

// ═══════════════════════════════════════════════════════════════════════════════
// PHASE 22: Chrome DevTools Protocol & Inspector
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_phase22_inspector_cdp_runtime_evaluate() {
    use v8_base_bits::inspector::InspectorSession;

    let mut session = InspectorSession::new();

    // 1. Runtime.enable
    let r1 = session.dispatch(r#"{"id": 1, "method": "Runtime.enable"}"#);
    assert!(r1.contains("\"id\":1") && r1.contains("\"result\""));

    // 2. Runtime.evaluate: expression = "40 + 2"
    let r2 = session.dispatch(r#"{"id": 2, "method": "Runtime.evaluate", "params": {"expression": "40 + 2"}}"#);
    assert!(r2.contains("\"id\":2"));
    assert!(r2.contains("\"value\":42") || r2.contains("\"description\":\"42\""));

    // 3. Runtime.evaluate: string expression
    let r3 = session.dispatch(r#"{"id": 3, "method": "Runtime.evaluate", "params": {"expression": "'Hello CDP'"}}"#);
    assert!(r3.contains("\"value\":\"Hello CDP\""));
}

#[test]
fn test_phase22_inspector_debugger_and_profiler() {
    use v8_base_bits::inspector::InspectorSession;

    let mut session = InspectorSession::new();

    // 1. Debugger.enable
    let r1 = session.dispatch(r#"{"id": 1, "method": "Debugger.enable"}"#);
    assert!(r1.contains("debuggerId"));

    // 2. Debugger.setBreakpointByUrl
    let r2 = session.dispatch(r#"{"id": 2, "method": "Debugger.setBreakpointByUrl", "params": {"url": "app.js", "lineNumber": 15}}"#);
    assert!(r2.contains("breakpointId"));

    // 3. Profiler.enable & start & stop
    let _ = session.dispatch(r#"{"id": 3, "method": "Profiler.enable"}"#);
    let _ = session.dispatch(r#"{"id": 4, "method": "Profiler.start"}"#);
    let r_stop = session.dispatch(r#"{"id": 5, "method": "Profiler.stop"}"#);
    assert!(r_stop.contains("profile") && r_stop.contains("callFrame"));
}

#[test]
fn test_phase22_eval_and_object_entries() {
    let mut ctx = Context::new();
    let code = r#"
        let evalRes = eval("let a = 15; let b = 25; a + b;"); // 40

        let o = { x: 1, y: 2 };
        Object.defineProperty(o, "z", { value: 3 });
        let entries = Object.entries(o);

        evalRes == 40 && o.z == 3 && entries.length == 3;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

// ═══════════════════════════════════════════════════════════════════════════════
// PHASE 23: ECMAScript Internationalization API (Intl)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_phase23_intl_number_format_and_locales() {
    let mut ctx = Context::new();
    let code = r#"
        let nfUS = new Intl.NumberFormat("en-US");
        let sUS = nfUS.format(1234567.89); // "1,234,567.89"

        let nfDE = new Intl.NumberFormat("de-DE");
        let sDE = nfDE.format(1234567.89); // "1.234.567,89"

        let nfCur = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });
        let sCur = nfCur.format(1234.5); // "$1,234.50"

        let nfPct = new Intl.NumberFormat("en-US", { style: "percent" });
        let sPct = nfPct.format(0.75); // "75%"

        let opts = nfUS.resolvedOptions();

        sUS == "1,234,567.89" && sDE == "1.234.567,89" && sCur == "$1,234.50" && sPct == "75%" && opts.locale == "en-US";
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase23_intl_date_time_format() {
    let mut ctx = Context::new();
    let code = r#"
        let dtfUS = new Intl.DateTimeFormat("en-US");
        // timestamp: 1608206400000 -> 2020-12-17 12:00:00 UTC
        let sUS = dtfUS.format(1608206400000); // "12/17/2020"

        let dtfDE = new Intl.DateTimeFormat("de-DE");
        let sDE = dtfDE.format(1608206400000); // "17.12.2020"

        let dtfOpts = new Intl.DateTimeFormat("en-US", { month: "short", day: "2-digit", year: "numeric" });
        let sOpts = dtfOpts.format(1608206400000); // "Dec/17/2020"

        sUS == "12/17/2020" && sDE == "17.12.2020" && sOpts == "Dec/17/2020";
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase23_intl_collator_and_canonical_locales() {
    let mut ctx = Context::new();
    let code = r#"
        let coll = new Intl.Collator("en-US");
        let c1 = coll.compare("a", "b"); // -1
        let c2 = coll.compare("b", "a"); // 1
        let c3 = coll.compare("abc", "abc"); // 0

        let collInsens = new Intl.Collator("en-US", { sensitivity: "base" });
        let c4 = collInsens.compare("a", "A"); // 0

        let locales = Intl.getCanonicalLocales(["en_US", "de_DE"]);

        c1 < 0 && c2 > 0 && c3 == 0 && c4 == 0 && locales[0] == "en-US" && locales[1] == "de-DE";
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

// ═══════════════════════════════════════════════════════════════════════════════
// PHASE 24: Binary Heap Snapshot (mksnapshot)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_phase24_snapshot_roundtrip_and_cold_start() {
    let mut ctx1 = Context::new();
    let init_code = r#"
        let baseValue = 100;
        let greeting = "Hello Snapshot";
        let config = { active: true, version: 1 };
    "#;
    let _ = ctx1.eval(init_code).unwrap();

    // 1. Serialize snapshot
    let snapshot_bytes = ctx1.create_snapshot();
    assert!(snapshot_bytes.len() > 100);
    assert_eq!(&snapshot_bytes[0..4], b"V8SN");

    // 2. Deserialize snapshot
    let mut ctx2 = Context::from_snapshot(&snapshot_bytes).expect("Deserialization failed");

    // 3. Verify state and execution on restored context
    let check_code = r#"
        baseValue == 100 && greeting == "Hello Snapshot" && config.active == true && config.version == 1;
    "#;
    let res = ctx2.eval(check_code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);

    // 4. Verify evaluation of new code and standard arithmetic
    let eval_res = ctx2.eval("let bonus = 50; baseValue + bonus;").unwrap();
    assert_eq!(JSValue::Smi(150), eval_res);
}

// ═══════════════════════════════════════════════════════════════════════════════
// PHASE 25: WebAssembly 128-Bit SIMD Extensions (v128)
// ═══════════════════════════════════════════════════════════════════════════════

fn make_simd_i32x4_wasm() -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(b"\0asm");
    m.extend_from_slice(&[1, 0, 0, 0]);

    // Type section (id=1): (i32) -> (i32)
    let type_body: &[u8] = &[0x01, 0x60, 0x01, 0x7F, 0x01, 0x7F];
    m.push(1); m.push(type_body.len() as u8); m.extend_from_slice(type_body);

    // Function section (id=3): func 0 -> type 0
    m.push(3); m.push(2); m.push(1); m.push(0);

    // Export section (id=7): "simd_add" -> func 0
    let exp: &[u8] = &[0x01, 0x08, b's', b'i', b'm', b'd', b'_', b'a', b'd', b'd', 0x00, 0x00];
    m.push(7); m.push(exp.len() as u8); m.extend_from_slice(exp);

    // Code section (id=10):
    // Takes arg0, splats to i32x4, duplicates, adds i32x4, extracts lane 0 -> 2 * arg0
    let body: &[u8] = &[
        0x00,             // 0 locals
        0x20, 0x00,       // local.get 0
        0xFD, 0x11,       // i32x4.splat
        0x20, 0x00,       // local.get 0
        0xFD, 0x11,       // i32x4.splat
        0xFD, 0x76,       // i32x4.add
        0xFD, 0x1B, 0x00, // i32x4.extract_lane 0
        0x0B,             // end
    ];
    let cs = { let mut v = vec![0x01u8]; v.push(body.len() as u8); v.extend_from_slice(body); v };
    m.push(10); m.push(cs.len() as u8); m.extend_from_slice(&cs);
    m
}

fn make_simd_f32x4_wasm() -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(b"\0asm");
    m.extend_from_slice(&[1, 0, 0, 0]);

    // Type section (id=1): (f32) -> (f32)
    let type_body: &[u8] = &[0x01, 0x60, 0x01, 0x7D, 0x01, 0x7D];
    m.push(1); m.push(type_body.len() as u8); m.extend_from_slice(type_body);

    // Function section (id=3): func 0 -> type 0
    m.push(3); m.push(2); m.push(1); m.push(0);

    // Export section (id=7): "simd_fmul" -> func 0
    let exp: &[u8] = &[0x01, 0x09, b's', b'i', b'm', b'd', b'_', b'f', b'm', b'u', b'l', 0x00, 0x00];
    m.push(7); m.push(exp.len() as u8); m.extend_from_slice(exp);

    // Code section (id=10):
    // Takes f32 arg0, splats to f32x4, multiplies by itself, extracts lane 0 -> arg0 * arg0
    let body: &[u8] = &[
        0x00,             // 0 locals
        0x20, 0x00,       // local.get 0
        0xFD, 0x13,       // f32x4.splat
        0x20, 0x00,       // local.get 0
        0xFD, 0x13,       // f32x4.splat
        0xFD, 0x96, 0x01, // f32x4.mul (150 = 0x96, 0x01 in LEB128)
        0xFD, 0x1F, 0x00, // f32x4.extract_lane 0
        0x0B,             // end
    ];
    let cs = { let mut v = vec![0x01u8]; v.push(body.len() as u8); v.extend_from_slice(body); v };
    m.push(10); m.push(cs.len() as u8); m.extend_from_slice(&cs);
    m
}

#[test]
fn test_phase25_wasm_simd_i32x4_add() {
    use v8_base_bits::wasm::binary_parser::parse;
    use v8_base_bits::wasm::instance::WasmInstance;
    use v8_base_bits::wasm::interpreter::{WasmInterpreter, WasmVal};

    let module = parse(&make_simd_i32x4_wasm()).unwrap();
    let mut instance = WasmInstance::instantiate(&module, vec![]);

    let results = WasmInterpreter::call_export(&module, &mut instance, "simd_add", vec![WasmVal::I32(21)]).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], WasmVal::I32(42));
}

#[test]
fn test_phase25_wasm_simd_f32x4_mul() {
    use v8_base_bits::wasm::binary_parser::parse;
    use v8_base_bits::wasm::instance::WasmInstance;
    use v8_base_bits::wasm::interpreter::{WasmInterpreter, WasmVal};

    let module = parse(&make_simd_f32x4_wasm()).unwrap();
    let mut instance = WasmInstance::instantiate(&module, vec![]);

    let results = WasmInterpreter::call_export(&module, &mut instance, "simd_fmul", vec![WasmVal::F32(3.5)]).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], WasmVal::F32(12.25));
}

// ═══════════════════════════════════════════════════════════════════════════════
// PHASE 26: Multi-Isolate Web Workers & SharedArrayBuffer with Atomics
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_phase26_shared_array_buffer_and_atomics() {
    let mut ctx = Context::new();
    let code = r#"
        let sab = new SharedArrayBuffer(16);
        let sabLen = sab.byteLength; // 16

        let ta = new Int32Array(sab);
        Atomics.store(ta, 0, 100);
        let v1 = Atomics.load(ta, 0); // 100

        let oldAdd = Atomics.add(ta, 0, 25); // 100, ta[0] is 125
        let v2 = Atomics.load(ta, 0); // 125

        let oldSub = Atomics.sub(ta, 0, 5); // 125, ta[0] is 120
        let v3 = Atomics.load(ta, 0); // 120

        let oldCas = Atomics.compareExchange(ta, 0, 120, 300); // 120, ta[0] is 300
        let v4 = Atomics.load(ta, 0); // 300

        let isLock = Atomics.isLockFree(4); // true
        let waitRes = Atomics.wait(ta, 0, 999, 10); // "not-equal"

        let sliced = sab.slice(0, 8);
        let sliceLen = sliced.byteLength; // 8

        sabLen == 16 && v1 == 100 && oldAdd == 100 && v2 == 125 && oldSub == 125 &&
        v3 == 120 && oldCas == 120 && v4 == 300 && isLock == true &&
        waitRes == "not-equal" && sliceLen == 8;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase26_multi_isolate_web_worker_thread_messaging() {
    let mut ctx = Context::new();
    let code = r#"
        let workerScript = "onmessage = function(msg) { postMessage('Echo: ' + msg); };";
        let w = new Worker(workerScript);

        w.postMessage("Hello Worker Thread");
        let reply = w.receiveMessage(1000);
        w.terminate();

        reply == "Echo: Hello Worker Thread";
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase26_shared_array_buffer_direct_indexing() {
    let mut ctx = Context::new();
    let code = r#"
        let sab = new SharedArrayBuffer(16);
        let ta = new Int32Array(sab);
        ta[0] = 77;
        ta[1] = 88;
        let sum = ta[0] + ta[1]; // 165
        let a0 = Atomics.load(ta, 0); // 77
        let a1 = Atomics.load(ta, 1); // 88
        sum == 165 && a0 == 77 && a1 == 88;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase27_arrow_functions() {
    let mut ctx = Context::new();
    let code = r#"
        let double = x => x * 2;
        let add = (a, b) => a + b;
        let getConstant = () => 42;
        let blockArrow = (x, y) => {
            let tmp = x * 10;
            return tmp + y;
        };
        double(5) == 10 && add(3, 7) == 10 && getConstant() == 42 && blockArrow(4, 5) == 45;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase27_default_and_rest_parameters() {
    let mut ctx = Context::new();
    let code = r#"
        function greet(name = "Guest", prefix = "Hello") {
            return prefix + " " + name;
        }
        function sumAll(base, ...numbers) {
            let total = base;
            for (let i = 0; i < numbers.length; i = i + 1) {
                total = total + numbers[i];
            }
            return total;
        }
        let g1 = greet();
        let g2 = greet("Alice", "Welcome");
        let s1 = sumAll(10, 1, 2, 3, 4);
        g1 == "Hello Guest" && g2 == "Welcome Alice" && s1 == 20;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase27_destructuring_assignment() {
    let mut ctx = Context::new();
    let code = r#"
        let [a, b, ...tail] = [10, 20, 30, 40, 50];
        let { x, y = 99 } = { x: 7 };
        let { name: heroName } = { name: "Batman" };
        a == 10 && b == 20 && tail.length == 3 && tail[0] == 30 && tail[2] == 50 &&
        x == 7 && y == 99 && heroName == "Batman";
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase27_template_literals() {
    let mut ctx = Context::new();
    let code = r#"
        let user = "World";
        let num = 42;
        let simple = `Hello`;
        let interpolated = `Hello ${user}, the answer is ${num}!`;
        simple == "Hello" && interpolated == "Hello World, the answer is 42!";
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase27_optional_chaining() {
    let mut ctx = Context::new();
    let code = r#"
        let obj = { a: { b: 123 } };
        let empty = undefined;
        let r1 = obj?.a?.b;
        let r2 = empty?.foo?.bar;
        let r3 = obj?.["a"]?.["b"];
        let r4 = empty?.["nested"];
        r1 == 123 && r2 === undefined && r3 == 123 && r4 === undefined;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase27_spread_operator() {
    let mut ctx = Context::new();
    let code = r#"
        let arr1 = [1, 2];
        let arr2 = [0, ...arr1, 3, 4];
        let obj1 = { a: 1, b: 2 };
        let obj2 = { ...obj1, c: 3 };
        function add3(x, y, z) {
            return x + y + z;
        }
        let callRes = add3(...[10, 20, 30]);
        arr2.length == 5 && arr2[0] == 0 && arr2[1] == 1 && arr2[2] == 2 && arr2[3] == 3 && arr2[4] == 4 &&
        obj2.a == 1 && obj2.b == 2 && obj2.c == 3 &&
        callRes == 60;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase27_logical_assignments() {
    let mut ctx = Context::new();
    let code = r#"
        let a = 1;
        a += 5;
        let b = 10;
        b -= 3;
        let c = null;
        c ??= 42;
        let d = 0;
        d ||= 99;
        let e = 100;
        e &&= 200;
        a == 6 && b == 7 && c == 42 && d == 99 && e == 200;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

// ═══════════════════════════════════════════════════════════════════════════════
// PHASE 28: BigInt Subsystem & 64-bit TypedArrays
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_phase28_bigint_literals_and_arithmetic() {
    let mut ctx = Context::new();
    let code = r#"
        let a = 100n;
        let b = 50n;
        let sum = a + b;       // 150n
        let diff = a - b;      // 50n
        let prod = a * b;      // 5000n
        let quot = a / b;      // 2n
        let rem = a % 30n;     // 10n
        let exp = 2n ** 10n;   // 1024n
        let neg = -a;          // -100n

        sum == 150n && diff == 50n && prod == 5000n && quot == 2n && rem == 10n && exp == 1024n && neg == -100n;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase28_bigint_bitwise_and_comparisons() {
    let mut ctx = Context::new();
    let code = r#"
        let a = 0b1100n; // 12n
        let b = 0b1010n; // 10n
        let and = a & b; // 8n (1000)
        let or = a | b;  // 14n (1110)
        let xor = a ^ b; // 6n (0110)
        let shl = 1n << 4n; // 16n
        let shr = 32n >> 2n; // 8n
        let not = ~0n; // -1n

        let t1 = typeof 42n === "bigint";
        let cmp1 = 10n < 20n;
        let cmp2 = 10n == 10;
        let cmp3 = 10n === 10; // false
        let cmp4 = 10n > 5;

        and == 8n && or == 14n && xor == 6n && shl == 16n && shr == 8n && not == -1n &&
        t1 && cmp1 && cmp2 && !cmp3 && cmp4;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase28_bigint_constructor_and_static_methods() {
    let mut ctx = Context::new();
    let code = r#"
        let b1 = BigInt(42);
        let b2 = BigInt("12345678901234567890");
        let b3 = BigInt(true);
        let b4 = BigInt(false);

        let clampedInt = BigInt.asIntN(8, 255n);   // -1n
        let clampedUint = BigInt.asUintN(8, 255n); // 255n
        let clampedUint2 = BigInt.asUintN(8, 256n); // 0n

        b1 == 42n && b2.toString() == "12345678901234567890" && b3 == 1n && b4 == 0n &&
        clampedInt == -1n && clampedUint == 255n && clampedUint2 == 0n;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase28_bigint_64bit_typed_arrays() {
    let mut ctx = Context::new();
    let code = r#"
        let i64arr = new BigInt64Array(4);
        i64arr[0] = 100n;
        i64arr[1] = -500n;
        i64arr[2] = 0x7FFFFFFFFFFFFFFFn;

        let u64arr = new BigUint64Array(4);
        u64arr[0] = 200n;
        u64arr[1] = 0xFFFFFFFFFFFFFFFFn;

        i64arr.length == 4 && i64arr[0] == 100n && i64arr[1] == -500n &&
        u64arr.length == 4 && u64arr[0] == 200n &&
        i64arr.byteLength == 32 && u64arr.byteLength == 32;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase29_generators_basic() {
    let mut ctx = Context::new();
    let code = r#"
        function* nums() {
            yield 1;
            yield 2;
            return 3;
        }
        let g = nums();
        let r1 = g.next();
        let r2 = g.next();
        let r3 = g.next();
        let r4 = g.next();

        r1.value === 1 && r1.done === false &&
        r2.value === 2 && r2.done === false &&
        r3.value === 3 && r3.done === true &&
        r4.value === undefined && r4.done === true;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase29_generators_input_passing() {
    let mut ctx = Context::new();
    let code = r#"
        function* adder() {
            let a = yield 10;
            let b = yield a + 5;
            return b * 2;
        }
        let g = adder();
        let r1 = g.next();
        let r2 = g.next(20);
        let r3 = g.next(7);

        r1.value === 10 && r1.done === false &&
        r2.value === 25 && r2.done === false &&
        r3.value === 14 && r3.done === true;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase29_iterators_and_for_of() {
    let mut ctx = Context::new();
    let code = r#"
        let arr = [10, 20, 30];
        let sum = 0;
        for (let x of arr) {
            sum += x;
        }

        let strSum = "";
        for (let c of "abc") {
            strSum += c;
        }

        function* range(n) {
            let i = 0;
            while (i < n) {
                yield i;
                i += 1;
            }
        }

        let genSum = 0;
        for (let v of range(4)) {
            genSum += v;
        }

        sum === 60 && strSum === "abc" && genSum === 6;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase29_generators_delegating_yield_star() {
    let mut ctx = Context::new();
    let code = r#"
        function* sub() {
            yield 10;
            yield 20;
        }
        function* main() {
            yield 1;
            yield* sub();
            yield* [30, 40];
            yield 2;
        }
        let res = [];
        for (let v of main()) {
            res.push(v);
        }
        res.length === 6 && res[0] === 1 && res[1] === 10 && res[2] === 20 && res[3] === 30 && res[4] === 40 && res[5] === 2;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase29_async_generators_and_for_await() {
    let mut ctx = Context::new();
    let code = r#"
        async function* asyncNums() {
            yield 100;
            yield 200;
            return 300;
        }
        let g = asyncNums();
        let p1 = g.next();
        let p2 = g.next();
        p1 !== undefined && p2 !== undefined;
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Boolean(true), res);
}

#[test]
fn test_phase30_esm_named_and_default_exports() {
    let mut ctx = Context::new();
    let math_code = r#"
        export const PI = 3.14;
        export function square(x) {
            return x * x;
        }
        export default 42;
    "#;
    ctx.eval_module("math.js", math_code).unwrap();

    let main_code = r#"
        import defVal, { PI, square } from "math.js";
        export const area = PI * square(2);
        export const ans = defVal;
    "#;
    let res = ctx.eval_module("main.js", main_code).unwrap();
    if let JSValue::Object(ref ns) = res {
        let area = ns.borrow().get_property("area");
        let ans = ns.borrow().get_property("ans");
        assert_eq!(JSValue::Number(12.56), area);
        assert_eq!(JSValue::Smi(42), ans);
    } else {
        panic!("Expected Module Namespace Object, got {:?}", res);
    }
}

#[test]
fn test_phase30_esm_star_reexport() {
    let mut ctx = Context::new();
    let a_code = r#"
        export const x = 10;
        export const y = 20;
    "#;
    ctx.eval_module("a.js", a_code).unwrap();

    let b_code = r#"
        export * from "a.js";
        export const z = 30;
    "#;
    let res = ctx.eval_module("b.js", b_code).unwrap();
    if let JSValue::Object(ref ns) = res {
        let x = ns.borrow().get_property("x");
        let y = ns.borrow().get_property("y");
        let z = ns.borrow().get_property("z");
        assert_eq!(JSValue::Smi(10), x);
        assert_eq!(JSValue::Smi(20), y);
        assert_eq!(JSValue::Smi(30), z);
    } else {
        panic!("Expected Module Namespace Object, got {:?}", res);
    }
}

#[test]
fn test_phase30_esm_dynamic_import() {
    let mut ctx = Context::new();
    let helper_code = r#"
        export const msg = "hello from dynamic";
        export default 999;
    "#;
    ctx.eval_module("helper.js", helper_code).unwrap();

    let script = r#"
        let p = import("helper.js");
        let resolvedVal = null;
        p.then(ns => {
            resolvedVal = ns.msg;
        });
        resolvedVal;
    "#;
    let _ = ctx.eval(script).unwrap();
    ctx.run_microtasks();

    let check = ctx.eval("resolvedVal").unwrap();
    assert_eq!(JSValue::String("hello from dynamic".to_string()), check);
}

#[test]
fn test_phase31_host_timers_and_event_loop() {
    let mut ctx = Context::new();
    let script = r#"
        let logs = [];
        let t1 = setTimeout((arg) => {
            logs.push("timeout 1: " + arg);
            Promise.resolve().then(() => {
                logs.push("microtask inside t1");
            });
        }, 10, "first");

        let t2 = setTimeout(() => {
            logs.push("cancelled");
        }, 5);
        clearTimeout(t2);

        let count = 0;
        let iv = setInterval(() => {
            count++;
            logs.push("interval: " + count);
            if (count === 3) {
                clearInterval(iv);
            }
        }, 20);

        let t3 = setTimeout(() => {
            logs.push("timeout 3");
        }, 30);
    "#;
    ctx.eval(script).unwrap();

    // Advance virtual timers to 100ms
    ctx.advance_timers(100);

    let logs_val = ctx.eval("logs.join(' | ')").unwrap();
    let logs_str = logs_val.to_string_val();

    // Verify order: t1 (10ms) -> microtask inside t1 -> iv(20ms: 1) -> t3(30ms) -> iv(40ms: 2) -> iv(60ms: 3)
    assert!(logs_str.contains("timeout 1: first"));
    assert!(logs_str.contains("microtask inside t1"));
    assert!(!logs_str.contains("cancelled"));
    assert!(logs_str.contains("timeout 3"));
    assert!(logs_str.contains("interval: 1"));
    assert!(logs_str.contains("interval: 2"));
    assert!(logs_str.contains("interval: 3"));
}

#[test]
fn test_phase31_text_encoding() {
    let mut ctx = Context::new();
    let script = r#"
        let encoder = new TextEncoder();
        let encName = encoder.encoding;
        let bytes = encoder.encode("Hello Safe Rust V8! 🦀");
        let bytesLen = bytes.length;

        let decoder = new TextDecoder("utf-8");
        let decName = decoder.encoding;
        let restored = decoder.decode(bytes);

        let dest = new Uint8Array(50);
        let res = encoder.encodeInto("abc", dest);
        let readCount = res.read;
        let writtenCount = res.written;

        ({ encName, bytesLen, decName, restored, readCount, writtenCount })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("utf-8".to_string()), o.borrow().get_property("encName"));
        assert_eq!(JSValue::String("utf-8".to_string()), o.borrow().get_property("decName"));
        assert_eq!(
            JSValue::String("Hello Safe Rust V8! 🦀".to_string()),
            o.borrow().get_property("restored")
        );
        assert_eq!(JSValue::Smi(3), o.borrow().get_property("readCount"));
        assert_eq!(JSValue::Smi(3), o.borrow().get_property("writtenCount"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase31_url_and_search_params() {
    let mut ctx = Context::new();
    let script = r#"
        let u = new URL("https://user:pass@example.com:8080/api/v1/test?query=alpha&lang=rust#header");
        let proto = u.protocol;
        let host = u.host;
        let hostname = u.hostname;
        let port = u.port;
        let pathname = u.pathname;
        let search = u.search;
        let hash = u.hash;
        let origin = u.origin;

        let qVal = u.searchParams.get("query");
        let hasLang = u.searchParams.has("lang");

        u.searchParams.append("flag", "true");
        let updatedSearch = u.search;

        let enc = encodeURIComponent("hello world & safe rust");
        let dec = decodeURIComponent(enc);

        ({ proto, host, hostname, port, pathname, search, hash, origin, qVal, hasLang, updatedSearch, enc, dec })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("https:".to_string()), o.borrow().get_property("proto"));
        assert_eq!(JSValue::String("example.com:8080".to_string()), o.borrow().get_property("host"));
        assert_eq!(JSValue::String("example.com".to_string()), o.borrow().get_property("hostname"));
        assert_eq!(JSValue::String("8080".to_string()), o.borrow().get_property("port"));
        assert_eq!(JSValue::String("/api/v1/test".to_string()), o.borrow().get_property("pathname"));
        assert_eq!(JSValue::String("?query=alpha&lang=rust".to_string()), o.borrow().get_property("search"));
        assert_eq!(JSValue::String("#header".to_string()), o.borrow().get_property("hash"));
        assert_eq!(JSValue::String("https://example.com:8080".to_string()), o.borrow().get_property("origin"));
        assert_eq!(JSValue::String("alpha".to_string()), o.borrow().get_property("qVal"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasLang"));
        assert!(o.borrow().get_property("updatedSearch").to_string_val().contains("flag=true"));
        assert_eq!(
            JSValue::String("hello%20world%20%26%20safe%20rust".to_string()),
            o.borrow().get_property("enc")
        );
        assert_eq!(
            JSValue::String("hello world & safe rust".to_string()),
            o.borrow().get_property("dec")
        );
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase31_crypto_api() {
    let mut ctx = Context::new();
    let script = r#"
        let uuid1 = crypto.randomUUID();
        let uuid2 = crypto.randomUUID();
        let diff = (uuid1 !== uuid2);
        let len = uuid1.length;
        let v4Char = uuid1.charAt(14); // 8-4-(4)-4-12 -> char 14 must be '4'

        let arr = new Uint8Array(16);
        crypto.getRandomValues(arr);
        let sum = 0;
        for (let i = 0; i < arr.length; i++) {
            sum += arr[i];
        }
        let hasEntropy = (sum > 0);

        ({ diff, len, v4Char, hasEntropy })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("diff"));
        assert_eq!(JSValue::Smi(36), o.borrow().get_property("len"));
        assert_eq!(JSValue::String("4".to_string()), o.borrow().get_property("v4Char"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasEntropy"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase31_modern_array_and_object_builtins() {
    let mut ctx = Context::new();
    let script = r#"
        // 1. Array.prototype toReversed, toSorted, toSpliced, with
        let original = [3, 1, 4, 2];
        let reversed = original.toReversed();
        let sorted = original.toSorted((a, b) => a - b);
        let spliced = original.toSpliced(1, 2, 99, 100);
        let replaced = original.with(2, 42);
        let origUnchanged = (original[0] === 3 && original[1] === 1 && original.length === 4);

        // 2. TypedArray toReversed, toSorted, with
        let ta = new Uint8Array([5, 2, 8, 1]);
        let taRev = ta.toReversed();
        let taSort = ta.toSorted((a, b) => a - b);
        let taWith = ta.with(1, 99);

        // 3. Object.groupBy
        let inventory = [
            { name: "asparagus", type: "vegetables" },
            { name: "bananas", type: "fruit" },
            { name: "goat", type: "meat" },
            { name: "cherries", type: "fruit" },
        ];
        let objGrouped = Object.groupBy(inventory, item => item.type);
        let fruitCount = objGrouped.fruit.length;
        let vegCount = objGrouped.vegetables.length;

        // 4. Map.groupBy
        let mapGrouped = Map.groupBy(inventory, item => item.type);
        let mapFruitCount = mapGrouped.get("fruit").length;

        // 5. Promise.withResolvers
        let { promise, resolve, reject } = Promise.withResolvers();
        let promiseResult = null;
        promise.then(val => {
            promiseResult = val;
        });
        resolve("success withResolvers");

        ({
            reversed: reversed.join(","),
            sorted: sorted.join(","),
            spliced: spliced.join(","),
            replaced: replaced.join(","),
            origUnchanged,
            taRev: taRev.join(","),
            taSort: taSort.join(","),
            taWith: taWith.join(","),
            fruitCount,
            vegCount,
            mapFruitCount
        })
    "#;
    let res = ctx.eval(script).unwrap();
    ctx.run_microtasks();

    let check_promise = ctx.eval("promiseResult").unwrap();
    assert_eq!(JSValue::String("success withResolvers".to_string()), check_promise);

    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("2,4,1,3".to_string()), o.borrow().get_property("reversed"));
        assert_eq!(JSValue::String("1,2,3,4".to_string()), o.borrow().get_property("sorted"));
        assert_eq!(JSValue::String("3,99,100,2".to_string()), o.borrow().get_property("spliced"));
        assert_eq!(JSValue::String("3,1,42,2".to_string()), o.borrow().get_property("replaced"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("origUnchanged"));

        assert_eq!(JSValue::String("1,8,2,5".to_string()), o.borrow().get_property("taRev"));
        assert_eq!(JSValue::String("1,2,5,8".to_string()), o.borrow().get_property("taSort"));
        assert_eq!(JSValue::String("5,99,8,1".to_string()), o.borrow().get_property("taWith"));

        assert_eq!(JSValue::Smi(2), o.borrow().get_property("fruitCount"));
        assert_eq!(JSValue::Smi(1), o.borrow().get_property("vegCount"));
        assert_eq!(JSValue::Smi(2), o.borrow().get_property("mapFruitCount"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase31_weak_references() {
    let mut ctx = Context::new();
    let script = r#"
        let target = { id: 123 };
        let wr = new WeakRef(target);
        let derefVal = wr.deref();
        let sameTarget = (derefVal.id === 123);

        let registry = new FinalizationRegistry(held => {});
        let token = {};
        registry.register(target, "heldData", token);
        let unregSuccess = registry.unregister(token);

        ({ sameTarget, unregSuccess })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("sameTarget"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("unregSuccess"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase32_private_fields_and_methods() {
    let mut ctx = Context::new();
    let script = r#"
        class Account {
            #balance = 1000;

            #calculateInterest(rate) {
                return this.#balance * rate;
            }

            deposit(amount) {
                this.#balance += amount;
                return this.#balance;
            }

            getBalanceWithInterest(rate) {
                let interest = this.#calculateInterest(rate);
                return this.#balance + interest;
            }

            hasBalance(obj) {
                return #balance in obj;
            }
        }

        let acc = new Account();
        let dep = acc.deposit(500);
        let total = acc.getBalanceWithInterest(0.10);
        let hasAcc = acc.hasBalance(acc);
        let hasOther = acc.hasBalance({});

        ({ dep, total, hasAcc, hasOther })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(1500), o.borrow().get_property("dep"));
        assert_eq!(JSValue::Number(1650.0), o.borrow().get_property("total"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasAcc"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("hasOther"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }

    // Verify syntax error on private access outside enclosing class
    let err_script = "let o = {}; o.#unknown;";
    assert!(ctx.eval(err_script).is_err());
}

#[test]
fn test_phase32_class_static_blocks_and_initializers() {
    let mut ctx = Context::new();
    let script = r#"
        class Config {
            static defaultPort = 8080;
            static #secretKey = 42;
            static combined = 0;
            static log = [];

            static {
                this.combined = this.defaultPort + this.#secretKey;
                this.log.push("block1");
            }

            static {
                this.log.push("block2");
            }

            instanceVal = 100;
            #privateVal = 25;

            getSum() {
                return this.instanceVal + this.#privateVal;
            }
        }

        let cfg = new Config();
        let port = Config.defaultPort;
        let combined = Config.combined;
        let block1 = Config.log[0];
        let block2 = Config.log[1];
        let instanceSum = cfg.getSum();

        ({ port, combined, block1, block2, instanceSum })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(8080), o.borrow().get_property("port"));
        assert_eq!(JSValue::Smi(8122), o.borrow().get_property("combined"));
        assert_eq!(JSValue::String("block1".to_string()), o.borrow().get_property("block1"));
        assert_eq!(JSValue::String("block2".to_string()), o.borrow().get_property("block2"));
        assert_eq!(JSValue::Smi(125), o.borrow().get_property("instanceSum"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase32_wasm_reference_types_and_tail_calls() {
    use v8_base_bits::wasm::binary_parser::parse;
    use v8_base_bits::wasm::instance::WasmInstance;
    use v8_base_bits::wasm::interpreter::{WasmInterpreter, WasmVal};

    // 1. Reference Types: ref.null, ref.is_null, ref.func
    let mut ref_module_bytes = Vec::new();
    ref_module_bytes.extend_from_slice(b"\0asm\x01\x00\x00\x00");
    // Type section: (empty) -> (i32)
    let type_body: &[u8] = &[0x01, 0x60, 0x00, 0x01, 0x7F];
    ref_module_bytes.push(1); ref_module_bytes.push(type_body.len() as u8); ref_module_bytes.extend_from_slice(type_body);
    // Function section: 2 functions of type 0
    ref_module_bytes.push(3); ref_module_bytes.push(3); ref_module_bytes.push(2); ref_module_bytes.push(0); ref_module_bytes.push(0);
    // Export section: "test_ref_null" -> func 0, "test_ref_func" -> func 1
    let exp: &[u8] = &[
        0x02,
        0x0D, b't', b'e', b's', b't', b'_', b'r', b'e', b'f', b'_', b'n', b'u', b'l', b'l', 0x00, 0x00,
        0x0D, b't', b'e', b's', b't', b'_', b'r', b'e', b'f', b'_', b'f', b'u', b'n', b'c', 0x00, 0x01,
    ];
    ref_module_bytes.push(7); ref_module_bytes.push(exp.len() as u8); ref_module_bytes.extend_from_slice(exp);
    // Code section:
    // func 0: 0 locals, 0xD0 0x6F, 0xD1, 0x0B (5 bytes)
    let f0_body: &[u8] = &[0x00, 0xD0, 0x6F, 0xD1, 0x0B];
    let f1_body: &[u8] = &[0x00, 0xD2, 0x00, 0xD1, 0x0B];
    let mut code_body = vec![0x02u8];
    code_body.push(f0_body.len() as u8);
    code_body.extend_from_slice(f0_body);
    code_body.push(f1_body.len() as u8);
    code_body.extend_from_slice(f1_body);
    ref_module_bytes.push(10); ref_module_bytes.push(code_body.len() as u8); ref_module_bytes.extend_from_slice(&code_body);

    let module1 = parse(&ref_module_bytes).unwrap();
    let mut inst1 = WasmInstance::instantiate(&module1, vec![]);

    let r_null = WasmInterpreter::call_export(&module1, &mut inst1, "test_ref_null", vec![]).unwrap();
    assert_eq!(WasmVal::I32(1), r_null[0]);

    let r_func = WasmInterpreter::call_export(&module1, &mut inst1, "test_ref_func", vec![]).unwrap();
    assert_eq!(WasmVal::I32(0), r_func[0]);

    // 2. Tail Calls: countdown(n, acc) -> acc + n via return_call 0
    let mut tc_bytes = Vec::new();
    tc_bytes.extend_from_slice(b"\0asm\x01\x00\x00\x00");
    // Type section: (i32, i32) -> (i32)
    let tc_type: &[u8] = &[0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F];
    tc_bytes.push(1); tc_bytes.push(tc_type.len() as u8); tc_bytes.extend_from_slice(tc_type);
    // Function section: func 0 -> type 0
    tc_bytes.push(3); tc_bytes.push(2); tc_bytes.push(1); tc_bytes.push(0);
    // Export section: "countdown" -> func 0
    let tc_exp: &[u8] = &[0x01, 0x09, b'c', b'o', b'u', b'n', b't', b'd', b'o', b'w', b'n', 0x00, 0x00];
    tc_bytes.push(7); tc_bytes.push(tc_exp.len() as u8); tc_bytes.extend_from_slice(tc_exp);
    // Code section:
    // func 0: 0 locals
    // local.get 0 (0x20 0x00); i32.eqz (0x45); if 0x7F (0x04 0x7F); local.get 1 (0x20 0x01);
    // else (0x05); local.get 0 (0x20 0x00); i32.const 1 (0x41 0x01); i32.sub (0x6B);
    // local.get 1 (0x20 0x01); i32.const 1 (0x41 0x01); i32.add (0x6A); return_call 0 (0x12 0x00);
    // end (0x0B); end (0x0B);
    let tc_code: &[u8] = &[
        0x00,
        0x20, 0x00, 0x45, 0x04, 0x7F, 0x20, 0x01,
        0x05, 0x20, 0x00, 0x41, 0x01, 0x6B, 0x20, 0x01, 0x41, 0x01, 0x6A, 0x12, 0x00,
        0x0B, 0x0B,
    ];
    let tc_cs = { let mut v = vec![0x01u8]; v.push(tc_code.len() as u8); v.extend_from_slice(tc_code); v };
    tc_bytes.push(10); tc_bytes.push(tc_cs.len() as u8); tc_bytes.extend_from_slice(&tc_cs);

    let module2 = parse(&tc_bytes).unwrap();
    let mut inst2 = WasmInstance::instantiate(&module2, vec![]);
    let tc_res = WasmInterpreter::call_export(&module2, &mut inst2, "countdown", vec![WasmVal::I32(50), WasmVal::I32(0)]).unwrap();
    assert_eq!(WasmVal::I32(50), tc_res[0]);

    // 3. Multi-Memory: 2 memories with size and grow
    let mut mm_bytes = Vec::new();
    mm_bytes.extend_from_slice(b"\0asm\x01\x00\x00\x00");
    // Type section: (i32) -> (i32)
    let mm_type: &[u8] = &[0x01, 0x60, 0x01, 0x7F, 0x01, 0x7F];
    mm_bytes.push(1); mm_bytes.push(mm_type.len() as u8); mm_bytes.extend_from_slice(mm_type);
    // Function section: func 0 -> type 0 (mem_size), func 1 -> type 0 (mem_grow1)
    mm_bytes.push(3); mm_bytes.push(3); mm_bytes.push(2); mm_bytes.push(0); mm_bytes.push(0);
    // Memory section (id=5): 2 memories (mem 0: min 1; mem 1: min 2)
    let mem_sec: &[u8] = &[0x02, 0x00, 0x01, 0x00, 0x02];
    mm_bytes.push(5); mm_bytes.push(mem_sec.len() as u8); mm_bytes.extend_from_slice(mem_sec);
    // Export section: "mem_size" -> func 0, "grow_mem1" -> func 1
    let mm_exp: &[u8] = &[
        0x02,
        0x08, b'm', b'e', b'm', b'_', b's', b'i', b'z', b'e', 0x00, 0x00,
        0x09, b'g', b'r', b'o', b'w', b'_', b'm', b'e', b'm', b'1', 0x00, 0x01,
    ];
    mm_bytes.push(7); mm_bytes.push(mm_exp.len() as u8); mm_bytes.extend_from_slice(mm_exp);
    // Code section:
    // func 0: local.get 0; if 0x7F; memory.size 1 (0x3F 0x01); else; memory.size 0 (0x3F 0x00); end; end;
    let f0_code: &[u8] = &[0x00, 0x20, 0x00, 0x04, 0x7F, 0x3F, 0x01, 0x05, 0x3F, 0x00, 0x0B, 0x0B];
    // func 1: local.get 0; memory.grow 1 (0x40 0x01); end;
    let f1_code: &[u8] = &[0x00, 0x20, 0x00, 0x40, 0x01, 0x0B];
    let mm_cs = {
        let mut v = vec![0x02u8];
        v.push(f0_code.len() as u8); v.extend_from_slice(f0_code);
        v.push(f1_code.len() as u8); v.extend_from_slice(f1_code);
        v
    };
    mm_bytes.push(10); mm_bytes.push(mm_cs.len() as u8); mm_bytes.extend_from_slice(&mm_cs);

    let module3 = parse(&mm_bytes).unwrap();
    let mut inst3 = WasmInstance::instantiate(&module3, vec![]);

    let s0 = WasmInterpreter::call_export(&module3, &mut inst3, "mem_size", vec![WasmVal::I32(0)]).unwrap();
    assert_eq!(WasmVal::I32(1), s0[0]);

    let s1 = WasmInterpreter::call_export(&module3, &mut inst3, "mem_size", vec![WasmVal::I32(1)]).unwrap();
    assert_eq!(WasmVal::I32(2), s1[0]);

    let old_s1 = WasmInterpreter::call_export(&module3, &mut inst3, "grow_mem1", vec![WasmVal::I32(3)]).unwrap();
    assert_eq!(WasmVal::I32(2), old_s1[0]);

    let new_s1 = WasmInterpreter::call_export(&module3, &mut inst3, "mem_size", vec![WasmVal::I32(1)]).unwrap();
    assert_eq!(WasmVal::I32(5), new_s1[0]);
}

// ═══════════════════════════════════════════════════════════════════════════════
// PHASE 33: Fundamental Constructors & Native Error Hierarchy
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_phase33_error_hierarchy_and_aggregate_error() {
    let mut ctx = Context::new();
    let script = r#"
        let baseErr = new Error("base message", { cause: 404 });
        let baseStr = baseErr.toString();
        let baseCause = baseErr.cause;

        let typeErr = new TypeError("type invalid");
        let rangeErr = new RangeError("out of range");
        let syntaxErr = new SyntaxError("syntax issue");

        let isTypeAnError = (typeErr instanceof Error);
        let isRangeAnError = (rangeErr instanceof Error);

        let sub1 = new TypeError("err1");
        let sub2 = new RangeError("err2");
        let agg = new AggregateError([sub1, sub2], "Combined error");
        let aggCount = agg.errors.length;
        let aggFirstMsg = agg.errors[0].message;
        let aggSecondMsg = agg.errors[1].message;
        let isAggAnError = (agg instanceof Error);

        ({
            baseStr,
            baseCause,
            typeStr: typeErr.toString(),
            rangeStr: rangeErr.toString(),
            syntaxStr: syntaxErr.toString(),
            isTypeAnError,
            isRangeAnError,
            aggCount,
            aggFirstMsg,
            aggSecondMsg,
            isAggAnError
        })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("Error: base message".to_string()), o.borrow().get_property("baseStr"));
        assert_eq!(JSValue::Smi(404), o.borrow().get_property("baseCause"));
        assert_eq!(JSValue::String("TypeError: type invalid".to_string()), o.borrow().get_property("typeStr"));
        assert_eq!(JSValue::String("RangeError: out of range".to_string()), o.borrow().get_property("rangeStr"));
        assert_eq!(JSValue::String("SyntaxError: syntax issue".to_string()), o.borrow().get_property("syntaxStr"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isTypeAnError"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isRangeAnError"));
        assert_eq!(JSValue::Smi(2), o.borrow().get_property("aggCount"));
        assert_eq!(JSValue::String("err1".to_string()), o.borrow().get_property("aggFirstMsg"));
        assert_eq!(JSValue::String("err2".to_string()), o.borrow().get_property("aggSecondMsg"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isAggAnError"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase33_number_and_boolean_constructors() {
    let mut ctx = Context::new();
    let script = r#"
        let maxSafe = Number.MAX_SAFE_INTEGER;
        let minSafe = Number.MIN_SAFE_INTEGER;
        let isInt1 = Number.isInteger(42);
        let isInt2 = Number.isInteger(42.5);
        let isSafe1 = Number.isSafeInteger(9007199254740991);
        let isSafe2 = Number.isSafeInteger(9007199254740992);
        let isNan1 = Number.isNaN(NaN);
        let isNan2 = Number.isNaN("hello"); // strict, unlike global isNaN
        let isFin1 = Number.isFinite(100);
        let isFin2 = Number.isFinite(Infinity);

        let fixed = (123.456).toFixed(2);
        let hex = (255).toString(16);

        let bTrue = Boolean(1);
        let bFalse = Boolean(0);
        let bStr = Boolean("hello");
        let bEmpty = Boolean("");

        ({
            maxSafe,
            minSafe,
            isInt1,
            isInt2,
            isSafe1,
            isSafe2,
            isNan1,
            isNan2,
            isFin1,
            isFin2,
            fixed,
            hex,
            bTrue,
            bFalse,
            bStr,
            bEmpty
        })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Number(9007199254740991.0), o.borrow().get_property("maxSafe"));
        assert_eq!(JSValue::Number(-9007199254740991.0), o.borrow().get_property("minSafe"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isInt1"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("isInt2"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isSafe1"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("isSafe2"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isNan1"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("isNan2"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isFin1"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("isFin2"));
        assert_eq!(JSValue::String("123.46".to_string()), o.borrow().get_property("fixed"));
        assert_eq!(JSValue::String("ff".to_string()), o.borrow().get_property("hex"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("bTrue"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("bFalse"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("bStr"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("bEmpty"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase33_function_prototype_call_apply_bind_and_constructor() {
    let mut ctx = Context::new();
    let script = r#"
        function greet(prefix, suffix) {
            return prefix + " " + this.name + suffix;
        }

        let user = { name: "Alice" };
        let callRes = greet.call(user, "Hello", "!");
        let applyRes = greet.apply(user, ["Hi", "!!"]);

        let boundGreet = greet.bind(user, "Greetings");
        let bindRes = boundGreet("???");

        let dynamicAdd = new Function("x", "y", "return x + y;");
        let dynSum = dynamicAdd(15, 25);

        ({ callRes, applyRes, bindRes, dynSum })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("Hello Alice!".to_string()), o.borrow().get_property("callRes"));
        assert_eq!(JSValue::String("Hi Alice!!".to_string()), o.borrow().get_property("applyRes"));
        assert_eq!(JSValue::String("Greetings Alice???".to_string()), o.borrow().get_property("bindRes"));
        assert_eq!(JSValue::Smi(40), o.borrow().get_property("dynSum"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase34_math_transcendental_and_helpers() {
    let mut ctx = Context::new();
    let script = r#"
        let rnd = Math.random();
        let rndValid = rnd >= 0 && rnd < 1;
        let s0 = Math.sin(0);
        let c0 = Math.cos(0);
        let t0 = Math.tan(0);
        let l1 = Math.log(1);
        let e0 = Math.exp(0);
        let hyp = Math.hypot(3, 4);
        let im = Math.imul(0xffffffff, 5);
        let clz = Math.clz32(1);
        let cb = Math.cbrt(27);

        ({ rndValid, s0, c0, t0, l1, e0, hyp, im, clz, cb })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("rndValid"));
        assert_eq!(JSValue::Number(0.0), o.borrow().get_property("s0"));
        assert_eq!(JSValue::Number(1.0), o.borrow().get_property("c0"));
        assert_eq!(JSValue::Number(0.0), o.borrow().get_property("t0"));
        assert_eq!(JSValue::Number(0.0), o.borrow().get_property("l1"));
        assert_eq!(JSValue::Number(1.0), o.borrow().get_property("e0"));
        assert_eq!(JSValue::Number(5.0), o.borrow().get_property("hyp"));
        assert_eq!(JSValue::Smi(-5), o.borrow().get_property("im"));
        assert_eq!(JSValue::Smi(31), o.borrow().get_property("clz"));
        assert_eq!(JSValue::Number(3.0), o.borrow().get_property("cb"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase34_object_methods_and_prototype() {
    let mut ctx = Context::new();
    let script = r#"
        let obj = { a: 1, b: 2 };
        let hasA = Object.hasOwn(obj, "a");
        let hasZ = Object.hasOwn(obj, "z");

        let sameZero = Object.is(0, -0);
        let sameNan = Object.is(NaN, NaN);
        let sameVal = Object.is(42, 42);

        let entries = [["foo", "bar"], ["baz", 100]];
        let fromEnt = Object.fromEntries(entries);

        let protoObj = { inherited: true };
        let child = Object.create(protoObj);
        child.own = "yes";

        let hasOwnChild = child.hasOwnProperty("own");
        let hasInherited = child.hasOwnProperty("inherited");
        let objProto = Object.getPrototypeOf(child);

        let frozen = Object.freeze(obj);
        let isFr = Object.isFrozen(frozen);

        let desc = Object.getOwnPropertyDescriptor(obj, "a");

        ({
            hasA, hasZ, sameZero, sameNan, sameVal,
            fromEntFoo: fromEnt.foo, fromEntBaz: fromEnt.baz,
            hasOwnChild, hasInherited,
            isFr, descVal: desc.value
        })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasA"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("hasZ"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("sameZero"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("sameNan"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("sameVal"));
        assert_eq!(JSValue::String("bar".to_string()), o.borrow().get_property("fromEntFoo"));
        assert_eq!(JSValue::Smi(100), o.borrow().get_property("fromEntBaz"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasOwnChild"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("hasInherited"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("isFr"));
        assert_eq!(JSValue::Smi(1), o.borrow().get_property("descVal"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase34_array_modern_methods() {
    let mut ctx = Context::new();
    let script = r#"
        let arr = [10, 20, 30, 40, 50];
        let atPos = arr.at(1);
        let atNeg = arr.at(-1);
        let atOutOfBounds = arr.at(99);

        let flatMapped = [1, 2, 3].flatMap(function(x) {
            return [x, x * 10];
        });

        let numbers = [5, 12, 50, 130, 44];
        let lastBig = numbers.findLast(function(n) { return n > 45; });
        let lastBigIdx = numbers.findLastIndex(function(n) { return n > 45; });

        let copyArr = [1, 2, 3, 4, 5];
        copyArr.copyWithin(0, 3, 4);

        ({
            atPos, atNeg, atOutOfBounds,
            flat0: flatMapped[0], flat1: flatMapped[1], flat2: flatMapped[2], flat3: flatMapped[3],
            lastBig, lastBigIdx,
            cp0: copyArr[0], cp1: copyArr[1], cp2: copyArr[2]
        })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(20), o.borrow().get_property("atPos"));
        assert_eq!(JSValue::Smi(50), o.borrow().get_property("atNeg"));
        assert_eq!(JSValue::Undefined, o.borrow().get_property("atOutOfBounds"));
        assert_eq!(JSValue::Smi(1), o.borrow().get_property("flat0"));
        assert_eq!(JSValue::Smi(10), o.borrow().get_property("flat1"));
        assert_eq!(JSValue::Smi(2), o.borrow().get_property("flat2"));
        assert_eq!(JSValue::Smi(20), o.borrow().get_property("flat3"));
        assert_eq!(JSValue::Smi(130), o.borrow().get_property("lastBig"));
        assert_eq!(JSValue::Smi(3), o.borrow().get_property("lastBigIdx"));
        assert_eq!(JSValue::Smi(4), o.borrow().get_property("cp0"));
        assert_eq!(JSValue::Smi(2), o.borrow().get_property("cp1"));
        assert_eq!(JSValue::Smi(3), o.borrow().get_property("cp2"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase34_string_modern_methods() {
    let mut ctx = Context::new();
    let script = r#"
        let str = "Hello World";
        let atPos = str.at(1);
        let atNeg = str.at(-1);

        let wellFormed = str.isWellFormed();
        let cleaned = str.toWellFormed();

        let codePointStr = String.fromCodePoint(65, 66, 67, 0x1f600);

        let rawStr = String.raw({ raw: ["Hello ", " World ", "!"] }, "JavaScript", 2026);

        ({ atPos, atNeg, wellFormed, cleaned, codePointStr, rawStr })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("e".to_string()), o.borrow().get_property("atPos"));
        assert_eq!(JSValue::String("d".to_string()), o.borrow().get_property("atNeg"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("wellFormed"));
        assert_eq!(JSValue::String("Hello World".to_string()), o.borrow().get_property("cleaned"));
        assert_eq!(JSValue::String("ABC😀".to_string()), o.borrow().get_property("codePointStr"));
        assert_eq!(JSValue::String("Hello JavaScript World 2026!".to_string()), o.borrow().get_property("rawStr"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase35_structured_clone() {
    let mut ctx = Context::new();
    let script = r#"
        // 1. Primitive cloning
        let num = structuredClone(42);
        let str = structuredClone("v8_rust");
        let boolVal = structuredClone(true);

        // 2. Deep object cloning with mutation isolation
        let origObj = { x: 10, inner: { y: 20 } };
        let cloneObj = structuredClone(origObj);
        cloneObj.x = 99;
        cloneObj.inner.y = 88;
        let origX = origObj.x;
        let origY = origObj.inner.y;

        // 3. Array cloning
        let origArr = [1, [2, 3], 4];
        let cloneArr = structuredClone(origArr);
        cloneArr[1][0] = 77;
        let origArrElem = origArr[1][0];

        // 4. Circular graph cloning
        let cycleObj = { name: "cycle" };
        cycleObj.self = cycleObj;
        let cloneCycle = structuredClone(cycleObj);
        let isCycle = (cloneCycle.self === cloneCycle) && (cloneCycle !== cycleObj);

        // 5. Date cloning
        let origDate = new Date(1600000000000);
        let cloneDate = structuredClone(origDate);
        let dateMatch = (cloneDate.getTime() === 1600000000000) && (cloneDate !== origDate);

        // 6. Function cloning error
        let funcErr = false;
        try {
            structuredClone(function() { return 1; });
        } catch (e) {
            funcErr = true;
        }

        ({
            num, str, boolVal,
            origX, origY, cloneX: cloneObj.x, cloneY: cloneObj.inner.y,
            origArrElem, cloneArrElem: cloneArr[1][0],
            isCycle, dateMatch, funcErr
        })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(42), o.borrow().get_property("num"));
        assert_eq!(JSValue::String("v8_rust".to_string()), o.borrow().get_property("str"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("boolVal"));
        assert_eq!(JSValue::Smi(10), o.borrow().get_property("origX"));
        assert_eq!(JSValue::Smi(20), o.borrow().get_property("origY"));
        assert_eq!(JSValue::Smi(99), o.borrow().get_property("cloneX"));
        assert_eq!(JSValue::Smi(88), o.borrow().get_property("cloneY"));
        assert_eq!(JSValue::Smi(2), o.borrow().get_property("origArrElem"));
        assert_eq!(JSValue::Smi(77), o.borrow().get_property("cloneArrElem"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isCycle"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("dateMatch"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("funcErr"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase35_base64_btoa_atob() {
    let mut ctx = Context::new();
    let script = r#"
        let encoded = btoa("Hello, world!");
        let decoded = atob(encoded);

        let emptyEnc = btoa("");
        let emptyDec = atob("");

        let asciiEnc = btoa("Chromium V8 Engine in Pure Safe Rust");
        let asciiDec = atob(asciiEnc);

        let invalidCharErr = false;
        try {
            btoa("Hello \u{1f600}");
        } catch (e) {
            invalidCharErr = true;
        }

        ({ encoded, decoded, emptyEnc, emptyDec, asciiDec, invalidCharErr })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("SGVsbG8sIHdvcmxkIQ==".to_string()), o.borrow().get_property("encoded"));
        assert_eq!(JSValue::String("Hello, world!".to_string()), o.borrow().get_property("decoded"));
        assert_eq!(JSValue::String("".to_string()), o.borrow().get_property("emptyEnc"));
        assert_eq!(JSValue::String("".to_string()), o.borrow().get_property("emptyDec"));
        assert_eq!(JSValue::String("Chromium V8 Engine in Pure Safe Rust".to_string()), o.borrow().get_property("asciiDec"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("invalidCharErr"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase35_performance_and_console() {
    let mut ctx = Context::new();
    let script = r#"
        let now = performance.now();
        let timeOrigin = performance.timeOrigin;

        console.time("phase35");
        console.assert(true, "assertion passed");
        console.count("test-counter");
        console.count("test-counter");
        console.timeEnd("phase35");

        let hasNow = (typeof now === "number") && (now >= 0);
        let hasOrigin = (typeof timeOrigin === "number") && (timeOrigin > 0);

        ({ hasNow, hasOrigin })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasNow"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasOrigin"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase36_set_methods() {
    let mut ctx = Context::new();
    let script = r#"
        let a = new Set([1, 2, 3]);
        let b = new Set([3, 4, 5]);

        let unionSet = a.union(b);
        let interSet = a.intersection(b);
        let diffSet = a.difference(b);
        let symSet = a.symmetricDifference(b);

        let subSet = new Set([1, 2]).isSubsetOf(a);
        let superSet = a.isSupersetOf(new Set([1, 2]));
        let disjoint = new Set([10, 20]).isDisjointFrom(a);
        let notDisjoint = a.isDisjointFrom(b);

        ({
            unionSize: unionSet.size,
            has1: unionSet.has(1),
            has4: unionSet.has(4),
            has5: unionSet.has(5),
            interSize: interSet.size,
            interHas3: interSet.has(3),
            diffSize: diffSet.size,
            diffHas1: diffSet.has(1),
            diffHas3: diffSet.has(3),
            symSize: symSet.size,
            symHas3: symSet.has(3),
            subSet,
            superSet,
            disjoint,
            notDisjoint
        })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(5), o.borrow().get_property("unionSize"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("has1"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("has4"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("has5"));
        assert_eq!(JSValue::Smi(1), o.borrow().get_property("interSize"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("interHas3"));
        assert_eq!(JSValue::Smi(2), o.borrow().get_property("diffSize"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("diffHas1"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("diffHas3"));
        assert_eq!(JSValue::Smi(4), o.borrow().get_property("symSize"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("symHas3"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("subSet"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("superSet"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("disjoint"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("notDisjoint"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase36_iterator_helpers() {
    let mut ctx = Context::new();
    let script = r#"
        let arr = [1, 2, 3, 4, 5, 6, 7, 8];
        let pipeline = Iterator.from(arr)
            .map(function(x) { return x * 10; })
            .filter(function(x) { return x > 30; })
            .take(3)
            .toArray();

        let dropped = Iterator.from([1, 2, 3, 4]).drop(2).toArray();
        let sum = Iterator.from([1, 2, 3, 4, 5]).reduce(function(acc, x) { return acc + x; }, 0);
        let hasEven = Iterator.from([1, 3, 4, 5]).some(function(x) { return x % 2 === 0; });
        let allPos = Iterator.from([1, 2, 3]).every(function(x) { return x > 0; });
        let found = Iterator.from([10, 20, 30]).find(function(x) { return x > 15; });

        ({
            pipeLen: pipeline.length,
            p0: pipeline[0],
            p1: pipeline[1],
            p2: pipeline[2],
            drop0: dropped[0],
            drop1: dropped[1],
            sum,
            hasEven,
            allPos,
            found
        })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(3), o.borrow().get_property("pipeLen"));
        assert_eq!(JSValue::Smi(40), o.borrow().get_property("p0"));
        assert_eq!(JSValue::Smi(50), o.borrow().get_property("p1"));
        assert_eq!(JSValue::Smi(60), o.borrow().get_property("p2"));
        assert_eq!(JSValue::Smi(3), o.borrow().get_property("drop0"));
        assert_eq!(JSValue::Smi(4), o.borrow().get_property("drop1"));
        assert_eq!(JSValue::Smi(15), o.borrow().get_property("sum"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasEven"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("allPos"));
        assert_eq!(JSValue::Smi(20), o.borrow().get_property("found"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase36_array_and_promise_and_annex_b() {
    let mut ctx = Context::new();
    let script = r#"
        let pAnyRes = null;
        let pAnyErr = null;

        Promise.any([
            Promise.reject("fail1"),
            Promise.resolve("firstSuccess"),
            Promise.resolve("secondSuccess")
        ]).then(function(val) {
            pAnyRes = val;
        });

        Promise.any([
            Promise.reject("errA"),
            Promise.reject("errB")
        ]).catch(function(err) {
            pAnyErr = err;
        });

        let asyncArr = null;
        Array.fromAsync([1, 2, 3], function(x) { return x * 10; }).then(function(res) {
            asyncArr = res;
        });

        // Annex B test
        let esc = escape("Hello World!");
        let unesc = unescape(esc);
        let boldStr = "v8".bold();
        let italicsStr = "rust".italics();

        let obj = {};
        let protoMatch = (obj.__proto__ === Object.prototype);
    "#;
    ctx.eval(script).unwrap();

    let check_script = r#"
        let hasAnyErr = (pAnyErr !== null);
        let errName = "";
        let err0 = "";
        let err1 = "";
        if (pAnyErr) {
            errName = pAnyErr.name;
            if (pAnyErr.errors) {
                err0 = pAnyErr.errors[0];
                err1 = pAnyErr.errors[1];
            }
        }

        let async0 = 0;
        let async1 = 0;
        let async2 = 0;
        if (asyncArr) {
            async0 = asyncArr[0];
            async1 = asyncArr[1];
            async2 = asyncArr[2];
        }

        ({
            pAnyRes,
            hasAnyErr,
            errName,
            err0,
            err1,
            async0,
            async1,
            async2,
            esc,
            unesc,
            boldStr,
            italicsStr,
            protoMatch
        })
    "#;
    let res = ctx.eval(check_script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("firstSuccess".to_string()), o.borrow().get_property("pAnyRes"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasAnyErr"));
        assert_eq!(JSValue::String("AggregateError".to_string()), o.borrow().get_property("errName"));
        assert_eq!(JSValue::String("errA".to_string()), o.borrow().get_property("err0"));
        assert_eq!(JSValue::String("errB".to_string()), o.borrow().get_property("err1"));
        assert_eq!(JSValue::Smi(10), o.borrow().get_property("async0"));
        assert_eq!(JSValue::Smi(20), o.borrow().get_property("async1"));
        assert_eq!(JSValue::Smi(30), o.borrow().get_property("async2"));
        assert_eq!(JSValue::String("Hello%20World%21".to_string()), o.borrow().get_property("esc"));
        assert_eq!(JSValue::String("Hello World!".to_string()), o.borrow().get_property("unesc"));
        assert_eq!(JSValue::String("<b>v8</b>".to_string()), o.borrow().get_property("boldStr"));
        assert_eq!(JSValue::String("<i>rust</i>".to_string()), o.borrow().get_property("italicsStr"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("protoMatch"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase37_intl_display_names_and_list_format() {
    let mut ctx = Context::new();
    let script = r#"
        let dnEnRegion = new Intl.DisplayNames(['en'], { type: 'region' });
        let usNameEn = dnEnRegion.of('US');
        let gbNameEn = dnEnRegion.of('GB');

        let dnFrRegion = new Intl.DisplayNames(['fr'], { type: 'region' });
        let usNameFr = dnFrRegion.of('US');

        let dnLang = new Intl.DisplayNames(['en'], { type: 'language' });
        let frName = dnLang.of('fr');
        let zhName = dnLang.of('zh');

        let dnCur = new Intl.DisplayNames(['en'], { type: 'currency' });
        let usdName = dnCur.of('USD');

        let lfConj = new Intl.ListFormat(['en'], { style: 'long', type: 'conjunction' });
        let conjRes = lfConj.format(['Motorcycle', 'Bus', 'Car']);
        let conjParts = lfConj.formatToParts(['Apple', 'Orange']);

        let lfDisj = new Intl.ListFormat(['en'], { style: 'long', type: 'disjunction' });
        let disjRes = lfDisj.format(['Apple', 'Orange']);

        let lfUnit = new Intl.ListFormat(['en'], { type: 'unit' });
        let unitRes = lfUnit.format(['5 feet', '7 inches']);

        let lfPartsLen = conjParts.length;

        ({
            usNameEn,
            gbNameEn,
            usNameFr,
            frName,
            zhName,
            usdName,
            conjRes,
            disjRes,
            unitRes,
            lfPartsLen
        })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("United States".to_string()), o.borrow().get_property("usNameEn"));
        assert_eq!(JSValue::String("United Kingdom".to_string()), o.borrow().get_property("gbNameEn"));
        assert_eq!(JSValue::String("États-Unis".to_string()), o.borrow().get_property("usNameFr"));
        assert_eq!(JSValue::String("French".to_string()), o.borrow().get_property("frName"));
        assert_eq!(JSValue::String("Chinese".to_string()), o.borrow().get_property("zhName"));
        assert_eq!(JSValue::String("US Dollar".to_string()), o.borrow().get_property("usdName"));
        assert_eq!(JSValue::String("Motorcycle, Bus, and Car".to_string()), o.borrow().get_property("conjRes"));
        assert_eq!(JSValue::String("Apple or Orange".to_string()), o.borrow().get_property("disjRes"));
        assert_eq!(JSValue::String("5 feet, 7 inches".to_string()), o.borrow().get_property("unitRes"));
        assert_eq!(JSValue::Smi(3), o.borrow().get_property("lfPartsLen"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase37_intl_plural_rules_and_relative_time() {
    let mut ctx = Context::new();
    let script = r#"
        let prCard = new Intl.PluralRules('en-US');
        let card0 = prCard.select(0);
        let card1 = prCard.select(1);
        let card2 = prCard.select(2);

        let prOrd = new Intl.PluralRules('en-US', { type: 'ordinal' });
        let ord1 = prOrd.select(1);
        let ord2 = prOrd.select(2);
        let ord3 = prOrd.select(3);
        let ord4 = prOrd.select(4);

        let rtfAuto = new Intl.RelativeTimeFormat('en', { numeric: 'auto' });
        let dayTomorrow = rtfAuto.format(1, 'day');
        let dayYesterday = rtfAuto.format(-1, 'day');
        let dayToday = rtfAuto.format(0, 'day');

        let rtfAlways = new Intl.RelativeTimeFormat('en', { numeric: 'always' });
        let quartersIn3 = rtfAlways.format(3, 'quarters');
        let year1Ago = rtfAlways.format(-1, 'year');

        let rtfParts = rtfAlways.formatToParts(10, 'seconds');
        let rtfPartsLen = rtfParts.length;

        ({
            card0,
            card1,
            card2,
            ord1,
            ord2,
            ord3,
            ord4,
            dayTomorrow,
            dayYesterday,
            dayToday,
            quartersIn3,
            year1Ago,
            rtfPartsLen
        })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("other".to_string()), o.borrow().get_property("card0"));
        assert_eq!(JSValue::String("one".to_string()), o.borrow().get_property("card1"));
        assert_eq!(JSValue::String("other".to_string()), o.borrow().get_property("card2"));
        assert_eq!(JSValue::String("one".to_string()), o.borrow().get_property("ord1"));
        assert_eq!(JSValue::String("two".to_string()), o.borrow().get_property("ord2"));
        assert_eq!(JSValue::String("few".to_string()), o.borrow().get_property("ord3"));
        assert_eq!(JSValue::String("other".to_string()), o.borrow().get_property("ord4"));
        assert_eq!(JSValue::String("tomorrow".to_string()), o.borrow().get_property("dayTomorrow"));
        assert_eq!(JSValue::String("yesterday".to_string()), o.borrow().get_property("dayYesterday"));
        assert_eq!(JSValue::String("today".to_string()), o.borrow().get_property("dayToday"));
        assert_eq!(JSValue::String("in 3 quarters".to_string()), o.borrow().get_property("quartersIn3"));
        assert_eq!(JSValue::String("1 year ago".to_string()), o.borrow().get_property("year1Ago"));
        assert_eq!(JSValue::Smi(3), o.borrow().get_property("rtfPartsLen"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase37_intl_segmenter_locale_duration() {
    let mut ctx = Context::new();
    let script = r#"
        let loc = new Intl.Locale('zh-Hans-CN-u-ca-chinese');
        let baseName = loc.baseName;
        let language = loc.language;
        let scriptName = loc.script;
        let region = loc.region;
        let calendar = loc.calendar;
        let maxBase = loc.maximize().baseName;
        let minBase = loc.minimize().baseName;

        let seg = new Intl.Segmenter('en', { granularity: 'word' });
        let segments = seg.segment('Hello world');
        let firstSeg = segments.containing(0).segment;
        let firstIsWord = segments.containing(0).isWordLike;

        let segCount = 0;
        for (let s of segments) {
            segCount = segCount + 1;
        }

        let dfLong = new Intl.DurationFormat('en', { style: 'long' });
        let durLong = dfLong.format({ hours: 1, minutes: 46, seconds: 40 });

        let dfShort = new Intl.DurationFormat('en', { style: 'short' });
        let durShort = dfShort.format({ hours: 1, minutes: 46, seconds: 40 });

        let dfDigital = new Intl.DurationFormat('en', { style: 'digital' });
        let durDigital = dfDigital.format({ hours: 1, minutes: 46, seconds: 40 });

        ({
            baseName,
            language,
            scriptName,
            region,
            calendar,
            maxBase,
            minBase,
            firstSeg,
            firstIsWord,
            segCount,
            durLong,
            durShort,
            durDigital
        })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("zh-Hans-CN".to_string()), o.borrow().get_property("baseName"));
        assert_eq!(JSValue::String("zh".to_string()), o.borrow().get_property("language"));
        assert_eq!(JSValue::String("Hans".to_string()), o.borrow().get_property("scriptName"));
        assert_eq!(JSValue::String("CN".to_string()), o.borrow().get_property("region"));
        assert_eq!(JSValue::String("chinese".to_string()), o.borrow().get_property("calendar"));
        assert_eq!(JSValue::String("zh-Hans-CN".to_string()), o.borrow().get_property("maxBase"));
        assert_eq!(JSValue::String("zh".to_string()), o.borrow().get_property("minBase"));
        assert_eq!(JSValue::String("Hello".to_string()), o.borrow().get_property("firstSeg"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("firstIsWord"));
        assert_eq!(JSValue::Smi(3), o.borrow().get_property("segCount"));
        assert_eq!(JSValue::String("1 hour, 46 minutes, 40 seconds".to_string()), o.borrow().get_property("durLong"));
        assert_eq!(JSValue::String("1 hr, 46 min, 40 sec".to_string()), o.borrow().get_property("durShort"));
        assert_eq!(JSValue::String("1:46:40".to_string()), o.borrow().get_property("durDigital"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase38_ternary_operator() {
    let mut ctx = Context::new();
    let script = r#"
        let a = 10 > 5 ? 42 : 99;
        let b = 10 < 5 ? 42 : 99;
        let c = 1 ? (0 ? 10 : 20) : 30;
        ({ a, b, c })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(42), o.borrow().get_property("a"));
        assert_eq!(JSValue::Smi(99), o.borrow().get_property("b"));
        assert_eq!(JSValue::Smi(20), o.borrow().get_property("c"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase38_delete_operator() {
    let mut ctx = Context::new();
    let script = r#"
        let o = { x: 1, y: 2 };
        let r1 = delete o.x;
        let r2 = o.x;
        let r3 = o.y;

        let arr = [10, 20, 30];
        let r4 = delete arr[1];
        let r5 = arr[1];

        let o2 = { a: 5 };
        let r6 = delete o2['a'];
        let r7 = delete nonExistent;

        ({ r1, r2, r3, r4, r5, r6, r7 })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("r1"));
        assert_eq!(JSValue::Undefined, o.borrow().get_property("r2"));
        assert_eq!(JSValue::Smi(2), o.borrow().get_property("r3"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("r4"));
        assert_eq!(JSValue::Undefined, o.borrow().get_property("r5"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("r6"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("r7"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase38_void_operator() {
    let mut ctx = Context::new();
    let script = r#"
        let a = void 0;
        let b = void (10 + 20);
        ({ a, b })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Undefined, o.borrow().get_property("a"));
        assert_eq!(JSValue::Undefined, o.borrow().get_property("b"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase38_debugger_statement() {
    let mut ctx = Context::new();
    let script = r#"
        debugger;
        let a = 123;
        a
    "#;
    let res = ctx.eval(script).unwrap();
    assert_eq!(JSValue::Smi(123), res);
}

#[test]
fn test_phase38_with_statement() {
    let mut ctx = Context::new();
    let script = r#"
        let obj = { x: 100, y: 200 };
        with (obj) {
            x = 42;
        }
        let a = obj.x;
        let b = 10;
        let res = 0;
        with (obj) {
            res = x + b;
        }
        ({ a, res })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(42), o.borrow().get_property("a"));
        assert_eq!(JSValue::Smi(52), o.borrow().get_property("res"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase38_labeled_statements_and_break_continue() {
    let mut ctx = Context::new();
    let script = r#"
        let sum = 0;
        outer: for (let i = 0; i < 3; i = i + 1) {
            for (let j = 0; j < 3; j = j + 1) {
                if (i === 1 && j === 1) {
                    break outer;
                }
                sum = sum + 1;
            }
        }

        let count = 0;
        loop1: for (let i = 0; i < 3; i = i + 1) {
            for (let j = 0; j < 3; j = j + 1) {
                if (j === 1) {
                    continue loop1;
                }
                count = count + 1;
            }
        }

        ({ sum, count })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        // sum: i=0 (j=0,1,2 -> 3), i=1 (j=0 -> 4, j=1 -> break outer) => 4
        assert_eq!(JSValue::Smi(4), o.borrow().get_property("sum"));
        // count: i=0 (j=0 -> 1, j=1 -> continue), i=1 (j=0 -> 2, j=1 -> continue), i=2 (j=0 -> 3, j=1 -> continue) => 3
        assert_eq!(JSValue::Smi(3), o.borrow().get_property("count"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase38_new_target() {
    let mut ctx = Context::new();
    let script = r#"
        function Foo() {
            return { isNew: new.target !== undefined };
        }
        let c = new Foo();
        let f = Foo();
        ({ c: c.isNew, f: f.isNew })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("c"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("f"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase38_destructuring_assignment() {
    let mut ctx = Context::new();
    let script = r#"
        let a = 0;
        let b = 0;
        [a, b] = [10, 20];

        let x = 0;
        let y = 0;
        ({ x, y } = { x: 50, y: 60 });

        ({ a, b, x, y })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(10), o.borrow().get_property("a"));
        assert_eq!(JSValue::Smi(20), o.borrow().get_property("b"));
        assert_eq!(JSValue::Smi(50), o.borrow().get_property("x"));
        assert_eq!(JSValue::Smi(60), o.borrow().get_property("y"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase39_float16_array_and_dataview() {
    let mut ctx = Context::new();
    let script = r#"
        let f16 = new Float16Array([1.5, -2.5, 4.0]);
        let bpe = Float16Array.BYTES_PER_ELEMENT;
        let len = f16.length;
        let v0 = f16[0];
        let v1 = f16[1];
        let v2 = f16[2];

        let buf = new ArrayBuffer(4);
        let dv = new DataView(buf);
        dv.setFloat16(0, 3.5, true);
        let dv_val = dv.getFloat16(0, true);

        let rounded = Math.f16round(1.337);

        ({ bpe, len, v0, v1, v2, dv_val, rounded_ok: rounded > 1.33 && rounded < 1.34 })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(2), o.borrow().get_property("bpe"));
        assert_eq!(JSValue::Smi(3), o.borrow().get_property("len"));
        assert_eq!(JSValue::Number(1.5), o.borrow().get_property("v0"));
        assert_eq!(JSValue::Number(-2.5), o.borrow().get_property("v1"));
        assert_eq!(JSValue::Number(4.0), o.borrow().get_property("v2"));
        assert_eq!(JSValue::Number(3.5), o.borrow().get_property("dv_val"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("rounded_ok"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase39_json_raw_json() {
    let mut ctx = Context::new();
    let script = r#"
        let raw = JSON.rawJSON("12345678901234567890");
        let isRaw = JSON.isRawJSON(raw);
        let isNotRaw = JSON.isRawJSON({ rawJSON: "123" });
        let obj = { big: raw, normal: 42 };
        let str = JSON.stringify(obj);

        ({ isRaw, isNotRaw, str })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isRaw"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("isNotRaw"));
        assert_eq!(
            JSValue::String(r#"{"big":12345678901234567890,"normal":42}"#.to_string()),
            o.borrow().get_property("str")
        );
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase39_disposable_stack_methods() {
    let mut ctx = Context::new();
    let script = r#"
        let events = [];
        let stack = new DisposableStack();

        let r1 = {
            [Symbol.dispose]: function() {
                events.push("r1_disposed");
            }
        };

        stack.use(r1);
        stack.adopt({ name: "r2" }, function(val) {
            events.push("r2_adopted_" + val.name);
        });
        stack.defer(function() {
            events.push("deferred");
        });

        let before = stack.disposed;
        stack.dispose();
        let after = stack.disposed;

        ({ before, after, events: events.join(",") })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("before"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("after"));
        // LIFO order: deferred -> r2_adopted_r2 -> r1_disposed
        assert_eq!(
            JSValue::String("deferred,r2_adopted_r2,r1_disposed".to_string()),
            o.borrow().get_property("events")
        );
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase39_async_disposable_stack() {
    let mut ctx = Context::new();
    let script = r#"
        let events = [];
        let stack = new AsyncDisposableStack();

        let r1 = {
            [Symbol.asyncDispose]: function() {
                events.push("async_r1");
            }
        };

        stack.use(r1);
        stack.defer(function() {
            events.push("async_deferred");
        });

        let before = stack.disposed;
        let p = stack.disposeAsync();
        let after = stack.disposed;

        ({ before, after, isPromise: p instanceof Promise, events: events.join(",") })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("before"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("after"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isPromise"));
        assert_eq!(
            JSValue::String("async_deferred,async_r1".to_string()),
            o.borrow().get_property("events")
        );
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase39_suppressed_error() {
    let mut ctx = Context::new();
    let script = r#"
        let err1 = new Error("first error");
        let err2 = new Error("second error");
        let sup = new SuppressedError(err2, err1, "A suppressed error occurred");

        ({
            name: sup.name,
            message: sup.message,
            errorMsg: sup.error.message,
            suppressedMsg: sup.suppressed.message
        })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("SuppressedError".to_string()), o.borrow().get_property("name"));
        assert_eq!(JSValue::String("A suppressed error occurred".to_string()), o.borrow().get_property("message"));
        assert_eq!(JSValue::String("second error".to_string()), o.borrow().get_property("errorMsg"));
        assert_eq!(JSValue::String("first error".to_string()), o.borrow().get_property("suppressedMsg"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase39_using_declaration_block_scope() {
    let mut ctx = Context::new();
    let script = r#"
        let log = [];
        {
            using r1 = {
                [Symbol.dispose]: function() {
                    log.push("dispose_r1");
                }
            };
            {
                using r2 = {
                    [Symbol.dispose]: function() {
                        log.push("dispose_r2");
                    }
                };
                log.push("inside_inner");
            }
            log.push("inside_outer");
        }
        log.push("finished");
        log.join(",")
    "#;
    let res = ctx.eval(script).unwrap();
    // Inner block exits -> dispose_r2 -> inside_outer -> Outer block exits -> dispose_r1 -> finished
    assert_eq!(
        JSValue::String("inside_inner,dispose_r2,inside_outer,dispose_r1,finished".to_string()),
        res
    );
}

#[test]
fn test_phase39_using_early_return_and_throw() {
    let mut ctx = Context::new();
    let script = r#"
        let log = [];

        function testEarlyReturn() {
            using r = {
                [Symbol.dispose]: function() {
                    log.push("early_return_disposed");
                }
            };
            return 42;
        }

        let ret = testEarlyReturn();

        function testThrow() {
            try {
                using r2 = {
                    [Symbol.dispose]: function() {
                        log.push("throw_disposed");
                    }
                };
                throw new Error("oops");
            } catch (e) {
                log.push("caught_" + e.message);
            }
        }

        testThrow();

        ({ ret, log: log.join(",") })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(42), o.borrow().get_property("ret"));
        assert_eq!(
            JSValue::String("early_return_disposed,throw_disposed,caught_oops".to_string()),
            o.borrow().get_property("log")
        );
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase39_regexp_d_and_v_flags() {
    let mut ctx = Context::new();
    let script = r#"
        let re = /a(?<mid>b+)c/d;
        let m = re.exec("zabbbcz");
        let hasIndices = re.hasIndices;
        let flags = re.flags;

        let fullStart = m.indices[0][0];
        let fullEnd = m.indices[0][1];
        let midStart = m.indices[1][0];
        let midEnd = m.indices[1][1];
        let groupMidStart = m.indices.groups.mid[0];
        let groupMidEnd = m.indices.groups.mid[1];

        let reV = /hello/v;
        let isUnicodeSets = reV.unicodeSets;

        ({
            hasIndices,
            flags,
            fullStart,
            fullEnd,
            midStart,
            midEnd,
            groupMidStart,
            groupMidEnd,
            isUnicodeSets
        })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasIndices"));
        assert_eq!(JSValue::String("d".to_string()), o.borrow().get_property("flags"));
        // "zabbbcz": match starts at index 1 ('a'), ends at index 6 ('c' + 1)
        assert_eq!(JSValue::Smi(1), o.borrow().get_property("fullStart"));
        assert_eq!(JSValue::Smi(6), o.borrow().get_property("fullEnd"));
        // group 'mid' ('bbb'): starts at 2, ends at 5
        assert_eq!(JSValue::Smi(2), o.borrow().get_property("midStart"));
        assert_eq!(JSValue::Smi(5), o.borrow().get_property("midEnd"));
        assert_eq!(JSValue::Smi(2), o.borrow().get_property("groupMidStart"));
        assert_eq!(JSValue::Smi(5), o.borrow().get_property("groupMidEnd"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isUnicodeSets"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Phase 40: Advanced WebAssembly Proposals Parity Tests
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_phase40_wasm_gc_struct_new_get_set() {
    use v8_base_bits::wasm::*;
    let mut module = WasmModule::default();
    module.type_defs.push(TypeDef::Struct(StructType {
        fields: vec![
            StructField { val_type: ValType::I32, mutable: true },
            StructField { val_type: ValType::I32, mutable: true },
        ],
    }));
    module.types.push(FuncType { params: vec![], results: vec![ValType::I32] });
    module.types.push(FuncType { params: vec![], results: vec![ValType::I32] });
    module.functions.push(1);

    let body = vec![
        0x41, 42,
        0x41, 100,
        0xFB, 0x00, 0x00,
        0x21, 0x00,
        0x20, 0x00,
        0xFB, 0x02, 0x00, 0x00,
        0x20, 0x00,
        0x41, 0xC8, 0x01, // 200 in LEB128
        0xFB, 0x05, 0x00, 0x01,
        0x20, 0x00,
        0xFB, 0x02, 0x00, 0x01,
        0x6A,
        0x0B,
    ];
    module.code.push(WasmCode {
        locals: vec![(1, ValType::StructRef(Some(0)))],
        body,
    });

    let mut instance = WasmInstance::instantiate(&module, vec![]);
    let results = WasmInterpreter::call_func(&module, &mut instance, 0, vec![], 0).unwrap();
    assert_eq!(results, vec![WasmVal::I32(242)]);
}

#[test]
fn test_phase40_wasm_gc_array_new_get_set_len() {
    use v8_base_bits::wasm::*;
    let mut module = WasmModule::default();
    module.type_defs.push(TypeDef::Array(ArrayType {
        elem_type: ValType::I32,
        mutable: true,
    }));
    module.types.push(FuncType { params: vec![], results: vec![ValType::I32] });
    module.types.push(FuncType { params: vec![], results: vec![ValType::I32] });
    module.functions.push(1);

    let body = vec![
        0x41, 10,
        0x41, 4,
        0xFB, 0x06, 0x00,
        0x21, 0x00,
        0x20, 0x00,
        0xFB, 0x0F,
        0x20, 0x00,
        0x41, 2,
        0x41, 50,
        0xFB, 0x0E, 0x00,
        0x20, 0x00,
        0x41, 2,
        0xFB, 0x0B, 0x00,
        0x6A,
        0x0B,
    ];
    module.code.push(WasmCode {
        locals: vec![(1, ValType::ArrayRef(Some(0)))],
        body,
    });

    let mut instance = WasmInstance::instantiate(&module, vec![]);
    let results = WasmInterpreter::call_func(&module, &mut instance, 0, vec![], 0).unwrap();
    assert_eq!(results, vec![WasmVal::I32(54)]);
}

#[test]
fn test_phase40_wasm_gc_i31_ref() {
    use v8_base_bits::wasm::*;
    let mut module = WasmModule::default();
    module.types.push(FuncType { params: vec![], results: vec![ValType::I32] });
    module.functions.push(0);

    let body = vec![
        0x41, 0xD2, 0x09, // 1234
        0xFB, 0x1C,
        0xFB, 0x1D,
        0x41, 0xC2, 0x00, // 66 in signed LEB128
        0xFB, 0x1C,
        0xFB, 0x1E,
        0x6A,
        0x0B,
    ];
    module.code.push(WasmCode {
        locals: vec![],
        body,
    });

    let mut instance = WasmInstance::instantiate(&module, vec![]);
    let results = WasmInterpreter::call_func(&module, &mut instance, 0, vec![], 0).unwrap();
    assert_eq!(results, vec![WasmVal::I32(1300)]);
}

#[test]
fn test_phase40_wasm_gc_ref_test_cast() {
    use v8_base_bits::wasm::*;
    let mut module = WasmModule::default();
    module.types.push(FuncType { params: vec![], results: vec![ValType::I32] });
    module.functions.push(0);

    let body = vec![
        0x41, 0xCD, 0x00, // 77 in signed LEB128
        0xFB, 0x1C,
        0xFB, 0x14, 0x6E, // test eqref (-18)
        0x41, 0xCD, 0x00, // 77 in signed LEB128
        0xFB, 0x1C,
        0xFB, 0x16, 0x6D, // cast i31ref (-19)
        0xFB, 0x1D,
        0x6A,
        0x0B,
    ];
    module.code.push(WasmCode {
        locals: vec![],
        body,
    });

    let mut instance = WasmInstance::instantiate(&module, vec![]);
    let results = WasmInterpreter::call_func(&module, &mut instance, 0, vec![], 0).unwrap();
    assert_eq!(results, vec![WasmVal::I32(78)]);
}

#[test]
fn test_phase40_wasm_exception_handling_try_table_throw() {
    use v8_base_bits::wasm::*;
    let mut module = WasmModule::default();
    module.types.push(FuncType { params: vec![ValType::I32], results: vec![] });
    module.types.push(FuncType { params: vec![], results: vec![ValType::I32] });
    module.functions.push(1);
    module.tags.push(WasmTag { attribute: 0, type_idx: 0 });

    let body = vec![
        0x02, 0x7F,
        0x1F, 0x7F, 0x01,
        0x00, 0x00, 0x00, // catch tag 0 -> depth 0
        0x41, 42,
        0x08, 0x00,       // throw tag 0
        0x0B,
        0x0B,
        0x41, 10,
        0x6A,
        0x0B,
    ];
    module.code.push(WasmCode {
        locals: vec![],
        body,
    });

    let mut instance = WasmInstance::instantiate(&module, vec![]);
    let results = WasmInterpreter::call_func(&module, &mut instance, 0, vec![], 0).unwrap();
    assert_eq!(results, vec![WasmVal::I32(52)]);
}

#[test]
fn test_phase40_wasm_js_api_tag_and_exception() {
    let mut ctx = Context::new();
    let script = r#"
        let tag1 = new WebAssembly.Tag({ parameters: ["i32"] });
        let tag2 = new WebAssembly.Tag({ parameters: ["f64"] });
        let type1 = tag1.type();
        let param0 = type1.parameters[0];

        let exc = new WebAssembly.Exception(tag1, [42]);
        let isTag1 = exc.is(tag1);
        let isTag2 = exc.is(tag2);
        let arg0 = exc.getArg(tag1, 0);

        let mem = new WebAssembly.Memory({ initial: 2, maximum: 10, address: "i64" });
        let isMem64 = mem.__memory64__;

        ({ param0, isTag1, isTag2, arg0, isMem64 })
    "#;
    let res = ctx.eval(script).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::String("i32".to_string()), o.borrow().get_property("param0"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isTag1"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("isTag2"));
        assert_eq!(JSValue::Smi(42), o.borrow().get_property("arg0"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("isMem64"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase40_wasm_memory64_load_store() {
    use v8_base_bits::wasm::*;
    let mut mem = WasmMemory::new_64(1, Some(10), true, false);
    assert_eq!(mem.is_memory64, true);
    assert_eq!(mem.size_pages_64(), 1);

    mem.store_i32_64(0x1000, 0x12345678).unwrap();
    let val = mem.load_i32_64(0x1000).unwrap();
    assert_eq!(val, 0x12345678);

    mem.store_i64_64(0x2000, 0x0102030405060708).unwrap();
    let val64 = mem.load_i64_64(0x2000).unwrap();
    assert_eq!(val64, 0x0102030405060708);

    let old_pages = mem.grow_64(2);
    assert_eq!(old_pages, 1);
    assert_eq!(mem.size_pages_64(), 3);
}

#[test]
fn test_phase40_wasm_atomics_rmw_cmpxchg_wait_notify() {
    use v8_base_bits::wasm::*;
    let mut mem = WasmMemory::new_64(1, Some(5), false, true);
    assert_eq!(mem.is_shared, true);

    mem.atomic_store_i32(0x100, 10).unwrap();
    assert_eq!(mem.atomic_load_i32(0x100).unwrap(), 10);

    let old = mem.atomic_rmw_add_i32(0x100, 5).unwrap();
    assert_eq!(old, 10);
    assert_eq!(mem.atomic_load_i32(0x100).unwrap(), 15);

    let old = mem.atomic_rmw_sub_i32(0x100, 3).unwrap();
    assert_eq!(old, 15);
    assert_eq!(mem.atomic_load_i32(0x100).unwrap(), 12);

    let old = mem.atomic_rmw_cmpxchg_i32(0x100, 12, 99).unwrap();
    assert_eq!(old, 12);
    assert_eq!(mem.atomic_load_i32(0x100).unwrap(), 99);

    let old = mem.atomic_rmw_cmpxchg_i32(0x100, 100, 200).unwrap();
    assert_eq!(old, 99);
    assert_eq!(mem.atomic_load_i32(0x100).unwrap(), 99);

    let wait_res = mem.atomic_wait32(0x100, 99, 1000).unwrap();
    assert_eq!(wait_res, 0);
    let not_eq = mem.atomic_wait32(0x100, 0, 1000).unwrap();
    assert_eq!(not_eq, 1);

    let woken = mem.atomic_notify(0x100, 1).unwrap();
    assert_eq!(woken, 0);
}

#[test]
fn test_phase41_inspector_websocket_handshake_and_dispatch() {
    use v8_base_bits::inspector::server::{
        compute_websocket_accept, decode_websocket_frame, encode_websocket_frame, sha1,
    };
    use v8_base_bits::inspector::InspectorSession;

    // 1. Verify RFC 3174 SHA-1
    let hash = sha1(b"The quick brown fox jumps over the lazy dog");
    let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
    assert_eq!(hex, "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12");

    // 2. Verify RFC 6455 Sec-WebSocket-Accept key derivation
    let client_key = "dGhlIHNhbXBsZSBub25jZQ==";
    let accept = compute_websocket_accept(client_key);
    assert_eq!(accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");

    // 3. Verify WebSocket framing encode & masked decode
    let msg = r#"{"id":1,"method":"Runtime.evaluate","params":{"expression":"2+2"}}"#;
    let encoded = encode_websocket_frame(msg);
    assert_eq!(encoded[0], 0x81); // Text opcode, FIN bit set

    // Simulate masked frame from client
    let mask = [0x12, 0x34, 0x56, 0x78];
    let payload = msg.as_bytes();
    let mut masked_frame = vec![0x81, 0x80 | (payload.len() as u8)];
    masked_frame.extend_from_slice(&mask);
    for (i, &b) in payload.iter().enumerate() {
        masked_frame.push(b ^ mask[i % 4]);
    }

    let decoded = decode_websocket_frame(&masked_frame).unwrap();
    assert!(decoded.is_some());
    let (decoded_str, consumed) = decoded.unwrap();
    assert_eq!(consumed, masked_frame.len());
    assert_eq!(decoded_str, msg);

    // 4. Verify CDP JSON-RPC dispatch
    let mut session = InspectorSession::new();
    let resp = session.dispatch(&decoded_str);
    assert!(resp.contains("\"id\":1"));
    assert!(resp.contains("\"type\":\"number\",\"value\":4"));
}

#[test]
fn test_phase41_heap_snapshot_v8_json_schema() {
    use v8_base_bits::inspector::export_heap_snapshot;
    use v8_base_bits::runtime::Context;

    let mut ctx = Context::new();
    let _ = ctx.eval("var myObject = { a: 123, b: 'hello', arr: [1, 2, 3] };");

    let snapshot_json = export_heap_snapshot(&ctx);

    // Verify root JSON schema elements
    assert!(snapshot_json.contains("\"snapshot\": {"));
    assert!(snapshot_json.contains("\"meta\": {"));
    assert!(snapshot_json.contains("\"node_fields\": [\"type\", \"name\", \"id\", \"self_size\", \"edge_count\", \"trace_node_id\"]"));
    assert!(snapshot_json.contains("\"edge_fields\": [\"type\", \"name_or_index\", \"to_node\"]"));
    assert!(snapshot_json.contains("\"nodes\": ["));
    assert!(snapshot_json.contains("\"edges\": ["));
    assert!(snapshot_json.contains("\"strings\": ["));

    // Verify strings table contains expected names
    assert!(snapshot_json.contains("\"(GC roots)\""));
    assert!(snapshot_json.contains("\"global\""));
    assert!(snapshot_json.contains("\"myObject\""));
}

#[test]
fn test_phase41_cpu_profile_v8_json_schema() {
    use v8_base_bits::inspector::{export_cpu_profile, CpuProfileBuilder, InspectorSession};

    let session = InspectorSession::new();
    let mut builder = CpuProfileBuilder::new();

    let n1 = builder.add_child_node(1, "fib", "script.js", "http://localhost/script.js", 1, 0);
    let n2 = builder.add_child_node(n1, "sum", "script.js", "http://localhost/script.js", 10, 0);

    builder.record_sample(n1, 1000);
    builder.record_sample(n2, 250);
    builder.record_sample(n1, 500);

    let profile_json = export_cpu_profile(&session);
    assert!(profile_json.contains("\"nodes\": ["));
    assert!(profile_json.contains("\"startTime\":"));
    assert!(profile_json.contains("\"endTime\":"));
    assert!(profile_json.contains("\"samples\": ["));
    assert!(profile_json.contains("\"timeDeltas\": ["));

    let custom_json = builder.to_json();
    assert!(custom_json.contains("\"functionName\": \"fib\""));
    assert!(custom_json.contains("\"functionName\": \"sum\""));
    assert!(custom_json.contains("\"samples\": [2, 3, 2]"));
}

#[test]
fn test_phase41_concurrent_old_space_sweeper() {
    use v8_base_bits::heap::{GarbageCollectionType, Heap, HeapPayload};

    let mut heap = Heap::new(512, 1024);

    // Allocate 100 objects directly into OldSpace
    let mut ids = Vec::new();
    for i in 0..100 {
        let id = heap.allocate_old(HeapPayload::String(format!("test-{}", i)), 32);
        ids.push(id);
    }

    // Ten survive as global roots, others become dead/unreachable
    for &id in &ids[0..10] {
        heap.add_global_root(id);
    }

    // Execute Concurrent OldSpace sweeping GC
    let reclaimed = heap.collect_garbage(GarbageCollectionType::ConcurrentMarkSweep);
    assert_eq!(reclaimed, 90);

    // Verify live objects still intact
    for i in 0..10 {
        let obj = heap.get(ids[i]);
        assert!(obj.is_some());
        assert!(matches!(obj.unwrap().payload, HeapPayload::String(_)));
    }

    // Verify free list recycled 90 slots
    assert_eq!(heap.old_space.free_list.len(), 90);
}

#[test]
fn test_phase41_d8_realm_api() {
    let mut ctx = Context::new();
    let code = r#"
        let r1 = Realm.create();
        let r2 = Realm.create();
        let cur = Realm.current();

        Realm.eval(r1, "var secret = 42;");
        Realm.eval(r2, "var secret = 100;");

        let val1 = Realm.eval(r1, "secret");
        let val2 = Realm.eval(r2, "secret");

        let g1 = Realm.global(r1);
        let g2 = Realm.global(r2);

        Realm.dispose(r2);

        ({
            r1: r1,
            r2: r2,
            cur: cur,
            val1: val1,
            val2: val2,
            hasG1: g1 !== undefined,
            hasG2: g2 !== undefined
        });
    "#;
    let res = ctx.eval(code).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(1), o.borrow().get_property("r1"));
        assert_eq!(JSValue::Smi(2), o.borrow().get_property("r2"));
        assert_eq!(JSValue::Smi(0), o.borrow().get_property("cur"));
        assert_eq!(JSValue::Smi(42), o.borrow().get_property("val1"));
        assert_eq!(JSValue::Smi(100), o.borrow().get_property("val2"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasG1"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasG2"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase42_whatwg_readable_stream_and_reader() {
    let mut ctx = Context::new();
    let code = r#"
        let stream = new ReadableStream({
            start(controller) {
                controller.enqueue("chunk-1");
                controller.enqueue("chunk-2");
                controller.close();
            }
        });

        let isLockedBefore = stream.locked;
        let reader = stream.getReader();
        let isLockedAfter = stream.locked;

        globalThis.streamRes = {};

        reader.read().then(res => { globalThis.streamRes.r1 = res; });
        reader.read().then(res => { globalThis.streamRes.r2 = res; });
        reader.read().then(res => { globalThis.streamRes.r3 = res; });

        reader.releaseLock();
        let isLockedReleased = stream.locked;

        ({
            before: isLockedBefore,
            after: isLockedAfter,
            released: isLockedReleased
        });
    "#;
    let res = ctx.eval(code).unwrap();
    ctx.run_microtasks();

    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("before"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("after"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("released"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }

    let check_code = r#"
        ({
            c1_val: globalThis.streamRes.r1.value,
            c1_done: globalThis.streamRes.r1.done,
            c2_val: globalThis.streamRes.r2.value,
            c2_done: globalThis.streamRes.r2.done,
            c3_val: globalThis.streamRes.r3.value,
            c3_done: globalThis.streamRes.r3.done
        });
    "#;
    let check = ctx.eval(check_code).unwrap();
    if let JSValue::Object(o) = check {
        assert_eq!(JSValue::String("chunk-1".to_string()), o.borrow().get_property("c1_val"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("c1_done"));
        assert_eq!(JSValue::String("chunk-2".to_string()), o.borrow().get_property("c2_val"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("c2_done"));
        assert_eq!(JSValue::Undefined, o.borrow().get_property("c3_val"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("c3_done"));
    } else {
        panic!("Expected result object, got {:?}", check);
    }
}

#[test]
fn test_phase42_whatwg_writable_and_transform_stream() {
    let mut ctx = Context::new();
    let code = r#"
        let received = [];
        let writable = new WritableStream({
            write(chunk) {
                received.push(chunk);
            }
        });

        let writer = writable.getWriter();
        writer.write(10);
        writer.write(20);
        writer.write(30);
        writer.close();

        let ts = new TransformStream({
            transform(chunk, controller) {
                controller.enqueue(chunk * 2);
            }
        });

        let tsWriter = ts.writable.getWriter();
        tsWriter.write(5);
        tsWriter.write(7);

        let tsReader = ts.readable.getReader();
        globalThis.tsRes = {};
        tsReader.read().then(res => { globalThis.tsRes.t1 = res.value; });
        tsReader.read().then(res => { globalThis.tsRes.t2 = res.value; });

        ({
            len: received.length,
            w0: received[0],
            w1: received[1],
            w2: received[2]
        });
    "#;
    let res = ctx.eval(code).unwrap();
    ctx.run_microtasks();

    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(3), o.borrow().get_property("len"));
        assert_eq!(JSValue::Smi(10), o.borrow().get_property("w0"));
        assert_eq!(JSValue::Smi(20), o.borrow().get_property("w1"));
        assert_eq!(JSValue::Smi(30), o.borrow().get_property("w2"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }

    let check_code = r#"
        ({
            t1: globalThis.tsRes.t1,
            t2: globalThis.tsRes.t2
        });
    "#;
    let check = ctx.eval(check_code).unwrap();
    if let JSValue::Object(o) = check {
        assert_eq!(JSValue::Smi(10), o.borrow().get_property("t1"));
        assert_eq!(JSValue::Smi(14), o.borrow().get_property("t2"));
    } else {
        panic!("Expected result object, got {:?}", check);
    }
}

#[test]
fn test_phase42_whatwg_event_target_and_custom_event() {
    let mut ctx = Context::new();
    let code = r#"
        var target = new EventTarget();
        target.clickCount = 0;
        target.onceCount = 0;
        target.customDetail = null;

        function onClick(e) {
            target.clickCount = target.clickCount + 1;
        }

        target.addEventListener("click", onClick);
        target.addEventListener("click", function(e) {
            target.onceCount = target.onceCount + 1;
        }, { once: true });

        let ev1 = new Event("click", { cancelable: true });
        let dispatchResult1 = target.dispatchEvent(ev1);

        let ev2 = new Event("click", { cancelable: true });
        let dispatchResult2 = target.dispatchEvent(ev2);

        target.removeEventListener("click", onClick);
        let ev3 = new Event("click");
        target.dispatchEvent(ev3);

        target.addEventListener("message", function(e) {
            target.customDetail = e.detail;
            e.preventDefault();
        });

        let customEv = new CustomEvent("message", { cancelable: true, detail: { user: "alice", age: 30 } });
        let customDispatched = target.dispatchEvent(customEv);

        ({
            clickCount: target.clickCount,
            onceCount: target.onceCount,
            dispatchResult1: dispatchResult1,
            dispatchResult2: dispatchResult2,
            detailUser: target.customDetail.user,
            detailAge: target.customDetail.age,
            customDispatched: customDispatched,
            defaultPrevented: customEv.defaultPrevented
        });
    "#;
    let res = ctx.eval(code).unwrap();
    if let JSValue::Object(o) = res {
        assert_eq!(JSValue::Smi(2), o.borrow().get_property("clickCount"));
        assert_eq!(JSValue::Smi(1), o.borrow().get_property("onceCount"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("dispatchResult1"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("dispatchResult2"));
        assert_eq!(JSValue::String("alice".to_string()), o.borrow().get_property("detailUser"));
        assert_eq!(JSValue::Smi(30), o.borrow().get_property("detailAge"));
        assert_eq!(JSValue::Boolean(false), o.borrow().get_property("customDispatched"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("defaultPrevented"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase42_process_host_bindings() {
    let mut ctx = Context::new();
    let code = r#"
        let hr = process.hrtime();
        let hrDiff = process.hrtime(hr);
        let mem = process.memoryUsage();

        ({
            platform: process.platform,
            arch: process.arch,
            version: process.version,
            v8Version: process.versions.v8,
            hasArgv: Array.isArray(process.argv) && process.argv.length > 0,
            hasCwd: typeof process.cwd() === "string" && process.cwd().length > 0,
            hasHrtime: Array.isArray(hr) && hr.length === 2,
            hasDiff: Array.isArray(hrDiff) && hrDiff.length === 2,
            hasMem: mem.heapUsed > 0 && mem.heapTotal > 0,
            uptime: typeof process.uptime() === "number"
        });
    "#;
    let res = ctx.eval(code).unwrap();
    if let JSValue::Object(o) = res {
        let plat = o.borrow().get_property("platform");
        assert!(matches!(plat, JSValue::String(_)));
        let arch = o.borrow().get_property("arch");
        assert!(matches!(arch, JSValue::String(_)));
        assert_eq!(JSValue::String("v22.0.0".to_string()), o.borrow().get_property("version"));
        assert_eq!(JSValue::String("12.8.0".to_string()), o.borrow().get_property("v8Version"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasArgv"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasCwd"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasHrtime"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasDiff"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("hasMem"));
        assert_eq!(JSValue::Boolean(true), o.borrow().get_property("uptime"));
    } else {
        panic!("Expected result object, got {:?}", res);
    }
}

#[test]
fn test_phase43_advanced_jit_and_pointer_compression() {
    use v8_base_bits::heap::pointer_compression::{CompressedPointer, CompressedValue, IsolateRoot, CompressedHeapPage};
    use v8_base_bits::compiler::sparkplug::SparkplugCompiler;

    // 1. Verify 32-bit pointer compression with 4GB base
    let root = IsolateRoot::new(0x0000_0001_0000_0000);
    assert_eq!(0x0000_0001_0000_0000, root.base_address);

    // Smi compression
    let smi_cp = CompressedPointer::from_smi(42);
    assert!(smi_cp.is_smi());
    assert!(!smi_cp.is_heap_object());
    assert_eq!(42, smi_cp.to_smi());

    // Negative Smi compression
    let neg_smi = CompressedPointer::from_smi(-100);
    assert!(neg_smi.is_smi());
    assert_eq!(-100, neg_smi.to_smi());

    // HeapObject pointer compression and decompression against 4GB base
    let full_ptr = 0x0000_0001_0004_2000u64;
    let heap_cp = root.compress(full_ptr);
    assert!(!heap_cp.is_smi());
    assert!(heap_cp.is_heap_object());
    let decompressed = root.decompress(heap_cp);
    assert_eq!(full_ptr, decompressed);

    // Compressed values
    let val_smi = CompressedValue::compress(&JSValue::Smi(123), &root);
    assert_eq!(JSValue::Smi(123), val_smi.decompress());

    let val_bool = CompressedValue::compress(&JSValue::Boolean(true), &root);
    assert_eq!(JSValue::Boolean(true), val_bool.decompress());

    let val_null = CompressedValue::compress(&JSValue::Null, &root);
    assert_eq!(JSValue::Null, val_null.decompress());

    let val_undef = CompressedValue::compress(&JSValue::Undefined, &root);
    assert_eq!(JSValue::Undefined, val_undef.decompress());

    // CompressedHeapPage slot storage (50% memory savings)
    let mut page = CompressedHeapPage::new(root, 64);
    assert_eq!(256, page.compressed_bytes());
    assert_eq!(512, page.uncompressed_bytes());
    page.store(0, &JSValue::Smi(42));
    page.store(1, &JSValue::Boolean(true));
    assert_eq!(JSValue::Smi(42), page.load(0));
    assert_eq!(JSValue::Boolean(true), page.load(1));

    // 2. Verify Sparkplug Baseline JIT Compilation
    let mut ctx = Context::new();
    let code = r#"
        function compute(a, b) {
            return (a + b) * 2 - 4;
        }
        compute(10, 20);
    "#;
    let res = ctx.eval(code).unwrap();
    assert_eq!(JSValue::Smi(56), res);

    // 3. Verify 3-tier function progression (Bytecode -> BaselineJit -> JitCompiled)
    let global_ref = ctx.global_object.borrow();
    let func_val = global_ref.get_property("compute");
    if let JSValue::Function(ref f) = func_val {
        // Can compile with Sparkplug
        if let Some(ref bc) = f.bytecode {
            assert!(SparkplugCompiler::can_compile(bc));
            let native_exec = SparkplugCompiler::compile(bc);
            assert!(native_exec.code_bytes().len() > 0);
        }
    } else {
        panic!("Expected function, got {:?}", func_val);
    }
}

#[test]
fn test_phase44_chromium_c_abi() {
    use std::ffi::{CStr, CString};

    unsafe {
        // 1. Isolate and Context creation
        let isolate = v8_isolate_new();
        assert!(!isolate.is_null());

        let ctx = v8_context_new(isolate);
        assert!(!ctx.is_null());

        // 2. Global object inspection
        let global_val = v8_context_global(ctx);
        assert!(!global_val.is_null());
        assert!(v8_value_is_object(global_val));

        // 3. Value construction & conversion
        let num_val = v8_value_new_number(isolate, 42.5);
        assert!(v8_value_is_number(num_val));
        assert_eq!(42.5, v8_value_to_number(num_val));

        let str_src = CString::new("Chromium Rust V8").unwrap();
        let str_val = v8_value_new_string(isolate, str_src.as_ptr());
        assert!(v8_value_is_string(str_val));
        let mut buf = [0u8; 64];
        let copied = v8_value_to_string(str_val, buf.as_mut_ptr() as *mut std::os::raw::c_char, buf.len());
        assert_eq!(16, copied);
        let read_back = CStr::from_ptr(buf.as_ptr() as *const std::os::raw::c_char).to_str().unwrap();
        assert_eq!("Chromium Rust V8", read_back);

        let bool_val = v8_value_new_boolean(isolate, true);
        assert!(v8_value_is_boolean(bool_val));
        assert_eq!(true, v8_value_to_boolean(bool_val));

        let undef_val = v8_value_new_undefined(isolate);
        assert!(v8_value_is_undefined(undef_val));

        let null_val = v8_value_new_null(isolate);
        assert!(v8_value_is_null(null_val));

        // 4. Object property manipulation via C-ABI
        let prop_name = CString::new("answer").unwrap();
        let ok = v8_object_set(global_val, prop_name.as_ptr(), num_val);
        assert!(ok);

        let retrieved = v8_object_get(global_val, prop_name.as_ptr());
        assert!(!retrieved.is_null());
        assert!(v8_value_is_number(retrieved));
        assert_eq!(42.5, v8_value_to_number(retrieved));

        // 5. Script compilation and execution
        let script_src = CString::new("let x = 100; let y = 200; x + y;").unwrap();
        let script = v8_script_compile(ctx, script_src.as_ptr());
        assert!(!script.is_null());

        let run_res = v8_script_run(script);
        assert!(!run_res.is_null());
        assert!(v8_value_is_number(run_res));
        assert_eq!(300.0, v8_value_to_number(run_res));

        // 6. Function invocation via C-ABI
        let fn_src = CString::new("function add(a, b) { return a + b; } add;").unwrap();
        let fn_script = v8_script_compile(ctx, fn_src.as_ptr());
        let fn_val = v8_script_run(fn_script);
        assert!(v8_value_is_function(fn_val));

        let arg1 = v8_value_new_number(isolate, 35.0);
        let arg2 = v8_value_new_number(isolate, 7.0);
        let args = [arg1, arg2];
        let call_res = v8_function_call(fn_val, std::ptr::null_mut(), 2, args.as_ptr());
        assert!(!call_res.is_null());
        assert_eq!(42.0, v8_value_to_number(call_res));

        // 7. Version metadata
        let ver = v8_version();
        let ver_str = CStr::from_ptr(ver).to_str().unwrap();
        assert!(ver_str.starts_with("12.4"));

        // 8. Proper resource disposal
        v8_value_dispose(call_res);
        v8_value_dispose(arg1);
        v8_value_dispose(arg2);
        v8_value_dispose(fn_val);
        v8_script_dispose(fn_script);
        v8_value_dispose(run_res);
        v8_script_dispose(script);
        v8_value_dispose(retrieved);
        v8_value_dispose(null_val);
        v8_value_dispose(undef_val);
        v8_value_dispose(bool_val);
        v8_value_dispose(str_val);
        v8_value_dispose(num_val);
        v8_value_dispose(global_val);
        v8_context_dispose(ctx);
        v8_isolate_dispose(isolate);
    }
}


