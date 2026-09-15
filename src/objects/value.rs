//! Safe Rust reimplementation of Google V8's runtime value system (`JSValue`).
//!
//! Encapsulates tagged JS values: Smi, Number, Boolean, String, Null, Undefined,
//! JSObject, and JSArray, along with full ECMAScript type coercion and operator semantics.

use super::bigint::BigIntData;
use super::function::JSFunction;
use super::js_object::JSObject;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// Heap-allocated representation for JavaScript Symbol primitive data.
#[derive(Clone, Debug, PartialEq)]
pub struct SymbolData {
    pub id: u32,
    pub description: Option<String>,
}

#[derive(Clone, Debug)]
pub enum JSValue {
    Smi(i32),
    Number(f64),
    Boolean(bool),
    String(String),
    Null,
    Undefined,
    Object(Rc<RefCell<JSObject>>),
    Array(Rc<RefCell<JSObject>>),
    Function(Rc<JSFunction>),
    Symbol(Rc<SymbolData>),
    BigInt(Rc<BigIntData>),
}

// Compile-time static assertion guaranteeing JSValue stays at or below 32 bytes (mem-assert-type-size).
const _: () = assert!(std::mem::size_of::<JSValue>() <= 32);

impl PartialEq for JSValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (JSValue::Smi(a), JSValue::Smi(b)) => a == b,
            (JSValue::Number(a), JSValue::Number(b)) => {
                if a.is_nan() || b.is_nan() {
                    false
                } else {
                    a == b
                }
            }
            (JSValue::Boolean(a), JSValue::Boolean(b)) => a == b,
            (JSValue::String(a), JSValue::String(b)) => a == b,
            (JSValue::Null, JSValue::Null) => true,
            (JSValue::Undefined, JSValue::Undefined) => true,
            (JSValue::Object(a), JSValue::Object(b)) => Rc::ptr_eq(a, b),
            (JSValue::Array(a), JSValue::Array(b)) => Rc::ptr_eq(a, b),
            (JSValue::Function(a), JSValue::Function(b)) => Rc::ptr_eq(a, b),
            (JSValue::Symbol(a), JSValue::Symbol(b)) => a.id == b.id,
            (JSValue::BigInt(a), JSValue::BigInt(b)) => a == b,

            // ECMAScript loose equality conversions (==)
            (JSValue::Smi(a), JSValue::Number(b)) => (*a as f64) == *b,
            (JSValue::Number(a), JSValue::Smi(b)) => *a == (*b as f64),
            (JSValue::Null, JSValue::Undefined) | (JSValue::Undefined, JSValue::Null) => true,
            (JSValue::BigInt(a), JSValue::Smi(b)) => a.to_i64() == (*b as i64) && a.limbs.len() <= 1,
            (JSValue::Smi(a), JSValue::BigInt(b)) => (*a as i64) == b.to_i64() && b.limbs.len() <= 1,
            (JSValue::BigInt(a), JSValue::Number(b)) => a.to_f64() == *b,
            (JSValue::Number(a), JSValue::BigInt(b)) => *a == b.to_f64(),
            (JSValue::BigInt(a), JSValue::String(b)) => {
                if let Ok(bi) = BigIntData::from_str(b) {
                    **a == bi
                } else {
                    false
                }
            }
            (JSValue::String(a), JSValue::BigInt(b)) => {
                if let Ok(bi) = BigIntData::from_str(a) {
                    bi == **b
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

impl Default for JSValue {
    #[inline(always)]
    fn default() -> Self {
        JSValue::Undefined
    }
}

impl JSValue {
    /// ECMAScript ToBoolean conversion.
    #[inline]
    pub fn to_boolean(&self) -> bool {
        match self {
            JSValue::Boolean(b) => *b,
            JSValue::Smi(n) => *n != 0,
            JSValue::Number(f) => *f != 0.0 && !f.is_nan(),
            JSValue::String(s) => !s.is_empty(),
            JSValue::Null | JSValue::Undefined => false,
            JSValue::Object(_) | JSValue::Array(_) | JSValue::Function(_) | JSValue::Symbol(_) => true,
            JSValue::BigInt(b) => !b.is_zero(),
        }
    }

    /// ECMAScript ToNumber conversion.
    #[inline]
    pub fn to_number(&self) -> f64 {
        match self {
            JSValue::Smi(n) => *n as f64,
            JSValue::Number(f) => *f,
            JSValue::Boolean(true) => 1.0,
            JSValue::Boolean(false) | JSValue::Null => 0.0,
            JSValue::Undefined | JSValue::Symbol(_) => f64::NAN,
            JSValue::BigInt(b) => b.to_f64(),
            JSValue::String(s) => s.trim().parse::<f64>().unwrap_or(f64::NAN),
            JSValue::Object(_) | JSValue::Function(_) => f64::NAN,
            JSValue::Array(arr) => {
                let borrowed = arr.borrow();
                if borrowed.elements.is_empty() {
                    0.0
                } else if borrowed.elements.len() == 1 {
                    borrowed.elements[0].to_number()
                } else {
                    f64::NAN
                }
            }
        }
    }

    /// ECMAScript ToString conversion.
    pub fn to_string_val(&self) -> String {
        match self {
            JSValue::String(s) => s.clone(),
            JSValue::Smi(n) => n.to_string(),
            JSValue::Number(f) => {
                if f.is_nan() {
                    "NaN".to_string()
                } else if *f == f64::INFINITY {
                    "Infinity".to_string()
                } else if *f == f64::NEG_INFINITY {
                    "-Infinity".to_string()
                } else {
                    f.to_string()
                }
            }
            JSValue::Boolean(b) => b.to_string(),
            JSValue::Null => "null".to_string(),
            JSValue::Undefined => "undefined".to_string(),
            JSValue::BigInt(b) => b.to_string(),
            JSValue::Object(_) => "[object Object]".to_string(),
            JSValue::Array(arr) => {
                let borrowed = arr.borrow();
                let parts: Vec<String> = borrowed.elements.iter().map(|e| e.to_string_val()).collect();
                parts.join(",")
            }
            JSValue::Function(func) => func.to_string(),
            JSValue::Symbol(s) => match &s.description {
                Some(d) => format!("Symbol({})", d),
                None => "Symbol()".to_string(),
            },
        }
    }

    /// ECMAScript addition (+) supporting string concatenation and numeric addition.
    #[inline]
    pub fn add(&self, other: &JSValue) -> JSValue {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return JSValue::BigInt(Rc::new(a.add(b)));
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            if let Some(res) = a.checked_add(*b) {
                return JSValue::Smi(res);
            }
            return JSValue::Number(*a as f64 + *b as f64);
        }
        if let (JSValue::String(a), JSValue::String(b)) = (self, other) {
            let mut res = String::with_capacity(a.len() + b.len());
            res.push_str(a);
            res.push_str(b);
            return JSValue::String(res);
        }
        if let (JSValue::String(a), b) = (self, other) {
            let bs = b.to_string_val();
            let mut res = String::with_capacity(a.len() + bs.len());
            res.push_str(a);
            res.push_str(&bs);
            return JSValue::String(res);
        }
        if let (a, JSValue::String(b)) = (self, other) {
            let as_str = a.to_string_val();
            let mut res = String::with_capacity(as_str.len() + b.len());
            res.push_str(&as_str);
            res.push_str(b);
            return JSValue::String(res);
        }
        JSValue::Number(self.to_number() + other.to_number())
    }

    /// ECMAScript subtraction (-).
    #[inline]
    pub fn sub(&self, other: &JSValue) -> JSValue {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return JSValue::BigInt(Rc::new(a.sub(b)));
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            if let Some(res) = a.checked_sub(*b) {
                return JSValue::Smi(res);
            }
            return JSValue::Number(*a as f64 - *b as f64);
        }
        if let (JSValue::Number(a), JSValue::Number(b)) = (self, other) {
            return JSValue::Number(a - b);
        }
        JSValue::Number(self.to_number() - other.to_number())
    }

    /// ECMAScript multiplication (*).
    #[inline]
    pub fn mul(&self, other: &JSValue) -> JSValue {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return JSValue::BigInt(Rc::new(a.mul(b)));
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            if let Some(res) = a.checked_mul(*b) {
                return JSValue::Smi(res);
            }
            return JSValue::Number(*a as f64 * *b as f64);
        }
        if let (JSValue::Number(a), JSValue::Number(b)) = (self, other) {
            return JSValue::Number(a * b);
        }
        JSValue::Number(self.to_number() * other.to_number())
    }

    /// ECMAScript division (/).
    #[inline]
    pub fn div(&self, other: &JSValue) -> JSValue {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return match a.div(b) {
                Ok(q) => JSValue::BigInt(Rc::new(q)),
                Err(_) => JSValue::BigInt(Rc::new(BigIntData::zero())),
            };
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            if *b != 0 && *a % *b == 0 {
                return JSValue::Smi(a / b);
            }
            let b_f = *b as f64;
            if b_f == 0.0 {
                let a_f = *a as f64;
                if a_f > 0.0 {
                    return JSValue::Number(f64::INFINITY);
                } else if a_f < 0.0 {
                    return JSValue::Number(f64::NEG_INFINITY);
                } else {
                    return JSValue::Number(f64::NAN);
                }
            }
            return JSValue::Number(*a as f64 / b_f);
        }
        if let (JSValue::Number(a), JSValue::Number(b)) = (self, other) {
            if *b == 0.0 {
                if *a > 0.0 {
                    return JSValue::Number(f64::INFINITY);
                } else if *a < 0.0 {
                    return JSValue::Number(f64::NEG_INFINITY);
                } else {
                    return JSValue::Number(f64::NAN);
                }
            }
            return JSValue::Number(a / b);
        }
        let b = other.to_number();
        if b == 0.0 {
            let a = self.to_number();
            if a > 0.0 {
                return JSValue::Number(f64::INFINITY);
            } else if a < 0.0 {
                return JSValue::Number(f64::NEG_INFINITY);
            } else {
                return JSValue::Number(f64::NAN);
            }
        }
        JSValue::Number(self.to_number() / b)
    }

    /// ECMAScript remainder (%).
    #[inline]
    pub fn modulo(&self, other: &JSValue) -> JSValue {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return match a.rem(b) {
                Ok(r) => JSValue::BigInt(Rc::new(r)),
                Err(_) => JSValue::BigInt(Rc::new(BigIntData::zero())),
            };
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            if *b != 0 {
                return JSValue::Smi(a % b);
            }
        }
        let b = other.to_number();
        if b == 0.0 {
            return JSValue::Number(f64::NAN);
        }
        JSValue::Number(self.to_number() % b)
    }

    /// ECMAScript exponentiation (**).
    #[inline]
    pub fn exp(&self, other: &JSValue) -> JSValue {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return match a.pow(b) {
                Ok(res) => JSValue::BigInt(Rc::new(res)),
                Err(_) => JSValue::BigInt(Rc::new(BigIntData::zero())),
            };
        }
        JSValue::Number(self.to_number().powf(other.to_number()))
    }

    /// Bitwise AND (&).
    #[inline]
    pub fn bitwise_and(&self, other: &JSValue) -> JSValue {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return JSValue::BigInt(Rc::new(a.bitand(b)));
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            return JSValue::Smi(a & b);
        }
        let a = self.to_number() as i32;
        let b = other.to_number() as i32;
        JSValue::Smi(a & b)
    }

    /// Bitwise OR (|).
    #[inline]
    pub fn bitwise_or(&self, other: &JSValue) -> JSValue {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return JSValue::BigInt(Rc::new(a.bitor(b)));
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            return JSValue::Smi(a | b);
        }
        let a = self.to_number() as i32;
        let b = other.to_number() as i32;
        JSValue::Smi(a | b)
    }

    /// Bitwise XOR (^).
    #[inline]
    pub fn bitwise_xor(&self, other: &JSValue) -> JSValue {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return JSValue::BigInt(Rc::new(a.bitxor(b)));
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            return JSValue::Smi(a ^ b);
        }
        let a = self.to_number() as i32;
        let b = other.to_number() as i32;
        JSValue::Smi(a ^ b)
    }

    /// Shift Left (<<).
    #[inline]
    pub fn shift_left(&self, other: &JSValue) -> JSValue {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return match a.shl(b) {
                Ok(res) => JSValue::BigInt(Rc::new(res)),
                Err(_) => JSValue::BigInt(Rc::new(BigIntData::zero())),
            };
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            return JSValue::Smi(a << ((*b as u32) & 0x1f));
        }
        let a = self.to_number() as i32;
        let b = (other.to_number() as u32) & 0x1f;
        JSValue::Smi(a << b)
    }

    /// Shift Right signed (>>).
    #[inline]
    pub fn shift_right(&self, other: &JSValue) -> JSValue {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return match a.shr(b) {
                Ok(res) => JSValue::BigInt(Rc::new(res)),
                Err(_) => JSValue::BigInt(Rc::new(BigIntData::zero())),
            };
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            return JSValue::Smi(a >> ((*b as u32) & 0x1f));
        }
        let a = self.to_number() as i32;
        let b = (other.to_number() as u32) & 0x1f;
        JSValue::Smi(a >> b)
    }

    /// Shift Right unsigned (>>>).
    #[inline]
    pub fn shift_right_logical(&self, other: &JSValue) -> JSValue {
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            let u = *a as u32;
            return JSValue::Number((u >> ((*b as u32) & 0x1f)) as f64);
        }
        let a = self.to_number() as u32;
        let b = (other.to_number() as u32) & 0x1f;
        JSValue::Number((a >> b) as f64)
    }

    /// Abstract equality comparison (==).
    #[inline]
    pub fn test_equal(&self, other: &JSValue) -> bool {
        self == other
    }

    /// Strict equality comparison (===).
    #[inline]
    pub fn test_equal_strict(&self, other: &JSValue) -> bool {
        match (self, other) {
            (JSValue::Smi(a), JSValue::Smi(b)) => a == b,
            (JSValue::Number(a), JSValue::Number(b)) => {
                if a.is_nan() || b.is_nan() {
                    false
                } else {
                    a == b
                }
            }
            (JSValue::Smi(a), JSValue::Number(b)) => (*a as f64) == *b,
            (JSValue::Number(a), JSValue::Smi(b)) => *a == (*b as f64),
            (JSValue::Boolean(a), JSValue::Boolean(b)) => a == b,
            (JSValue::String(a), JSValue::String(b)) => a == b,
            (JSValue::Null, JSValue::Null) => true,
            (JSValue::Undefined, JSValue::Undefined) => true,
            (JSValue::Object(a), JSValue::Object(b)) => Rc::ptr_eq(a, b),
            (JSValue::Array(a), JSValue::Array(b)) => Rc::ptr_eq(a, b),
            (JSValue::Function(a), JSValue::Function(b)) => Rc::ptr_eq(a, b),
            (JSValue::Symbol(a), JSValue::Symbol(b)) => a.id == b.id,
            (JSValue::BigInt(a), JSValue::BigInt(b)) => a == b,
            _ => false,
        }
    }

    /// Less-than comparison (<).
    #[inline]
    pub fn test_less_than(&self, other: &JSValue) -> bool {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return a < b;
        }
        if let (JSValue::BigInt(a), JSValue::Smi(b)) = (self, other) {
            return **a < BigIntData::from_i64(*b as i64);
        }
        if let (JSValue::Smi(a), JSValue::BigInt(b)) = (self, other) {
            return BigIntData::from_i64(*a as i64) < **b;
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            return a < b;
        }
        if let (JSValue::String(a), JSValue::String(b)) = (self, other) {
            return a < b;
        }
        self.to_number() < other.to_number()
    }

    /// Greater-than comparison (>).
    #[inline]
    pub fn test_greater_than(&self, other: &JSValue) -> bool {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return a > b;
        }
        if let (JSValue::BigInt(a), JSValue::Smi(b)) = (self, other) {
            return **a > BigIntData::from_i64(*b as i64);
        }
        if let (JSValue::Smi(a), JSValue::BigInt(b)) = (self, other) {
            return BigIntData::from_i64(*a as i64) > **b;
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            return a > b;
        }
        if let (JSValue::String(a), JSValue::String(b)) = (self, other) {
            return a > b;
        }
        self.to_number() > other.to_number()
    }

    /// Less-than-or-equal comparison (<=).
    #[inline]
    pub fn test_less_than_or_equal(&self, other: &JSValue) -> bool {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return a <= b;
        }
        if let (JSValue::BigInt(a), JSValue::Smi(b)) = (self, other) {
            return **a <= BigIntData::from_i64(*b as i64);
        }
        if let (JSValue::Smi(a), JSValue::BigInt(b)) = (self, other) {
            return BigIntData::from_i64(*a as i64) <= **b;
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            return a <= b;
        }
        if let (JSValue::String(a), JSValue::String(b)) = (self, other) {
            return a <= b;
        }
        self.to_number() <= other.to_number()
    }

    /// Greater-than-or-equal comparison (>=).
    #[inline]
    pub fn test_greater_than_or_equal(&self, other: &JSValue) -> bool {
        if let (JSValue::BigInt(a), JSValue::BigInt(b)) = (self, other) {
            return a >= b;
        }
        if let (JSValue::BigInt(a), JSValue::Smi(b)) = (self, other) {
            return **a >= BigIntData::from_i64(*b as i64);
        }
        if let (JSValue::Smi(a), JSValue::BigInt(b)) = (self, other) {
            return BigIntData::from_i64(*a as i64) >= **b;
        }
        if let (JSValue::Smi(a), JSValue::Smi(b)) = (self, other) {
            return a >= b;
        }
        if let (JSValue::String(a), JSValue::String(b)) = (self, other) {
            return a >= b;
        }
        self.to_number() >= other.to_number()
    }

    pub fn mod_op(&self, other: &JSValue) -> JSValue {
        self.modulo(other)
    }

    pub fn strict_equal(&self, other: &JSValue) -> bool {
        self.test_equal_strict(other)
    }

    pub fn less_than(&self, other: &JSValue) -> bool {
        self.test_less_than(other)
    }

    pub fn greater_than(&self, other: &JSValue) -> bool {
        self.test_greater_than(other)
    }

    pub fn less_than_or_equal(&self, other: &JSValue) -> bool {
        self.test_less_than_or_equal(other)
    }

    pub fn greater_than_or_equal(&self, other: &JSValue) -> bool {
        self.test_greater_than_or_equal(other)
    }
}

impl fmt::Display for JSValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_val())
    }
}
