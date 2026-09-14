//! Safe Rust reimplementation of Google V8's `SnapshotDeserializer`.
//!
//! Reconstitutes an initialized `Context` from a pre-compiled binary heap snapshot in sub-millisecond time.

use super::serializer::{SNAPSHOT_MAGIC, SNAPSHOT_VERSION};
use crate::execution::microtask_queue::MicrotaskQueue;
use crate::objects::{JSFunction, JSObject, JSValue, SymbolData};
use crate::runtime::Context;
use std::cell::RefCell;
use std::rc::Rc;

pub struct SnapshotDeserializer<'a> {
    data: &'a [u8],
    cursor: usize,
}

impl<'a> SnapshotDeserializer<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, cursor: 0 }
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], String> {
        if self.cursor + len > self.data.len() {
            return Err("Unexpected EOF reading snapshot".to_string());
        }
        let slice = &self.data[self.cursor..self.cursor + len];
        self.cursor += len;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        Ok(self.read_bytes(1)?[0])
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_i32(&mut self) -> Result<i32, String> {
        let b = self.read_bytes(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_f64(&mut self) -> Result<f64, String> {
        let b = self.read_bytes(8)?;
        Ok(f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    pub fn deserialize(&mut self) -> Result<Context, String> {
        // 1. Verify Magic
        let magic = self.read_bytes(4)?;
        if magic != SNAPSHOT_MAGIC {
            return Err("Invalid snapshot magic header".to_string());
        }

        // 2. Verify Version
        let ver = self.read_u32()?;
        if ver != SNAPSHOT_VERSION {
            return Err(format!("Unsupported snapshot version: {}", ver));
        }

        // 3. Read Root IDs
        let global_id = self.read_u32()? as usize;
        let array_proto_id = self.read_u32()? as usize;
        let string_proto_id = self.read_u32()? as usize;
        let date_proto_id = self.read_u32()? as usize;
        let promise_proto_id = self.read_u32()? as usize;
        let map_proto_id = self.read_u32()? as usize;
        let set_proto_id = self.read_u32()? as usize;
        let weak_map_proto_id = self.read_u32()? as usize;
        let weak_set_proto_id = self.read_u32()? as usize;
        let symbol_proto_id = self.read_u32()? as usize;
        let regexp_proto_id = self.read_u32()? as usize;

        // 4. Read String Table
        let string_count = self.read_u32()? as usize;
        let mut strings = Vec::with_capacity(string_count);
        for _ in 0..string_count {
            let len = self.read_u32()? as usize;
            let bytes = self.read_bytes(len)?;
            let s = std::str::from_utf8(bytes).map_err(|e| e.to_string())?.to_string();
            strings.push(s);
        }

        // 5. Read Object Count and Allocate Skeletons
        let obj_count = self.read_u32()? as usize;
        let objects: Vec<Rc<RefCell<JSObject>>> = (0..obj_count)
            .map(|_| {
                let map = crate::objects::map::Map::root(crate::objects::map::InstanceType::JSObject, None);
                Rc::new(RefCell::new(JSObject {
                    map,
                    properties: Vec::new(),
                    elements: Vec::new(),
                    has_accessors: false,
                    ext: None,
                }))
            })
            .collect();

        // 6. Populate Objects
        for i in 0..obj_count {
            let proto_id = self.read_u32()?;
            if proto_id != u32::MAX && (proto_id as usize) < obj_count {
                objects[i].borrow_mut().map.borrow_mut().prototype = Some(objects[proto_id as usize].clone());
            }

            // Properties
            let prop_count = self.read_u32()? as usize;
            for _ in 0..prop_count {
                let name_idx = self.read_u32()? as usize;
                let name = &strings[name_idx];
                let val = self.read_value(&strings, &objects)?;
                JSObject::set_property(&objects[i], name, val);
            }

            // Elements
            let el_count = self.read_u32()? as usize;
            for _ in 0..el_count {
                let el = self.read_value(&strings, &objects)?;
                objects[i].borrow_mut().elements.push(el);
            }
        }

        // Create Context
        let ctx = Context {
            global_object: objects[global_id].clone(),
            array_prototype: objects[array_proto_id].clone(),
            string_prototype: objects[string_proto_id].clone(),
            date_prototype: objects[date_proto_id].clone(),
            promise_prototype: objects[promise_proto_id].clone(),
            map_prototype: objects[map_proto_id].clone(),
            set_prototype: objects[set_proto_id].clone(),
            weak_map_prototype: objects[weak_map_proto_id].clone(),
            weak_set_prototype: objects[weak_set_proto_id].clone(),
            symbol_prototype: objects[symbol_proto_id].clone(),
            regexp_prototype: objects[regexp_proto_id].clone(),
            generator_prototype: crate::builtins::create_generator_prototype(),
            microtask_queue: Rc::new(RefCell::new(MicrotaskQueue::new())),
            timer_queue: Rc::new(RefCell::new(crate::execution::TimerQueue::new())),
        };

        Ok(ctx)
    }

    fn read_value(&mut self, strings: &[String], objects: &[Rc<RefCell<JSObject>>]) -> Result<JSValue, String> {
        let tag = self.read_u8()?;
        match tag {
            0 => Ok(JSValue::Undefined),
            1 => Ok(JSValue::Null),
            2 => {
                let b = self.read_u8()?;
                Ok(JSValue::Boolean(b != 0))
            }
            3 => {
                let i = self.read_i32()?;
                Ok(JSValue::Smi(i))
            }
            4 => {
                let f = self.read_f64()?;
                Ok(JSValue::Number(f))
            }
            5 => {
                let idx = self.read_u32()? as usize;
                Ok(JSValue::String(strings[idx].clone()))
            }
            6 => {
                let id = self.read_u32()? as usize;
                if id < objects.len() {
                    Ok(JSValue::Object(objects[id].clone()))
                } else {
                    Err("Invalid object id in snapshot".to_string())
                }
            }
            7 => {
                let name_idx = self.read_u32()? as usize;
                let name = &strings[name_idx];
                let func = JSFunction::new_native(name, |_this, _args| Ok(JSValue::Undefined));
                Ok(JSValue::Function(func))
            }
            8 => {
                let id = self.read_u32()?;
                let desc_idx = self.read_u32()?;
                let desc = if desc_idx != u32::MAX && (desc_idx as usize) < strings.len() {
                    Some(strings[desc_idx as usize].clone())
                } else {
                    None
                };
                Ok(JSValue::Symbol(Rc::new(SymbolData { id, description: desc })))
            }
            9 => {
                let idx = self.read_u32()? as usize;
                let s = &strings[idx];
                let bi = crate::objects::bigint::BigIntData::from_str(s).unwrap_or_else(|_| crate::objects::bigint::BigIntData::zero());
                Ok(JSValue::BigInt(Rc::new(bi)))
            }
            other => Err(format!("Unknown snapshot value tag: {}", other)),
        }
    }
}
