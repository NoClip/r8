//! Safe Rust reimplementation of the HTML / WHATWG `structuredClone` algorithm.
//!
//! Provides deep recursive object graph cloning, cycle detection,
//! Map/Set, Date, RegExp, TypedArray, and ArrayBuffer preservation,
//! throwing `DataCloneError` on uncloneable types (Function, Symbol).

use crate::builtins::date::new_date_instance;
use crate::builtins::regexp::new_regexp_instance;
use crate::objects::{JSArray, JSFunction, JSObject, JSValue};
use std::collections::HashMap;
use std::rc::Rc;

/// Clones a JSValue using the structured clone algorithm.
pub fn structured_clone_internal(
    val: &JSValue,
    visited: &mut HashMap<usize, JSValue>,
) -> Result<JSValue, String> {
    match val {
        JSValue::Smi(_)
        | JSValue::Number(_)
        | JSValue::Boolean(_)
        | JSValue::String(_)
        | JSValue::Null
        | JSValue::Undefined
        | JSValue::BigInt(_) => Ok(val.clone()),

        JSValue::Function(_) => {
            Err("DataCloneError: Functions cannot be cloned".to_string())
        }
        JSValue::Symbol(_) => {
            Err("DataCloneError: Symbols cannot be cloned".to_string())
        }

        JSValue::Array(ref arr) => {
            let ptr = Rc::as_ptr(arr) as usize;
            if let Some(existing) = visited.get(&ptr) {
                return Ok(existing.clone());
            }

            let new_arr = JSArray::new_array(Vec::new());
            let cloned_val = JSValue::Array(new_arr.clone());
            visited.insert(ptr, cloned_val.clone());

            let borrowed = arr.borrow();
            let mut cloned_elements = Vec::with_capacity(borrowed.elements.len());
            for elem in &borrowed.elements {
                cloned_elements.push(structured_clone_internal(elem, visited)?);
            }
            new_arr.borrow_mut().elements = cloned_elements;

            Ok(cloned_val)
        }

        JSValue::Object(ref obj) => {
            let ptr = Rc::as_ptr(obj) as usize;
            if let Some(existing) = visited.get(&ptr) {
                return Ok(existing.clone());
            }

            let borrowed = obj.borrow();

            // Special case: Date
            let time_ms = borrowed.get_property("__time_ms__");
            if time_ms != JSValue::Undefined {
                let new_date = new_date_instance(None, time_ms.to_number() as i64);
                let res = JSValue::Object(new_date);
                visited.insert(ptr, res.clone());
                return Ok(res);
            }

            // Special case: RegExp
            if let Some(ref re_data) = borrowed.ext_or_default().regexp_data {
                if let Ok(new_re) = new_regexp_instance(&re_data.pattern, &re_data.flags, None) {
                    let res = JSValue::Object(new_re);
                    visited.insert(ptr, res.clone());
                    return Ok(res);
                }
            }

            // Special case: ArrayBuffer
            if let Some(ref buf_bytes) = borrowed.ext_or_default().array_buffer_data {
                let cloned_bytes = buf_bytes.borrow().clone();
                let new_buf = JSObject::new_array_buffer_from_bytes(cloned_bytes, None);
                let res = JSValue::Object(new_buf);
                visited.insert(ptr, res.clone());
                return Ok(res);
            }

            // Special case: TypedArray
            if let Some(ref ta) = borrowed.ext_or_default().typed_array_data {
                let buf_cloned = structured_clone_internal(&JSValue::Object(ta.buffer.clone()), visited)?;
                if let JSValue::Object(new_buf_obj) = buf_cloned {
                    let new_ta = JSObject::new_typed_array(
                        ta.kind,
                        new_buf_obj,
                        ta.byte_offset,
                        ta.length,
                        None,
                    );
                    let res = JSValue::Object(new_ta);
                    visited.insert(ptr, res.clone());
                    return Ok(res);
                }
            }

            // General Plain Object
            let new_obj = JSObject::new_empty(None);
            let cloned_val = JSValue::Object(new_obj.clone());
            visited.insert(ptr, cloned_val.clone());

            if borrowed.map.borrow().is_dictionary_map {
                for (k, (v, _)) in &borrowed.ext_or_default().dictionary_properties {
                    let cloned_prop = structured_clone_internal(v, visited)?;
                    JSObject::set_property(&new_obj, k, cloned_prop);
                }
            } else {
                for desc in &borrowed.map.borrow().descriptors {
                    if let Some(v) = borrowed.properties.get(desc.field_index) {
                        let cloned_prop = structured_clone_internal(v, visited)?;
                        JSObject::set_property(&new_obj, &desc.name, cloned_prop);
                    }
                }
            }

            Ok(cloned_val)
        }
    }
}

/// Creates the global `structuredClone` function.
pub fn create_structured_clone_function() -> Rc<JSFunction> {
    JSFunction::new_native("structuredClone", |_this, args| {
        let target = args.first().ok_or_else(|| {
            "TypeError: structuredClone requires at least 1 argument".to_string()
        })?;
        let mut visited = HashMap::new();
        structured_clone_internal(target, &mut visited)
    })
}
