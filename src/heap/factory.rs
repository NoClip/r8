//! Safe Rust reimplementation of Google V8's `Factory`.
//!
//! Provides high-level object construction routines that allocate into
//! the managed heap and return rooted handles.

use super::handle::Handle;
use super::heap::Heap;
use super::heap_object::HeapPayload;
use crate::objects::function::{FunctionKind, JSFunction};
use crate::objects::js_array::JSArray;
use crate::objects::js_object::JSObject;
use crate::objects::map::{InstanceType, Map};
use crate::objects::value::JSValue;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Factory;

impl Factory {
    /// Allocates an empty JSObject in the nursery heap, returning a rooted Handle.
    pub fn new_object(
        heap: &mut Heap,
        prototype: Option<Rc<RefCell<JSObject>>>,
    ) -> Handle<JSObject> {
        let obj_rc = JSObject::new_empty(prototype);
        let obj = obj_rc.borrow().clone();
        let size = 64; // approximate base object size
        let id = heap.allocate(HeapPayload::Object(obj), size);
        heap.create_handle(id)
    }

    /// Allocates a JSArray in the nursery heap, returning a rooted Handle.
    pub fn new_array(heap: &mut Heap, elements: Vec<JSValue>) -> Handle<JSObject> {
        let arr_rc = JSArray::new_array(elements);
        let arr = arr_rc.borrow().clone();
        let size = 64 + (arr.elements.len() * 16);
        let id = heap.allocate(HeapPayload::Array(arr), size);
        heap.create_handle(id)
    }

    /// Allocates a string on the heap, returning a rooted Handle.
    pub fn new_string(heap: &mut Heap, s: &str) -> Handle<String> {
        let size = s.len() + 16;
        let id = heap.allocate(HeapPayload::String(s.to_string()), size);
        heap.create_handle(id)
    }

    /// Allocates a hidden class Map on the heap, returning a rooted Handle.
    pub fn new_map(heap: &mut Heap, instance_type: InstanceType) -> Handle<Map> {
        let map_rc = Map::root(instance_type, None);
        let map = map_rc.borrow().clone();
        let size = 48;
        let id = heap.allocate(HeapPayload::Map(map), size);
        heap.create_handle(id)
    }

    /// Allocates a callable JSFunction on the heap, returning a rooted Handle.
    pub fn new_function(heap: &mut Heap, name: &str, kind: FunctionKind) -> Handle<JSFunction> {
        let func = match kind {
            FunctionKind::Native(cb) => (*JSFunction::new_native(name, cb)).clone(),
            FunctionKind::Closure(cb) => JSFunction {
                name: name.to_string(),
                bytecode: None,
                is_jit: std::cell::Cell::new(false),
                kind: std::cell::RefCell::new(FunctionKind::Closure(cb)),
                invocation_count: std::cell::Cell::new(0),
            },
            FunctionKind::Bytecode(bc) => (*JSFunction::new_bytecode(name, bc)).clone(),
            FunctionKind::BaselineJit { bytecode, executable } => JSFunction {
                name: name.to_string(),
                bytecode: Some(bytecode.clone()),
                is_jit: std::cell::Cell::new(true),
                kind: std::cell::RefCell::new(FunctionKind::BaselineJit { bytecode, executable }),
                invocation_count: std::cell::Cell::new(0),
            },
            FunctionKind::JitCompiled { bytecode, executable } => (*JSFunction::new_jit_compiled(name, bytecode, executable)).clone(),
        };
        let size = 56;
        let id = heap.allocate(HeapPayload::Function(func), size);
        heap.create_handle(id)
    }
}
