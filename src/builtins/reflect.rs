//! Safe Rust reimplementation of Google V8's `Reflect` built-in object.
//!
//! Exposes standard reflection methods mirroring Proxy traps.

use crate::objects::js_object::JSObject;
use crate::objects::value::JSValue;
use crate::objects::JSFunction;
use std::cell::RefCell;
use std::rc::Rc;

pub fn create_reflect_object() -> Rc<RefCell<JSObject>> {
    let reflect = JSObject::new_empty(None);

    // Reflect.get(target, propertyKey, receiver)
    let get_fn = JSFunction::new_native("get", |_this, args| {
        let target = args.get(0).ok_or_else(|| "Reflect.get requires target".to_string())?;
        let prop = args.get(1).map(|v| v.to_string()).unwrap_or_else(|| "undefined".to_string());

        match target {
            JSValue::Object(o) => Ok(o.borrow().get_property(&prop)),
            JSValue::Array(a) => {
                if let Ok(idx) = prop.parse::<usize>() {
                    Ok(a.borrow().get_element(idx))
                } else {
                    Ok(a.borrow().get_property(&prop))
                }
            }
            _ => Err("Reflect.get called on non-object".to_string()),
        }
    });
    JSObject::set_property(&reflect, "get", JSValue::Function(get_fn));

    // Reflect.set(target, propertyKey, value, receiver)
    let set_fn = JSFunction::new_native("set", |_this, args| {
        let target = args.get(0).ok_or_else(|| "Reflect.set requires target".to_string())?;
        let prop = args.get(1).map(|v| v.to_string()).unwrap_or_else(|| "undefined".to_string());
        let val = args.get(2).cloned().unwrap_or(JSValue::Undefined);

        match target {
            JSValue::Object(o) => {
                JSObject::set_property(o, &prop, val);
                Ok(JSValue::Boolean(true))
            }
            JSValue::Array(a) => {
                if let Ok(idx) = prop.parse::<usize>() {
                    a.borrow_mut().set_element(idx, val);
                } else {
                    JSObject::set_property(a, &prop, val);
                }
                Ok(JSValue::Boolean(true))
            }
            _ => Err("Reflect.set called on non-object".to_string()),
        }
    });
    JSObject::set_property(&reflect, "set", JSValue::Function(set_fn));

    // Reflect.has(target, propertyKey)
    let has_fn = JSFunction::new_native("has", |_this, args| {
        let target = args.get(0).ok_or_else(|| "Reflect.has requires target".to_string())?;
        let prop = args.get(1).map(|v| v.to_string()).unwrap_or_else(|| "undefined".to_string());

        match target {
            JSValue::Object(o) => Ok(JSValue::Boolean(o.borrow().has_property(&prop))),
            JSValue::Array(a) => Ok(JSValue::Boolean(a.borrow().has_property(&prop))),
            _ => Err("Reflect.has called on non-object".to_string()),
        }
    });
    JSObject::set_property(&reflect, "has", JSValue::Function(has_fn));

    // Reflect.deleteProperty(target, propertyKey)
    let delete_fn = JSFunction::new_native("deleteProperty", |_this, args| {
        let target = args.get(0).ok_or_else(|| "Reflect.deleteProperty requires target".to_string())?;
        let prop = args.get(1).map(|v| v.to_string()).unwrap_or_else(|| "undefined".to_string());

        match target {
            JSValue::Object(o) => Ok(JSValue::Boolean(o.borrow_mut().delete_property(&prop))),
            JSValue::Array(a) => Ok(JSValue::Boolean(a.borrow_mut().delete_property(&prop))),
            _ => Err("Reflect.deleteProperty called on non-object".to_string()),
        }
    });
    JSObject::set_property(&reflect, "deleteProperty", JSValue::Function(delete_fn));

    // Reflect.apply(target, thisArgument, argumentsList)
    let apply_fn = JSFunction::new_native("apply", |_this, args| {
        let target = args.get(0).ok_or_else(|| "Reflect.apply requires target".to_string())?;
        let this_arg = args.get(1).unwrap_or(&JSValue::Undefined);
        let args_list = args.get(2).unwrap_or(&JSValue::Undefined);

        let call_args: Vec<JSValue> = match args_list {
            JSValue::Array(arr) => arr.borrow().elements.clone(),
            _ => Vec::new(),
        };

        match target {
            JSValue::Function(f) => f.call(this_arg, &call_args),
            _ => Err("Reflect.apply target must be a function".to_string()),
        }
    });
    JSObject::set_property(&reflect, "apply", JSValue::Function(apply_fn));

    // Reflect.construct(target, argumentsList, newTarget)
    let construct_fn = JSFunction::new_native("construct", |_this, args| {
        let target = args.get(0).ok_or_else(|| "Reflect.construct requires target".to_string())?;
        let args_list = args.get(1).unwrap_or(&JSValue::Undefined);

        let call_args: Vec<JSValue> = match args_list {
            JSValue::Array(arr) => arr.borrow().elements.clone(),
            _ => Vec::new(),
        };

        match target {
            JSValue::Function(f) => {
                let new_inst = JSObject::new_empty(None);
                let inst_val = JSValue::Object(new_inst);
                let res = f.call(&inst_val, &call_args)?;
                match res {
                    JSValue::Object(_) | JSValue::Array(_) => Ok(res),
                    _ => Ok(inst_val),
                }
            }
            _ => Err("Reflect.construct target must be a constructor".to_string()),
        }
    });
    JSObject::set_property(&reflect, "construct", JSValue::Function(construct_fn));

    // Reflect.ownKeys(target)
    let own_keys_fn = JSFunction::new_native("ownKeys", |_this, args| {
        let target = args.get(0).ok_or_else(|| "Reflect.ownKeys requires target".to_string())?;
        match target {
            JSValue::Object(o) => {
                let obj = o.borrow();
                let mut keys = Vec::new();
                if obj.map.borrow().is_dictionary_map {
                    for k in obj.ext_or_default().dictionary_properties.keys() {
                        keys.push(JSValue::String(k.clone()));
                    }
                } else {
                    for desc in &obj.map.borrow().descriptors {
                        keys.push(JSValue::String(desc.name.clone()));
                    }
                }
                let arr = JSObject::new_empty(None);
                arr.borrow_mut().elements = keys;
                arr.borrow_mut().map.borrow_mut().instance_type = crate::objects::map::InstanceType::JSArray;
                Ok(JSValue::Array(arr))
            }
            _ => Err("Reflect.ownKeys called on non-object".to_string()),
        }
    });
    JSObject::set_property(&reflect, "ownKeys", JSValue::Function(own_keys_fn));

    reflect
}
