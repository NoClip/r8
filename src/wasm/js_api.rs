//! JavaScript `WebAssembly` global API.
//!
//! Implements `WebAssembly.compile`, `WebAssembly.instantiate`,
//! `WebAssembly.Module`, and `WebAssembly.Instance` as callable
//! built-in objects in the JS engine runtime.

use super::binary_parser::{self, WasmModule};
use super::instance::{WasmInstance, build_exports_object};
use crate::objects::function::JSFunction;
use crate::objects::js_array::JSArray;
use crate::objects::js_object::JSObject;
use crate::objects::JSValue;
use std::cell::RefCell;
use std::rc::Rc;

/// Build the `WebAssembly` global namespace object.
pub fn create_webassembly_object() -> Rc<RefCell<JSObject>> {
    let obj = JSObject::new_empty(None);

    // WebAssembly.validate(bytes) → boolean
    JSObject::set_property(&obj, "validate", JSValue::Function(JSFunction::new_native(
        "validate",
        |_this, args| {
            let bytes = js_args_to_bytes(args)?;
            Ok(JSValue::Boolean(binary_parser::parse(&bytes).is_ok()))
        },
    )));

    // WebAssembly.compile(bytes) → { module } (synchronous in our engine)
    JSObject::set_property(&obj, "compile", JSValue::Function(JSFunction::new_native(
        "compile",
        |_this, args| {
            let bytes = js_args_to_bytes(args)?;
            let module = binary_parser::parse(&bytes)
                .map_err(|e| format!("WebAssembly.compile: {}", e))?;
            Ok(wrap_module(module))
        },
    )));

    // WebAssembly.instantiate(bytes_or_module) → { instance }
    JSObject::set_property(&obj, "instantiate", JSValue::Function(JSFunction::new_closure(
        "instantiate",
        |_this, args| {
            let bytes = js_args_to_bytes(args)?;
            let module = binary_parser::parse(&bytes)
                .map_err(|e| format!("WebAssembly.instantiate (bytes len={}): {}", bytes.len(), e))?;

            let instance = WasmInstance::instantiate(&module, vec![]);
            let instance_rc = Rc::new(RefCell::new(instance));
            let exports_rc = build_exports_object(&module, instance_rc.clone());

            // Build { instance: { exports: { ... } } }
            let instance_js = JSObject::new_empty(None);
            JSObject::set_property(&instance_js, "exports", JSValue::Object(exports_rc));

            let result = JSObject::new_empty(None);
            JSObject::set_property(&result, "instance", JSValue::Object(instance_js));

            Ok(JSValue::Object(result))
        },
    )));

    // WebAssembly.Module constructor stub
    let module_ctor = JSObject::new_empty(None);
    JSObject::set_property(&module_ctor, "name", JSValue::String("Module".to_string()));
    JSObject::set_property(&obj, "Module", JSValue::Object(module_ctor));

    // WebAssembly.Instance constructor stub
    let instance_ctor = JSObject::new_empty(None);
    JSObject::set_property(&instance_ctor, "name", JSValue::String("Instance".to_string()));
    JSObject::set_property(&obj, "Instance", JSValue::Object(instance_ctor));

    // WebAssembly.Tag constructor
    let tag_ctor = JSFunction::new_native("Tag", |_this, args| {
        let tag_obj = JSObject::new_empty(None);
        JSObject::set_property(&tag_obj, "__type__", JSValue::String("WebAssembly.Tag".to_string()));
        let mut param_types = Vec::new();
        if let Some(JSValue::Object(type_desc)) = args.first() {
            let td = type_desc.borrow();
            if let JSValue::Array(params) = td.get_property("parameters") {
                let p = params.borrow();
                for i in 0..p.elements.len() {
                    if let JSValue::String(s) = p.get_element(i) {
                        param_types.push(s);
                    }
                }
            }
        }
        let pt_clone = param_types.clone();
        JSObject::set_property(&tag_obj, "type", JSValue::Function(JSFunction::new_closure("type", move |_this, _args| {
            let res = JSObject::new_empty(None);
            let param_vals: Vec<JSValue> = pt_clone.iter().map(|s| JSValue::String(s.clone())).collect();
            let arr = JSArray::new_array(param_vals);
            JSObject::set_property(&res, "parameters", JSValue::Array(arr));
            Ok(JSValue::Object(res))
        })));
        static NEXT_TAG_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        let tag_id = NEXT_TAG_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        JSObject::set_property(&tag_obj, "__tag_id__", JSValue::Smi(tag_id as i32));
        Ok(JSValue::Object(tag_obj))
    });
    JSObject::set_property(&obj, "Tag", JSValue::Function(tag_ctor));

    // WebAssembly.Exception constructor
    let exception_ctor = JSFunction::new_closure("Exception", |_this, args| {
        let exc_obj = JSObject::new_empty(None);
        JSObject::set_property(&exc_obj, "__type__", JSValue::String("WebAssembly.Exception".to_string()));
        let tag_id = if let Some(JSValue::Object(t)) = args.first() {
            match t.borrow().get_property("__tag_id__") {
                JSValue::Smi(id) => id,
                _ => 0,
            }
        } else {
            0
        };
        JSObject::set_property(&exc_obj, "__tag_id__", JSValue::Smi(tag_id));

        let mut payload_vec = Vec::new();
        if let Some(JSValue::Array(arr)) = args.get(1) {
            let a = arr.borrow();
            for i in 0..a.elements.len() {
                payload_vec.push(a.get_element(i));
            }
        }
        let payload_arr = JSArray::new_array(payload_vec);
        JSObject::set_property(&exc_obj, "__payload__", JSValue::Array(payload_arr));

        // .is(tag)
        JSObject::set_property(&exc_obj, "is", JSValue::Function(JSFunction::new_native("is", move |this, args| {
            let this_tag = if let JSValue::Object(o) = this {
                match o.borrow().get_property("__tag_id__") {
                    JSValue::Smi(id) => id,
                    _ => -1,
                }
            } else {
                -1
            };
            let arg_tag = if let Some(JSValue::Object(t)) = args.first() {
                match t.borrow().get_property("__tag_id__") {
                    JSValue::Smi(id) => id,
                    _ => -2,
                }
            } else {
                -2
            };
            Ok(JSValue::Boolean(this_tag == arg_tag && this_tag > 0))
        })));

        // .getArg(tag, index)
        JSObject::set_property(&exc_obj, "getArg", JSValue::Function(JSFunction::new_native("getArg", move |this, args| {
            let (this_tag, payload) = if let JSValue::Object(o) = this {
                let ob = o.borrow();
                let tid = match ob.get_property("__tag_id__") {
                    JSValue::Smi(id) => id,
                    _ => -1,
                };
                let pl = ob.get_property("__payload__");
                (tid, pl)
            } else {
                (-1, JSValue::Undefined)
            };
            let arg_tag = if let Some(JSValue::Object(t)) = args.first() {
                match t.borrow().get_property("__tag_id__") {
                    JSValue::Smi(id) => id,
                    _ => -2,
                }
            } else {
                -2
            };
            if this_tag != arg_tag || this_tag <= 0 {
                return Err("TypeError: tag does not match".to_string());
            }
            let idx = match args.get(1) {
                Some(JSValue::Smi(n)) => *n as usize,
                Some(JSValue::Number(f)) => *f as usize,
                _ => 0,
            };
            if let JSValue::Array(arr) = payload {
                let ab = arr.borrow();
                if idx < ab.elements.len() {
                    Ok(ab.get_element(idx))
                } else {
                    Err("RangeError: index out of range".to_string())
                }
            } else {
                Ok(JSValue::Undefined)
            }
        })));

        Ok(JSValue::Object(exc_obj))
    });
    JSObject::set_property(&obj, "Exception", JSValue::Function(exception_ctor));

    // WebAssembly.Memory constructor
    let memory_ctor = JSFunction::new_native("Memory", |_this, args| {
        let mem_obj = JSObject::new_empty(None);
        let mut initial = 1u64;
        let mut _maximum = None;
        let mut is_memory64 = false;
        let mut is_shared = false;
        if let Some(JSValue::Object(desc)) = args.first() {
            let d = desc.borrow();
            if let JSValue::Smi(n) = d.get_property("initial") {
                initial = n.max(0) as u64;
            }
            if let JSValue::Smi(n) = d.get_property("maximum") {
                _maximum = Some(n.max(0) as u64);
            }
            if let JSValue::String(s) = d.get_property("address") {
                if s == "i64" { is_memory64 = true; }
            }
            if let JSValue::String(s) = d.get_property("index") {
                if s == "i64" { is_memory64 = true; }
            }
            if let JSValue::Boolean(b) = d.get_property("shared") {
                is_shared = b;
            }
        }
        let byte_len = (initial * 65536) as usize;
        let ab = JSObject::new_array_buffer_from_bytes(vec![0u8; byte_len], None);
        JSObject::set_property(&mem_obj, "buffer", JSValue::Object(ab));
        JSObject::set_property(&mem_obj, "__type__", JSValue::String("WebAssembly.Memory".to_string()));
        JSObject::set_property(&mem_obj, "__memory64__", JSValue::Boolean(is_memory64));
        JSObject::set_property(&mem_obj, "__shared__", JSValue::Boolean(is_shared));
        Ok(JSValue::Object(mem_obj))
    });
    JSObject::set_property(&obj, "Memory", JSValue::Function(memory_ctor));

    obj
}

/// Wrap a parsed `WasmModule` into a JS object with internal `__wasm_module__` marker.
fn wrap_module(module: WasmModule) -> JSValue {
    let obj = JSObject::new_empty(None);
    JSObject::set_property(&obj, "__type__", JSValue::String("WebAssembly.Module".to_string()));
    // Store function/export count as inspectable properties
    JSObject::set_property(&obj, "__func_count__", JSValue::Smi(module.functions.len() as i32));
    JSObject::set_property(&obj, "__export_count__", JSValue::Smi(module.exports.len() as i32));
    JSValue::Object(obj)
}

/// Convert a JS argument (Array of byte integers) to `Vec<u8>`.
///
/// Accepts: `[0, 97, 115, 109, ...]` — a JS Array where each element is an integer 0–255.
fn js_args_to_bytes(args: &[JSValue]) -> Result<Vec<u8>, String> {
    let first = args.first().ok_or("WebAssembly API: expected bytes argument")?;
    match first {
        JSValue::Array(arr) => {
            let arr = arr.borrow();
            let len = arr.elements.len();
            let mut bytes = Vec::with_capacity(len);
            for i in 0..len {
                // Array elements are stored in `elements` vec; use get_element
                let byte = match arr.get_element(i) {
                    JSValue::Smi(n) => n as u8,
                    JSValue::Number(f) => f as u8,
                    _ => 0,
                };
                bytes.push(byte);
            }
            Ok(bytes)
        }
        JSValue::Object(obj) => {
            // Also accept plain Object with numeric keys and "length"
            let obj = obj.borrow();
            let len_val = obj.get_property("length");
            let len = match len_val {
                JSValue::Smi(n) => n as usize,
                JSValue::Number(f) => f as usize,
                _ => return Err("WebAssembly API: bytes argument has no length".to_string()),
            };
            let mut bytes = Vec::with_capacity(len);
            for i in 0..len {
                let byte = match obj.get_element(i) {
                    JSValue::Smi(n) => n as u8,
                    JSValue::Number(f) => f as u8,
                    _ => 0,
                };
                bytes.push(byte);
            }
            Ok(bytes)
        }
        _ => Err(format!("WebAssembly API: expected Array of bytes, got {:?}", first)),
    }
}
