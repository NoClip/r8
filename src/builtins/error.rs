//! Safe Rust reimplementation of Google V8's ECMAScript `Error` hierarchy.
//!
//! Implements `Error`, `TypeError`, `RangeError`, `ReferenceError`,
//! `SyntaxError`, `URIError`, `EvalError`, and `AggregateError` (ES2021).

use crate::objects::{JSArray, JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

/// Creates the root `Error.prototype` object.
pub fn create_error_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);
    JSObject::set_property(&proto, "name", JSValue::String("Error".to_string()));
    JSObject::set_property(&proto, "message", JSValue::String("".to_string()));

    // Error.prototype.toString()
    let to_string_fn = JSFunction::new_native("toString", |this, _args| {
        if let JSValue::Object(obj) = this {
            let name_val = obj.borrow().get_property("name");
            let name = if name_val == JSValue::Undefined {
                "Error".to_string()
            } else {
                name_val.to_string_val()
            };

            let msg_val = obj.borrow().get_property("message");
            let msg = if msg_val == JSValue::Undefined {
                "".to_string()
            } else {
                msg_val.to_string_val()
            };

            if name.is_empty() {
                Ok(JSValue::String(msg))
            } else if msg.is_empty() {
                Ok(JSValue::String(name))
            } else {
                Ok(JSValue::String(format!("{}: {}", name, msg)))
            }
        } else {
            Err("TypeError: Error.prototype.toString called on non-object".to_string())
        }
    });
    JSObject::set_property(&proto, "toString", JSValue::Function(to_string_fn));

    proto
}

/// Helper to instantiate an error object with proper prototype and standard properties.
pub fn new_error_instance(
    proto: Option<Rc<RefCell<JSObject>>>,
    name: &str,
    message: Option<String>,
    cause: Option<JSValue>,
) -> Rc<RefCell<JSObject>> {
    let err = JSObject::new_empty(proto);
    JSObject::set_property(&err, "name", JSValue::String(name.to_string()));
    let msg_str = message.unwrap_or_default();
    JSObject::set_property(&err, "message", JSValue::String(msg_str.clone()));

    // Synthetic V8-compatible call stack
    let stack_str = if msg_str.is_empty() {
        format!("{}\n    at <anonymous>:1:1", name)
    } else {
        format!("{}: {}\n    at <anonymous>:1:1", name, msg_str)
    };
    JSObject::set_property(&err, "stack", JSValue::String(stack_str));

    if let Some(c) = cause {
        JSObject::set_property(&err, "cause", c);
    }

    err
}

/// Generic factory for standard Error constructor functions.
pub fn create_error_constructor(
    name: &'static str,
    prototype: Rc<RefCell<JSObject>>,
) -> Rc<RefCell<JSObject>> {
    let ctor = JSObject::new_empty(None);
    JSObject::set_property(&ctor, "prototype", JSValue::Object(prototype.clone()));
    JSObject::set_property(&prototype, "constructor", JSValue::Object(ctor.clone()));
    JSObject::set_property(&ctor, "name", JSValue::String(name.to_string()));

    let proto_clone = prototype.clone();
    let ctor_fn = JSFunction::new_closure(name, move |_this, args| {
        let msg = args.first().map(|v| v.to_string_val());
        let cause = args.get(1).and_then(|opts| {
            if let JSValue::Object(o) = opts {
                let c = o.borrow().get_property("cause");
                if c != JSValue::Undefined { Some(c) } else { None }
            } else {
                None
            }
        });
        let instance = new_error_instance(Some(proto_clone.clone()), name, msg, cause);
        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&ctor, "__call__", JSValue::Function(ctor_fn));
    ctor
}

/// Creates the base `Error` constructor with `Error.captureStackTrace`.
pub fn create_base_error_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let ctor = create_error_constructor("Error", prototype);

    // Error.captureStackTrace(targetObject, constructorOpt?)
    let capture_stack_fn = JSFunction::new_native("captureStackTrace", |_this, args| {
        if let Some(JSValue::Object(target)) = args.first() {
            let name_val = target.borrow().get_property("name");
            let name = if name_val == JSValue::Undefined { "Error".to_string() } else { name_val.to_string_val() };
            let msg_val = target.borrow().get_property("message");
            let msg = if msg_val == JSValue::Undefined { "".to_string() } else { msg_val.to_string_val() };
            let stack = if msg.is_empty() {
                format!("{}\n    at <anonymous>:1:1", name)
            } else {
                format!("{}: {}\n    at <anonymous>:1:1", name, msg)
            };
            JSObject::set_property(target, "stack", JSValue::String(stack));
        }
        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&ctor, "captureStackTrace", JSValue::Function(capture_stack_fn));

    ctor
}

/// Creates the `AggregateError` constructor and prototype (ES2021).
pub fn create_aggregate_error_constructor(
    parent_proto: Rc<RefCell<JSObject>>,
) -> (Rc<RefCell<JSObject>>, Rc<RefCell<JSObject>>) {
    let proto = JSObject::new_empty(Some(parent_proto));
    JSObject::set_property(&proto, "name", JSValue::String("AggregateError".to_string()));
    JSObject::set_property(&proto, "message", JSValue::String("".to_string()));

    let ctor = JSObject::new_empty(None);
    JSObject::set_property(&ctor, "prototype", JSValue::Object(proto.clone()));
    JSObject::set_property(&proto, "constructor", JSValue::Object(ctor.clone()));
    JSObject::set_property(&ctor, "name", JSValue::String("AggregateError".to_string()));

    let proto_clone = proto.clone();
    let ctor_fn = JSFunction::new_closure("AggregateError", move |_this, args| {
        let errors_val = match args.first() {
            Some(JSValue::Array(arr)) => arr.borrow().elements.clone(),
            Some(other) => vec![other.clone()],
            None => Vec::new(),
        };
        let msg = args.get(1).map(|v| v.to_string_val());
        let cause = args.get(2).and_then(|opts| {
            if let JSValue::Object(o) = opts {
                let c = o.borrow().get_property("cause");
                if c != JSValue::Undefined { Some(c) } else { None }
            } else {
                None
            }
        });

        let instance = new_error_instance(Some(proto_clone.clone()), "AggregateError", msg, cause);
        let errors_arr = JSArray::new_array(errors_val);
        JSObject::set_property(&instance, "errors", JSValue::Array(errors_arr));

        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&ctor, "__call__", JSValue::Function(ctor_fn));
    (ctor, proto)
}

/// Creates the `SuppressedError` constructor and prototype (ES2025 Explicit Resource Management).
pub fn create_suppressed_error_constructor(
    parent_proto: Rc<RefCell<JSObject>>,
) -> (Rc<RefCell<JSObject>>, Rc<RefCell<JSObject>>) {
    let proto = JSObject::new_empty(Some(parent_proto));
    JSObject::set_property(&proto, "name", JSValue::String("SuppressedError".to_string()));
    JSObject::set_property(&proto, "message", JSValue::String("".to_string()));

    let ctor = JSObject::new_empty(None);
    JSObject::set_property(&ctor, "prototype", JSValue::Object(proto.clone()));
    JSObject::set_property(&proto, "constructor", JSValue::Object(ctor.clone()));
    JSObject::set_property(&ctor, "name", JSValue::String("SuppressedError".to_string()));

    let proto_clone = proto.clone();
    let ctor_fn = JSFunction::new_closure("SuppressedError", move |_this, args| {
        let error = args.first().cloned().unwrap_or(JSValue::Undefined);
        let suppressed = args.get(1).cloned().unwrap_or(JSValue::Undefined);
        let msg = args.get(2).map(|v| v.to_string_val());

        let instance = new_error_instance(Some(proto_clone.clone()), "SuppressedError", msg, None);
        JSObject::set_property(&instance, "error", error);
        JSObject::set_property(&instance, "suppressed", suppressed);

        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&ctor, "__call__", JSValue::Function(ctor_fn));
    (ctor, proto)
}
