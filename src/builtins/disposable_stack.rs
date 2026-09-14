//! Safe Rust reimplementation of Google V8's ECMAScript `DisposableStack` and `AsyncDisposableStack` (ES2025).
//!
//! Provides the explicit resource management container types with `use`, `adopt`, `defer`,
//! `move`, and LIFO `dispose` / `disposeAsync` execution.

use crate::objects::{JSArray, JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

fn get_disposable_entries(obj: &Rc<RefCell<JSObject>>) -> Vec<JSValue> {
    match obj.borrow().get_property("__disposable_entries__") {
        JSValue::Array(arr) => arr.borrow().elements.clone(),
        _ => Vec::new(),
    }
}

fn set_disposable_entries(obj: &Rc<RefCell<JSObject>>, entries: Vec<JSValue>) {
    let arr = JSArray::new_array(entries);
    JSObject::set_property(obj, "__disposable_entries__", JSValue::Array(arr));
}

fn is_disposed(obj: &Rc<RefCell<JSObject>>) -> bool {
    obj.borrow().get_property("disposed") == JSValue::Boolean(true)
}

fn set_disposed(obj: &Rc<RefCell<JSObject>>, val: bool) {
    JSObject::set_property(obj, "disposed", JSValue::Boolean(val));
}

fn create_resolved_promise(val: JSValue) -> Rc<RefCell<JSObject>> {
    let p = crate::builtins::promise::new_promise_instance(None);
    JSObject::set_property(&p, "__promise_state__", JSValue::Smi(crate::builtins::promise::PROMISE_FULFILLED));
    JSObject::set_property(&p, "__promise_result__", val);
    p
}

/// Creates the `DisposableStack.prototype` object.
pub fn create_disposable_stack_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // .use(value)
    let use_fn = JSFunction::new_native("use", |this, args| {
        let this_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("TypeError: DisposableStack.prototype.use called on non-object".to_string()),
        };

        if is_disposed(&this_obj) {
            return Err("ReferenceError: Cannot use value in a disposed DisposableStack".to_string());
        }

        let val = args.first().cloned().unwrap_or(JSValue::Undefined);
        if val == JSValue::Undefined || val == JSValue::Null {
            return Ok(val);
        }

        let dispose_method = match &val {
            JSValue::Object(o) => {
                let m = o.borrow().get_property("Symbol(Symbol.dispose)");
                if m != JSValue::Undefined {
                    m
                } else {
                    let m2 = o.borrow().get_property("[Symbol.dispose]");
                    if m2 != JSValue::Undefined {
                        m2
                    } else {
                        o.borrow().get_property("dispose")
                    }
                }
            }
            _ => JSValue::Undefined,
        };

        if !matches!(dispose_method, JSValue::Function(_)) {
            return Err("TypeError: Property [Symbol.dispose] is not callable".to_string());
        }

        let mut entries = get_disposable_entries(&this_obj);
        // Entry format: [value, method, is_callback (false)]
        let record = JSArray::new_array(vec![val.clone(), dispose_method, JSValue::Boolean(false)]);
        entries.push(JSValue::Array(record));
        set_disposable_entries(&this_obj, entries);

        Ok(val)
    });
    JSObject::set_property(&proto, "use", JSValue::Function(use_fn));

    // .adopt(value, onDispose)
    let adopt_fn = JSFunction::new_native("adopt", |this, args| {
        let this_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("TypeError: DisposableStack.prototype.adopt called on non-object".to_string()),
        };

        if is_disposed(&this_obj) {
            return Err("ReferenceError: Cannot adopt value in a disposed DisposableStack".to_string());
        }

        let val = args.first().cloned().unwrap_or(JSValue::Undefined);
        let on_dispose = args.get(1).cloned().unwrap_or(JSValue::Undefined);
        if !matches!(on_dispose, JSValue::Function(_)) {
            return Err("TypeError: onDispose must be a function".to_string());
        }

        let mut entries = get_disposable_entries(&this_obj);
        let record = JSArray::new_array(vec![val.clone(), on_dispose, JSValue::Boolean(true)]);
        entries.push(JSValue::Array(record));
        set_disposable_entries(&this_obj, entries);

        Ok(val)
    });
    JSObject::set_property(&proto, "adopt", JSValue::Function(adopt_fn));

    // .defer(onDispose)
    let defer_fn = JSFunction::new_native("defer", |this, args| {
        let this_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("TypeError: DisposableStack.prototype.defer called on non-object".to_string()),
        };

        if is_disposed(&this_obj) {
            return Err("ReferenceError: Cannot defer callback in a disposed DisposableStack".to_string());
        }

        let on_dispose = args.first().cloned().unwrap_or(JSValue::Undefined);
        if !matches!(on_dispose, JSValue::Function(_)) {
            return Err("TypeError: onDispose must be a function".to_string());
        }

        let mut entries = get_disposable_entries(&this_obj);
        let record = JSArray::new_array(vec![JSValue::Undefined, on_dispose, JSValue::Boolean(true)]);
        entries.push(JSValue::Array(record));
        set_disposable_entries(&this_obj, entries);

        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&proto, "defer", JSValue::Function(defer_fn));

    // .move()
    let proto_for_move = proto.clone();
    let move_fn = JSFunction::new_closure("move", move |this, _args| {
        let this_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("TypeError: DisposableStack.prototype.move called on non-object".to_string()),
        };

        if is_disposed(&this_obj) {
            return Err("ReferenceError: Cannot move a disposed DisposableStack".to_string());
        }

        let entries = get_disposable_entries(&this_obj);
        set_disposable_entries(&this_obj, Vec::new());
        set_disposed(&this_obj, true);

        let new_stack = JSObject::new_empty(Some(proto_for_move.clone()));
        set_disposable_entries(&new_stack, entries);
        set_disposed(&new_stack, false);

        Ok(JSValue::Object(new_stack))
    });
    JSObject::set_property(&proto, "move", JSValue::Function(move_fn));

    // .dispose()
    let dispose_fn = JSFunction::new_native("dispose", |this, _args| {
        let this_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("TypeError: DisposableStack.prototype.dispose called on non-object".to_string()),
        };

        if is_disposed(&this_obj) {
            return Ok(JSValue::Undefined);
        }

        set_disposed(&this_obj, true);
        let entries = get_disposable_entries(&this_obj);
        set_disposable_entries(&this_obj, Vec::new());

        let mut current_error: Option<String> = None;

        // Dispose in reverse order (LIFO)
        for record_val in entries.into_iter().rev() {
            if let JSValue::Array(arr) = record_val {
                let el = arr.borrow().elements.clone();
                let val = el.first().cloned().unwrap_or(JSValue::Undefined);
                let fn_val = el.get(1).cloned().unwrap_or(JSValue::Undefined);
                let is_callback = el.get(2) == Some(&JSValue::Boolean(true));

                let result = if is_callback {
                    if let JSValue::Function(f) = fn_val {
                        f.call(&JSValue::Undefined, &[val])
                    } else {
                        Ok(JSValue::Undefined)
                    }
                } else if let JSValue::Function(f) = fn_val {
                    f.call(&val, &[])
                } else {
                    Ok(JSValue::Undefined)
                };

                if let Err(e) = result {
                    if let Some(prev) = current_error {
                        current_error = Some(format!("SuppressedError: {}; Suppressed: {}", e, prev));
                    } else {
                        current_error = Some(e);
                    }
                }
            }
        }

        if let Some(err) = current_error {
            Err(err)
        } else {
            Ok(JSValue::Undefined)
        }
    });
    JSObject::set_property(&proto, "dispose", JSValue::Function(dispose_fn.clone()));
    JSObject::set_property(&proto, "Symbol(Symbol.dispose)", JSValue::Function(dispose_fn.clone()));
    JSObject::set_property(&proto, "[Symbol.dispose]", JSValue::Function(dispose_fn));

    proto
}

/// Creates the `DisposableStack` constructor object.
pub fn create_disposable_stack_constructor(proto: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let ctor = JSObject::new_empty(None);
    JSObject::set_property(&ctor, "prototype", JSValue::Object(proto.clone()));
    JSObject::set_property(&proto, "constructor", JSValue::Object(ctor.clone()));
    JSObject::set_property(&ctor, "name", JSValue::String("DisposableStack".to_string()));

    let proto_clone = proto.clone();
    let ctor_fn = JSFunction::new_closure("DisposableStack", move |_this, _args| {
        let instance = JSObject::new_empty(Some(proto_clone.clone()));
        set_disposable_entries(&instance, Vec::new());
        set_disposed(&instance, false);
        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&ctor, "__call__", JSValue::Function(ctor_fn));
    ctor
}

/// Creates the `AsyncDisposableStack.prototype` object.
pub fn create_async_disposable_stack_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // .use(value)
    let use_fn = JSFunction::new_native("use", |this, args| {
        let this_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("TypeError: AsyncDisposableStack.prototype.use called on non-object".to_string()),
        };

        if is_disposed(&this_obj) {
            return Err("ReferenceError: Cannot use value in a disposed AsyncDisposableStack".to_string());
        }

        let val = args.first().cloned().unwrap_or(JSValue::Undefined);
        if val == JSValue::Undefined || val == JSValue::Null {
            return Ok(val);
        }

        let dispose_method = match &val {
            JSValue::Object(o) => {
                let m = o.borrow().get_property("Symbol(Symbol.asyncDispose)");
                if m != JSValue::Undefined {
                    m
                } else {
                    let m2 = o.borrow().get_property("[Symbol.asyncDispose]");
                    if m2 != JSValue::Undefined {
                        m2
                    } else {
                        let m3 = o.borrow().get_property("Symbol(Symbol.dispose)");
                        if m3 != JSValue::Undefined {
                            m3
                        } else {
                            o.borrow().get_property("dispose")
                        }
                    }
                }
            }
            _ => JSValue::Undefined,
        };

        if !matches!(dispose_method, JSValue::Function(_)) {
            return Err("TypeError: Property [Symbol.asyncDispose] or [Symbol.dispose] is not callable".to_string());
        }

        let mut entries = get_disposable_entries(&this_obj);
        let record = JSArray::new_array(vec![val.clone(), dispose_method, JSValue::Boolean(false)]);
        entries.push(JSValue::Array(record));
        set_disposable_entries(&this_obj, entries);

        Ok(val)
    });
    JSObject::set_property(&proto, "use", JSValue::Function(use_fn));

    // .adopt(value, onDispose)
    let adopt_fn = JSFunction::new_native("adopt", |this, args| {
        let this_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("TypeError: AsyncDisposableStack.prototype.adopt called on non-object".to_string()),
        };

        if is_disposed(&this_obj) {
            return Err("ReferenceError: Cannot adopt value in a disposed AsyncDisposableStack".to_string());
        }

        let val = args.first().cloned().unwrap_or(JSValue::Undefined);
        let on_dispose = args.get(1).cloned().unwrap_or(JSValue::Undefined);
        if !matches!(on_dispose, JSValue::Function(_)) {
            return Err("TypeError: onDispose must be a function".to_string());
        }

        let mut entries = get_disposable_entries(&this_obj);
        let record = JSArray::new_array(vec![val.clone(), on_dispose, JSValue::Boolean(true)]);
        entries.push(JSValue::Array(record));
        set_disposable_entries(&this_obj, entries);

        Ok(val)
    });
    JSObject::set_property(&proto, "adopt", JSValue::Function(adopt_fn));

    // .defer(onDispose)
    let defer_fn = JSFunction::new_native("defer", |this, args| {
        let this_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("TypeError: AsyncDisposableStack.prototype.defer called on non-object".to_string()),
        };

        if is_disposed(&this_obj) {
            return Err("ReferenceError: Cannot defer callback in a disposed AsyncDisposableStack".to_string());
        }

        let on_dispose = args.first().cloned().unwrap_or(JSValue::Undefined);
        if !matches!(on_dispose, JSValue::Function(_)) {
            return Err("TypeError: onDispose must be a function".to_string());
        }

        let mut entries = get_disposable_entries(&this_obj);
        let record = JSArray::new_array(vec![JSValue::Undefined, on_dispose, JSValue::Boolean(true)]);
        entries.push(JSValue::Array(record));
        set_disposable_entries(&this_obj, entries);

        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&proto, "defer", JSValue::Function(defer_fn));

    // .disposeAsync()
    let dispose_async_fn = JSFunction::new_native("disposeAsync", move |this, _args| {
        let this_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("TypeError: AsyncDisposableStack.prototype.disposeAsync called on non-object".to_string()),
        };

        if is_disposed(&this_obj) {
            let p = create_resolved_promise(JSValue::Undefined);
            return Ok(JSValue::Object(p));
        }

        set_disposed(&this_obj, true);
        let entries = get_disposable_entries(&this_obj);
        set_disposable_entries(&this_obj, Vec::new());

        let mut current_error: Option<String> = None;

        for record_val in entries.into_iter().rev() {
            if let JSValue::Array(arr) = record_val {
                let el = arr.borrow().elements.clone();
                let val = el.first().cloned().unwrap_or(JSValue::Undefined);
                let fn_val = el.get(1).cloned().unwrap_or(JSValue::Undefined);
                let is_callback = el.get(2) == Some(&JSValue::Boolean(true));

                let result = if is_callback {
                    if let JSValue::Function(f) = fn_val {
                        f.call(&JSValue::Undefined, &[val])
                    } else {
                        Ok(JSValue::Undefined)
                    }
                } else if let JSValue::Function(f) = fn_val {
                    f.call(&val, &[])
                } else {
                    Ok(JSValue::Undefined)
                };

                if let Err(e) = result {
                    if let Some(prev) = current_error {
                        current_error = Some(format!("SuppressedError: {}; Suppressed: {}", e, prev));
                    } else {
                        current_error = Some(e);
                    }
                }
            }
        }

        if let Some(err) = current_error {
            Err(err)
        } else {
            let p = create_resolved_promise(JSValue::Undefined);
            Ok(JSValue::Object(p))
        }
    });

    JSObject::set_property(&proto, "disposeAsync", JSValue::Function(dispose_async_fn.clone()));
    JSObject::set_property(&proto, "Symbol(Symbol.asyncDispose)", JSValue::Function(dispose_async_fn.clone()));
    JSObject::set_property(&proto, "[Symbol.asyncDispose]", JSValue::Function(dispose_async_fn));

    proto
}

/// Creates the `AsyncDisposableStack` constructor object.
pub fn create_async_disposable_stack_constructor(proto: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let ctor = JSObject::new_empty(None);
    JSObject::set_property(&ctor, "prototype", JSValue::Object(proto.clone()));
    JSObject::set_property(&proto, "constructor", JSValue::Object(ctor.clone()));
    JSObject::set_property(&ctor, "name", JSValue::String("AsyncDisposableStack".to_string()));

    let proto_clone = proto.clone();
    let ctor_fn = JSFunction::new_closure("AsyncDisposableStack", move |_this, _args| {
        let instance = JSObject::new_empty(Some(proto_clone.clone()));
        set_disposable_entries(&instance, Vec::new());
        set_disposed(&instance, false);
        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&ctor, "__call__", JSValue::Function(ctor_fn));
    ctor
}
