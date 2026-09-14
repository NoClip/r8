//! Safe Rust reimplementation of Google V8's Generator built-in prototypes and constructors.
//!
//! Exposes `Generator.prototype` (with `next`, `return`, `throw`, `[Symbol.iterator]`)
//! and the `GeneratorFunction` constructor.

use crate::builtins::promise::new_promise_capability;
use crate::interpreter::interpreter::InterpreterVM;
use crate::objects::function::JSFunction;
use crate::objects::js_object::JSObject;
use crate::objects::value::JSValue;
use std::cell::RefCell;
use std::rc::Rc;

/// Creates the `Generator.prototype` object.
pub fn create_generator_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // Generator.prototype.next(value)
    JSObject::set_property(
        &proto,
        "next",
        JSValue::Function(JSFunction::new_native("next", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let is_async = obj
                    .borrow()
                    .ext_ref()
                    .and_then(|e| e.generator_data.as_ref())
                    .map(|g| g.borrow().is_async)
                    .unwrap_or(false);

                let input_val = args.first().cloned();
                let res = InterpreterVM::execute_generator_step(obj, input_val, false, false);
                if is_async {
                    let (promise, resolve_fn, reject_fn) = new_promise_capability(None);
                    match res {
                        Ok(v) => {
                            let _ = resolve_fn.call(&JSValue::Undefined, &[v]);
                        }
                        Err(e) => {
                            let _ = reject_fn.call(
                                &JSValue::Undefined,
                                &[JSValue::String(e.message)],
                            );
                        }
                    }
                    Ok(JSValue::Object(promise))
                } else {
                    res.map_err(|e| e.message)
                }
            } else {
                Err("TypeError: Method Generator.prototype.next called on incompatible receiver".to_string())
            }
        })),
    );

    // Generator.prototype.return(value)
    JSObject::set_property(
        &proto,
        "return",
        JSValue::Function(JSFunction::new_native("return", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let is_async = obj
                    .borrow()
                    .ext_ref()
                    .and_then(|e| e.generator_data.as_ref())
                    .map(|g| g.borrow().is_async)
                    .unwrap_or(false);

                let input_val = args.first().cloned();
                let res = InterpreterVM::execute_generator_step(obj, input_val, true, false);
                if is_async {
                    let (promise, resolve_fn, reject_fn) = new_promise_capability(None);
                    match res {
                        Ok(v) => {
                            let _ = resolve_fn.call(&JSValue::Undefined, &[v]);
                        }
                        Err(e) => {
                            let _ = reject_fn.call(
                                &JSValue::Undefined,
                                &[JSValue::String(e.message)],
                            );
                        }
                    }
                    Ok(JSValue::Object(promise))
                } else {
                    res.map_err(|e| e.message)
                }
            } else {
                Err("TypeError: Method Generator.prototype.return called on incompatible receiver".to_string())
            }
        })),
    );

    // Generator.prototype.throw(exception)
    JSObject::set_property(
        &proto,
        "throw",
        JSValue::Function(JSFunction::new_native("throw", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let is_async = obj
                    .borrow()
                    .ext_ref()
                    .and_then(|e| e.generator_data.as_ref())
                    .map(|g| g.borrow().is_async)
                    .unwrap_or(false);

                let input_val = args.first().cloned();
                let res = InterpreterVM::execute_generator_step(obj, input_val, false, true);
                if is_async {
                    let (promise, resolve_fn, reject_fn) = new_promise_capability(None);
                    match res {
                        Ok(v) => {
                            let _ = resolve_fn.call(&JSValue::Undefined, &[v]);
                        }
                        Err(e) => {
                            let _ = reject_fn.call(
                                &JSValue::Undefined,
                                &[JSValue::String(e.message)],
                            );
                        }
                    }
                    Ok(JSValue::Object(promise))
                } else {
                    res.map_err(|e| e.message)
                }
            } else {
                Err("TypeError: Method Generator.prototype.throw called on incompatible receiver".to_string())
            }
        })),
    );

    // Generator.prototype[Symbol.iterator]() { return this; }
    let sym_iter_fn = JSFunction::new_native("[Symbol.iterator]", |this, _args| {
        Ok(this.clone())
    });
    JSObject::set_property(&proto, "Symbol(Symbol.iterator)", JSValue::Function(sym_iter_fn.clone()));
    JSObject::set_property(&proto, "[Symbol.iterator]", JSValue::Function(sym_iter_fn.clone()));
    JSObject::set_property(&proto, "iterator", JSValue::Function(sym_iter_fn));

    JSObject::set_property(&proto, "Symbol(Symbol.toStringTag)", JSValue::String("Generator".to_string()));

    proto
}

/// Creates the `GeneratorFunction.prototype` object.
pub fn create_generator_function_prototype(generator_prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);
    JSObject::set_property(&proto, "prototype", JSValue::Object(generator_prototype));
    JSObject::set_property(&proto, "Symbol(Symbol.toStringTag)", JSValue::String("GeneratorFunction".to_string()));
    proto
}

/// Creates the `GeneratorFunction` constructor.
pub fn create_generator_function_constructor(generator_function_prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let ctor = JSObject::new_empty(None);
    JSObject::set_property(&ctor, "name", JSValue::String("GeneratorFunction".to_string()));
    JSObject::set_property(&ctor, "prototype", JSValue::Object(generator_function_prototype.clone()));
    JSObject::set_property(&generator_function_prototype, "constructor", JSValue::Object(ctor.clone()));
    ctor
}
