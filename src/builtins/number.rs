//! Safe Rust reimplementation of Google V8's ECMAScript `Number` constructor and prototype.
//!
//! Implements IEEE 754 number conversions, constants (`MAX_SAFE_INTEGER`, `EPSILON`),
//! static validation methods (`isInteger`, `isSafeInteger`), and formatting (`toFixed`).

use crate::objects::{JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

/// Creates the `Number.prototype` object.
pub fn create_number_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);
    JSObject::set_property(&proto, "name", JSValue::String("Number".to_string()));

    // Number.prototype.valueOf()
    let value_of_fn = JSFunction::new_native("valueOf", |this, _args| {
        match this {
            JSValue::Smi(n) => Ok(JSValue::Smi(*n)),
            JSValue::Number(f) => Ok(JSValue::Number(*f)),
            JSValue::Object(obj) => {
                let v = obj.borrow().get_property("__value__");
                if v != JSValue::Undefined {
                    Ok(v)
                } else {
                    Ok(JSValue::Smi(0))
                }
            }
            _ => Err("TypeError: Number.prototype.valueOf requires that 'this' be a Number".to_string()),
        }
    });
    JSObject::set_property(&proto, "valueOf", JSValue::Function(value_of_fn));

    // Number.prototype.toString(radix?)
    let to_string_fn = JSFunction::new_native("toString", |this, args| {
        let num = match this {
            JSValue::Smi(n) => *n as f64,
            JSValue::Number(f) => *f,
            JSValue::Object(obj) => {
                let v = obj.borrow().get_property("__value__");
                v.to_number()
            }
            _ => return Err("TypeError: Number.prototype.toString requires that 'this' be a Number".to_string()),
        };

        let radix = args.first().map(|v| v.to_number() as u32).unwrap_or(10);
        if !(2..=36).contains(&radix) {
            return Err("RangeError: toString() radix must be an integer between 2 and 36".to_string());
        }

        if radix == 10 {
            if num.is_nan() {
                Ok(JSValue::String("NaN".to_string()))
            } else if num.is_infinite() {
                if num > 0.0 { Ok(JSValue::String("Infinity".to_string())) } else { Ok(JSValue::String("-Infinity".to_string())) }
            } else if num == 0.0 {
                Ok(JSValue::String("0".to_string()))
            } else if num.fract() == 0.0 && num >= i64::MIN as f64 && num <= i64::MAX as f64 {
                Ok(JSValue::String((num as i64).to_string()))
            } else {
                Ok(JSValue::String(num.to_string()))
            }
        } else if num.is_nan() || num.is_infinite() {
            Ok(JSValue::String(num.to_string()))
        } else {
            let int_part = num as i64;
            let mut n = int_part.abs();
            let mut digits = Vec::new();
            if n == 0 {
                digits.push('0');
            } else {
                while n > 0 {
                    let rem = (n % (radix as i64)) as u8;
                    let ch = if rem < 10 { (b'0' + rem) as char } else { (b'a' + rem - 10) as char };
                    digits.push(ch);
                    n /= radix as i64;
                }
            }
            if int_part < 0 { digits.push('-'); }
            digits.reverse();
            Ok(JSValue::String(digits.into_iter().collect()))
        }
    });
    JSObject::set_property(&proto, "toString", JSValue::Function(to_string_fn));

    // Number.prototype.toFixed(digits?)
    let to_fixed_fn = JSFunction::new_native("toFixed", |this, args| {
        let num = match this {
            JSValue::Smi(n) => *n as f64,
            JSValue::Number(f) => *f,
            JSValue::Object(obj) => obj.borrow().get_property("__value__").to_number(),
            _ => return Err("TypeError: Number.prototype.toFixed requires that 'this' be a Number".to_string()),
        };

        let digits = args.first().map(|v| v.to_number() as i32).unwrap_or(0);
        if !(0..=100).contains(&digits) {
            return Err("RangeError: toFixed() digits argument must be between 0 and 100".to_string());
        }

        if num.is_nan() {
            Ok(JSValue::String("NaN".to_string()))
        } else if num.is_infinite() {
            if num > 0.0 { Ok(JSValue::String("Infinity".to_string())) } else { Ok(JSValue::String("-Infinity".to_string())) }
        } else {
            Ok(JSValue::String(format!("{:.*}", digits as usize, num)))
        }
    });
    JSObject::set_property(&proto, "toFixed", JSValue::Function(to_fixed_fn));

    // Number.prototype.toExponential(fractionDigits?)
    let to_exp_fn = JSFunction::new_native("toExponential", |this, args| {
        let num = match this {
            JSValue::Smi(n) => *n as f64,
            JSValue::Number(f) => *f,
            JSValue::Object(obj) => obj.borrow().get_property("__value__").to_number(),
            _ => return Err("TypeError: Number.prototype.toExponential requires that 'this' be a Number".to_string()),
        };

        if num.is_nan() || num.is_infinite() {
            return Ok(JSValue::String(num.to_string()));
        }

        let s = if let Some(fd) = args.first() {
            let digits = fd.to_number() as usize;
            format!("{:.*e}", digits, num)
        } else {
            format!("{:e}", num)
        };
        Ok(JSValue::String(s))
    });
    JSObject::set_property(&proto, "toExponential", JSValue::Function(to_exp_fn));

    // Number.prototype.toPrecision(precision?)
    let to_prec_fn = JSFunction::new_native("toPrecision", |this, args| {
        let num = match this {
            JSValue::Smi(n) => *n as f64,
            JSValue::Number(f) => *f,
            JSValue::Object(obj) => obj.borrow().get_property("__value__").to_number(),
            _ => return Err("TypeError: Number.prototype.toPrecision requires that 'this' be a Number".to_string()),
        };

        if num.is_nan() || num.is_infinite() || args.is_empty() {
            return Ok(JSValue::String(num.to_string()));
        }

        let prec = args[0].to_number() as usize;
        if !(1..=100).contains(&prec) {
            return Err("RangeError: toPrecision() argument must be between 1 and 100".to_string());
        }
        Ok(JSValue::String(format!("{:.*}", prec, num)))
    });
    JSObject::set_property(&proto, "toPrecision", JSValue::Function(to_prec_fn));

    proto
}

/// Creates the global `Number` constructor object.
pub fn create_number_constructor(prototype: &Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let ctor = JSObject::new_empty(None);
    JSObject::set_property(&ctor, "prototype", JSValue::Object(prototype.clone()));
    JSObject::set_property(prototype, "constructor", JSValue::Object(ctor.clone()));
    JSObject::set_property(&ctor, "name", JSValue::String("Number".to_string()));

    // Constants
    JSObject::set_property(&ctor, "MAX_SAFE_INTEGER", JSValue::Number(9007199254740991.0));
    JSObject::set_property(&ctor, "MIN_SAFE_INTEGER", JSValue::Number(-9007199254740991.0));
    JSObject::set_property(&ctor, "EPSILON", JSValue::Number(std::f64::EPSILON));
    JSObject::set_property(&ctor, "MAX_VALUE", JSValue::Number(std::f64::MAX));
    JSObject::set_property(&ctor, "MIN_VALUE", JSValue::Number(5e-324));
    JSObject::set_property(&ctor, "NaN", JSValue::Number(f64::NAN));
    JSObject::set_property(&ctor, "POSITIVE_INFINITY", JSValue::Number(f64::INFINITY));
    JSObject::set_property(&ctor, "NEGATIVE_INFINITY", JSValue::Number(f64::NEG_INFINITY));

    // Number.isInteger(x)
    let is_int_fn = JSFunction::new_native("isInteger", |_this, args| {
        let arg = match args.first() {
            Some(v) => v,
            None => return Ok(JSValue::Boolean(false)),
        };
        match arg {
            JSValue::Smi(_) => Ok(JSValue::Boolean(true)),
            JSValue::Number(f) => {
                let is_int = !f.is_nan() && !f.is_infinite() && f.fract() == 0.0;
                Ok(JSValue::Boolean(is_int))
            }
            _ => Ok(JSValue::Boolean(false)),
        }
    });
    JSObject::set_property(&ctor, "isInteger", JSValue::Function(is_int_fn));

    // Number.isSafeInteger(x)
    let is_safe_int_fn = JSFunction::new_native("isSafeInteger", |_this, args| {
        let arg = match args.first() {
            Some(v) => v,
            None => return Ok(JSValue::Boolean(false)),
        };
        let n = match arg {
            JSValue::Smi(_) => return Ok(JSValue::Boolean(true)),
            JSValue::Number(f) => *f,
            _ => return Ok(JSValue::Boolean(false)),
        };
        let is_safe = !n.is_nan() && !n.is_infinite() && n.fract() == 0.0 && n >= -9007199254740991.0 && n <= 9007199254740991.0;
        Ok(JSValue::Boolean(is_safe))
    });
    JSObject::set_property(&ctor, "isSafeInteger", JSValue::Function(is_safe_int_fn));

    // Number.isNaN(x) (strict, does not coerce)
    let is_nan_fn = JSFunction::new_native("isNaN", |_this, args| {
        let is_nan = match args.first() {
            Some(JSValue::Number(f)) => f.is_nan(),
            _ => false,
        };
        Ok(JSValue::Boolean(is_nan))
    });
    JSObject::set_property(&ctor, "isNaN", JSValue::Function(is_nan_fn));

    // Number.isFinite(x) (strict, does not coerce)
    let is_finite_fn = JSFunction::new_native("isFinite", |_this, args| {
        let is_fin = match args.first() {
            Some(JSValue::Smi(_)) => true,
            Some(JSValue::Number(f)) => !f.is_nan() && !f.is_infinite(),
            _ => false,
        };
        Ok(JSValue::Boolean(is_fin))
    });
    JSObject::set_property(&ctor, "isFinite", JSValue::Function(is_finite_fn));

    // Number.parseInt & Number.parseFloat
    JSObject::set_property(&ctor, "parseInt", JSValue::Function(crate::builtins::global::parse_int_function()));
    JSObject::set_property(&ctor, "parseFloat", JSValue::Function(crate::builtins::global::parse_float_function()));

    // Invocation: Number(value)
    let ctor_fn = JSFunction::new_native("Number", |_this, args| {
        if let Some(arg) = args.first() {
            match arg {
                JSValue::Smi(n) => Ok(JSValue::Smi(*n)),
                other => {
                    let num = other.to_number();
                    if num.fract() == 0.0 && num >= i32::MIN as f64 && num <= i32::MAX as f64 {
                        Ok(JSValue::Smi(num as i32))
                    } else {
                        Ok(JSValue::Number(num))
                    }
                }
            }
        } else {
            Ok(JSValue::Smi(0))
        }
    });
    JSObject::set_property(&ctor, "__call__", JSValue::Function(ctor_fn));

    ctor
}
