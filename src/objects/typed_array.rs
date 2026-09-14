//! Safe Rust reimplementation of Google V8's TypedArray and DataView data structures.
//!
//! Implements buffer-backed typed views (Int8Array, Uint8Array, Uint8ClampedArray,
//! Int16Array, Uint16Array, Int32Array, Uint32Array, Float32Array, Float64Array)
//! and DataView with little/big-endian binary conversions.

use super::js_object::JSObject;
use super::value::JSValue;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TypedArrayKind {
    Int8,
    Uint8,
    Uint8Clamped,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Float16,
    Float32,
    Float64,
    BigInt64,
    BigUint64,
}

impl TypedArrayKind {
    #[inline(always)]
    pub fn element_size(&self) -> usize {
        match self {
            TypedArrayKind::Int8 | TypedArrayKind::Uint8 | TypedArrayKind::Uint8Clamped => 1,
            TypedArrayKind::Int16 | TypedArrayKind::Uint16 | TypedArrayKind::Float16 => 2,
            TypedArrayKind::Int32 | TypedArrayKind::Uint32 | TypedArrayKind::Float32 => 4,
            TypedArrayKind::Float64 | TypedArrayKind::BigInt64 | TypedArrayKind::BigUint64 => 8,
        }
    }

    #[inline(always)]
    pub fn name(&self) -> &'static str {
        match self {
            TypedArrayKind::Int8 => "Int8Array",
            TypedArrayKind::Uint8 => "Uint8Array",
            TypedArrayKind::Uint8Clamped => "Uint8ClampedArray",
            TypedArrayKind::Int16 => "Int16Array",
            TypedArrayKind::Uint16 => "Uint16Array",
            TypedArrayKind::Int32 => "Int32Array",
            TypedArrayKind::Uint32 => "Uint32Array",
            TypedArrayKind::Float16 => "Float16Array",
            TypedArrayKind::Float32 => "Float32Array",
            TypedArrayKind::Float64 => "Float64Array",
            TypedArrayKind::BigInt64 => "BigInt64Array",
            TypedArrayKind::BigUint64 => "BigUint64Array",
        }
    }

    #[inline(always)]
    pub fn read_element(&self, bytes: &[u8], byte_offset: usize, index: usize) -> JSValue {
        let size = self.element_size();
        let pos = byte_offset + index * size;
        if pos + size > bytes.len() {
            return JSValue::Undefined;
        }

        match self {
            TypedArrayKind::Int8 => JSValue::Smi(bytes[pos] as i8 as i32),
            TypedArrayKind::Uint8 | TypedArrayKind::Uint8Clamped => JSValue::Smi(bytes[pos] as u8 as i32),
            TypedArrayKind::Int16 => {
                let val = i16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
                JSValue::Smi(val as i32)
            }
            TypedArrayKind::Uint16 => {
                let val = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
                JSValue::Smi(val as i32)
            }
            TypedArrayKind::Int32 => {
                let val = i32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
                JSValue::Smi(val)
            }
            TypedArrayKind::Uint32 => {
                let val = u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
                JSValue::Number(val as f64)
            }
            TypedArrayKind::Float16 => {
                let u = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
                JSValue::Number(f16_to_f32(u) as f64)
            }
            TypedArrayKind::Float32 => {
                let val = f32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
                JSValue::Number(val as f64)
            }
            TypedArrayKind::Float64 => {
                let mut b = [0u8; 8];
                b.copy_from_slice(&bytes[pos..pos + 8]);
                let val = f64::from_le_bytes(b);
                JSValue::Number(val)
            }
            TypedArrayKind::BigInt64 => {
                let mut b = [0u8; 8];
                b.copy_from_slice(&bytes[pos..pos + 8]);
                let val = i64::from_le_bytes(b);
                JSValue::BigInt(Rc::new(crate::objects::bigint::BigIntData::from_i64(val)))
            }
            TypedArrayKind::BigUint64 => {
                let mut b = [0u8; 8];
                b.copy_from_slice(&bytes[pos..pos + 8]);
                let val = u64::from_le_bytes(b);
                JSValue::BigInt(Rc::new(crate::objects::bigint::BigIntData::from_u64(val)))
            }
        }
    }

    #[inline(always)]
    pub fn write_element(&self, bytes: &mut [u8], byte_offset: usize, index: usize, val: &JSValue) {
        let size = self.element_size();
        let pos = byte_offset + index * size;
        if pos + size > bytes.len() {
            return;
        }

        match self {
            TypedArrayKind::Int8 => {
                let n = match val {
                    JSValue::Smi(s) => *s as i8,
                    _ => val.to_number() as i64 as i8,
                };
                bytes[pos] = n as u8;
            }
            TypedArrayKind::Uint8 => {
                let n = match val {
                    JSValue::Smi(s) => *s as u8,
                    _ => val.to_number() as i64 as u8,
                };
                bytes[pos] = n;
            }
            TypedArrayKind::Uint8Clamped => {
                let num = val.to_number();
                let clamped = if num <= 0.0 {
                    0u8
                } else if num >= 255.0 {
                    255u8
                } else {
                    num.round() as u8
                };
                bytes[pos] = clamped;
            }
            TypedArrayKind::Int16 => {
                let n = match val {
                    JSValue::Smi(s) => *s as i16,
                    _ => val.to_number() as i64 as i16,
                };
                let b = n.to_le_bytes();
                bytes[pos..pos + 2].copy_from_slice(&b);
            }
            TypedArrayKind::Uint16 => {
                let n = match val {
                    JSValue::Smi(s) => *s as u16,
                    _ => val.to_number() as i64 as u16,
                };
                let b = n.to_le_bytes();
                bytes[pos..pos + 2].copy_from_slice(&b);
            }
            TypedArrayKind::Int32 => {
                let n = match val {
                    JSValue::Smi(s) => *s,
                    _ => val.to_number() as i64 as i32,
                };
                let b = n.to_le_bytes();
                bytes[pos..pos + 4].copy_from_slice(&b);
            }
            TypedArrayKind::Uint32 => {
                let n = match val {
                    JSValue::Smi(s) => *s as u32,
                    _ => val.to_number() as i64 as u32,
                };
                let b = n.to_le_bytes();
                bytes[pos..pos + 4].copy_from_slice(&b);
            }
            TypedArrayKind::Float16 => {
                let f = val.to_number() as f32;
                let u = f32_to_f16(f);
                let b = u.to_le_bytes();
                bytes[pos..pos + 2].copy_from_slice(&b);
            }
            TypedArrayKind::Float32 => {
                let f = val.to_number() as f32;
                let b = f.to_le_bytes();
                bytes[pos..pos + 4].copy_from_slice(&b);
            }
            TypedArrayKind::Float64 => {
                let f = val.to_number();
                let b = f.to_le_bytes();
                bytes[pos..pos + 8].copy_from_slice(&b);
            }
            TypedArrayKind::BigInt64 => {
                let n = match val {
                    JSValue::BigInt(bi) => bi.to_i64(),
                    _ => val.to_number() as i64,
                };
                let b = n.to_le_bytes();
                bytes[pos..pos + 8].copy_from_slice(&b);
            }
            TypedArrayKind::BigUint64 => {
                let n = match val {
                    JSValue::BigInt(bi) => bi.to_u64(),
                    _ => val.to_number() as u64,
                };
                let b = n.to_le_bytes();
                bytes[pos..pos + 8].copy_from_slice(&b);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct TypedArrayData {
    pub kind: TypedArrayKind,
    pub buffer: Rc<RefCell<JSObject>>,
    pub byte_offset: usize,
    pub length: usize,
}

impl TypedArrayData {
    pub fn byte_length(&self) -> usize {
        self.length * self.kind.element_size()
    }
}

#[derive(Clone, Debug)]
pub struct DataViewData {
    pub buffer: Rc<RefCell<JSObject>>,
    pub byte_offset: usize,
    pub byte_length: usize,
}

/// Decodes an IEEE 754-2008 16-bit half-precision floating point number to f32.
#[inline(always)]
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as u32;
    let mant = (h & 0x3FF) as u32;

    if exp == 0 {
        if mant == 0 {
            return f32::from_bits(sign << 31);
        }
        // Subnormal: (-1)^sign * 2^(-14) * (mant / 1024)
        let val = (mant as f32) / 1024.0 * (2.0f32.powi(-14));
        if sign == 1 { -val } else { val }
    } else if exp == 31 {
        if mant == 0 {
            f32::from_bits((sign << 31) | 0x7F800000)
        } else {
            f32::from_bits((sign << 31) | 0x7F800000 | (mant << 13))
        }
    } else {
        let f32_exp = exp + 127 - 15;
        let f32_mant = mant << 13;
        f32::from_bits((sign << 31) | (f32_exp << 23) | f32_mant)
    }
}

/// Encodes an f32 into an IEEE 754-2008 16-bit half-precision floating point number.
#[inline(always)]
pub fn f32_to_f16(val: f32) -> u16 {
    let bits = val.to_bits();
    let sign = ((bits >> 31) & 1) as u16;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mant = bits & 0x7FFFFF;

    if exp == 255 {
        if mant == 0 {
            // Infinity
            (sign << 15) | 0x7C00
        } else {
            // NaN
            (sign << 15) | 0x7C00 | ((mant >> 13).max(1) as u16)
        }
    } else {
        let f16_exp = exp - 127 + 15;
        if f16_exp >= 31 {
            // Overflow to Infinity
            (sign << 15) | 0x7C00
        } else if f16_exp <= 0 {
            // Underflow / subnormal
            if f16_exp < -10 {
                sign << 15
            } else {
                let full_mant = mant | 0x800000;
                let shift = (14 - f16_exp) as u32;
                let f16_mant = (full_mant >> shift) as u16;
                (sign << 15) | f16_mant
            }
        } else {
            let f16_mant = ((mant + 0x1000) >> 13) as u16;
            (sign << 15) | ((f16_exp as u16) << 10) | (f16_mant & 0x3FF)
        }
    }
}
