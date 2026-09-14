//! Safe Rust reimplementation of Google V8's `src/objects/js-array.h`.
//!
//! Provides array creation, indexing, length synchronization, and element manipulation.

use super::js_object::JSObject;
use super::map::{InstanceType, Map};
use super::value::JSValue;
use std::cell::RefCell;
use std::rc::Rc;

pub struct JSArray;

impl JSArray {
    /// Allocates a new JavaScript array object with the given initial elements.
    pub fn new_array(initial_elements: Vec<JSValue>) -> Rc<RefCell<JSObject>> {
        Self::new_array_with_proto(initial_elements, None)
    }

    /// Allocates a new JavaScript array object with the given initial elements and prototype.
    pub fn new_array_with_proto(
        initial_elements: Vec<JSValue>,
        prototype: Option<Rc<RefCell<JSObject>>>,
    ) -> Rc<RefCell<JSObject>> {
        let map = Map::root(InstanceType::JSArray, prototype);
        Rc::new(RefCell::new(JSObject {
            map,
            properties: Vec::new(),
            elements: initial_elements,
            has_accessors: false,
            ext: None,
        }))
    }

    /// Pushes a value to the end of the array, returning the new length.
    pub fn push(arr: &Rc<RefCell<JSObject>>, value: JSValue) -> usize {
        let mut obj = arr.borrow_mut();
        obj.elements.push(value);
        obj.elements.len()
    }

    /// Pops a value from the end of the array.
    pub fn pop(arr: &Rc<RefCell<JSObject>>) -> JSValue {
        let mut obj = arr.borrow_mut();
        obj.elements.pop().unwrap_or(JSValue::Undefined)
    }

    /// Retrieves the current length of the array.
    pub fn length(arr: &Rc<RefCell<JSObject>>) -> usize {
        arr.borrow().elements.len()
    }
}
