//! Safe Rust reimplementation of Google V8's `src/objects/js-objects.h`.
//!
//! Represents JavaScript object instances, fast in-object/out-of-object property
//! storage, dictionary mode fallback, indexed element storage, and prototype inheritance.

use super::map::{InstanceType, Map};
use super::property_details::PropertyDetails;
use super::proxy::ProxyData;
use super::typed_array::{DataViewData, TypedArrayData, TypedArrayKind};
use super::value::JSValue;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct RegExpData {
    pub pattern: String,
    pub flags: String,
    pub global: bool,
    pub ignore_case: bool,
    pub multiline: bool,
    pub dot_all: bool,
    pub unicode: bool,
    pub unicode_sets: bool,
    pub sticky: bool,
    pub has_indices: bool,
    pub last_index: usize,
    pub bytecode: Rc<crate::regexp::bytecodes::RegExpBytecode>,
    pub capture_count: usize,
    pub named_groups: HashMap<String, usize>,
}

/// Exotic data slots for specialized JSObject types (dictionary mode, Map/Set,
/// RegExp, ArrayBuffer, TypedArray, DataView, Proxy). Boxed behind `Box<...>`
/// so that plain objects only pay for an 8-byte pointer instead of ~330 bytes.
#[derive(Clone, Debug)]
pub struct ExoticSlots {
    pub dictionary_properties: HashMap<String, (JSValue, PropertyDetails)>,
    pub map_data: Vec<(JSValue, JSValue)>,
    pub set_data: Vec<JSValue>,
    pub regexp_data: Option<RegExpData>,
    pub array_buffer_data: Option<Rc<RefCell<Vec<u8>>>>,
    pub typed_array_data: Option<TypedArrayData>,
    pub data_view_data: Option<DataViewData>,
    pub proxy_data: Option<ProxyData>,
    pub shared_array_buffer_data: Option<Arc<RwLock<Vec<u8>>>>,
    pub generator_data: Option<Rc<RefCell<crate::objects::generator::GeneratorData>>>,
    pub array_iterator_data: Option<Rc<RefCell<crate::objects::generator::ArrayIteratorData>>>,
    pub string_iterator_data: Option<Rc<RefCell<crate::objects::generator::StringIteratorData>>>,
}

impl Default for ExoticSlots {
    fn default() -> Self {
        Self {
            dictionary_properties: HashMap::new(),
            map_data: Vec::new(),
            set_data: Vec::new(),
            regexp_data: None,
            array_buffer_data: None,
            typed_array_data: None,
            data_view_data: None,
            proxy_data: None,
            shared_array_buffer_data: None,
            generator_data: None,
            array_iterator_data: None,
            string_iterator_data: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct JSObject {
    pub map: Rc<RefCell<Map>>,
    pub properties: Vec<JSValue>,
    pub elements: Vec<JSValue>,
    pub has_accessors: bool,
    /// Lazily-allocated exotic slots — None for plain objects, saving 336 bytes
    /// of heap allocation per object. Only allocated when dictionary_properties,
    /// map/set data, regexp, typed array, data view, proxy, or array buffer
    /// features are needed.
    pub ext: Option<Box<ExoticSlots>>,
}

impl JSObject {
    /// Returns a mutable reference to the exotic slots, allocating if needed.
    #[inline(always)]
    pub fn ext_mut(&mut self) -> &mut ExoticSlots {
        self.ext.get_or_insert_with(|| Box::new(ExoticSlots::default()))
    }

    /// Returns a reference to the exotic slots. If ext is None (plain object),
    /// returns a static empty ExoticSlots reference (all fields default/empty/None).
    #[inline(always)]
    pub fn ext_or_default(&self) -> &ExoticSlots {
        match self.ext.as_deref() {
            Some(ext) => ext,
            None => {
                thread_local! {
                    static DEFAULT: &'static ExoticSlots = Box::leak(Box::new(ExoticSlots::default()));
                }
                DEFAULT.with(|d| *d)
            }
        }
    }

    /// Returns a reference to the exotic slots, or None for plain objects.
    #[inline(always)]
    pub fn ext_ref(&self) -> Option<&ExoticSlots> {
        self.ext.as_deref()
    }
}

thread_local! {
    static ROOT_OBJECT_MAP: Rc<RefCell<Map>> = Map::root(InstanceType::JSObject, None);
    static RECYCLED_OBJECTS: RefCell<Vec<Rc<RefCell<JSObject>>>> = RefCell::new(Vec::with_capacity(32));
}

/// Recycles a dead JSObject if it has no other strong or weak references.
#[inline(always)]
pub fn recycle_dead_object(val: JSValue) {
    if let JSValue::Object(rc) = val {
        if Rc::strong_count(&rc) == 1 && Rc::weak_count(&rc) == 0 {
            RECYCLED_OBJECTS.with(|pool| {
                let mut p = pool.borrow_mut();
                if p.len() < 32 {
                    p.push(rc);
                }
            });
        }
    }
}

impl JSObject {
    /// Returns the root object map (empty shape, no prototype).
    pub fn root_object_map() -> Rc<RefCell<Map>> {
        ROOT_OBJECT_MAP.with(|m| m.clone())
    }

    /// Creates a new empty JSObject with an initial empty Map and an optional prototype.
    pub fn new_empty(prototype: Option<Rc<RefCell<JSObject>>>) -> Rc<RefCell<Self>> {
        let map = if prototype.is_none() {
            ROOT_OBJECT_MAP.with(|m| m.clone())
        } else {
            Map::root(InstanceType::JSObject, prototype)
        };
        if let Some(reused) = RECYCLED_OBJECTS.with(|p| p.borrow_mut().pop()) {
            {
                let mut obj = reused.borrow_mut();
                obj.map = map;
                obj.properties.clear();
                obj.elements.clear();
                obj.has_accessors = false;
                obj.ext = None;
            }
            return reused;
        }
        Rc::new(RefCell::new(Self {
            map,
            properties: Vec::new(),
            elements: Vec::new(),
            has_accessors: false,
            ext: None,
        }))
    }

    /// Creates a new empty JSObject with pre-allocated properties capacity.
    /// Used for object literals where the property count is known at compile time.
    pub fn new_empty_with_capacity(capacity: usize, prototype: Option<Rc<RefCell<JSObject>>>) -> Rc<RefCell<Self>> {
        let map = if prototype.is_none() {
            ROOT_OBJECT_MAP.with(|m| m.clone())
        } else {
            Map::root(InstanceType::JSObject, prototype)
        };
        if let Some(reused) = RECYCLED_OBJECTS.with(|p| p.borrow_mut().pop()) {
            {
                let mut obj = reused.borrow_mut();
                obj.map = map;
                obj.properties.clear();
                let cur_cap = obj.properties.capacity();
                if cur_cap < capacity {
                    obj.properties.reserve(capacity - cur_cap);
                }
                obj.elements.clear();
                obj.has_accessors = false;
                obj.ext = None;
            }
            return reused;
        }
        Rc::new(RefCell::new(Self {
            map,
            properties: Vec::with_capacity(capacity),
            elements: Vec::new(),
            has_accessors: false,
            ext: None,
        }))
    }

    /// Creates a new JSObject configured with an existing Map.
    pub fn new_with_map(map: Rc<RefCell<Map>>) -> Rc<RefCell<Self>> {
        let size = map.borrow().instance_size;
        if let Some(reused) = RECYCLED_OBJECTS.with(|p| p.borrow_mut().pop()) {
            {
                let mut obj = reused.borrow_mut();
                obj.map = map;
                obj.properties.clear();
                obj.properties.resize(size, JSValue::Undefined);
                obj.elements.clear();
                obj.has_accessors = false;
                obj.ext = None;
            }
            return reused;
        }

        Rc::new(RefCell::new(Self {
            map,
            properties: vec![JSValue::Undefined; size],
            elements: Vec::new(),
            has_accessors: false,
            ext: None,
        }))
    }

    /// Creates a new ArrayBuffer JSObject of the specified byte length.
    pub fn new_array_buffer(byte_length: usize, prototype: Option<Rc<RefCell<JSObject>>>) -> Rc<RefCell<Self>> {
        let map = Map::root(InstanceType::JSArrayBuffer, prototype);
        Rc::new(RefCell::new(Self {
            map,
            properties: Vec::new(),
            elements: Vec::new(),
            has_accessors: false,
            ext: Some(Box::new(ExoticSlots {
                array_buffer_data: Some(Rc::new(RefCell::new(vec![0u8; byte_length]))),
                ..ExoticSlots::default()
            })),
        }))
    }

    /// Creates a new ArrayBuffer JSObject wrapping existing raw bytes.
    pub fn new_array_buffer_from_bytes(bytes: Vec<u8>, prototype: Option<Rc<RefCell<JSObject>>>) -> Rc<RefCell<Self>> {
        let map = Map::root(InstanceType::JSArrayBuffer, prototype);
        Rc::new(RefCell::new(Self {
            map,
            properties: Vec::new(),
            elements: Vec::new(),
            has_accessors: false,
            ext: Some(Box::new(ExoticSlots {
                array_buffer_data: Some(Rc::new(RefCell::new(bytes))),
                ..ExoticSlots::default()
            })),
        }))
    }

    /// Creates a new SharedArrayBuffer JSObject of the specified byte length.
    pub fn new_shared_array_buffer(byte_length: usize, prototype: Option<Rc<RefCell<JSObject>>>) -> Rc<RefCell<Self>> {
        let map = Map::root(InstanceType::JSSharedArrayBuffer, prototype);
        let shared = Arc::new(RwLock::new(vec![0u8; byte_length]));
        Rc::new(RefCell::new(Self {
            map,
            properties: Vec::new(),
            elements: Vec::new(),
            has_accessors: false,
            ext: Some(Box::new(ExoticSlots {
                shared_array_buffer_data: Some(shared),
                ..ExoticSlots::default()
            })),
        }))
    }

    /// Creates a new TypedArray JSObject (e.g. Uint8Array, Int32Array).
    pub fn new_typed_array(
        kind: TypedArrayKind,
        buffer: Rc<RefCell<JSObject>>,
        byte_offset: usize,
        length: usize,
        prototype: Option<Rc<RefCell<JSObject>>>,
    ) -> Rc<RefCell<Self>> {
        let map = Map::root(InstanceType::JSTypedArray, prototype);
        Rc::new(RefCell::new(Self {
            map,
            properties: Vec::new(),
            elements: Vec::new(),
            has_accessors: false,
            ext: Some(Box::new(ExoticSlots {
                typed_array_data: Some(TypedArrayData {
                    kind,
                    buffer,
                    byte_offset,
                    length,
                }),
                ..ExoticSlots::default()
            })),
        }))
    }

    /// Creates a new DataView JSObject.
    pub fn new_data_view(
        buffer: Rc<RefCell<JSObject>>,
        byte_offset: usize,
        byte_length: usize,
        prototype: Option<Rc<RefCell<JSObject>>>,
    ) -> Rc<RefCell<Self>> {
        let map = Map::root(InstanceType::JSDataView, prototype);
        Rc::new(RefCell::new(Self {
            map,
            properties: Vec::new(),
            elements: Vec::new(),
            has_accessors: false,
            ext: Some(Box::new(ExoticSlots {
                data_view_data: Some(DataViewData {
                    buffer,
                    byte_offset,
                    byte_length,
                }),
                ..ExoticSlots::default()
            })),
        }))
    }

    /// Creates a new Proxy JSObject wrapping target and handler.
    pub fn new_proxy(target: JSValue, handler: JSValue) -> Rc<RefCell<Self>> {
        let map = Map::root(InstanceType::JSProxy, None);
        Rc::new(RefCell::new(Self {
            map,
            properties: Vec::new(),
            elements: Vec::new(),
            has_accessors: false,
            ext: Some(Box::new(ExoticSlots {
                proxy_data: Some(ProxyData::new(target, handler)),
                ..ExoticSlots::default()
            })),
        }))
    }

    /// Creates a new Generator JSObject wrapping suspended bytecode execution state.
    pub fn new_generator(
        bytecode_array: Rc<crate::interpreter::bytecode_array::BytecodeArray>,
        receiver: JSValue,
        arguments: Vec<JSValue>,
        prototype: Option<Rc<RefCell<Self>>>,
        is_async: bool,
    ) -> Rc<RefCell<Self>> {
        let map = Map::root(InstanceType::JSObject, prototype);
        Rc::new(RefCell::new(Self {
            map,
            properties: Vec::new(),
            elements: Vec::new(),
            has_accessors: false,
            ext: Some(Box::new(ExoticSlots {
                generator_data: Some(Rc::new(RefCell::new(
                    crate::objects::generator::GeneratorData::new(
                        bytecode_array,
                        receiver,
                        arguments,
                        is_async,
                    ),
                ))),
                ..ExoticSlots::default()
            })),
        }))
    }

    /// Returns true if this object or any object in its prototype chain has getter/setter accessors.
    #[inline(always)]
    pub fn has_accessor_in_chain(&self) -> bool {
        if self.has_accessors {
            return true;
        }
        let map = self.map.borrow();
        if map.prototype.is_none() {
            return false;
        }
        let mut curr = map.prototype.clone();
        drop(map);
        while let Some(proto) = curr {
            let p_borrow = proto.borrow();
            if p_borrow.has_accessors {
                return true;
            }
            curr = p_borrow.map.borrow().prototype.clone();
        }
        false
    }

    /// Retrieves a property value by name, searching fast slots/dictionary,
    /// special built-in properties (e.g. array length), and traversing the prototype chain.
    pub fn get_property(&self, name: &str) -> JSValue {
        let map = self.map.borrow();
        if map.instance_type == InstanceType::JSObject && self.ext_or_default().proxy_data.is_none() {
            if map.is_dictionary_map {
                if let Some((val, _)) = self.ext_or_default().dictionary_properties.get(name) {
                    return val.clone();
                }
            } else if let Some(idx) = map.find_field_index(name) {
                if let Some(val) = self.properties.get(idx) {
                    return val.clone();
                }
            }
            if let Some(proto) = &map.prototype {
                return proto.borrow().get_property(name);
            }
            return JSValue::Undefined;
        }

        // Special case: Proxy get trap
        if let Some(ref proxy) = self.ext_or_default().proxy_data {
            let handler_obj = match &proxy.handler {
                JSValue::Object(o) => Some(o.clone()),
                _ => None,
            };
            if let Some(h) = handler_obj {
                let trap = h.borrow().get_property("get");
                if let JSValue::Function(f) = trap {
                    let receiver = JSValue::Object(Rc::new(RefCell::new(self.clone())));
                    if let Ok(res) = f.call(&proxy.handler, &[proxy.target.clone(), JSValue::String(name.to_string()), receiver]) {
                        return res;
                    }
                }
            }
            match &proxy.target {
                JSValue::Object(o) => return o.borrow().get_property(name),
                _ => return JSValue::Undefined,
            }
        }

        // Special case: Array "length"
        if name == "length" && map.instance_type == InstanceType::JSArray {
            return JSValue::Smi(self.elements.len() as i32);
        }

        // Special case: TypedArray properties and indexed access
        if let Some(ref ta) = self.ext_or_default().typed_array_data {
            if name == "length" {
                return JSValue::Smi(ta.length as i32);
            }
            if name == "byteLength" {
                return JSValue::Smi(ta.byte_length() as i32);
            }
            if name == "byteOffset" {
                return JSValue::Smi(ta.byte_offset as i32);
            }
            if name == "buffer" {
                return JSValue::Object(ta.buffer.clone());
            }
            if let Ok(idx) = name.parse::<usize>() {
                if idx < ta.length {
                    let buf_obj = ta.buffer.borrow();
                    if let Some(ref buf_bytes) = buf_obj.ext_or_default().array_buffer_data {
                        return ta.kind.read_element(&buf_bytes.borrow(), ta.byte_offset, idx);
                    } else if let Some(ref sab_bytes) = buf_obj.ext_or_default().shared_array_buffer_data {
                        return ta.kind.read_element(&sab_bytes.read().unwrap(), ta.byte_offset, idx);
                    }
                }
                return JSValue::Undefined;
            }
        }

        // Special case: ArrayBuffer "byteLength"
        if map.instance_type == InstanceType::JSArrayBuffer && name == "byteLength" {
            if let Some(ref b) = self.ext_or_default().array_buffer_data {
                return JSValue::Smi(b.borrow().len() as i32);
            }
            return JSValue::Smi(0);
        }

        // Special case: SharedArrayBuffer "byteLength"
        if map.instance_type == InstanceType::JSSharedArrayBuffer && name == "byteLength" {
            if let Some(ref b) = self.ext_or_default().shared_array_buffer_data {
                return JSValue::Smi(b.read().unwrap().len() as i32);
            }
            return JSValue::Smi(0);
        }

        // Special case: DataView properties
        if let Some(ref dv) = self.ext_or_default().data_view_data {
            if name == "byteLength" {
                return JSValue::Smi(dv.byte_length as i32);
            }
            if name == "byteOffset" {
                return JSValue::Smi(dv.byte_offset as i32);
            }
            if name == "buffer" {
                return JSValue::Object(dv.buffer.clone());
            }
        }

        // Special case: RegExp "lastIndex"
        if name == "lastIndex" {
            if let Some(ref re_data) = self.ext_or_default().regexp_data {
                return JSValue::Smi(re_data.last_index as i32);
            }
        }

        // Fast mode vs Dictionary mode
        if map.is_dictionary_map {
            if let Some((val, _)) = self.ext_or_default().dictionary_properties.get(name) {
                return val.clone();
            }
        } else if let Some(idx) = map.find_field_index(name) {
            if let Some(val) = self.properties.get(idx) {
                return val.clone();
            }
        }

        // Prototype chain lookup
        if let Some(proto) = &map.prototype {
            return proto.borrow().get_property(name);
        }

        JSValue::Undefined
    }

    /// Sets a named property on an object.
    pub fn set_property(self_rc: &Rc<RefCell<JSObject>>, name: &str, value: JSValue) {
        let mut obj = self_rc.borrow_mut();
        if obj.ext_or_default().proxy_data.is_none() {
            let (is_jsobject, is_dict, existing_index) = {
                let map = obj.map.borrow();
                (
                    map.instance_type == InstanceType::JSObject,
                    map.is_dictionary_map,
                    map.find_field_index(name),
                )
            };
            if is_jsobject {
                if name.starts_with("__get_") || name.starts_with("__set_") {
                    obj.has_accessors = true;
                }
                if is_dict {
                    obj.ext_mut().dictionary_properties.insert(
                        name.to_string(),
                        (value, PropertyDetails::default()),
                    );
                    return;
                }
                if let Some(idx) = existing_index {
                    if idx < obj.properties.len() {
                        obj.properties[idx] = value;
                    } else {
                        obj.properties.resize(idx + 1, JSValue::Undefined);
                        obj.properties[idx] = value;
                    }
                    return;
                } else {
                    let map_rc = obj.map.clone();
                    let next_map = Map::transition_to_property(&map_rc, name, PropertyDetails::default());
                    obj.map = next_map;
                    obj.properties.push(value);
                    return;
                }
            }
        }
        drop(obj);

        // Special case: Proxy set trap
        if self_rc.borrow().map.borrow().instance_type == InstanceType::JSProxy {
            let proxy_opt = self_rc.borrow().ext_or_default().proxy_data.clone();
            if let Some(proxy) = proxy_opt {
                let handler_obj = match &proxy.handler {
                    JSValue::Object(o) => Some(o.clone()),
                    _ => None,
                };
                if let Some(h) = handler_obj {
                    let trap = h.borrow().get_property("set");
                    if let JSValue::Function(f) = trap {
                        let receiver = JSValue::Object(self_rc.clone());
                        let _ = f.call(&proxy.handler, &[proxy.target.clone(), JSValue::String(name.to_string()), value, receiver]);
                        return;
                    }
                }
                if let JSValue::Object(target_rc) = &proxy.target {
                    JSObject::set_property(target_rc, name, value);
                    return;
                }
            }
        }

        // Special case: Array "length"
        if name == "length" && self_rc.borrow().map.borrow().instance_type == InstanceType::JSArray {
            let new_len = value.to_number() as usize;
            self_rc.borrow_mut().elements.resize(new_len, JSValue::Undefined);
            return;
        }

        // Special case: TypedArray indexed property store
        if self_rc.borrow().map.borrow().instance_type == InstanceType::JSTypedArray {
            let ta_opt = self_rc.borrow().ext_or_default().typed_array_data.clone();
            if let Some(ta) = ta_opt {
                if let Ok(idx) = name.parse::<usize>() {
                    if idx < ta.length {
                        let buf_rc = ta.buffer.clone();
                        let buf_obj = buf_rc.borrow();
                        if let Some(ref buf_bytes) = buf_obj.ext_or_default().array_buffer_data {
                            ta.kind.write_element(&mut buf_bytes.borrow_mut(), ta.byte_offset, idx, &value);
                            return;
                        } else if let Some(ref sab_bytes) = buf_obj.ext_or_default().shared_array_buffer_data {
                            ta.kind.write_element(&mut sab_bytes.write().unwrap(), ta.byte_offset, idx, &value);
                            return;
                        }
                    }
                }
            }
        }

        // Special case: RegExp "lastIndex"
        if name == "lastIndex" {
            if let Some(ref mut re_data) = self_rc.borrow_mut().ext_mut().regexp_data {
                re_data.last_index = value.to_number().max(0.0) as usize;
            }
        }

        if name.starts_with("__get_") || name.starts_with("__set_") {
            self_rc.borrow_mut().has_accessors = true;
        }

        let is_dict = self_rc.borrow().map.borrow().is_dictionary_map;
        if is_dict {
            self_rc.borrow_mut().ext_mut().dictionary_properties.insert(
                name.to_string(),
                (value, PropertyDetails::default()),
            );
            return;
        }

        // Fast mode
        let existing_index = self_rc.borrow().map.borrow().find_field_index(name);
        if let Some(idx) = existing_index {
            let mut obj = self_rc.borrow_mut();
            if idx < obj.properties.len() {
                obj.properties[idx] = value;
            } else {
                obj.properties.resize(idx + 1, JSValue::Undefined);
                obj.properties[idx] = value;
            }
        } else {
            // Transition Map to include new property
            let current_map = self_rc.borrow().map.clone();
            let next_map = Map::transition_to_property(&current_map, name, PropertyDetails::default());

            let mut obj = self_rc.borrow_mut();
            obj.map = next_map;
            obj.properties.push(value);
        }
    }

    /// Deletes an indexed element from the object.
    pub fn delete_element(&mut self, index: usize) -> bool {
        if index < self.elements.len() {
            self.elements[index] = JSValue::Undefined;
        }
        true
    }

    /// Retrieves an indexed element (e.g. `obj[0]`).
    pub fn get_element(&self, index: usize) -> JSValue {
        if let Some(ref ta) = self.ext_or_default().typed_array_data {
            if index < ta.length {
                let buf_obj = ta.buffer.borrow();
                if let Some(ref buf_bytes) = buf_obj.ext_or_default().array_buffer_data {
                    return ta.kind.read_element(&buf_bytes.borrow(), ta.byte_offset, index);
                } else if let Some(ref sab_bytes) = buf_obj.ext_or_default().shared_array_buffer_data {
                    return ta.kind.read_element(&sab_bytes.read().unwrap(), ta.byte_offset, index);
                }
            }
            return JSValue::Undefined;
        }
        self.elements.get(index).cloned().unwrap_or(JSValue::Undefined)
    }

    /// Sets an indexed element (e.g. `obj[0] = val`), automatically growing `elements`.
    pub fn set_element(&mut self, index: usize, value: JSValue) {
        if let Some(ref ta) = self.ext_or_default().typed_array_data {
            if index < ta.length {
                let buf_obj = ta.buffer.borrow();
                if let Some(ref buf_bytes) = buf_obj.ext_or_default().array_buffer_data {
                    ta.kind.write_element(&mut buf_bytes.borrow_mut(), ta.byte_offset, index, &value);
                    return;
                } else if let Some(ref sab_bytes) = buf_obj.ext_or_default().shared_array_buffer_data {
                    ta.kind.write_element(&mut sab_bytes.write().unwrap(), ta.byte_offset, index, &value);
                    return;
                }
            }
            return;
        }
        if index >= self.elements.len() {
            self.elements.resize(index + 1, JSValue::Undefined);
        }
        self.elements[index] = value;
    }

    /// Checks if the object has the specified property (own or prototype).
    pub fn has_property(&self, name: &str) -> bool {
        if let Some(ref proxy) = self.ext_or_default().proxy_data {
            let handler_obj = match &proxy.handler {
                JSValue::Object(o) => Some(o.clone()),
                _ => None,
            };
            if let Some(h) = handler_obj {
                let trap = h.borrow().get_property("has");
                if let JSValue::Function(f) = trap {
                    if let Ok(res) = f.call(&proxy.handler, &[proxy.target.clone(), JSValue::String(name.to_string())]) {
                        return res.to_boolean();
                    }
                }
            }
            if let JSValue::Object(ref target_rc) = proxy.target {
                return target_rc.borrow().has_property(name);
            }
        }
        if self.has_own_property(name) {
            return true;
        }
        if let Some(proto) = &self.map.borrow().prototype {
            return proto.borrow().has_property(name);
        }
        false
    }

    /// Checks if the object has the specified property on itself (excluding prototype chain).
    pub fn has_own_property(&self, name: &str) -> bool {
        if name == "length" && self.map.borrow().instance_type == InstanceType::JSArray {
            return true;
        }
        if self.ext_or_default().typed_array_data.is_some() {
            if name == "length" || name == "byteLength" || name == "byteOffset" || name == "buffer" {
                return true;
            }
            if let Ok(idx) = name.parse::<usize>() {
                if let Some(ref ta) = self.ext_or_default().typed_array_data {
                    return idx < ta.length;
                }
            }
        }
        if self.map.borrow().instance_type == InstanceType::JSArrayBuffer && name == "byteLength" {
            return true;
        }
        if self.ext_or_default().data_view_data.is_some() && (name == "byteLength" || name == "byteOffset" || name == "buffer") {
            return true;
        }
        if self.map.borrow().is_dictionary_map {
            self.ext_or_default().dictionary_properties.contains_key(name)
        } else {
            self.map.borrow().find_field_index(name).is_some()
        }
    }

    /// Deletes a property from this object, transitioning to dictionary mode if in fast mode.
    pub fn delete_property(&mut self, name: &str) -> bool {
        if let Some(ref proxy) = self.ext_or_default().proxy_data {
            let handler_obj = match &proxy.handler {
                JSValue::Object(o) => Some(o.clone()),
                _ => None,
            };
            if let Some(h) = handler_obj {
                let trap = h.borrow().get_property("deleteProperty");
                if let JSValue::Function(f) = trap {
                    if let Ok(res) = f.call(&proxy.handler, &[proxy.target.clone(), JSValue::String(name.to_string())]) {
                        return res.to_boolean();
                    }
                }
            }
            if let JSValue::Object(ref target_rc) = proxy.target {
                return target_rc.borrow_mut().delete_property(name);
            }
        }
        if !self.map.borrow().is_dictionary_map {
            // Transition to dictionary mode
            let dict_map = self.map.borrow().to_dictionary_mode();
            let mut dict = HashMap::new();
            for desc in &self.map.borrow().descriptors {
                if let Some(val) = self.properties.get(desc.field_index) {
                    dict.insert(desc.name.clone(), (val.clone(), desc.details));
                }
            }
            self.map = dict_map;
            self.ext_mut().dictionary_properties = dict;
            self.properties.clear();
        }

        self.ext_mut().dictionary_properties.remove(name);
        true
    }
}
