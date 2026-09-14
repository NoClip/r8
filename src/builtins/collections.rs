//! Safe Rust reimplementation of Google V8's ECMAScript 2015 Collections.
//!
//! Provides standard library implementations of `Map`, `Set`, `WeakMap`, and `WeakSet`,
//! adhering to ECMAScript SameValueZero equality semantics, insertion order preservation,
//! and fluent method chaining.

use crate::objects::js_array::JSArray;
use crate::objects::map::{InstanceType, Map};
use crate::objects::{JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

/// ECMAScript SameValueZero comparison.
/// Returns true if two values are equal, treating +0 and -0 as equal, and NaN as equal to NaN.
pub fn same_value_zero(x: &JSValue, y: &JSValue) -> bool {
    if let (JSValue::Number(a), JSValue::Number(b)) = (x, y) {
        if a.is_nan() && b.is_nan() {
            return true;
        }
    }
    x == y
}

// -----------------------------------------------------------------------------
// Map Implementation
// -----------------------------------------------------------------------------

/// Allocates a new `Map` instance with the given prototype.
pub fn new_map_instance(prototype: Option<Rc<RefCell<JSObject>>>) -> Rc<RefCell<JSObject>> {
    let map = Map::root(InstanceType::JSMap, prototype);
    JSObject::new_with_map(map)
}

/// Creates the `Map.prototype` object.
pub fn create_map_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // Map.prototype.set(key, value)
    JSObject::set_property(
        &proto,
        "set",
        JSValue::Function(JSFunction::new_native("set", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let key = args.first().cloned().unwrap_or(JSValue::Undefined);
                let val = args.get(1).cloned().unwrap_or(JSValue::Undefined);

                let mut borrowed = obj.borrow_mut();
                let mut found = false;
                for entry in borrowed.ext_mut().map_data.iter_mut() {
                    if same_value_zero(&entry.0, &key) {
                        entry.1 = val.clone();
                        found = true;
                        break;
                    }
                }
                if !found {
                    borrowed.ext_mut().map_data.push((key, val));
                }
                Ok(this.clone())
            } else {
                Err("TypeError: Method Map.prototype.set called on incompatible receiver".to_string())
            }
        })),
    );

    // Map.prototype.get(key)
    JSObject::set_property(
        &proto,
        "get",
        JSValue::Function(JSFunction::new_native("get", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let key = args.first().cloned().unwrap_or(JSValue::Undefined);
                let borrowed = obj.borrow();
                for entry in borrowed.ext_or_default().map_data.iter() {
                    if same_value_zero(&entry.0, &key) {
                        return Ok(entry.1.clone());
                    }
                }
                Ok(JSValue::Undefined)
            } else {
                Err("TypeError: Method Map.prototype.get called on incompatible receiver".to_string())
            }
        })),
    );

    // Map.prototype.has(key)
    JSObject::set_property(
        &proto,
        "has",
        JSValue::Function(JSFunction::new_native("has", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let key = args.first().cloned().unwrap_or(JSValue::Undefined);
                let borrowed = obj.borrow();
                for entry in borrowed.ext_or_default().map_data.iter() {
                    if same_value_zero(&entry.0, &key) {
                        return Ok(JSValue::Boolean(true));
                    }
                }
                Ok(JSValue::Boolean(false))
            } else {
                Err("TypeError: Method Map.prototype.has called on incompatible receiver".to_string())
            }
        })),
    );

    // Map.prototype.delete(key)
    JSObject::set_property(
        &proto,
        "delete",
        JSValue::Function(JSFunction::new_native("delete", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let key = args.first().cloned().unwrap_or(JSValue::Undefined);
                let mut borrowed = obj.borrow_mut();
                let mut remove_idx = None;
                for (i, entry) in borrowed.ext_or_default().map_data.iter().enumerate() {
                    if same_value_zero(&entry.0, &key) {
                        remove_idx = Some(i);
                        break;
                    }
                }
                if let Some(idx) = remove_idx {
                    borrowed.ext_mut().map_data.remove(idx);
                    Ok(JSValue::Boolean(true))
                } else {
                    Ok(JSValue::Boolean(false))
                }
            } else {
                Err("TypeError: Method Map.prototype.delete called on incompatible receiver".to_string())
            }
        })),
    );

    // Map.prototype.clear()
    JSObject::set_property(
        &proto,
        "clear",
        JSValue::Function(JSFunction::new_native("clear", |this, _args| {
            if let JSValue::Object(ref obj) = this {
                obj.borrow_mut().ext_mut().map_data.clear();
                Ok(JSValue::Undefined)
            } else {
                Err("TypeError: Method Map.prototype.clear called on incompatible receiver".to_string())
            }
        })),
    );

    // Map.prototype.size getter
    JSObject::set_property(
        &proto,
        "__get_size__",
        JSValue::Function(JSFunction::new_native("size", |this, _args| {
            if let JSValue::Object(ref obj) = this {
                let len = obj.borrow().ext_or_default().map_data.len();
                Ok(JSValue::Smi(len as i32))
            } else {
                Err("TypeError: Method Map.prototype.size called on incompatible receiver".to_string())
            }
        })),
    );

    // Map.prototype.forEach(callback, thisArg?)
    JSObject::set_property(
        &proto,
        "forEach",
        JSValue::Function(JSFunction::new_native("forEach", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let cb = match args.first() {
                    Some(JSValue::Function(f)) => f.clone(),
                    _ => return Err("TypeError: Map.prototype.forEach callback is not a function".to_string()),
                };
                let this_arg = args.get(1).cloned().unwrap_or(JSValue::Undefined);
                let entries = obj.borrow().ext_or_default().map_data.clone();
                for (k, v) in entries {
                    cb.call(&this_arg, &[v, k, this.clone()])?;
                }
                Ok(JSValue::Undefined)
            } else {
                Err("TypeError: Method Map.prototype.forEach called on incompatible receiver".to_string())
            }
        })),
    );

    // Map.prototype.keys()
    JSObject::set_property(
        &proto,
        "keys",
        JSValue::Function(JSFunction::new_native("keys", |this, _args| {
            if let JSValue::Object(ref obj) = this {
                let keys: Vec<JSValue> = obj.borrow().ext_or_default().map_data.iter().map(|e| e.0.clone()).collect();
                Ok(JSValue::Array(JSArray::new_array_with_proto(keys, None)))
            } else {
                Err("TypeError: Method Map.prototype.keys called on incompatible receiver".to_string())
            }
        })),
    );

    // Map.prototype.values()
    JSObject::set_property(
        &proto,
        "values",
        JSValue::Function(JSFunction::new_native("values", |this, _args| {
            if let JSValue::Object(ref obj) = this {
                let vals: Vec<JSValue> = obj.borrow().ext_or_default().map_data.iter().map(|e| e.1.clone()).collect();
                Ok(JSValue::Array(JSArray::new_array_with_proto(vals, None)))
            } else {
                Err("TypeError: Method Map.prototype.values called on incompatible receiver".to_string())
            }
        })),
    );

    // Map.prototype.entries()
    JSObject::set_property(
        &proto,
        "entries",
        JSValue::Function(JSFunction::new_native("entries", |this, _args| {
            if let JSValue::Object(ref obj) = this {
                let mut result = Vec::new();
                for (k, v) in obj.borrow().ext_or_default().map_data.iter() {
                    let pair = JSArray::new_array_with_proto(vec![k.clone(), v.clone()], None);
                    result.push(JSValue::Array(pair));
                }
                Ok(JSValue::Array(JSArray::new_array_with_proto(result, None)))
            } else {
                Err("TypeError: Method Map.prototype.entries called on incompatible receiver".to_string())
            }
        })),
    );

    // Map.prototype[Symbol.iterator]()
    let map_iter_fn = JSFunction::new_native("[Symbol.iterator]", |this, _args| {
        if let JSValue::Object(ref obj) = this {
            let mut result = Vec::new();
            for (k, v) in obj.borrow().ext_or_default().map_data.iter() {
                let pair = JSArray::new_array_with_proto(vec![k.clone(), v.clone()], None);
                result.push(JSValue::Array(pair));
            }
            Ok(crate::objects::generator::new_array_iterator(JSArray::new_array_with_proto(result, None)))
        } else {
            Err("TypeError: Method Map.prototype[Symbol.iterator] called on incompatible receiver".to_string())
        }
    });
    JSObject::set_property(&proto, "Symbol(Symbol.iterator)", JSValue::Function(map_iter_fn.clone()));
    JSObject::set_property(&proto, "[Symbol.iterator]", JSValue::Function(map_iter_fn.clone()));
    JSObject::set_property(&proto, "iterator", JSValue::Function(map_iter_fn));

    proto
}

/// Allocates the global `Map` constructor object.
pub fn create_map_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let map_ctor = JSObject::new_empty(None);
    JSObject::set_property(&map_ctor, "prototype", JSValue::Object(prototype.clone()));

    let proto_clone = prototype.clone();
    let ctor_fn = JSFunction::new_closure("Map", move |_this, args| {
        let instance = new_map_instance(Some(proto_clone.clone()));
        if let Some(first) = args.first() {
            if let JSValue::Array(ref arr) = first {
                let elements = arr.borrow().elements.clone();
                for elem in elements {
                    if let JSValue::Array(ref pair_arr) = elem {
                        let k = pair_arr.borrow().elements.get(0).cloned().unwrap_or(JSValue::Undefined);
                        let v = pair_arr.borrow().elements.get(1).cloned().unwrap_or(JSValue::Undefined);
                        instance.borrow_mut().ext_mut().map_data.push((k, v));
                    }
                }
            }
        }
        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&map_ctor, "__call__", JSValue::Function(ctor_fn));

    // Map.groupBy(items, callback) (ES2024)
    let proto_for_group_by = prototype.clone();
    JSObject::set_property(
        &map_ctor,
        "groupBy",
        JSValue::Function(JSFunction::new_closure("groupBy", move |_this, args| {
            let items = match args.first() {
                Some(JSValue::Array(arr)) => arr.borrow().elements.clone(),
                _ => return Err("TypeError: Map.groupBy requires an array or iterable".to_string()),
            };
            let callback = match args.get(1) {
                Some(JSValue::Function(cb)) => cb.clone(),
                _ => return Err("TypeError: Map.groupBy requires a callback function".to_string()),
            };

            let map_instance = new_map_instance(Some(proto_for_group_by.clone()));
            for (i, item) in items.into_iter().enumerate() {
                let key = callback.call(&JSValue::Undefined, &[item.clone(), JSValue::Smi(i as i32)])?;

                let mut borrowed = map_instance.borrow_mut();
                let mut found = false;
                for entry in borrowed.ext_mut().map_data.iter_mut() {
                    if same_value_zero(&entry.0, &key) {
                        if let JSValue::Array(ref arr) = entry.1 {
                            arr.borrow_mut().elements.push(item.clone());
                        }
                        found = true;
                        break;
                    }
                }
                if !found {
                    let group_arr = JSArray::new_array(vec![item]);
                    borrowed.ext_mut().map_data.push((key, JSValue::Array(group_arr)));
                }
            }
            Ok(JSValue::Object(map_instance))
        })),
    );

    map_ctor
}

// -----------------------------------------------------------------------------
// Set Implementation
// -----------------------------------------------------------------------------

/// Allocates a new `Set` instance with the given prototype.
pub fn new_set_instance(prototype: Option<Rc<RefCell<JSObject>>>) -> Rc<RefCell<JSObject>> {
    let map = Map::root(InstanceType::JSSet, prototype);
    JSObject::new_with_map(map)
}

/// Extracts elements from a Set or Set-like object / array.
fn get_set_or_iterable_elements(val: &JSValue) -> Result<Vec<JSValue>, String> {
    match val {
        JSValue::Object(ref obj) => {
            let borrowed = obj.borrow();
            if borrowed.map.borrow().instance_type == InstanceType::JSSet || !borrowed.ext_or_default().set_data.is_empty() {
                return Ok(borrowed.ext_or_default().set_data.clone());
            }
            drop(borrowed);
            let vals = obj.borrow().get_property("values");
            if let JSValue::Function(f) = vals {
                if let Ok(JSValue::Array(arr)) = f.call(&JSValue::Object(obj.clone()), &[]) {
                    return Ok(arr.borrow().elements.clone());
                }
            }
            Err("TypeError: Value must be a Set or Set-like object".to_string())
        }
        JSValue::Array(ref arr) => Ok(arr.borrow().elements.clone()),
        _ => Err("TypeError: Value must be a Set or Set-like object".to_string()),
    }
}

/// Helper to create a new Set instance populated with the given elements.
fn create_set_from_elements(elements: Vec<JSValue>, proto: Option<Rc<RefCell<JSObject>>>) -> Rc<RefCell<JSObject>> {
    let set = new_set_instance(proto);
    let mut data: Vec<JSValue> = Vec::new();
    for elem in elements {
        if !data.iter().any(|item| same_value_zero(item, &elem)) {
            data.push(elem);
        }
    }
    set.borrow_mut().ext_mut().set_data = data;
    set
}

/// Creates the `Set.prototype` object.
pub fn create_set_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // Set.prototype.add(value)
    JSObject::set_property(
        &proto,
        "add",
        JSValue::Function(JSFunction::new_native("add", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let val = args.first().cloned().unwrap_or(JSValue::Undefined);
                let mut borrowed = obj.borrow_mut();
                if !borrowed.ext_or_default().set_data.iter().any(|item| same_value_zero(item, &val)) {
                    borrowed.ext_mut().set_data.push(val);
                }
                Ok(this.clone())
            } else {
                Err("TypeError: Method Set.prototype.add called on incompatible receiver".to_string())
            }
        })),
    );

    // Set.prototype.has(value)
    JSObject::set_property(
        &proto,
        "has",
        JSValue::Function(JSFunction::new_native("has", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let val = args.first().cloned().unwrap_or(JSValue::Undefined);
                let exists = obj.borrow().ext_or_default().set_data.iter().any(|item| same_value_zero(item, &val));
                Ok(JSValue::Boolean(exists))
            } else {
                Err("TypeError: Method Set.prototype.has called on incompatible receiver".to_string())
            }
        })),
    );

    // Set.prototype.delete(value)
    JSObject::set_property(
        &proto,
        "delete",
        JSValue::Function(JSFunction::new_native("delete", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let val = args.first().cloned().unwrap_or(JSValue::Undefined);
                let mut borrowed = obj.borrow_mut();
                if let Some(idx) = borrowed.ext_or_default().set_data.iter().position(|item| same_value_zero(item, &val)) {
                    borrowed.ext_mut().set_data.remove(idx);
                    Ok(JSValue::Boolean(true))
                } else {
                    Ok(JSValue::Boolean(false))
                }
            } else {
                Err("TypeError: Method Set.prototype.delete called on incompatible receiver".to_string())
            }
        })),
    );

    // Set.prototype.clear()
    JSObject::set_property(
        &proto,
        "clear",
        JSValue::Function(JSFunction::new_native("clear", |this, _args| {
            if let JSValue::Object(ref obj) = this {
                obj.borrow_mut().ext_mut().set_data.clear();
                Ok(JSValue::Undefined)
            } else {
                Err("TypeError: Method Set.prototype.clear called on incompatible receiver".to_string())
            }
        })),
    );

    // Set.prototype.size getter
    JSObject::set_property(
        &proto,
        "__get_size__",
        JSValue::Function(JSFunction::new_native("size", |this, _args| {
            if let JSValue::Object(ref obj) = this {
                let len = obj.borrow().ext_or_default().set_data.len();
                Ok(JSValue::Smi(len as i32))
            } else {
                Err("TypeError: Method Set.prototype.size called on incompatible receiver".to_string())
            }
        })),
    );

    // Set.prototype.forEach(callback, thisArg?)
    JSObject::set_property(
        &proto,
        "forEach",
        JSValue::Function(JSFunction::new_native("forEach", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let cb = match args.first() {
                    Some(JSValue::Function(f)) => f.clone(),
                    _ => return Err("TypeError: Set.prototype.forEach callback is not a function".to_string()),
                };
                let this_arg = args.get(1).cloned().unwrap_or(JSValue::Undefined);
                let items = obj.borrow().ext_or_default().set_data.clone();
                for item in items {
                    cb.call(&this_arg, &[item.clone(), item, this.clone()])?;
                }
                Ok(JSValue::Undefined)
            } else {
                Err("TypeError: Method Set.prototype.forEach called on incompatible receiver".to_string())
            }
        })),
    );

    // Set.prototype.values() & keys()
    let values_fn = JSValue::Function(JSFunction::new_native("values", |this, _args| {
        if let JSValue::Object(ref obj) = this {
            let items = obj.borrow().ext_or_default().set_data.clone();
            Ok(JSValue::Array(JSArray::new_array_with_proto(items, None)))
        } else {
            Err("TypeError: Method Set.prototype.values called on incompatible receiver".to_string())
        }
    }));
    JSObject::set_property(&proto, "values", values_fn.clone());
    JSObject::set_property(&proto, "keys", values_fn);

    // Set.prototype.entries()
    JSObject::set_property(
        &proto,
        "entries",
        JSValue::Function(JSFunction::new_native("entries", |this, _args| {
            if let JSValue::Object(ref obj) = this {
                let mut result = Vec::new();
                for item in obj.borrow().ext_or_default().set_data.iter() {
                    let pair = JSArray::new_array_with_proto(vec![item.clone(), item.clone()], None);
                    result.push(JSValue::Array(pair));
                }
                Ok(JSValue::Array(JSArray::new_array_with_proto(result, None)))
            } else {
                Err("TypeError: Method Set.prototype.entries called on incompatible receiver".to_string())
            }
        })),
    );

    // Set.prototype[Symbol.iterator]()
    let set_iter_fn = JSFunction::new_native("[Symbol.iterator]", |this, _args| {
        if let JSValue::Object(ref obj) = this {
            let elements = obj.borrow().ext_or_default().set_data.clone();
            Ok(crate::objects::generator::new_array_iterator(JSArray::new_array_with_proto(elements, None)))
        } else {
            Err("TypeError: Method Set.prototype[Symbol.iterator] called on incompatible receiver".to_string())
        }
    });
    JSObject::set_property(&proto, "Symbol(Symbol.iterator)", JSValue::Function(set_iter_fn.clone()));
    JSObject::set_property(&proto, "[Symbol.iterator]", JSValue::Function(set_iter_fn.clone()));
    JSObject::set_property(&proto, "iterator", JSValue::Function(set_iter_fn));

    // ES2024 Set Methods: union, intersection, difference, symmetricDifference, isSubsetOf, isSupersetOf, isDisjointFrom
    let proto_for_union = proto.clone();
    JSObject::set_property(
        &proto,
        "union",
        JSValue::Function(JSFunction::new_closure("union", move |this, args| {
            let this_elems = get_set_or_iterable_elements(this)?;
            let other_elems = get_set_or_iterable_elements(args.first().unwrap_or(&JSValue::Undefined))?;
            let mut combined = this_elems;
            combined.extend(other_elems);
            let new_set = create_set_from_elements(combined, Some(proto_for_union.clone()));
            Ok(JSValue::Object(new_set))
        })),
    );

    let proto_for_intersect = proto.clone();
    JSObject::set_property(
        &proto,
        "intersection",
        JSValue::Function(JSFunction::new_closure("intersection", move |this, args| {
            let this_elems = get_set_or_iterable_elements(this)?;
            let other_elems = get_set_or_iterable_elements(args.first().unwrap_or(&JSValue::Undefined))?;
            let common: Vec<JSValue> = this_elems
                .into_iter()
                .filter(|x| other_elems.iter().any(|y| same_value_zero(x, y)))
                .collect();
            let new_set = create_set_from_elements(common, Some(proto_for_intersect.clone()));
            Ok(JSValue::Object(new_set))
        })),
    );

    let proto_for_diff = proto.clone();
    JSObject::set_property(
        &proto,
        "difference",
        JSValue::Function(JSFunction::new_closure("difference", move |this, args| {
            let this_elems = get_set_or_iterable_elements(this)?;
            let other_elems = get_set_or_iterable_elements(args.first().unwrap_or(&JSValue::Undefined))?;
            let diff: Vec<JSValue> = this_elems
                .into_iter()
                .filter(|x| !other_elems.iter().any(|y| same_value_zero(x, y)))
                .collect();
            let new_set = create_set_from_elements(diff, Some(proto_for_diff.clone()));
            Ok(JSValue::Object(new_set))
        })),
    );

    let proto_for_sym = proto.clone();
    JSObject::set_property(
        &proto,
        "symmetricDifference",
        JSValue::Function(JSFunction::new_closure("symmetricDifference", move |this, args| {
            let this_elems = get_set_or_iterable_elements(this)?;
            let other_elems = get_set_or_iterable_elements(args.first().unwrap_or(&JSValue::Undefined))?;
            let mut sym = Vec::new();
            for x in &this_elems {
                if !other_elems.iter().any(|y| same_value_zero(x, y)) {
                    sym.push(x.clone());
                }
            }
            for y in &other_elems {
                if !this_elems.iter().any(|x| same_value_zero(y, x)) {
                    sym.push(y.clone());
                }
            }
            let new_set = create_set_from_elements(sym, Some(proto_for_sym.clone()));
            Ok(JSValue::Object(new_set))
        })),
    );

    JSObject::set_property(
        &proto,
        "isSubsetOf",
        JSValue::Function(JSFunction::new_native("isSubsetOf", |this, args| {
            let this_elems = get_set_or_iterable_elements(this)?;
            let other_elems = get_set_or_iterable_elements(args.first().unwrap_or(&JSValue::Undefined))?;
            let is_sub = this_elems.iter().all(|x| other_elems.iter().any(|y| same_value_zero(x, y)));
            Ok(JSValue::Boolean(is_sub))
        })),
    );

    JSObject::set_property(
        &proto,
        "isSupersetOf",
        JSValue::Function(JSFunction::new_native("isSupersetOf", |this, args| {
            let this_elems = get_set_or_iterable_elements(this)?;
            let other_elems = get_set_or_iterable_elements(args.first().unwrap_or(&JSValue::Undefined))?;
            let is_super = other_elems.iter().all(|y| this_elems.iter().any(|x| same_value_zero(y, x)));
            Ok(JSValue::Boolean(is_super))
        })),
    );

    JSObject::set_property(
        &proto,
        "isDisjointFrom",
        JSValue::Function(JSFunction::new_native("isDisjointFrom", |this, args| {
            let this_elems = get_set_or_iterable_elements(this)?;
            let other_elems = get_set_or_iterable_elements(args.first().unwrap_or(&JSValue::Undefined))?;
            let has_common = this_elems.iter().any(|x| other_elems.iter().any(|y| same_value_zero(x, y)));
            Ok(JSValue::Boolean(!has_common))
        })),
    );

    proto
}

/// Allocates the global `Set` constructor object.
pub fn create_set_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let set_ctor = JSObject::new_empty(None);
    JSObject::set_property(&set_ctor, "prototype", JSValue::Object(prototype.clone()));

    let proto_clone = prototype.clone();
    let ctor_fn = JSFunction::new_closure("Set", move |_this, args| {
        let instance = new_set_instance(Some(proto_clone.clone()));
        if let Some(first) = args.first() {
            if let JSValue::Array(ref arr) = first {
                let elements = arr.borrow().elements.clone();
                for elem in elements {
                    let mut borrowed = instance.borrow_mut();
                    if !borrowed.ext_or_default().set_data.iter().any(|item| same_value_zero(item, &elem)) {
                        borrowed.ext_mut().set_data.push(elem);
                    }
                }
            }
        }
        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&set_ctor, "__call__", JSValue::Function(ctor_fn));
    set_ctor
}

// -----------------------------------------------------------------------------
// WeakMap Implementation
// -----------------------------------------------------------------------------

/// Allocates a new `WeakMap` instance with the given prototype.
pub fn new_weak_map_instance(prototype: Option<Rc<RefCell<JSObject>>>) -> Rc<RefCell<JSObject>> {
    let map = Map::root(InstanceType::JSWeakMap, prototype);
    JSObject::new_with_map(map)
}

/// Creates the `WeakMap.prototype` object.
pub fn create_weak_map_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // WeakMap.prototype.set(key, value)
    JSObject::set_property(
        &proto,
        "set",
        JSValue::Function(JSFunction::new_native("set", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let key = match args.first() {
                    Some(k @ (JSValue::Object(_) | JSValue::Array(_) | JSValue::Function(_))) => k.clone(),
                    _ => return Err("TypeError: Invalid value used as weak map key".to_string()),
                };
                let val = args.get(1).cloned().unwrap_or(JSValue::Undefined);

                let mut borrowed = obj.borrow_mut();
                let mut found = false;
                for entry in borrowed.ext_mut().map_data.iter_mut() {
                    if entry.0 == key {
                        entry.1 = val.clone();
                        found = true;
                        break;
                    }
                }
                if !found {
                    borrowed.ext_mut().map_data.push((key, val));
                }
                Ok(this.clone())
            } else {
                Err("TypeError: Method WeakMap.prototype.set called on incompatible receiver".to_string())
            }
        })),
    );

    // WeakMap.prototype.get(key)
    JSObject::set_property(
        &proto,
        "get",
        JSValue::Function(JSFunction::new_native("get", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let key = match args.first() {
                    Some(k @ (JSValue::Object(_) | JSValue::Array(_) | JSValue::Function(_))) => k,
                    _ => return Ok(JSValue::Undefined),
                };
                let borrowed = obj.borrow();
                for entry in borrowed.ext_or_default().map_data.iter() {
                    if entry.0 == *key {
                        return Ok(entry.1.clone());
                    }
                }
                Ok(JSValue::Undefined)
            } else {
                Err("TypeError: Method WeakMap.prototype.get called on incompatible receiver".to_string())
            }
        })),
    );

    // WeakMap.prototype.has(key)
    JSObject::set_property(
        &proto,
        "has",
        JSValue::Function(JSFunction::new_native("has", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let key = match args.first() {
                    Some(k @ (JSValue::Object(_) | JSValue::Array(_) | JSValue::Function(_))) => k,
                    _ => return Ok(JSValue::Boolean(false)),
                };
                let exists = obj.borrow().ext_or_default().map_data.iter().any(|entry| entry.0 == *key);
                Ok(JSValue::Boolean(exists))
            } else {
                Err("TypeError: Method WeakMap.prototype.has called on incompatible receiver".to_string())
            }
        })),
    );

    // WeakMap.prototype.delete(key)
    JSObject::set_property(
        &proto,
        "delete",
        JSValue::Function(JSFunction::new_native("delete", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let key = match args.first() {
                    Some(k @ (JSValue::Object(_) | JSValue::Array(_) | JSValue::Function(_))) => k,
                    _ => return Ok(JSValue::Boolean(false)),
                };
                let mut borrowed = obj.borrow_mut();
                if let Some(idx) = borrowed.ext_or_default().map_data.iter().position(|entry| entry.0 == *key) {
                    borrowed.ext_mut().map_data.remove(idx);
                    Ok(JSValue::Boolean(true))
                } else {
                    Ok(JSValue::Boolean(false))
                }
            } else {
                Err("TypeError: Method WeakMap.prototype.delete called on incompatible receiver".to_string())
            }
        })),
    );

    proto
}

/// Allocates the global `WeakMap` constructor object.
pub fn create_weak_map_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let wm_ctor = JSObject::new_empty(None);
    JSObject::set_property(&wm_ctor, "prototype", JSValue::Object(prototype.clone()));

    let proto_clone = prototype.clone();
    let ctor_fn = JSFunction::new_closure("WeakMap", move |_this, _args| {
        let instance = new_weak_map_instance(Some(proto_clone.clone()));
        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&wm_ctor, "__call__", JSValue::Function(ctor_fn));
    wm_ctor
}

// -----------------------------------------------------------------------------
// WeakSet Implementation
// -----------------------------------------------------------------------------

/// Allocates a new `WeakSet` instance with the given prototype.
pub fn new_weak_set_instance(prototype: Option<Rc<RefCell<JSObject>>>) -> Rc<RefCell<JSObject>> {
    let map = Map::root(InstanceType::JSWeakSet, prototype);
    JSObject::new_with_map(map)
}

/// Creates the `WeakSet.prototype` object.
pub fn create_weak_set_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // WeakSet.prototype.add(value)
    JSObject::set_property(
        &proto,
        "add",
        JSValue::Function(JSFunction::new_native("add", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let val = match args.first() {
                    Some(v @ (JSValue::Object(_) | JSValue::Array(_) | JSValue::Function(_))) => v.clone(),
                    _ => return Err("TypeError: Invalid value used in weak set".to_string()),
                };

                let mut borrowed = obj.borrow_mut();
                if !borrowed.ext_or_default().set_data.iter().any(|item| item == &val) {
                    borrowed.ext_mut().set_data.push(val);
                }
                Ok(this.clone())
            } else {
                Err("TypeError: Method WeakSet.prototype.add called on incompatible receiver".to_string())
            }
        })),
    );

    // WeakSet.prototype.has(value)
    JSObject::set_property(
        &proto,
        "has",
        JSValue::Function(JSFunction::new_native("has", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let val = match args.first() {
                    Some(v @ (JSValue::Object(_) | JSValue::Array(_) | JSValue::Function(_))) => v,
                    _ => return Ok(JSValue::Boolean(false)),
                };
                let exists = obj.borrow().ext_or_default().set_data.iter().any(|item| item == val);
                Ok(JSValue::Boolean(exists))
            } else {
                Err("TypeError: Method WeakSet.prototype.has called on incompatible receiver".to_string())
            }
        })),
    );

    // WeakSet.prototype.delete(value)
    JSObject::set_property(
        &proto,
        "delete",
        JSValue::Function(JSFunction::new_native("delete", |this, args| {
            if let JSValue::Object(ref obj) = this {
                let val = match args.first() {
                    Some(v @ (JSValue::Object(_) | JSValue::Array(_) | JSValue::Function(_))) => v,
                    _ => return Ok(JSValue::Boolean(false)),
                };
                let mut borrowed = obj.borrow_mut();
                if let Some(idx) = borrowed.ext_or_default().set_data.iter().position(|item| item == val) {
                    borrowed.ext_mut().set_data.remove(idx);
                    Ok(JSValue::Boolean(true))
                } else {
                    Ok(JSValue::Boolean(false))
                }
            } else {
                Err("TypeError: Method WeakSet.prototype.delete called on incompatible receiver".to_string())
            }
        })),
    );

    proto
}

/// Allocates the global `WeakSet` constructor object.
pub fn create_weak_set_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let ws_ctor = JSObject::new_empty(None);
    JSObject::set_property(&ws_ctor, "prototype", JSValue::Object(prototype.clone()));

    let proto_clone = prototype.clone();
    let ctor_fn = JSFunction::new_closure("WeakSet", move |_this, _args| {
        let instance = new_weak_set_instance(Some(proto_clone.clone()));
        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&ws_ctor, "__call__", JSValue::Function(ctor_fn));
    ws_ctor
}
