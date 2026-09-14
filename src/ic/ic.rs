//! Safe Rust reimplementation of Google V8's Inline Cache (IC) Subsystem.
//!
//! Implements fast-path property loading (`LoadIC`), property storing (`StoreIC`),
//! and element access (`KeyedIC`), accelerating lookups via cached shape/map pointers.

use crate::objects::feedback_vector::{FeedbackSlot, FeedbackVector};
use crate::objects::js_object::JSObject;
use crate::objects::value::JSValue;
use std::cell::RefCell;
use std::rc::Rc;

/// Inline Cache for loading named properties.
pub struct LoadIC;

impl LoadIC {
    /// Loads a property from an object, utilizing the feedback vector's cached map & field index.
    ///
    /// Returns a tuple of `(result_value, cache_hit: bool)`.
    pub fn load(
        object: &Rc<RefCell<JSObject>>,
        name: &str,
        feedback: Option<(&FeedbackVector, FeedbackSlot)>,
    ) -> (JSValue, bool) {
        let map_ptr = {
            let borrowed = object.borrow();
            Rc::as_ptr(&borrowed.map) as usize
        };

        // 1. Fast Path: Monomorphic / Polymorphic cache hit
        if let Some((vector, slot)) = feedback {
            if let Some(field_idx) = vector.get_cached_field_index(slot, map_ptr) {
                let borrowed = object.borrow();
                if let Some(val) = borrowed.properties.get(field_idx) {
                    return (val.clone(), true);
                }
            }
        }

        // 2. Slow Path: Full property lookup through Map descriptors / dictionary
        let value = object.borrow().get_property(name);

        // Update inline cache feedback if in fast mode
        if let Some((vector, slot)) = feedback {
            let borrowed = object.borrow();
            if !borrowed.map.borrow().is_dictionary_map {
                if let Some(field_idx) = borrowed.map.borrow().find_field_index(name) {
                    vector.record_lookup(slot, map_ptr, field_idx);
                }
            }
        }

        (value, false)
    }
}

/// Inline Cache for storing named properties.
pub struct StoreIC;

impl StoreIC {
    /// Stores a property value into an object, accelerating monomorphic updates.
    ///
    /// Returns `cache_hit: bool`.
    pub fn store(
        object: &Rc<RefCell<JSObject>>,
        name: &str,
        value: JSValue,
        feedback: Option<(&FeedbackVector, FeedbackSlot)>,
    ) -> bool {
        let map_ptr = {
            let borrowed = object.borrow();
            Rc::as_ptr(&borrowed.map) as usize
        };

        // 1. Fast Path: Monomorphic cache hit
        if let Some((vector, slot)) = feedback {
            if let Some(field_idx) = vector.get_cached_field_index(slot, map_ptr) {
                let mut borrowed = object.borrow_mut();
                if field_idx < borrowed.properties.len() {
                    borrowed.properties[field_idx] = value;
                    return true;
                }
            }
        }

        // 2. Slow Path: Normal property mutation / shape transition
        JSObject::set_property(object, name, value);

        // Update inline cache
        if let Some((vector, slot)) = feedback {
            let borrowed = object.borrow();
            let new_map_ptr = Rc::as_ptr(&borrowed.map) as usize;
            if !borrowed.map.borrow().is_dictionary_map {
                if let Some(field_idx) = borrowed.map.borrow().find_field_index(name) {
                    vector.record_lookup(slot, new_map_ptr, field_idx);
                }
            }
        }

        false
    }
}

/// Keyed Inline Cache for array and indexed element operations.
pub struct KeyedIC;

impl KeyedIC {
    /// Fast element loading bypassing string conversion and map lookups.
    pub fn load_element(receiver: &JSValue, index: usize) -> Option<JSValue> {
        match receiver {
            JSValue::Array(arr) => {
                let borrowed = arr.borrow();
                borrowed.elements.get(index).cloned()
            }
            JSValue::Object(obj) => {
                let borrowed = obj.borrow();
                borrowed.elements.get(index).cloned()
            }
            _ => None,
        }
    }

    /// Fast element storing directly into element storage.
    pub fn store_element(receiver: &JSValue, index: usize, value: JSValue) -> bool {
        match receiver {
            JSValue::Array(arr) => {
                let mut borrowed = arr.borrow_mut();
                if index < borrowed.elements.len() {
                    borrowed.elements[index] = value;
                    true
                } else if index == borrowed.elements.len() {
                    borrowed.elements.push(value);
                    true
                } else {
                    borrowed.elements.resize(index, JSValue::Undefined);
                    borrowed.elements.push(value);
                    true
                }
            }
            JSValue::Object(obj) => {
                let mut borrowed = obj.borrow_mut();
                if index < borrowed.elements.len() {
                    borrowed.elements[index] = value;
                    true
                } else {
                    borrowed.elements.resize(index, JSValue::Undefined);
                    borrowed.elements.push(value);
                    true
                }
            }
            _ => false,
        }
    }
}
