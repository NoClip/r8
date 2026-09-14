//! Safe Rust reimplementation of Google V8's `SnapshotSerializer`.
//!
//! Traverses the runtime `Context` heap graph and encodes all live shapes (Maps),
//! prototype chains, and global properties into a binary byte buffer.

use crate::objects::{JSObject, JSValue};
use crate::runtime::Context;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const SNAPSHOT_MAGIC: &[u8; 4] = b"V8SN";
pub const SNAPSHOT_VERSION: u32 = 1;

pub struct SnapshotSerializer {
    strings: Vec<String>,
    string_map: HashMap<String, u32>,
    objects: Vec<Rc<RefCell<JSObject>>>,
    object_map: HashMap<usize, u32>,
}

impl Default for SnapshotSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotSerializer {
    pub fn new() -> Self {
        Self {
            strings: Vec::new(),
            string_map: HashMap::new(),
            objects: Vec::new(),
            object_map: HashMap::new(),
        }
    }

    fn intern_string(&mut self, s: &str) -> u32 {
        if let Some(&idx) = self.string_map.get(s) {
            idx
        } else {
            let idx = self.strings.len() as u32;
            self.strings.push(s.to_string());
            self.string_map.insert(s.to_string(), idx);
            idx
        }
    }

    fn register_object(&mut self, obj: &Rc<RefCell<JSObject>>) -> u32 {
        let ptr = Rc::as_ptr(obj) as usize;
        if let Some(&idx) = self.object_map.get(&ptr) {
            idx
        } else {
            let idx = self.objects.len() as u32;
            self.objects.push(obj.clone());
            self.object_map.insert(ptr, idx);
            idx
        }
    }

    pub fn serialize(mut self, ctx: &Context) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32768);

        // 1. Magic & Version
        buf.extend_from_slice(SNAPSHOT_MAGIC);
        buf.extend_from_slice(&SNAPSHOT_VERSION.to_le_bytes());

        // 2. Discover objects starting from Context roots
        let global_id = self.register_object(&ctx.global_object);
        let array_proto_id = self.register_object(&ctx.array_prototype);
        let string_proto_id = self.register_object(&ctx.string_prototype);
        let date_proto_id = self.register_object(&ctx.date_prototype);
        let promise_proto_id = self.register_object(&ctx.promise_prototype);
        let map_proto_id = self.register_object(&ctx.map_prototype);
        let set_proto_id = self.register_object(&ctx.set_prototype);
        let weak_map_proto_id = self.register_object(&ctx.weak_map_prototype);
        let weak_set_proto_id = self.register_object(&ctx.weak_set_prototype);
        let symbol_proto_id = self.register_object(&ctx.symbol_prototype);
        let regexp_proto_id = self.register_object(&ctx.regexp_prototype);

        // BFS to discover all reachable objects and intern strings
        let mut queue_idx = 0;
        while queue_idx < self.objects.len() {
            let obj_rc = self.objects[queue_idx].clone();
            let obj = obj_rc.borrow();

            // Properties
            for (name, idx) in &obj.map.borrow().descriptor_index {
                self.intern_string(name);
                if let Some(val) = obj.properties.get(*idx) {
                    self.inspect_value(val);
                }
            }

            // Elements
            for el in &obj.elements {
                self.inspect_value(el);
            }

            // Prototype
            if let Some(proto) = &obj.map.borrow().prototype {
                self.register_object(proto);
            }

            queue_idx += 1;
        }

        // 3. Write Root IDs
        buf.extend_from_slice(&global_id.to_le_bytes());
        buf.extend_from_slice(&array_proto_id.to_le_bytes());
        buf.extend_from_slice(&string_proto_id.to_le_bytes());
        buf.extend_from_slice(&date_proto_id.to_le_bytes());
        buf.extend_from_slice(&promise_proto_id.to_le_bytes());
        buf.extend_from_slice(&map_proto_id.to_le_bytes());
        buf.extend_from_slice(&set_proto_id.to_le_bytes());
        buf.extend_from_slice(&weak_map_proto_id.to_le_bytes());
        buf.extend_from_slice(&weak_set_proto_id.to_le_bytes());
        buf.extend_from_slice(&symbol_proto_id.to_le_bytes());
        buf.extend_from_slice(&regexp_proto_id.to_le_bytes());

        // 4. Write String Table
        buf.extend_from_slice(&(self.strings.len() as u32).to_le_bytes());
        for s in &self.strings {
            let bytes = s.as_bytes();
            buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(bytes);
        }

        // 5. Write Object Count and Table
        buf.extend_from_slice(&(self.objects.len() as u32).to_le_bytes());
        for obj_rc in &self.objects {
            let obj = obj_rc.borrow();
            let proto_id = obj.map.borrow().prototype.as_ref()
                .and_then(|p| self.object_map.get(&(Rc::as_ptr(p) as usize)).copied())
                .unwrap_or(u32::MAX);
            buf.extend_from_slice(&proto_id.to_le_bytes());

            // Write properties
            let desc_map = &obj.map.borrow().descriptor_index;
            buf.extend_from_slice(&(desc_map.len() as u32).to_le_bytes());
            for (name, &idx) in desc_map {
                let name_idx = *self.string_map.get(name).unwrap();
                buf.extend_from_slice(&name_idx.to_le_bytes());
                let val = obj.properties.get(idx).unwrap_or(&JSValue::Undefined);
                self.write_value(&mut buf, val);
            }

            // Write elements
            buf.extend_from_slice(&(obj.elements.len() as u32).to_le_bytes());
            for el in &obj.elements {
                self.write_value(&mut buf, el);
            }
        }

        buf
    }

    fn inspect_value(&mut self, val: &JSValue) {
        match val {
            JSValue::String(s) => { self.intern_string(s); }
            JSValue::BigInt(b) => { self.intern_string(&b.to_string()); }
            JSValue::Object(o) | JSValue::Array(o) => { self.register_object(o); }
            JSValue::Function(f) => { self.intern_string(&f.name); }
            JSValue::Symbol(s) => {
                if let Some(desc) = &s.description {
                    self.intern_string(desc);
                }
            }
            _ => {}
        }
    }

    fn write_value(&self, buf: &mut Vec<u8>, val: &JSValue) {
        match val {
            JSValue::Undefined => buf.push(0),
            JSValue::Null => buf.push(1),
            JSValue::Boolean(b) => {
                buf.push(2);
                buf.push(if *b { 1 } else { 0 });
            }
            JSValue::Smi(i) => {
                buf.push(3);
                buf.extend_from_slice(&i.to_le_bytes());
            }
            JSValue::Number(f) => {
                buf.push(4);
                buf.extend_from_slice(&f.to_le_bytes());
            }
            JSValue::String(s) => {
                buf.push(5);
                let idx = *self.string_map.get(s).unwrap();
                buf.extend_from_slice(&idx.to_le_bytes());
            }
            JSValue::Object(o) | JSValue::Array(o) => {
                buf.push(6);
                let id = *self.object_map.get(&(Rc::as_ptr(o) as usize)).unwrap();
                buf.extend_from_slice(&id.to_le_bytes());
            }
            JSValue::Function(f) => {
                buf.push(7);
                let name_idx = *self.string_map.get(&f.name).unwrap();
                buf.extend_from_slice(&name_idx.to_le_bytes());
            }
            JSValue::Symbol(s) => {
                buf.push(8);
                buf.extend_from_slice(&s.id.to_le_bytes());
                let desc_idx = s.description.as_ref()
                    .and_then(|d| self.string_map.get(d).copied())
                    .unwrap_or(u32::MAX);
                buf.extend_from_slice(&desc_idx.to_le_bytes());
            }
            JSValue::BigInt(b) => {
                buf.push(9);
                let s = b.to_string();
                let idx = *self.string_map.get(&s).unwrap();
                buf.extend_from_slice(&idx.to_le_bytes());
            }
        }
    }
}
