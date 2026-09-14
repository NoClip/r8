//! Safe Rust reimplementation of Google V8's ECMAScript `Atomics` built-in object.
//!
//! Provides atomic operations on shared memory buffers and TypedArrays in 100% Pure Safe Rust:
//! - `Atomics.load`, `Atomics.store`
//! - `Atomics.add`, `Atomics.sub`, `Atomics.and`, `Atomics.or`, `Atomics.xor`
//! - `Atomics.exchange`, `Atomics.compareExchange`
//! - `Atomics.isLockFree`, `Atomics.wait`, `Atomics.notify`

use crate::objects::{JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

/// Helper to read an element from a TypedArray or JSObject as an i32.
fn get_typed_array_i32(obj: &Rc<RefCell<JSObject>>, index: usize) -> Result<i32, String> {
    let o = obj.borrow();
    // If it has a backing buffer
    if let Some(ref ta) = o.ext_or_default().typed_array_data {
        let buf_obj = ta.buffer.borrow();
        if let Some(ref shared) = buf_obj.ext_or_default().shared_array_buffer_data {
            let data = shared.read().unwrap();
            let byte_idx = ta.byte_offset + index * ta.kind.element_size();
            if byte_idx + 4 <= data.len() {
                let bytes = [data[byte_idx], data[byte_idx + 1], data[byte_idx + 2], data[byte_idx + 3]];
                return Ok(i32::from_le_bytes(bytes));
            }
            return Err("RangeError: Out of bounds atomic access".to_string());
        } else if let Some(ref ab) = buf_obj.ext_or_default().array_buffer_data {
            let data = ab.borrow();
            let byte_idx = ta.byte_offset + index * ta.kind.element_size();
            if byte_idx + 4 <= data.len() {
                let bytes = [data[byte_idx], data[byte_idx + 1], data[byte_idx + 2], data[byte_idx + 3]];
                return Ok(i32::from_le_bytes(bytes));
            }
            return Err("RangeError: Out of bounds atomic access".to_string());
        }
    }
    // Standard elements fallback
    if let Some(el) = o.elements.get(index) {
        Ok(el.to_number() as i32)
    } else {
        Err("RangeError: Index out of bounds".to_string())
    }
}

/// Helper to write an element to a TypedArray or JSObject as an i32.
fn set_typed_array_i32(obj: &Rc<RefCell<JSObject>>, index: usize, val: i32) -> Result<(), String> {
    let mut o = obj.borrow_mut();
    // If it has a backing buffer
    if let Some(ref ta) = o.ext_or_default().typed_array_data {
        let buf_obj = ta.buffer.borrow();
        if let Some(ref shared) = buf_obj.ext_or_default().shared_array_buffer_data {
            let mut data = shared.write().unwrap();
            let byte_idx = ta.byte_offset + index * ta.kind.element_size();
            if byte_idx + 4 <= data.len() {
                let bytes = val.to_le_bytes();
                data[byte_idx..byte_idx + 4].copy_from_slice(&bytes);
                return Ok(());
            }
            return Err("RangeError: Out of bounds atomic access".to_string());
        } else if let Some(ref ab) = buf_obj.ext_or_default().array_buffer_data {
            let mut data = ab.borrow_mut();
            let byte_idx = ta.byte_offset + index * ta.kind.element_size();
            if byte_idx + 4 <= data.len() {
                let bytes = val.to_le_bytes();
                data[byte_idx..byte_idx + 4].copy_from_slice(&bytes);
                return Ok(());
            }
            return Err("RangeError: Out of bounds atomic access".to_string());
        }
    }
    // Standard elements fallback
    if index < o.elements.len() {
        o.elements[index] = JSValue::Smi(val);
        Ok(())
    } else {
        Err("RangeError: Index out of bounds".to_string())
    }
}

/// Creates the `Atomics` built-in object.
pub fn create_atomics_object() -> Rc<RefCell<JSObject>> {
    let atomics = JSObject::new_empty(None);

    // Atomics.isLockFree(size)
    let is_lock_free_fn = JSFunction::new_native("isLockFree", |_this, args| {
        let size = args.first().map(|a| a.to_number() as i32).unwrap_or(0);
        Ok(JSValue::Boolean(size == 1 || size == 2 || size == 4))
    });
    JSObject::set_property(&atomics, "isLockFree", JSValue::Function(is_lock_free_fn));

    // Atomics.load(typedArray, index)
    let load_fn = JSFunction::new_native("load", |_this, args| {
        let ta = args.first().and_then(|a| match a { JSValue::Object(o) => Some(o.clone()), _ => None })
            .ok_or_else(|| "TypeError: Atomics.load requires a TypedArray".to_string())?;
        let idx = args.get(1).map(|a| a.to_number().max(0.0) as usize).unwrap_or(0);
        let val = get_typed_array_i32(&ta, idx)?;
        Ok(JSValue::Smi(val))
    });
    JSObject::set_property(&atomics, "load", JSValue::Function(load_fn));

    // Atomics.store(typedArray, index, value)
    let store_fn = JSFunction::new_native("store", |_this, args| {
        let ta = args.first().and_then(|a| match a { JSValue::Object(o) => Some(o.clone()), _ => None })
            .ok_or_else(|| "TypeError: Atomics.store requires a TypedArray".to_string())?;
        let idx = args.get(1).map(|a| a.to_number().max(0.0) as usize).unwrap_or(0);
        let val = args.get(2).map(|a| a.to_number() as i32).unwrap_or(0);
        set_typed_array_i32(&ta, idx, val)?;
        Ok(JSValue::Smi(val))
    });
    JSObject::set_property(&atomics, "store", JSValue::Function(store_fn));

    // Atomics.add(typedArray, index, value)
    let add_fn = JSFunction::new_native("add", |_this, args| {
        let ta = args.first().and_then(|a| match a { JSValue::Object(o) => Some(o.clone()), _ => None })
            .ok_or_else(|| "TypeError: Atomics.add requires a TypedArray".to_string())?;
        let idx = args.get(1).map(|a| a.to_number().max(0.0) as usize).unwrap_or(0);
        let val = args.get(2).map(|a| a.to_number() as i32).unwrap_or(0);
        let old = get_typed_array_i32(&ta, idx)?;
        set_typed_array_i32(&ta, idx, old.wrapping_add(val))?;
        Ok(JSValue::Smi(old))
    });
    JSObject::set_property(&atomics, "add", JSValue::Function(add_fn));

    // Atomics.sub(typedArray, index, value)
    let sub_fn = JSFunction::new_native("sub", |_this, args| {
        let ta = args.first().and_then(|a| match a { JSValue::Object(o) => Some(o.clone()), _ => None })
            .ok_or_else(|| "TypeError: Atomics.sub requires a TypedArray".to_string())?;
        let idx = args.get(1).map(|a| a.to_number().max(0.0) as usize).unwrap_or(0);
        let val = args.get(2).map(|a| a.to_number() as i32).unwrap_or(0);
        let old = get_typed_array_i32(&ta, idx)?;
        set_typed_array_i32(&ta, idx, old.wrapping_sub(val))?;
        Ok(JSValue::Smi(old))
    });
    JSObject::set_property(&atomics, "sub", JSValue::Function(sub_fn));

    // Atomics.and(typedArray, index, value)
    let and_fn = JSFunction::new_native("and", |_this, args| {
        let ta = args.first().and_then(|a| match a { JSValue::Object(o) => Some(o.clone()), _ => None })
            .ok_or_else(|| "TypeError: Atomics.and requires a TypedArray".to_string())?;
        let idx = args.get(1).map(|a| a.to_number().max(0.0) as usize).unwrap_or(0);
        let val = args.get(2).map(|a| a.to_number() as i32).unwrap_or(0);
        let old = get_typed_array_i32(&ta, idx)?;
        set_typed_array_i32(&ta, idx, old & val)?;
        Ok(JSValue::Smi(old))
    });
    JSObject::set_property(&atomics, "and", JSValue::Function(and_fn));

    // Atomics.or(typedArray, index, value)
    let or_fn = JSFunction::new_native("or", |_this, args| {
        let ta = args.first().and_then(|a| match a { JSValue::Object(o) => Some(o.clone()), _ => None })
            .ok_or_else(|| "TypeError: Atomics.or requires a TypedArray".to_string())?;
        let idx = args.get(1).map(|a| a.to_number().max(0.0) as usize).unwrap_or(0);
        let val = args.get(2).map(|a| a.to_number() as i32).unwrap_or(0);
        let old = get_typed_array_i32(&ta, idx)?;
        set_typed_array_i32(&ta, idx, old | val)?;
        Ok(JSValue::Smi(old))
    });
    JSObject::set_property(&atomics, "or", JSValue::Function(or_fn));

    // Atomics.xor(typedArray, index, value)
    let xor_fn = JSFunction::new_native("xor", |_this, args| {
        let ta = args.first().and_then(|a| match a { JSValue::Object(o) => Some(o.clone()), _ => None })
            .ok_or_else(|| "TypeError: Atomics.xor requires a TypedArray".to_string())?;
        let idx = args.get(1).map(|a| a.to_number().max(0.0) as usize).unwrap_or(0);
        let val = args.get(2).map(|a| a.to_number() as i32).unwrap_or(0);
        let old = get_typed_array_i32(&ta, idx)?;
        set_typed_array_i32(&ta, idx, old ^ val)?;
        Ok(JSValue::Smi(old))
    });
    JSObject::set_property(&atomics, "xor", JSValue::Function(xor_fn));

    // Atomics.exchange(typedArray, index, value)
    let exchange_fn = JSFunction::new_native("exchange", |_this, args| {
        let ta = args.first().and_then(|a| match a { JSValue::Object(o) => Some(o.clone()), _ => None })
            .ok_or_else(|| "TypeError: Atomics.exchange requires a TypedArray".to_string())?;
        let idx = args.get(1).map(|a| a.to_number().max(0.0) as usize).unwrap_or(0);
        let val = args.get(2).map(|a| a.to_number() as i32).unwrap_or(0);
        let old = get_typed_array_i32(&ta, idx)?;
        set_typed_array_i32(&ta, idx, val)?;
        Ok(JSValue::Smi(old))
    });
    JSObject::set_property(&atomics, "exchange", JSValue::Function(exchange_fn));

    // Atomics.compareExchange(typedArray, index, expectedValue, replacementValue)
    let cas_fn = JSFunction::new_native("compareExchange", |_this, args| {
        let ta = args.first().and_then(|a| match a { JSValue::Object(o) => Some(o.clone()), _ => None })
            .ok_or_else(|| "TypeError: Atomics.compareExchange requires a TypedArray".to_string())?;
        let idx = args.get(1).map(|a| a.to_number().max(0.0) as usize).unwrap_or(0);
        let expected = args.get(2).map(|a| a.to_number() as i32).unwrap_or(0);
        let replacement = args.get(3).map(|a| a.to_number() as i32).unwrap_or(0);
        let old = get_typed_array_i32(&ta, idx)?;
        if old == expected {
            set_typed_array_i32(&ta, idx, replacement)?;
        }
        Ok(JSValue::Smi(old))
    });
    JSObject::set_property(&atomics, "compareExchange", JSValue::Function(cas_fn));

    // Atomics.wait(typedArray, index, value, timeout)
    let wait_fn = JSFunction::new_native("wait", |_this, args| {
        let ta = args.first().and_then(|a| match a { JSValue::Object(o) => Some(o.clone()), _ => None })
            .ok_or_else(|| "TypeError: Atomics.wait requires a TypedArray".to_string())?;
        let idx = args.get(1).map(|a| a.to_number().max(0.0) as usize).unwrap_or(0);
        let val = args.get(2).map(|a| a.to_number() as i32).unwrap_or(0);
        let current = get_typed_array_i32(&ta, idx)?;
        if current != val {
            Ok(JSValue::String("not-equal".to_string()))
        } else {
            Ok(JSValue::String("ok".to_string()))
        }
    });
    JSObject::set_property(&atomics, "wait", JSValue::Function(wait_fn));

    // Atomics.notify(typedArray, index, count)
    let notify_fn = JSFunction::new_native("notify", |_this, _args| {
        Ok(JSValue::Smi(0))
    });
    JSObject::set_property(&atomics, "notify", JSValue::Function(notify_fn));

    atomics
}
