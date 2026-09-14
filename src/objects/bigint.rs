//! Safe Rust reimplementation of Google V8's BigInt subsystem (`src/objects/bigint.h`).
//!
//! Provides arbitrary-precision integer arithmetic, bitwise logic, and two's complement
//! conversions complying with ECMAScript specifications without external dependencies.

use std::cmp::Ordering;
use std::fmt;

/// Arbitrary-precision integer storage using sign-magnitude representation.
/// Limbs are 64-bit digits stored in little-endian order (limbs[0] is least significant).
#[derive(Clone, Eq)]
pub struct BigIntData {
    pub negative: bool,
    pub limbs: Vec<u64>,
}

impl BigIntData {
    /// Creates a BigInt representing zero.
    #[inline]
    pub fn zero() -> Self {
        Self {
            negative: false,
            limbs: Vec::new(),
        }
    }

    /// Creates a BigInt representing one.
    #[inline]
    pub fn one() -> Self {
        Self {
            negative: false,
            limbs: vec![1],
        }
    }

    /// Normalizes the limb vector by trimming trailing zero limbs and enforcing that zero has no negative sign.
    pub fn normalize(&mut self) {
        while let Some(&0) = self.limbs.last() {
            self.limbs.pop();
        }
        if self.limbs.is_empty() {
            self.negative = false;
        }
    }

    /// Returns true if the BigInt is zero.
    #[inline]
    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty() || self.limbs.iter().all(|&l| l == 0)
    }

    /// Constructs a BigInt from an `i64`.
    pub fn from_i64(n: i64) -> Self {
        if n == 0 {
            return Self::zero();
        }
        let negative = n < 0;
        let mag = if n == i64::MIN {
            (i64::MAX as u64) + 1
        } else {
            n.unsigned_abs()
        };
        Self {
            negative,
            limbs: vec![mag],
        }
    }

    /// Constructs a BigInt from a `u64`.
    pub fn from_u64(n: u64) -> Self {
        if n == 0 {
            Self::zero()
        } else {
            Self {
                negative: false,
                limbs: vec![n],
            }
        }
    }

    /// Parses a string into a BigInt according to ECMAScript rules.
    /// Supports prefixes `0x` (hex), `0o` (octal), `0b` (binary), optional signs `+`/`-`,
    /// numeric separators `_`, and optional trailing `'n'`.
    pub fn from_str(input: &str) -> Result<Self, String> {
        let mut s = input.trim();
        if s.is_empty() {
            return Err("Cannot convert empty string to BigInt".to_string());
        }

        if s.ends_with('n') || s.ends_with('N') {
            s = &s[..s.len() - 1];
        }

        let mut negative = false;
        if s.starts_with('+') {
            s = &s[1..];
        } else if s.starts_with('-') {
            negative = true;
            s = &s[1..];
        }

        if s.is_empty() {
            return Err("Cannot convert string with only sign to BigInt".to_string());
        }

        let (radix, body) = if s.starts_with("0x") || s.starts_with("0X") {
            (16, &s[2..])
        } else if s.starts_with("0o") || s.starts_with("0O") {
            (8, &s[2..])
        } else if s.starts_with("0b") || s.starts_with("0B") {
            (2, &s[2..])
        } else {
            (10, s)
        };

        if body.is_empty() {
            return Err("Invalid BigInt literal format".to_string());
        }

        let mut result = Self::zero();
        let radix_bi = Self::from_u64(radix as u64);

        for c in body.chars() {
            if c == '_' {
                continue;
            }
            let digit = c
                .to_digit(radix)
                .ok_or_else(|| format!("Invalid character '{}' in BigInt literal", c))?;
            result = result.mul(&radix_bi);
            result = result.add(&Self::from_u64(digit as u64));
        }

        result.negative = negative;
        result.normalize();
        Ok(result)
    }

    /// Converts BigInt to decimal string representation.
    pub fn to_string(&self) -> String {
        if self.is_zero() {
            return "0".to_string();
        }

        let mut temp = self.clone();
        temp.negative = false;
        let mut digits = Vec::new();
        let ten = Self::from_u64(10);

        while !temp.is_zero() {
            let (q, r) = temp.div_rem_magnitude(&ten);
            let d = if r.limbs.is_empty() { 0 } else { r.limbs[0] };
            digits.push((b'0' + d as u8) as char);
            temp = q;
        }

        let mut s = String::new();
        if self.negative {
            s.push('-');
        }
        for c in digits.into_iter().rev() {
            s.push(c);
        }
        s
    }

    /// Converts to `f64` approximation.
    pub fn to_f64(&self) -> f64 {
        if self.is_zero() {
            return 0.0;
        }
        let mut val = 0.0f64;
        let base = 18446744073709551616.0; // 2^64
        for &limb in self.limbs.iter().rev() {
            val = val * base + (limb as f64);
        }
        if self.negative {
            -val
        } else {
            val
        }
    }

    /// Truncates / casts to `i64`.
    pub fn to_i64(&self) -> i64 {
        let mag = self.limbs.first().copied().unwrap_or(0);
        if self.negative {
            -(mag as i64)
        } else {
            mag as i64
        }
    }

    /// Truncates / casts to `u64`.
    pub fn to_u64(&self) -> u64 {
        let mag = self.limbs.first().copied().unwrap_or(0);
        if self.negative {
            (mag as i64).wrapping_neg() as u64
        } else {
            mag
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Internal magnitude helpers
    // ─────────────────────────────────────────────────────────────────────────

    fn cmp_magnitude(&self, other: &Self) -> Ordering {
        if self.limbs.len() != other.limbs.len() {
            return self.limbs.len().cmp(&other.limbs.len());
        }
        for (a, b) in self.limbs.iter().rev().zip(other.limbs.iter().rev()) {
            if a != b {
                return a.cmp(b);
            }
        }
        Ordering::Equal
    }

    fn add_magnitude(a: &[u64], b: &[u64]) -> Vec<u64> {
        let len = a.len().max(b.len());
        let mut res = Vec::with_capacity(len + 1);
        let mut carry = 0u128;

        for i in 0..len {
            let val_a = a.get(i).copied().unwrap_or(0) as u128;
            let val_b = b.get(i).copied().unwrap_or(0) as u128;
            let sum = val_a + val_b + carry;
            res.push(sum as u64);
            carry = sum >> 64;
        }
        if carry > 0 {
            res.push(carry as u64);
        }
        res
    }

    /// Requires `a >= b` in magnitude.
    fn sub_magnitude(a: &[u64], b: &[u64]) -> Vec<u64> {
        let mut res = Vec::with_capacity(a.len());
        let mut borrow = 0i128;

        for i in 0..a.len() {
            let val_a = a[i] as i128;
            let val_b = b.get(i).copied().unwrap_or(0) as i128;
            let diff = val_a - val_b - borrow;
            if diff < 0 {
                res.push((diff + (1i128 << 64)) as u64);
                borrow = 1;
            } else {
                res.push(diff as u64);
                borrow = 0;
            }
        }
        while let Some(&0) = res.last() {
            res.pop();
        }
        res
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Arithmetic operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Negation (`-x`).
    pub fn neg(&self) -> Self {
        if self.is_zero() {
            Self::zero()
        } else {
            Self {
                negative: !self.negative,
                limbs: self.limbs.clone(),
            }
        }
    }

    /// Addition (`a + b`).
    pub fn add(&self, other: &Self) -> Self {
        if self.negative == other.negative {
            let limbs = Self::add_magnitude(&self.limbs, &other.limbs);
            let mut res = Self {
                negative: self.negative,
                limbs,
            };
            res.normalize();
            res
        } else {
            match self.cmp_magnitude(other) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let limbs = Self::sub_magnitude(&self.limbs, &other.limbs);
                    let mut res = Self {
                        negative: self.negative,
                        limbs,
                    };
                    res.normalize();
                    res
                }
                Ordering::Less => {
                    let limbs = Self::sub_magnitude(&other.limbs, &self.limbs);
                    let mut res = Self {
                        negative: other.negative,
                        limbs,
                    };
                    res.normalize();
                    res
                }
            }
        }
    }

    /// Subtraction (`a - b`).
    pub fn sub(&self, other: &Self) -> Self {
        self.add(&other.neg())
    }

    /// Multiplication (`a * b`).
    pub fn mul(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }

        let mut res = vec![0u64; self.limbs.len() + other.limbs.len()];
        for (i, &d1) in self.limbs.iter().enumerate() {
            let mut carry = 0u128;
            for (j, &d2) in other.limbs.iter().enumerate() {
                let cur = res[i + j] as u128 + (d1 as u128 * d2 as u128) + carry;
                res[i + j] = cur as u64;
                carry = cur >> 64;
            }
            res[i + other.limbs.len()] += carry as u64;
        }

        let mut out = Self {
            negative: self.negative ^ other.negative,
            limbs: res,
        };
        out.normalize();
        out
    }

    /// Magnitude division and remainder via binary long division.
    fn div_rem_magnitude(&self, divisor: &Self) -> (Self, Self) {
        if divisor.is_zero() {
            panic!("Division by zero");
        }
        if self.cmp_magnitude(divisor) == Ordering::Less {
            return (Self::zero(), self.clone());
        }

        let total_bits = self.limbs.len() * 64;
        let mut quotient = Self::zero();
        let mut remainder = Self::zero();

        for bit in (0..total_bits).rev() {
            remainder = remainder.shl_bits(1);
            let limb_idx = bit / 64;
            let bit_idx = bit % 64;
            if limb_idx < self.limbs.len() && (self.limbs[limb_idx] & (1 << bit_idx)) != 0 {
                remainder = remainder.add(&Self::one());
            }

            if remainder.cmp_magnitude(divisor) != Ordering::Less {
                remainder = Self {
                    negative: false,
                    limbs: Self::sub_magnitude(&remainder.limbs, &divisor.limbs),
                };
                remainder.normalize();

                let q_limb = bit / 64;
                let q_bit = bit % 64;
                while quotient.limbs.len() <= q_limb {
                    quotient.limbs.push(0);
                }
                quotient.limbs[q_limb] |= 1 << q_bit;
            }
        }

        quotient.normalize();
        (quotient, remainder)
    }

    /// Division (`a / b`) with truncation towards zero.
    pub fn div(&self, other: &Self) -> Result<Self, String> {
        if other.is_zero() {
            return Err("RangeError: Division by zero".to_string());
        }
        let (mut q, _) = self.div_rem_magnitude(other);
        q.negative = self.negative ^ other.negative;
        q.normalize();
        Ok(q)
    }

    /// Modulo / Remainder (`a % b`). Result has same sign as dividend `a`.
    pub fn rem(&self, other: &Self) -> Result<Self, String> {
        if other.is_zero() {
            return Err("RangeError: Division by zero".to_string());
        }
        let (_, mut r) = self.div_rem_magnitude(other);
        r.negative = self.negative;
        r.normalize();
        Ok(r)
    }

    /// Exponentiation (`a ** b`).
    pub fn pow(&self, exp: &Self) -> Result<Self, String> {
        if exp.negative {
            return Err("RangeError: BigInt negative exponentiation".to_string());
        }
        if exp.is_zero() {
            return Ok(Self::one());
        }

        let mut base = self.clone();
        let mut e = exp.clone();
        let mut result = Self::one();
        let two = Self::from_u64(2);

        while !e.is_zero() {
            let is_odd = (e.limbs.first().copied().unwrap_or(0) & 1) != 0;
            if is_odd {
                result = result.mul(&base);
            }
            base = base.mul(&base);
            let (next_e, _) = e.div_rem_magnitude(&two);
            e = next_e;
        }

        Ok(result)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Bitwise & Shift operations (Two's complement semantics)
    // ─────────────────────────────────────────────────────────────────────────

    fn shl_bits(&self, shift: usize) -> Self {
        if self.is_zero() || shift == 0 {
            return self.clone();
        }
        let limb_shift = shift / 64;
        let bit_shift = shift % 64;

        let mut res = vec![0u64; limb_shift];
        let mut carry = 0u64;

        for &limb in &self.limbs {
            if bit_shift == 0 {
                res.push(limb);
            } else {
                let low = (limb << bit_shift) | carry;
                carry = limb >> (64 - bit_shift);
                res.push(low);
            }
        }
        if carry > 0 {
            res.push(carry);
        }

        let mut out = Self {
            negative: self.negative,
            limbs: res,
        };
        out.normalize();
        out
    }

    fn shr_bits(&self, shift: usize) -> Self {
        if self.is_zero() || shift == 0 {
            return self.clone();
        }
        let limb_shift = shift / 64;
        let bit_shift = shift % 64;

        if limb_shift >= self.limbs.len() {
            return Self::zero();
        }

        let mut res = Vec::new();
        let mut carry = 0u64;

        for &limb in self.limbs[limb_shift..].iter().rev() {
            if bit_shift == 0 {
                res.push(limb);
            } else {
                let high = (limb >> bit_shift) | carry;
                carry = limb << (64 - bit_shift);
                res.push(high);
            }
        }
        res.reverse();
        while let Some(&0) = res.first() {
            res.remove(0);
        }

        let mut out = Self {
            negative: self.negative,
            limbs: res,
        };
        out.normalize();
        out
    }

    /// Shift Left (`<<`).
    pub fn shl(&self, shift: &Self) -> Result<Self, String> {
        if shift.negative {
            return self.shr(&shift.neg());
        }
        let s = shift.to_i64();
        if s > 1_000_000 {
            return Err("RangeError: Maximum BigInt shift exceeded".to_string());
        }
        Ok(self.shl_bits(s as usize))
    }

    /// Shift Right (`>>`) with arithmetic sign propagation.
    pub fn shr(&self, shift: &Self) -> Result<Self, String> {
        if shift.negative {
            return self.shl(&shift.neg());
        }
        let s = shift.to_i64();
        if self.negative {
            let not_x = self.not();
            let shifted = not_x.shr_bits(s as usize);
            Ok(shifted.not())
        } else {
            Ok(self.shr_bits(s as usize))
        }
    }

    /// Bitwise NOT (`~x = -x - 1`).
    pub fn not(&self) -> Self {
        self.neg().sub(&Self::one())
    }

    /// Bitwise AND (`&`).
    pub fn bitand(&self, other: &Self) -> Self {
        match (self.negative, other.negative) {
            (false, false) => {
                let len = self.limbs.len().min(other.limbs.len());
                let mut limbs = Vec::with_capacity(len);
                for i in 0..len {
                    limbs.push(self.limbs[i] & other.limbs[i]);
                }
                let mut res = Self { negative: false, limbs };
                res.normalize();
                res
            }
            (true, true) => {
                let not_a = self.not();
                let not_b = other.not();
                not_a.bitor(&not_b).not()
            }
            (true, false) => {
                let not_a = self.not();
                let max_len = other.limbs.len();
                let mut limbs = Vec::with_capacity(max_len);
                for i in 0..max_len {
                    let d_b = other.limbs[i];
                    let d_not_a = not_a.limbs.get(i).copied().unwrap_or(0);
                    limbs.push(d_b & !d_not_a);
                }
                let mut res = Self { negative: false, limbs };
                res.normalize();
                res
            }
            (false, true) => other.bitand(self),
        }
    }

    /// Bitwise OR (`|`).
    pub fn bitor(&self, other: &Self) -> Self {
        match (self.negative, other.negative) {
            (false, false) => {
                let len = self.limbs.len().max(other.limbs.len());
                let mut limbs = Vec::with_capacity(len);
                for i in 0..len {
                    let a = self.limbs.get(i).copied().unwrap_or(0);
                    let b = other.limbs.get(i).copied().unwrap_or(0);
                    limbs.push(a | b);
                }
                let mut res = Self { negative: false, limbs };
                res.normalize();
                res
            }
            (true, true) => {
                let not_a = self.not();
                let not_b = other.not();
                not_a.bitand(&not_b).not()
            }
            (true, false) => {
                let not_a = self.not();
                let max_len = not_a.limbs.len().max(other.limbs.len());
                let mut limbs = Vec::with_capacity(max_len);
                for i in 0..max_len {
                    let d_not_a = not_a.limbs.get(i).copied().unwrap_or(0);
                    let d_b = other.limbs.get(i).copied().unwrap_or(0);
                    limbs.push(d_not_a & !d_b);
                }
                let not_res = Self { negative: false, limbs };
                not_res.not()
            }
            (false, true) => other.bitor(self),
        }
    }

    /// Bitwise XOR (`^`).
    pub fn bitxor(&self, other: &Self) -> Self {
        match (self.negative, other.negative) {
            (false, false) => {
                let len = self.limbs.len().max(other.limbs.len());
                let mut limbs = Vec::with_capacity(len);
                for i in 0..len {
                    let a = self.limbs.get(i).copied().unwrap_or(0);
                    let b = other.limbs.get(i).copied().unwrap_or(0);
                    limbs.push(a ^ b);
                }
                let mut res = Self { negative: false, limbs };
                res.normalize();
                res
            }
            (true, true) => {
                self.not().bitxor(&other.not())
            }
            (true, false) => {
                self.not().bitxor(other).not()
            }
            (false, true) => other.bitxor(self),
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // ECMAScript BigInt static helpers: asIntN and asUintN
    // ─────────────────────────────────────────────────────────────────────────

    /// Clamps value to unsigned `bits`-bit integer modulo `2^bits`.
    pub fn as_uint_n(bits: usize, bi: &Self) -> Self {
        if bits == 0 {
            return Self::zero();
        }
        let two = Self::from_u64(2);
        let mask_mod = two.pow(&Self::from_u64(bits as u64)).unwrap();
        let mut res = bi.rem(&mask_mod).unwrap();
        if res.negative {
            res = res.add(&mask_mod);
        }
        res
    }

    /// Clamps value to signed `bits`-bit integer in `[-2^(bits-1), 2^(bits-1) - 1]`.
    pub fn as_int_n(bits: usize, bi: &Self) -> Self {
        if bits == 0 {
            return Self::zero();
        }
        let uint_val = Self::as_uint_n(bits, bi);
        let two = Self::from_u64(2);
        let bound = two.pow(&Self::from_u64((bits - 1) as u64)).unwrap();
        if uint_val.cmp_magnitude(&bound) != Ordering::Less {
            let full = two.pow(&Self::from_u64(bits as u64)).unwrap();
            uint_val.sub(&full)
        } else {
            uint_val
        }
    }
}

impl PartialEq for BigIntData {
    fn eq(&self, other: &Self) -> bool {
        if self.negative != other.negative {
            return false;
        }
        self.limbs == other.limbs
    }
}

impl PartialOrd for BigIntData {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BigIntData {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.negative && !other.negative {
            return Ordering::Less;
        }
        if !self.negative && other.negative {
            return Ordering::Greater;
        }
        if self.negative {
            other.cmp_magnitude(self)
        } else {
            self.cmp_magnitude(other)
        }
    }
}

impl fmt::Debug for BigIntData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}n", self.to_string())
    }
}

impl fmt::Display for BigIntData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}
