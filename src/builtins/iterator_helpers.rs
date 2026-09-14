//! Safe Rust reimplementation of Google V8's ECMAScript 2025 `Iterator` and Iterator Helpers.
//!
//! Provides the standard global `Iterator` constructor, `Iterator.from(iterable)`,
//! and prototype pipeline methods: `map`, `filter`, `take`, `drop`, `flatMap`,
//! `reduce`, `toArray`, `forEach`, `some`, `every`, and `find`.

use crate::objects::generator::create_iter_result;
use crate::objects::js_array::JSArray;
use crate::objects::{JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

/// Helper to step an iterator object by calling its `.next()` method.
pub fn step_iterator(iter_obj: &Rc<RefCell<JSObject>>) -> Result<(JSValue, bool), String> {
    let next_val = iter_obj.borrow().get_property("next");
    let next_fn = match next_val {
        JSValue::Function(ref f) => f.clone(),
        _ => return Err("TypeError: Iterator object does not have a next method".to_string()),
    };
    let res = next_fn.call(&JSValue::Object(iter_obj.clone()), &[])?;
    match res {
        JSValue::Object(ref o) => {
            let done_val = o.borrow().get_property("done");
            let done = done_val.to_boolean();
            let val = o.borrow().get_property("value");
            Ok((val, done))
        }
        _ => Err("TypeError: Iterator next() did not return an object".to_string()),
    }
}

/// Helper to extract an iterator from any JSValue (Array, Set, Map, String, or Iterator object).
pub fn get_iterator_from_value(val: &JSValue) -> Result<Rc<RefCell<JSObject>>, String> {
    match val {
        JSValue::Object(ref obj) => {
            // If it already has a next() method, treat as iterator
            let next_prop = obj.borrow().get_property("next");
            if matches!(next_prop, JSValue::Function(_)) {
                return Ok(obj.clone());
            }

            // Otherwise check [Symbol.iterator]
            for prop in &["[Symbol.iterator]", "Symbol(Symbol.iterator)", "iterator"] {
                let iter_prop = obj.borrow().get_property(prop);
                if let JSValue::Function(ref f) = iter_prop {
                    let res = f.call(&JSValue::Object(obj.clone()), &[])?;
                    if let JSValue::Object(res_obj) = res {
                        return Ok(res_obj);
                    }
                }
            }

            // Check if it's a Set
            let set_data = obj.borrow().ext_or_default().set_data.clone();
            if !set_data.is_empty() {
                let arr = JSArray::new_array(set_data);
                if let JSValue::Object(it) = crate::objects::generator::new_array_iterator(arr) {
                    return Ok(it);
                }
            }

            Err("TypeError: Object is not iterable or iterator".to_string())
        }
        JSValue::Array(ref arr) => {
            if let JSValue::Object(it) = crate::objects::generator::new_array_iterator(arr.clone()) {
                Ok(it)
            } else {
                Err("TypeError: Failed to create array iterator".to_string())
            }
        }
        JSValue::String(ref s) => {
            let chars: Vec<JSValue> = s.chars().map(|c| JSValue::String(c.to_string())).collect();
            let arr = JSArray::new_array(chars);
            if let JSValue::Object(it) = crate::objects::generator::new_array_iterator(arr) {
                return Ok(it);
            }
            Err("TypeError: Failed to create string iterator".to_string())
        }
        _ => Err("TypeError: Value is not iterable".to_string()),
    }
}

/// Allocates the standard `Iterator.prototype` object.
pub fn create_iterator_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // Iterator.prototype[Symbol.iterator]()
    let return_self_fn = JSFunction::new_native("[Symbol.iterator]", |this, _args| {
        Ok(this.clone())
    });
    JSObject::set_property(&proto, "Symbol(Symbol.iterator)", JSValue::Function(return_self_fn.clone()));
    JSObject::set_property(&proto, "[Symbol.iterator]", JSValue::Function(return_self_fn.clone()));
    JSObject::set_property(&proto, "iterator", JSValue::Function(return_self_fn));

    // Iterator.prototype.map(mapper)
    let proto_for_map = proto.clone();
    JSObject::set_property(
        &proto,
        "map",
        JSValue::Function(JSFunction::new_closure("map", move |this, args| {
            let mapper = match args.first() {
                Some(JSValue::Function(ref f)) => f.clone(),
                _ => return Err("TypeError: Iterator.prototype.map requires a callback function".to_string()),
            };
            let underlying = get_iterator_from_value(this)?;
            let index = Rc::new(RefCell::new(0usize));

            let helper = JSObject::new_empty(Some(proto_for_map.clone()));
            let mapper_clone = mapper.clone();
            let next_fn = JSFunction::new_closure("next", move |_this, _args| {
                let (val, done) = step_iterator(&underlying)?;
                if done {
                    Ok(create_iter_result(JSValue::Undefined, true))
                } else {
                    let idx = *index.borrow();
                    *index.borrow_mut() += 1;
                    let mapped = mapper_clone.call(&JSValue::Undefined, &[val, JSValue::Smi(idx as i32)])?;
                    Ok(create_iter_result(mapped, false))
                }
            });
            JSObject::set_property(&helper, "next", JSValue::Function(next_fn));
            Ok(JSValue::Object(helper))
        })),
    );

    // Iterator.prototype.filter(predicate)
    let proto_for_filter = proto.clone();
    JSObject::set_property(
        &proto,
        "filter",
        JSValue::Function(JSFunction::new_closure("filter", move |this, args| {
            let predicate = match args.first() {
                Some(JSValue::Function(ref f)) => f.clone(),
                _ => return Err("TypeError: Iterator.prototype.filter requires a callback function".to_string()),
            };
            let underlying = get_iterator_from_value(this)?;
            let index = Rc::new(RefCell::new(0usize));

            let helper = JSObject::new_empty(Some(proto_for_filter.clone()));
            let pred_clone = predicate.clone();
            let next_fn = JSFunction::new_closure("next", move |_this, _args| {
                loop {
                    let (val, done) = step_iterator(&underlying)?;
                    if done {
                        return Ok(create_iter_result(JSValue::Undefined, true));
                    }
                    let idx = *index.borrow();
                    *index.borrow_mut() += 1;
                    let test = pred_clone.call(&JSValue::Undefined, &[val.clone(), JSValue::Smi(idx as i32)])?;
                    if test.to_boolean() {
                        return Ok(create_iter_result(val, false));
                    }
                }
            });
            JSObject::set_property(&helper, "next", JSValue::Function(next_fn));
            Ok(JSValue::Object(helper))
        })),
    );

    // Iterator.prototype.take(limit)
    let proto_for_take = proto.clone();
    JSObject::set_property(
        &proto,
        "take",
        JSValue::Function(JSFunction::new_closure("take", move |this, args| {
            let limit = args.first().map(|v| v.to_number().max(0.0) as usize).unwrap_or(0);
            let underlying = get_iterator_from_value(this)?;
            let count = Rc::new(RefCell::new(0usize));

            let helper = JSObject::new_empty(Some(proto_for_take.clone()));
            let next_fn = JSFunction::new_closure("next", move |_this, _args| {
                if *count.borrow() >= limit {
                    return Ok(create_iter_result(JSValue::Undefined, true));
                }
                let (val, done) = step_iterator(&underlying)?;
                if done {
                    *count.borrow_mut() = limit;
                    Ok(create_iter_result(JSValue::Undefined, true))
                } else {
                    *count.borrow_mut() += 1;
                    Ok(create_iter_result(val, false))
                }
            });
            JSObject::set_property(&helper, "next", JSValue::Function(next_fn));
            Ok(JSValue::Object(helper))
        })),
    );

    // Iterator.prototype.drop(limit)
    let proto_for_drop = proto.clone();
    JSObject::set_property(
        &proto,
        "drop",
        JSValue::Function(JSFunction::new_closure("drop", move |this, args| {
            let limit = args.first().map(|v| v.to_number().max(0.0) as usize).unwrap_or(0);
            let underlying = get_iterator_from_value(this)?;
            let dropped = Rc::new(RefCell::new(false));

            let helper = JSObject::new_empty(Some(proto_for_drop.clone()));
            let next_fn = JSFunction::new_closure("next", move |_this, _args| {
                if !*dropped.borrow() {
                    *dropped.borrow_mut() = true;
                    for _ in 0..limit {
                        let (_, done) = step_iterator(&underlying)?;
                        if done {
                            return Ok(create_iter_result(JSValue::Undefined, true));
                        }
                    }
                }
                let (val, done) = step_iterator(&underlying)?;
                if done {
                    Ok(create_iter_result(JSValue::Undefined, true))
                } else {
                    Ok(create_iter_result(val, false))
                }
            });
            JSObject::set_property(&helper, "next", JSValue::Function(next_fn));
            Ok(JSValue::Object(helper))
        })),
    );

    // Iterator.prototype.flatMap(mapper)
    let proto_for_flat_map = proto.clone();
    JSObject::set_property(
        &proto,
        "flatMap",
        JSValue::Function(JSFunction::new_closure("flatMap", move |this, args| {
            let mapper = match args.first() {
                Some(JSValue::Function(ref f)) => f.clone(),
                _ => return Err("TypeError: Iterator.prototype.flatMap requires a callback function".to_string()),
            };
            let underlying = get_iterator_from_value(this)?;
            let current_inner: Rc<RefCell<Option<Rc<RefCell<JSObject>>>>> = Rc::new(RefCell::new(None));
            let index = Rc::new(RefCell::new(0usize));

            let helper = JSObject::new_empty(Some(proto_for_flat_map.clone()));
            let mapper_clone = mapper.clone();
            let inner_slot = current_inner.clone();
            let next_fn = JSFunction::new_closure("next", move |_this, _args| {
                loop {
                    if let Some(ref inner) = *inner_slot.borrow() {
                        let (inner_val, inner_done) = step_iterator(inner)?;
                        if !inner_done {
                            return Ok(create_iter_result(inner_val, false));
                        }
                        *inner_slot.borrow_mut() = None;
                    }

                    let (val, done) = step_iterator(&underlying)?;
                    if done {
                        return Ok(create_iter_result(JSValue::Undefined, true));
                    }

                    let idx = *index.borrow();
                    *index.borrow_mut() += 1;
                    let mapped = mapper_clone.call(&JSValue::Undefined, &[val, JSValue::Smi(idx as i32)])?;
                    let mapped_iter = get_iterator_from_value(&mapped)?;
                    *inner_slot.borrow_mut() = Some(mapped_iter);
                }
            });
            JSObject::set_property(&helper, "next", JSValue::Function(next_fn));
            Ok(JSValue::Object(helper))
        })),
    );

    // Iterator.prototype.toArray()
    JSObject::set_property(
        &proto,
        "toArray",
        JSValue::Function(JSFunction::new_native("toArray", |this, _args| {
            let underlying = get_iterator_from_value(this)?;
            let mut items = Vec::new();
            loop {
                let (val, done) = step_iterator(&underlying)?;
                if done {
                    break;
                }
                items.push(val);
            }
            Ok(JSValue::Array(JSArray::new_array(items)))
        })),
    );

    // Iterator.prototype.forEach(fn)
    JSObject::set_property(
        &proto,
        "forEach",
        JSValue::Function(JSFunction::new_native("forEach", |this, args| {
            let callback = match args.first() {
                Some(JSValue::Function(ref f)) => f.clone(),
                _ => return Err("TypeError: Iterator.prototype.forEach requires a callback function".to_string()),
            };
            let underlying = get_iterator_from_value(this)?;
            let mut idx = 0usize;
            loop {
                let (val, done) = step_iterator(&underlying)?;
                if done {
                    break;
                }
                callback.call(&JSValue::Undefined, &[val, JSValue::Smi(idx as i32)])?;
                idx += 1;
            }
            Ok(JSValue::Undefined)
        })),
    );

    // Iterator.prototype.some(predicate)
    JSObject::set_property(
        &proto,
        "some",
        JSValue::Function(JSFunction::new_native("some", |this, args| {
            let predicate = match args.first() {
                Some(JSValue::Function(ref f)) => f.clone(),
                _ => return Err("TypeError: Iterator.prototype.some requires a callback function".to_string()),
            };
            let underlying = get_iterator_from_value(this)?;
            let mut idx = 0usize;
            loop {
                let (val, done) = step_iterator(&underlying)?;
                if done {
                    return Ok(JSValue::Boolean(false));
                }
                let test = predicate.call(&JSValue::Undefined, &[val, JSValue::Smi(idx as i32)])?;
                if test.to_boolean() {
                    return Ok(JSValue::Boolean(true));
                }
                idx += 1;
            }
        })),
    );

    // Iterator.prototype.every(predicate)
    JSObject::set_property(
        &proto,
        "every",
        JSValue::Function(JSFunction::new_native("every", |this, args| {
            let predicate = match args.first() {
                Some(JSValue::Function(ref f)) => f.clone(),
                _ => return Err("TypeError: Iterator.prototype.every requires a callback function".to_string()),
            };
            let underlying = get_iterator_from_value(this)?;
            let mut idx = 0usize;
            loop {
                let (val, done) = step_iterator(&underlying)?;
                if done {
                    return Ok(JSValue::Boolean(true));
                }
                let test = predicate.call(&JSValue::Undefined, &[val, JSValue::Smi(idx as i32)])?;
                if !test.to_boolean() {
                    return Ok(JSValue::Boolean(false));
                }
                idx += 1;
            }
        })),
    );

    // Iterator.prototype.find(predicate)
    JSObject::set_property(
        &proto,
        "find",
        JSValue::Function(JSFunction::new_native("find", |this, args| {
            let predicate = match args.first() {
                Some(JSValue::Function(ref f)) => f.clone(),
                _ => return Err("TypeError: Iterator.prototype.find requires a callback function".to_string()),
            };
            let underlying = get_iterator_from_value(this)?;
            let mut idx = 0usize;
            loop {
                let (val, done) = step_iterator(&underlying)?;
                if done {
                    return Ok(JSValue::Undefined);
                }
                let test = predicate.call(&JSValue::Undefined, &[val.clone(), JSValue::Smi(idx as i32)])?;
                if test.to_boolean() {
                    return Ok(val);
                }
                idx += 1;
            }
        })),
    );

    // Iterator.prototype.reduce(reducer, initialValue?)
    JSObject::set_property(
        &proto,
        "reduce",
        JSValue::Function(JSFunction::new_native("reduce", |this, args| {
            let reducer = match args.first() {
                Some(JSValue::Function(ref f)) => f.clone(),
                _ => return Err("TypeError: Iterator.prototype.reduce requires a reducer function".to_string()),
            };
            let underlying = get_iterator_from_value(this)?;
            let (mut accum, mut idx) = if args.len() > 1 {
                (args[1].clone(), 0usize)
            } else {
                let (first, done) = step_iterator(&underlying)?;
                if done {
                    return Err("TypeError: Reduce of empty iterator with no initial value".to_string());
                }
                (first, 1usize)
            };

            loop {
                let (val, done) = step_iterator(&underlying)?;
                if done {
                    break;
                }
                accum = reducer.call(&JSValue::Undefined, &[accum, val, JSValue::Smi(idx as i32)])?;
                idx += 1;
            }

            Ok(accum)
        })),
    );

    proto
}

/// Allocates the global `Iterator` constructor.
pub fn create_iterator_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let iter_ctor = JSObject::new_empty(None);
    JSObject::set_property(&iter_ctor, "prototype", JSValue::Object(prototype.clone()));

    let proto_for_from = prototype.clone();
    JSObject::set_property(
        &iter_ctor,
        "from",
        JSValue::Function(JSFunction::new_closure("from", move |_this, args| {
            let target = args.first().ok_or_else(|| {
                "TypeError: Iterator.from requires an argument".to_string()
            })?;
            let it = get_iterator_from_value(target)?;
            // Create a wrapper helper inheriting from Iterator.prototype
            let helper = JSObject::new_empty(Some(proto_for_from.clone()));
            let it_clone = it.clone();
            let next_fn = JSFunction::new_closure("next", move |_this, _args| {
                let (val, done) = step_iterator(&it_clone)?;
                Ok(create_iter_result(val, done))
            });
            JSObject::set_property(&helper, "next", JSValue::Function(next_fn));
            Ok(JSValue::Object(helper))
        })),
    );

    let ctor_fn = JSFunction::new_native("Iterator", |_this, _args| {
        Err("TypeError: Abstract class Iterator not directly constructable".to_string())
    });
    JSObject::set_property(&iter_ctor, "__call__", JSValue::Function(ctor_fn));

    iter_ctor
}
