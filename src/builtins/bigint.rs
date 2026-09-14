//! Safe Rust reimplementation of Google V8's BigInt built-ins (`src/builtins/builtins-bigint.cc`).
//!
//! Exposes the global `BigInt` function, static helpers (`asIntN`, `asUintN`), and `BigInt.prototype`.

use crate::objects::bigint::BigIntData;
use crate::objects::js_object::JSObject;
use crate::objects::value::JSValue;
use crate::objects::JSFunction;
use std::cell::RefCell;
use std::rc::Rc;

pub fn create_bigint_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    let to_string_fn = JSFunction::new_native("toString", |this, _args| {
        match this {
            JSValue::BigInt(b) => Ok(JSValue::String(b.to_string())),
            _ => Err("TypeError: BigInt.prototype.toString called on non-BigInt".to_string()),
        }
    });
    JSObject::set_property(&proto, "toString", JSValue::Function(to_string_fn));

    let value_of_fn = JSFunction::new_native("valueOf", |this, _args| {
        match this {
            JSValue::BigInt(_) => Ok(this.clone()),
            _ => Err("TypeError: BigInt.prototype.valueOf called on non-BigInt".to_string()),
        }
    });
    JSObject::set_property(&proto, "valueOf", JSValue::Function(value_of_fn));

    proto
}

pub fn create_bigint_constructor(proto: &Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let ctor_fn = JSFunction::new_native("BigInt", |_this, args| {
        let arg = args.first().unwrap_or(&JSValue::Undefined);
        match arg {
            JSValue::BigInt(b) => Ok(JSValue::BigInt(b.clone())),
            JSValue::Smi(n) => Ok(JSValue::BigInt(Rc::new(BigIntData::from_i64(*n as i64)))),
            JSValue::Number(f) => {
                if !f.is_finite() || f.fract() != 0.0 {
                    return Err("RangeError: The number cannot be converted to a BigInt because it is not an integer".to_string());
                }
                let s = format!("{:.0}", f);
                let bi = BigIntData::from_str(&s).map_err(|e| format!("RangeError: {}", e))?;
                Ok(JSValue::BigInt(Rc::new(bi)))
            }
            JSValue::Boolean(b) => {
                let bi = if *b { BigIntData::one() } else { BigIntData::zero() };
                Ok(JSValue::BigInt(Rc::new(bi)))
            }
            JSValue::String(s) => {
                let bi = BigIntData::from_str(s).map_err(|e| format!("SyntaxError: Cannot convert {} to a BigInt: {}", s, e))?;
                Ok(JSValue::BigInt(Rc::new(bi)))
            }
            _ => Err("TypeError: Cannot convert to a BigInt".to_string()),
        }
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "name", JSValue::String("BigInt".to_string()));
    JSObject::set_property(&ctor_obj, "__not_constructor__", JSValue::Boolean(true));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor_fn));

    // BigInt.asIntN(bits, bigint)
    let as_int_n_fn = JSFunction::new_native("asIntN", |_this, args| {
        let bits = args.first().map(|v| v.to_number().max(0.0) as usize).unwrap_or(0);
        let bi = match args.get(1) {
            Some(JSValue::BigInt(b)) => b.clone(),
            Some(other) => {
                if let Ok(b) = BigIntData::from_str(&other.to_string_val()) {
                    Rc::new(b)
                } else {
                    return Err("TypeError: Cannot convert to a BigInt".to_string());
                }
            }
            None => Rc::new(BigIntData::zero()),
        };
        let clamped = BigIntData::as_int_n(bits, &bi);
        Ok(JSValue::BigInt(Rc::new(clamped)))
    });
    JSObject::set_property(&ctor_obj, "asIntN", JSValue::Function(as_int_n_fn));

    // BigInt.asUintN(bits, bigint)
    let as_uint_n_fn = JSFunction::new_native("asUintN", |_this, args| {
        let bits = args.first().map(|v| v.to_number().max(0.0) as usize).unwrap_or(0);
        let bi = match args.get(1) {
            Some(JSValue::BigInt(b)) => b.clone(),
            Some(other) => {
                if let Ok(b) = BigIntData::from_str(&other.to_string_val()) {
                    Rc::new(b)
                } else {
                    return Err("TypeError: Cannot convert to a BigInt".to_string());
                }
            }
            None => Rc::new(BigIntData::zero()),
        };
        let clamped = BigIntData::as_uint_n(bits, &bi);
        Ok(JSValue::BigInt(Rc::new(clamped)))
    });
    JSObject::set_property(&ctor_obj, "asUintN", JSValue::Function(as_uint_n_fn));

    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(proto.clone()));
    JSObject::set_property(proto, "constructor", JSValue::Object(ctor_obj.clone()));

    ctor_obj
}
