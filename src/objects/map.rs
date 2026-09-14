//! Safe Rust reimplementation of Google V8's `src/objects/map.h`.
//!
//! Represents Hidden Classes (Shapes) in V8, tracking property descriptors,
//! in-object layout, instance types, and transition trees.

use super::js_object::JSObject;
use super::property_details::PropertyDetails;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InstanceType {
    JSObject,
    JSArray,
    JSFunction,
    JSPromise,
    JSPrimitive,
    JSMap,
    JSSet,
    JSWeakMap,
    JSWeakSet,
    JSSymbol,
    JSRegExp,
    JSArrayBuffer,
    JSSharedArrayBuffer,
    JSTypedArray,
    JSDataView,
    JSProxy,
}

/// A descriptor entry mapping a property name to its storage slot and details.
#[derive(Clone, Debug, PartialEq)]
pub struct Descriptor {
    pub name: String,
    pub details: PropertyDetails,
    pub field_index: usize,
}

/// Hidden Class (Shape) describing the layout and prototype of a JSObject.
#[derive(Clone, Debug)]
pub struct Map {
    pub instance_type: InstanceType,
    pub instance_size: usize,
    pub descriptors: Vec<Descriptor>,
    pub descriptor_index: HashMap<String, usize>,
    pub transitions: HashMap<String, Rc<RefCell<Map>>>,
    pub prototype: Option<Rc<RefCell<JSObject>>>,
    pub is_dictionary_map: bool,
}

impl Map {
    /// Creates a new root Map with empty descriptors and transitions.
    pub fn root(instance_type: InstanceType, prototype: Option<Rc<RefCell<JSObject>>>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            instance_type,
            instance_size: 0,
            descriptors: Vec::new(),
            descriptor_index: HashMap::new(),
            transitions: HashMap::new(),
            prototype,
            is_dictionary_map: false,
        }))
    }

    /// Finds the property storage field index for a named property in fast mode.
    #[inline(always)]
    pub fn find_field_index(&self, name: &str) -> Option<usize> {
        self.descriptor_index.get(name).map(|&idx| self.descriptors[idx].field_index)
    }

    /// Finds the descriptor for a named property.
    #[inline(always)]
    pub fn find_descriptor(&self, name: &str) -> Option<&Descriptor> {
        self.descriptor_index.get(name).map(|&idx| &self.descriptors[idx])
    }

    /// Transitions this Map to a new Map that has the added property.
    ///
    /// If an identical transition branch already exists in `transitions`,
    /// it is reused, ensuring structural shape sharing between objects.
    pub fn transition_to_property(
        current_rc: &Rc<RefCell<Map>>,
        name: &str,
        details: PropertyDetails,
    ) -> Rc<RefCell<Map>> {
        let mut current = current_rc.borrow_mut();

        // 1. Check existing transition tree branch
        if let Some(existing_target) = current.transitions.get(name) {
            return existing_target.clone();
        }

        // 2. Create new Map branching from current descriptors
        let mut new_descriptors = current.descriptors.clone();
        let mut new_descriptor_index = current.descriptor_index.clone();
        let field_index = new_descriptors.len();
        new_descriptors.push(Descriptor {
            name: name.to_string(),
            details,
            field_index,
        });
        new_descriptor_index.insert(name.to_string(), field_index);

        let new_map = Rc::new(RefCell::new(Map {
            instance_type: current.instance_type,
            instance_size: current.instance_size + 1,
            descriptors: new_descriptors,
            descriptor_index: new_descriptor_index,
            transitions: HashMap::new(),
            prototype: current.prototype.clone(),
            is_dictionary_map: false,
        }));

        current.transitions.insert(name.to_string(), new_map.clone());
        new_map
    }

    /// Transitions a Map to slow dictionary mode (e.g. after property deletion).
    pub fn to_dictionary_mode(&self) -> Rc<RefCell<Map>> {
        Rc::new(RefCell::new(Map {
            instance_type: self.instance_type,
            instance_size: 0,
            descriptors: Vec::new(),
            descriptor_index: HashMap::new(),
            transitions: HashMap::new(),
            prototype: self.prototype.clone(),
            is_dictionary_map: true,
        }))
    }
}
