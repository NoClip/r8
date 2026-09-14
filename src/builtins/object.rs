//! Safe Rust reimplementation of Google V8's ECMAScript `Object` built-in constructor.
//!
//! Implements `Object.keys`, `Object.values`, `Object.assign`, and `Object.create`.

use crate::objects::{JSArray, JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

pub fn create_object_constructor() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);
    let obj_ctor = JSObject::new_empty(None);
    JSObject::set_property(&obj_ctor, "prototype", JSValue::Object(proto.clone()));
    JSObject::set_property(&proto, "constructor", JSValue::Object(obj_ctor.clone()));
    JSObject::set_property(&obj_ctor, "name", JSValue::String("Object".to_string()));

    // Object.prototype.hasOwnProperty(prop)
    let has_own_prop_fn = JSFunction::new_native("hasOwnProperty", |this, args| {
        let prop_name = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let has = match this {
            JSValue::Object(obj) => obj.borrow().has_own_property(&prop_name),
            JSValue::Array(arr) => {
                if prop_name == "length" {
                    true
                } else if let Ok(idx) = prop_name.parse::<usize>() {
                    idx < arr.borrow().elements.len()
                } else {
                    arr.borrow().has_own_property(&prop_name)
                }
            }
            _ => false,
        };
        Ok(JSValue::Boolean(has))
    });
    JSObject::set_property(&proto, "hasOwnProperty", JSValue::Function(has_own_prop_fn));

    // Object.prototype.toString()
    let to_string_fn = JSFunction::new_native("toString", |this, _args| {
        let tag = match this {
            JSValue::Undefined => "Undefined",
            JSValue::Null => "Null",
            JSValue::Array(_) => "Array",
            JSValue::Function(_) => "Function",
            JSValue::String(_) => "String",
            JSValue::Number(_) | JSValue::Smi(_) => "Number",
            JSValue::Boolean(_) => "Boolean",
            JSValue::Symbol(_) => "Symbol",
            JSValue::BigInt(_) => "BigInt",
            JSValue::Object(_) => "Object",
        };
        Ok(JSValue::String(format!("[object {}]", tag)))
    });
    JSObject::set_property(&proto, "toString", JSValue::Function(to_string_fn));

    // Object.prototype.valueOf()
    let value_of_fn = JSFunction::new_native("valueOf", |this, _args| {
        Ok(this.clone())
    });
    JSObject::set_property(&proto, "valueOf", JSValue::Function(value_of_fn));

    // Object.prototype.__proto__ getter/setter (Annex B.2.2.1)
    let get_proto_fn = JSFunction::new_native("get __proto__", |this, _args| {
        if let JSValue::Object(ref obj) = this {
            let global = crate::runtime::current_global();
            let obj_proto = global.as_ref().and_then(|g| {
                if let JSValue::Object(ctor) = g.borrow().get_property("Object") {
                    if let JSValue::Object(p) = ctor.borrow().get_property("prototype") {
                        return Some(p);
                    }
                }
                None
            });

            if let Some(ref op) = obj_proto {
                if Rc::ptr_eq(obj, op) {
                    return Ok(JSValue::Null);
                }
            }

            let p = obj.borrow().map.borrow().prototype.clone();
            match p {
                Some(proto_obj) => Ok(JSValue::Object(proto_obj)),
                None => {
                    if let Some(op) = obj_proto {
                        Ok(JSValue::Object(op))
                    } else {
                        Ok(JSValue::Null)
                    }
                }
            }
        } else {
            Ok(JSValue::Undefined)
        }
    });
    let set_proto_fn = JSFunction::new_native("set __proto__", |this, args| {
        if let JSValue::Object(ref obj) = this {
            let new_proto = match args.first() {
                Some(JSValue::Object(p)) => Some(p.clone()),
                Some(JSValue::Null) => None,
                _ => return Ok(JSValue::Undefined),
            };
            let mut obj_mut = obj.borrow_mut();
            if Rc::strong_count(&obj_mut.map) > 1 {
                let mut new_map = (*obj_mut.map.borrow()).clone();
                new_map.prototype = new_proto;
                obj_mut.map = Rc::new(RefCell::new(new_map));
            } else {
                obj_mut.map.borrow_mut().prototype = new_proto;
            }
        }
        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&proto, "__proto__", JSValue::Function(get_proto_fn.clone()));
    JSObject::set_property(&proto, "__get___proto__", JSValue::Function(get_proto_fn));
    JSObject::set_property(&proto, "__set___proto__", JSValue::Function(set_proto_fn));

    let ctor_fn = JSFunction::new_native("Object", |_this, args| {
        if let Some(first) = args.first() {
            match first {
                JSValue::Object(_) | JSValue::Array(_) => Ok(first.clone()),
                _ => Ok(JSValue::Object(JSObject::new_empty(None))),
            }
        } else {
            Ok(JSValue::Object(JSObject::new_empty(None)))
        }
    });
    JSObject::set_property(&obj_ctor, "__call__", JSValue::Function(ctor_fn));

    // Object.keys(obj)
    JSObject::set_property(
        &obj_ctor,
        "keys",
        JSValue::Function(JSFunction::new_native("keys", |_this, args| {
            let target = match args.first() {
                Some(JSValue::Object(obj)) => obj.clone(),
                Some(JSValue::Array(arr)) => arr.clone(),
                _ => return Ok(JSValue::Array(JSArray::new_array(Vec::new()))),
            };

            let borrowed = target.borrow();
            let mut keys = Vec::new();
            if borrowed.map.borrow().is_dictionary_map {
                for k in borrowed.ext_or_default().dictionary_properties.keys() {
                    keys.push(JSValue::String(k.clone()));
                }
            } else {
                for desc in &borrowed.map.borrow().descriptors {
                    if !desc.details.is_dont_enum() {
                        keys.push(JSValue::String(desc.name.clone()));
                    }
                }
            }

            Ok(JSValue::Array(JSArray::new_array(keys)))
        })),
    );

    // Object.values(obj)
    JSObject::set_property(
        &obj_ctor,
        "values",
        JSValue::Function(JSFunction::new_native("values", |_this, args| {
            let target = match args.first() {
                Some(JSValue::Object(obj)) => obj.clone(),
                Some(JSValue::Array(arr)) => arr.clone(),
                _ => return Ok(JSValue::Array(JSArray::new_array(Vec::new()))),
            };

            let borrowed = target.borrow();
            let mut values = Vec::new();
            if borrowed.map.borrow().is_dictionary_map {
                for (v, _) in borrowed.ext_or_default().dictionary_properties.values() {
                    values.push(v.clone());
                }
            } else {
                for desc in &borrowed.map.borrow().descriptors {
                    if !desc.details.is_dont_enum() {
                        if let Some(val) = borrowed.properties.get(desc.field_index) {
                            values.push(val.clone());
                        }
                    }
                }
            }

            Ok(JSValue::Array(JSArray::new_array(values)))
        })),
    );

    // Object.assign(target, ...sources)
    JSObject::set_property(
        &obj_ctor,
        "assign",
        JSValue::Function(JSFunction::new_native("assign", |_this, args| {
            let target = match args.first() {
                Some(JSValue::Object(obj)) => obj.clone(),
                Some(other) => return Ok(other.clone()),
                None => return Ok(JSValue::Undefined),
            };

            for source_val in args.iter().skip(1) {
                if let JSValue::Object(source_obj) = source_val {
                    let borrowed = source_obj.borrow();
                    if borrowed.map.borrow().is_dictionary_map {
                        for (k, (v, _)) in &borrowed.ext_or_default().dictionary_properties {
                            JSObject::set_property(&target, k, v.clone());
                        }
                    } else {
                        for desc in &borrowed.map.borrow().descriptors {
                            if !desc.details.is_dont_enum() {
                                if let Some(v) = borrowed.properties.get(desc.field_index) {
                                    JSObject::set_property(&target, &desc.name, v.clone());
                                }
                            }
                        }
                    }
                }
            }

            Ok(JSValue::Object(target))
        })),
    );

    // Object.create(proto)
    JSObject::set_property(
        &obj_ctor,
        "create",
        JSValue::Function(JSFunction::new_native("create", |_this, args| {
            let proto = match args.first() {
                Some(JSValue::Object(p)) => Some(p.clone()),
                _ => None,
            };
            Ok(JSValue::Object(JSObject::new_empty(proto)))
        })),
    );

    // Object.entries(obj)
    JSObject::set_property(
        &obj_ctor,
        "entries",
        JSValue::Function(JSFunction::new_native("entries", |_this, args| {
            let target = match args.first() {
                Some(JSValue::Object(obj)) => obj.clone(),
                Some(JSValue::Array(arr)) => arr.clone(),
                _ => return Ok(JSValue::Array(JSArray::new_array(Vec::new()))),
            };

            let borrowed = target.borrow();
            let mut entries = Vec::new();
            if borrowed.map.borrow().is_dictionary_map {
                for (k, (v, details)) in &borrowed.ext_or_default().dictionary_properties {
                    if !details.is_dont_enum() {
                        let pair = JSArray::new_array(vec![JSValue::String(k.clone()), v.clone()]);
                        entries.push(JSValue::Array(pair));
                    }
                }
            } else {
                for desc in &borrowed.map.borrow().descriptors {
                    if !desc.details.is_dont_enum() {
                        if let Some(v) = borrowed.properties.get(desc.field_index) {
                            let pair = JSArray::new_array(vec![JSValue::String(desc.name.clone()), v.clone()]);
                            entries.push(JSValue::Array(pair));
                        }
                    }
                }
            }

            Ok(JSValue::Array(JSArray::new_array(entries)))
        })),
    );

    // Object.defineProperty(obj, prop, descriptor)
    JSObject::set_property(
        &obj_ctor,
        "defineProperty",
        JSValue::Function(JSFunction::new_native("defineProperty", |_this, args| {
            let target = match args.first() {
                Some(JSValue::Object(obj)) => obj.clone(),
                Some(JSValue::Array(arr)) => arr.clone(),
                _ => return Err("TypeError: Object.defineProperty called on non-object".to_string()),
            };

            let prop = match args.get(1) {
                Some(v) => v.to_string_val(),
                None => return Err("TypeError: Property name required".to_string()),
            };

            let descriptor = match args.get(2) {
                Some(JSValue::Object(desc)) => desc.clone(),
                _ => return Err("TypeError: Property descriptor must be an object".to_string()),
            };

            let val = descriptor.borrow().get_property("value");
            JSObject::set_property(&target, &prop, val);

            Ok(JSValue::Object(target))
        })),
    );

    // Object.groupBy(items, callback) (ES2024)
    JSObject::set_property(
        &obj_ctor,
        "groupBy",
        JSValue::Function(JSFunction::new_native("groupBy", |_this, args| {
            let items = match args.first() {
                Some(JSValue::Array(arr)) => arr.borrow().elements.clone(),
                _ => return Err("TypeError: Object.groupBy requires an array or iterable".to_string()),
            };
            let callback = match args.get(1) {
                Some(JSValue::Function(cb)) => cb.clone(),
                _ => return Err("TypeError: Object.groupBy requires a callback function".to_string()),
            };

            let result_obj = JSObject::new_empty(None);
            for (i, item) in items.into_iter().enumerate() {
                let key_val = callback.call(&JSValue::Undefined, &[item.clone(), JSValue::Smi(i as i32)])?;
                let key = key_val.to_string_val();

                let existing = result_obj.borrow().get_property(&key);
                match existing {
                    JSValue::Array(arr) => {
                        arr.borrow_mut().elements.push(item);
                    }
                    _ => {
                        let group_arr = JSArray::new_array(vec![item]);
                        JSObject::set_property(&result_obj, &key, JSValue::Array(group_arr));
                    }
                }
            }
            Ok(JSValue::Object(result_obj))
        })),
    );

    // Object.hasOwn(obj, prop) (ES2022)
    JSObject::set_property(
        &obj_ctor,
        "hasOwn",
        JSValue::Function(JSFunction::new_native("hasOwn", |_this, args| {
            let target = args.get(0).ok_or_else(|| "TypeError: Cannot convert undefined or null to object".to_string())?;
            let prop_name = args.get(1).map(|v| v.to_string_val()).unwrap_or_default();
            let has = match target {
                JSValue::Object(obj) => obj.borrow().has_own_property(&prop_name),
                JSValue::Array(arr) => {
                    if prop_name == "length" {
                        true
                    } else if let Ok(idx) = prop_name.parse::<usize>() {
                        idx < arr.borrow().elements.len()
                    } else {
                        arr.borrow().has_own_property(&prop_name)
                    }
                }
                JSValue::String(s) => {
                    if prop_name == "length" {
                        true
                    } else if let Ok(idx) = prop_name.parse::<usize>() {
                        idx < s.chars().count()
                    } else {
                        false
                    }
                }
                _ => false,
            };
            Ok(JSValue::Boolean(has))
        })),
    );

    // Object.is(v1, v2)
    JSObject::set_property(
        &obj_ctor,
        "is",
        JSValue::Function(JSFunction::new_native("is", |_this, args| {
            let v1 = args.get(0).unwrap_or(&JSValue::Undefined);
            let v2 = args.get(1).unwrap_or(&JSValue::Undefined);
            let same = match (v1, v2) {
                (JSValue::Number(x), JSValue::Number(y)) => {
                    if x.is_nan() && y.is_nan() {
                        true
                    } else if *x == 0.0 && *y == 0.0 {
                        x.to_bits() == y.to_bits()
                    } else {
                        x == y
                    }
                }
                (JSValue::Number(x), JSValue::Smi(y)) => {
                    let yf = *y as f64;
                    if *x == 0.0 && yf == 0.0 {
                        x.to_bits() == yf.to_bits()
                    } else {
                        *x == yf
                    }
                }
                (JSValue::Smi(x), JSValue::Number(y)) => {
                    let xf = *x as f64;
                    if xf == 0.0 && *y == 0.0 {
                        xf.to_bits() == y.to_bits()
                    } else {
                        xf == *y
                    }
                }
                _ => v1.strict_equal(v2),
            };
            Ok(JSValue::Boolean(same))
        })),
    );

    // Object.fromEntries(iterable)
    JSObject::set_property(
        &obj_ctor,
        "fromEntries",
        JSValue::Function(JSFunction::new_native("fromEntries", |_this, args| {
            let target = args.first().ok_or_else(|| "TypeError: Object.fromEntries requires an iterable".to_string())?;
            let pairs: Vec<JSValue> = match target {
                JSValue::Array(arr) => arr.borrow().elements.clone(),
                _ => return Err("TypeError: Object.fromEntries requires an iterable".to_string()),
            };
            let res = JSObject::new_empty(None);
            for pair in pairs {
                if let JSValue::Array(arr) = pair {
                    let borrowed = arr.borrow();
                    let key = borrowed.elements.get(0).map(|v| v.to_string_val()).unwrap_or_default();
                    let val = borrowed.elements.get(1).cloned().unwrap_or(JSValue::Undefined);
                    JSObject::set_property(&res, &key, val);
                }
            }
            Ok(JSValue::Object(res))
        })),
    );

    // Object.freeze(obj)
    JSObject::set_property(
        &obj_ctor,
        "freeze",
        JSValue::Function(JSFunction::new_native("freeze", |_this, args| {
            let target = args.first().cloned().unwrap_or(JSValue::Undefined);
            Ok(target)
        })),
    );

    // Object.seal(obj)
    JSObject::set_property(
        &obj_ctor,
        "seal",
        JSValue::Function(JSFunction::new_native("seal", |_this, args| {
            let target = args.first().cloned().unwrap_or(JSValue::Undefined);
            Ok(target)
        })),
    );

    // Object.isFrozen(obj)
    JSObject::set_property(
        &obj_ctor,
        "isFrozen",
        JSValue::Function(JSFunction::new_native("isFrozen", |_this, args| {
            match args.first() {
                Some(JSValue::Object(_)) | Some(JSValue::Array(_)) => Ok(JSValue::Boolean(false)),
                _ => Ok(JSValue::Boolean(true)),
            }
        })),
    );

    // Object.isSealed(obj)
    JSObject::set_property(
        &obj_ctor,
        "isSealed",
        JSValue::Function(JSFunction::new_native("isSealed", |_this, args| {
            match args.first() {
                Some(JSValue::Object(_)) | Some(JSValue::Array(_)) => Ok(JSValue::Boolean(false)),
                _ => Ok(JSValue::Boolean(true)),
            }
        })),
    );

    // Object.getOwnPropertyDescriptor(obj, prop)
    JSObject::set_property(
        &obj_ctor,
        "getOwnPropertyDescriptor",
        JSValue::Function(JSFunction::new_native("getOwnPropertyDescriptor", |_this, args| {
            let target = match args.first() {
                Some(JSValue::Object(obj)) => obj.clone(),
                Some(JSValue::Array(arr)) => arr.clone(),
                _ => return Ok(JSValue::Undefined),
            };
            let prop = match args.get(1) {
                Some(p) => p.to_string_val(),
                None => return Ok(JSValue::Undefined),
            };
            let borrowed = target.borrow();
            let val = if borrowed.map.borrow().is_dictionary_map {
                borrowed.ext_or_default().dictionary_properties.get(&prop).map(|(v, _)| v.clone())
            } else if let Some(idx) = borrowed.map.borrow().find_field_index(&prop) {
                borrowed.properties.get(idx).cloned()
            } else {
                None
            };
            if let Some(v) = val {
                let desc = JSObject::new_empty(None);
                JSObject::set_property(&desc, "value", v);
                JSObject::set_property(&desc, "writable", JSValue::Boolean(true));
                JSObject::set_property(&desc, "enumerable", JSValue::Boolean(true));
                JSObject::set_property(&desc, "configurable", JSValue::Boolean(true));
                Ok(JSValue::Object(desc))
            } else {
                Ok(JSValue::Undefined)
            }
        })),
    );

    // Object.getOwnPropertyNames(obj)
    JSObject::set_property(
        &obj_ctor,
        "getOwnPropertyNames",
        JSValue::Function(JSFunction::new_native("getOwnPropertyNames", |_this, args| {
            let target = match args.first() {
                Some(JSValue::Object(obj)) => obj.clone(),
                Some(JSValue::Array(arr)) => arr.clone(),
                _ => return Ok(JSValue::Array(JSArray::new_array(Vec::new()))),
            };
            let borrowed = target.borrow();
            let mut names = Vec::new();
            if borrowed.map.borrow().is_dictionary_map {
                for k in borrowed.ext_or_default().dictionary_properties.keys() {
                    names.push(JSValue::String(k.clone()));
                }
            } else {
                for desc in &borrowed.map.borrow().descriptors {
                    names.push(JSValue::String(desc.name.clone()));
                }
            }
            Ok(JSValue::Array(JSArray::new_array(names)))
        })),
    );

    // Object.getPrototypeOf(obj)
    JSObject::set_property(
        &obj_ctor,
        "getPrototypeOf",
        JSValue::Function(JSFunction::new_native("getPrototypeOf", |_this, args| {
            let proto_opt = match args.first() {
                Some(JSValue::Object(obj)) => obj.borrow().map.borrow().prototype.clone(),
                Some(JSValue::Array(arr)) => arr.borrow().map.borrow().prototype.clone(),
                _ => return Err("TypeError: Object.getPrototypeOf called on non-object".to_string()),
            };
            match proto_opt {
                Some(p) => Ok(JSValue::Object(p)),
                None => Ok(JSValue::Null),
            }
        })),
    );

    // Object.setPrototypeOf(obj, proto)
    JSObject::set_property(
        &obj_ctor,
        "setPrototypeOf",
        JSValue::Function(JSFunction::new_native("setPrototypeOf", |_this, args| {
            let target = match args.first() {
                Some(JSValue::Object(obj)) => obj.clone(),
                Some(JSValue::Array(arr)) => arr.clone(),
                _ => return Err("TypeError: Object.setPrototypeOf called on non-object".to_string()),
            };
            let new_proto = match args.get(1) {
                Some(JSValue::Object(p)) => Some(p.clone()),
                Some(JSValue::Null) => None,
                _ => return Err("TypeError: Object prototype may only be an Object or null".to_string()),
            };
            let mut target_mut = target.borrow_mut();
            if Rc::strong_count(&target_mut.map) > 1 {
                let mut new_map = (*target_mut.map.borrow()).clone();
                new_map.prototype = new_proto;
                target_mut.map = Rc::new(RefCell::new(new_map));
            } else {
                target_mut.map.borrow_mut().prototype = new_proto;
            }
            drop(target_mut);
            Ok(JSValue::Object(target))
        })),
    );

    obj_ctor
}

