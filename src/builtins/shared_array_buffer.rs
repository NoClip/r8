//! Safe Rust reimplementation of ECMAScript `SharedArrayBuffer` built-in object.
//!
//! Provides thread-shareable raw binary buffers backed by `Arc<RwLock<Vec<u8>>>`
//! for lock-free and synchronized multithreaded operations.

use crate::objects::{JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

/// Creates the `SharedArrayBuffer.prototype` object.
pub fn create_shared_array_buffer_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // slice(start, end)
    let slice_fn = JSFunction::new_native("slice", |this, args| {
        if let JSValue::Object(obj_rc) = this {
            let obj = obj_rc.borrow();
            if let Some(ref shared_data) = obj.ext_or_default().shared_array_buffer_data {
                let data = shared_data.read().unwrap();
                let len = data.len() as i64;

                let start = args.first().map(|a| a.to_number() as i64).unwrap_or(0);
                let end = args.get(1).map(|a| a.to_number() as i64).unwrap_or(len);

                let actual_start = if start < 0 { (len + start).max(0) as usize } else { (start.min(len)) as usize };
                let actual_end = if end < 0 { (len + end).max(0) as usize } else { (end.min(len)) as usize };

                let slice_len = if actual_end > actual_start { actual_end - actual_start } else { 0 };
                let mut new_bytes = vec![0u8; slice_len];
                if slice_len > 0 {
                    new_bytes.copy_from_slice(&data[actual_start..actual_end]);
                }

                let new_sab = JSObject::new_shared_array_buffer(slice_len, None);
                if let Some(ref new_shared) = new_sab.borrow().ext_or_default().shared_array_buffer_data {
                    *new_shared.write().unwrap() = new_bytes;
                }
                return Ok(JSValue::Object(new_sab));
            }
        }
        Err("TypeError: Method SharedArrayBuffer.prototype.slice called on incompatible receiver".to_string())
    });
    JSObject::set_property(&proto, "slice", JSValue::Function(slice_fn));

    proto
}

/// Creates the `SharedArrayBuffer` constructor function.
pub fn create_shared_array_buffer_constructor(prototype: &Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let proto_clone = prototype.clone();
    let ctor = JSFunction::new_closure("SharedArrayBuffer", move |_this, args| {
        let byte_length = args.first().map(|a| a.to_number().max(0.0) as usize).unwrap_or(0);
        let sab = JSObject::new_shared_array_buffer(byte_length, Some(proto_clone.clone()));
        Ok(JSValue::Object(sab))
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(prototype.clone()));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor));

    ctor_obj
}
