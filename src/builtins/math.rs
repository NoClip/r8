//! Safe Rust reimplementation of Google V8's ECMAScript `Math` built-in object.
//!
//! Provides standard mathematical constants and functions.

use crate::objects::{JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

pub fn create_math_object() -> Rc<RefCell<JSObject>> {
    let math = JSObject::new_empty(None);

    // Mathematical Constants
    JSObject::set_property(&math, "PI", JSValue::Number(std::f64::consts::PI));
    JSObject::set_property(&math, "E", JSValue::Number(std::f64::consts::E));
    JSObject::set_property(&math, "LN2", JSValue::Number(std::f64::consts::LN_2));
    JSObject::set_property(&math, "LN10", JSValue::Number(std::f64::consts::LN_10));
    JSObject::set_property(&math, "LOG2E", JSValue::Number(std::f64::consts::LOG2_E));
    JSObject::set_property(&math, "LOG10E", JSValue::Number(std::f64::consts::LOG10_E));
    JSObject::set_property(&math, "SQRT2", JSValue::Number(std::f64::consts::SQRT_2));
    JSObject::set_property(&math, "SQRT1_2", JSValue::Number(std::f64::consts::FRAC_1_SQRT_2));

    // Math.abs(x)
    JSObject::set_property(
        &math,
        "abs",
        JSValue::Function(JSFunction::new_native("abs", |_this, args| {
            if let Some(arg) = args.first() {
                if let JSValue::Smi(n) = arg {
                    if let Some(res) = n.checked_abs() {
                        return Ok(JSValue::Smi(res));
                    }
                }
                Ok(JSValue::Number(arg.to_number().abs()))
            } else {
                Ok(JSValue::Number(f64::NAN))
            }
        })),
    );

    // Math.floor(x)
    JSObject::set_property(
        &math,
        "floor",
        JSValue::Function(JSFunction::new_native("floor", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            let fl = n.floor();
            if fl >= i32::MIN as f64 && fl <= i32::MAX as f64 && fl == (fl as i32 as f64) {
                Ok(JSValue::Smi(fl as i32))
            } else {
                Ok(JSValue::Number(fl))
            }
        })),
    );

    // Math.ceil(x)
    JSObject::set_property(
        &math,
        "ceil",
        JSValue::Function(JSFunction::new_native("ceil", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            let c = n.ceil();
            if c >= i32::MIN as f64 && c <= i32::MAX as f64 && c == (c as i32 as f64) {
                Ok(JSValue::Smi(c as i32))
            } else {
                Ok(JSValue::Number(c))
            }
        })),
    );

    // Math.round(x)
    JSObject::set_property(
        &math,
        "round",
        JSValue::Function(JSFunction::new_native("round", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            let r = n.round();
            if r >= i32::MIN as f64 && r <= i32::MAX as f64 && r == (r as i32 as f64) {
                Ok(JSValue::Smi(r as i32))
            } else {
                Ok(JSValue::Number(r))
            }
        })),
    );

    // Math.trunc(x)
    JSObject::set_property(
        &math,
        "trunc",
        JSValue::Function(JSFunction::new_native("trunc", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            let t = n.trunc();
            if t >= i32::MIN as f64 && t <= i32::MAX as f64 && t == (t as i32 as f64) {
                Ok(JSValue::Smi(t as i32))
            } else {
                Ok(JSValue::Number(t))
            }
        })),
    );

    // Math.sqrt(x)
    JSObject::set_property(
        &math,
        "sqrt",
        JSValue::Function(JSFunction::new_native("sqrt", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            let s = n.sqrt();
            if s >= 0.0 && s <= i32::MAX as f64 && s == (s as i32 as f64) {
                Ok(JSValue::Smi(s as i32))
            } else {
                Ok(JSValue::Number(s))
            }
        })),
    );

    // Math.pow(x, y)
    JSObject::set_property(
        &math,
        "pow",
        JSValue::Function(JSFunction::new_native("pow", |_this, args| {
            let x = args.get(0).map(|v| v.to_number()).unwrap_or(f64::NAN);
            let y = args.get(1).map(|v| v.to_number()).unwrap_or(f64::NAN);
            let p = x.powf(y);
            if p >= i32::MIN as f64 && p <= i32::MAX as f64 && p == (p as i32 as f64) {
                Ok(JSValue::Smi(p as i32))
            } else {
                Ok(JSValue::Number(p))
            }
        })),
    );

    // Math.min(...args)
    JSObject::set_property(
        &math,
        "min",
        JSValue::Function(JSFunction::new_native("min", |_this, args| {
            if args.is_empty() {
                return Ok(JSValue::Number(f64::INFINITY));
            }
            let mut min_val = f64::INFINITY;
            for a in args {
                let n = a.to_number();
                if n.is_nan() {
                    return Ok(JSValue::Number(f64::NAN));
                }
                if n < min_val {
                    min_val = n;
                }
            }
            if min_val >= i32::MIN as f64 && min_val <= i32::MAX as f64 && min_val == (min_val as i32 as f64) {
                Ok(JSValue::Smi(min_val as i32))
            } else {
                Ok(JSValue::Number(min_val))
            }
        })),
    );

    // Math.max(...args)
    JSObject::set_property(
        &math,
        "max",
        JSValue::Function(JSFunction::new_native("max", |_this, args| {
            if args.is_empty() {
                return Ok(JSValue::Number(f64::NEG_INFINITY));
            }
            let mut max_val = f64::NEG_INFINITY;
            for a in args {
                let n = a.to_number();
                if n.is_nan() {
                    return Ok(JSValue::Number(f64::NAN));
                }
                if n > max_val {
                    max_val = n;
                }
            }
            if max_val >= i32::MIN as f64 && max_val <= i32::MAX as f64 && max_val == (max_val as i32 as f64) {
                Ok(JSValue::Smi(max_val as i32))
            } else {
                Ok(JSValue::Number(max_val))
            }
        })),
    );

    // Math.sign(x)
    JSObject::set_property(
        &math,
        "sign",
        JSValue::Function(JSFunction::new_native("sign", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            if n.is_nan() {
                Ok(JSValue::Number(f64::NAN))
            } else if n > 0.0 {
                Ok(JSValue::Smi(1))
            } else if n < 0.0 {
                Ok(JSValue::Smi(-1))
            } else {
                Ok(JSValue::Smi(0))
            }
        })),
    );

    // Math.random()
    JSObject::set_property(
        &math,
        "random",
        JSValue::Function(JSFunction::new_native("random", |_this, _args| {
            Ok(JSValue::Number(next_random_f64()))
        })),
    );

    // Trigonometric functions
    JSObject::set_property(
        &math,
        "sin",
        JSValue::Function(JSFunction::new_native("sin", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.sin()))
        })),
    );

    JSObject::set_property(
        &math,
        "cos",
        JSValue::Function(JSFunction::new_native("cos", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.cos()))
        })),
    );

    JSObject::set_property(
        &math,
        "tan",
        JSValue::Function(JSFunction::new_native("tan", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.tan()))
        })),
    );

    JSObject::set_property(
        &math,
        "asin",
        JSValue::Function(JSFunction::new_native("asin", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.asin()))
        })),
    );

    JSObject::set_property(
        &math,
        "acos",
        JSValue::Function(JSFunction::new_native("acos", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.acos()))
        })),
    );

    JSObject::set_property(
        &math,
        "atan",
        JSValue::Function(JSFunction::new_native("atan", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.atan()))
        })),
    );

    JSObject::set_property(
        &math,
        "atan2",
        JSValue::Function(JSFunction::new_native("atan2", |_this, args| {
            let y = args.get(0).map(|v| v.to_number()).unwrap_or(f64::NAN);
            let x = args.get(1).map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(y.atan2(x)))
        })),
    );

    // Hyperbolic functions
    JSObject::set_property(
        &math,
        "sinh",
        JSValue::Function(JSFunction::new_native("sinh", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.sinh()))
        })),
    );

    JSObject::set_property(
        &math,
        "cosh",
        JSValue::Function(JSFunction::new_native("cosh", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.cosh()))
        })),
    );

    JSObject::set_property(
        &math,
        "tanh",
        JSValue::Function(JSFunction::new_native("tanh", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.tanh()))
        })),
    );

    JSObject::set_property(
        &math,
        "asinh",
        JSValue::Function(JSFunction::new_native("asinh", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.asinh()))
        })),
    );

    JSObject::set_property(
        &math,
        "acosh",
        JSValue::Function(JSFunction::new_native("acosh", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.acosh()))
        })),
    );

    JSObject::set_property(
        &math,
        "atanh",
        JSValue::Function(JSFunction::new_native("atanh", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.atanh()))
        })),
    );

    // Exponential & Logarithmic functions
    JSObject::set_property(
        &math,
        "exp",
        JSValue::Function(JSFunction::new_native("exp", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.exp()))
        })),
    );

    JSObject::set_property(
        &math,
        "expm1",
        JSValue::Function(JSFunction::new_native("expm1", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.exp_m1()))
        })),
    );

    JSObject::set_property(
        &math,
        "log",
        JSValue::Function(JSFunction::new_native("log", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.ln()))
        })),
    );

    JSObject::set_property(
        &math,
        "log1p",
        JSValue::Function(JSFunction::new_native("log1p", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.ln_1p()))
        })),
    );

    JSObject::set_property(
        &math,
        "log10",
        JSValue::Function(JSFunction::new_native("log10", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.log10()))
        })),
    );

    JSObject::set_property(
        &math,
        "log2",
        JSValue::Function(JSFunction::new_native("log2", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.log2()))
        })),
    );

    // Other math helpers: cbrt, hypot, imul, clz32, fround
    JSObject::set_property(
        &math,
        "cbrt",
        JSValue::Function(JSFunction::new_native("cbrt", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number(n.cbrt()))
        })),
    );

    JSObject::set_property(
        &math,
        "hypot",
        JSValue::Function(JSFunction::new_native("hypot", |_this, args| {
            if args.is_empty() {
                return Ok(JSValue::Smi(0));
            }
            let mut sum = 0.0;
            for a in args {
                let n = a.to_number();
                if n.is_infinite() {
                    return Ok(JSValue::Number(f64::INFINITY));
                }
                if n.is_nan() {
                    return Ok(JSValue::Number(f64::NAN));
                }
                sum += n * n;
            }
            Ok(JSValue::Number(sum.sqrt()))
        })),
    );

    JSObject::set_property(
        &math,
        "imul",
        JSValue::Function(JSFunction::new_native("imul", |_this, args| {
            let x = args.get(0).map(|v| v.to_number() as i64 as i32).unwrap_or(0);
            let y = args.get(1).map(|v| v.to_number() as i64 as i32).unwrap_or(0);
            Ok(JSValue::Smi(x.wrapping_mul(y)))
        })),
    );

    JSObject::set_property(
        &math,
        "clz32",
        JSValue::Function(JSFunction::new_native("clz32", |_this, args| {
            let n = args.first().map(|v| v.to_number() as i64 as u32).unwrap_or(0);
            Ok(JSValue::Smi(n.leading_zeros() as i32))
        })),
    );

    JSObject::set_property(
        &math,
        "fround",
        JSValue::Function(JSFunction::new_native("fround", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            Ok(JSValue::Number((n as f32) as f64))
        })),
    );

    // Math.f16round(x) (ES2025)
    JSObject::set_property(
        &math,
        "f16round",
        JSValue::Function(JSFunction::new_native("f16round", |_this, args| {
            let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
            if n.is_nan() {
                Ok(JSValue::Number(f64::NAN))
            } else {
                let u = crate::objects::typed_array::f32_to_f16(n as f32);
                let f = crate::objects::typed_array::f16_to_f32(u);
                Ok(JSValue::Number(f as f64))
            }
        })),
    );

    math
}

static RNG_STATE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn next_random_f64() -> f64 {
    use std::sync::atomic::Ordering;
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut state = RNG_STATE.load(Ordering::Relaxed);
    if state == 0 {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x12345678_9abcdef0);
        state = seed ^ 0x517cc1b727220a95;
    }
    let z_next = state.wrapping_add(0x9e3779b97f4a7c15);
    RNG_STATE.store(z_next, Ordering::Relaxed);
    let mut z = z_next;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z = z ^ (z >> 31);
    ((z >> 11) as f64) / ((1u64 << 53) as f64)
}
