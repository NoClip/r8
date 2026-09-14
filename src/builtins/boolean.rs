//! Safe Rust reimplementation of Google V8's ECMAScript `Boolean` constructor and prototype.

use crate::objects::{JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

/// Creates the `Boolean.prototype` object.
pub fn create_boolean_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);
    JSObject::set_property(&proto, "name", JSValue::String("Boolean".to_string()));

    // Boolean.prototype.valueOf()
    let value_of_fn = JSFunction::new_native("valueOf", |this, _args| {
        match this {
            JSValue::Boolean(b) => Ok(JSValue::Boolean(*b)),
            JSValue::Object(obj) => {
                let v = obj.borrow().get_property("__value__");
                if let JSValue::Boolean(b) = v {
                    Ok(JSValue::Boolean(b))
                } else {
                    Ok(JSValue::Boolean(false))
                }
            }
            _ => Err("TypeError: Boolean.prototype.valueOf requires that 'this' be a Boolean".to_string()),
        }
    });
    JSObject::set_property(&proto, "valueOf", JSValue::Function(value_of_fn));

    // Boolean.prototype.toString()
    let to_string_fn = JSFunction::new_native("toString", |this, _args| {
        let b = match this {
            JSValue::Boolean(b) => *b,
            JSValue::Object(obj) => {
                let v = obj.borrow().get_property("__value__");
                v.to_boolean()
            }
            _ => return Err("TypeError: Boolean.prototype.toString requires that 'this' be a Boolean".to_string()),
        };
        Ok(JSValue::String(if b { "true".to_string() } else { "false".to_string() }))
    });
    JSObject::set_property(&proto, "toString", JSValue::Function(to_string_fn));

    proto
}

/// Creates the global `Boolean` constructor object.
pub fn create_boolean_constructor(prototype: &Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let ctor = JSObject::new_empty(None);
    JSObject::set_property(&ctor, "prototype", JSValue::Object(prototype.clone()));
    JSObject::set_property(prototype, "constructor", JSValue::Object(ctor.clone()));
    JSObject::set_property(&ctor, "name", JSValue::String("Boolean".to_string()));

    // Invocation: Boolean(val)
    let ctor_fn = JSFunction::new_native("Boolean", |_this, args| {
        let val = args.first().map(|v| v.to_boolean()).unwrap_or(false);
        Ok(JSValue::Boolean(val))
    });
    JSObject::set_property(&ctor, "__call__", JSValue::Function(ctor_fn));

    ctor
}
