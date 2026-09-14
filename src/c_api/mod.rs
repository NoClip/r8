//! Safe Rust implementation of Google V8 C-ABI (`include/v8.h`).
//!
//! Provides the physical foreign-function interface for embedding this V8 engine
//! inside Chromium, Blink, Node.js, and other C/C++ host environments.

use crate::objects::{JSObject, JSValue};
use crate::runtime::Context;
use std::ffi::CStr;
use std::os::raw::{c_char, c_double};
use std::slice;

/// C-ABI wrapper for an execution Isolate.
#[repr(C)]
pub struct V8Isolate {
    pub context: Context,
}

/// C-ABI wrapper for an execution Context realm.
#[repr(C)]
pub struct V8Context {
    pub context: Context,
}

/// C-ABI wrapper for a compiled Script.
#[repr(C)]
pub struct V8Script {
    pub source: String,
    pub context: Context,
}

/// C-ABI wrapper for a JavaScript value handle.
#[repr(C)]
pub struct V8Value {
    pub value: JSValue,
}

// -----------------------------------------------------------------------------
// Isolate Management
// -----------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn v8_isolate_new() -> *mut V8Isolate {
    let isolate = Box::new(V8Isolate {
        context: Context::new(),
    });
    Box::into_raw(isolate)
}

#[no_mangle]
pub unsafe extern "C" fn v8_isolate_dispose(isolate: *mut V8Isolate) {
    if !isolate.is_null() {
        drop(Box::from_raw(isolate));
    }
}

// -----------------------------------------------------------------------------
// Context Management
// -----------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn v8_context_new(isolate: *mut V8Isolate) -> *mut V8Context {
    let context = if !isolate.is_null() {
        (*isolate).context.clone()
    } else {
        Context::new()
    };
    Box::into_raw(Box::new(V8Context { context }))
}

#[no_mangle]
pub unsafe extern "C" fn v8_context_dispose(ctx: *mut V8Context) {
    if !ctx.is_null() {
        drop(Box::from_raw(ctx));
    }
}

#[no_mangle]
pub unsafe extern "C" fn v8_context_global(ctx: *mut V8Context) -> *mut V8Value {
    if ctx.is_null() {
        return std::ptr::null_mut();
    }
    let global_val = JSValue::Object((*ctx).context.global_object.clone());
    Box::into_raw(Box::new(V8Value { value: global_val }))
}

// -----------------------------------------------------------------------------
// Script Compilation & Execution
// -----------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn v8_script_compile(
    ctx: *mut V8Context,
    source: *const c_char,
) -> *mut V8Script {
    if ctx.is_null() || source.is_null() {
        return std::ptr::null_mut();
    }
    let c_str = CStr::from_ptr(source);
    let src = match c_str.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return std::ptr::null_mut(),
    };
    Box::into_raw(Box::new(V8Script {
        source: src,
        context: (*ctx).context.clone(),
    }))
}

#[no_mangle]
pub unsafe extern "C" fn v8_script_run(script: *mut V8Script) -> *mut V8Value {
    if script.is_null() {
        return std::ptr::null_mut();
    }
    let res = (*script).context.eval(&(*script).source);
    let val = match res {
        Ok(v) => v,
        Err(err) => JSValue::String(format!("Uncaught {}", err)),
    };
    Box::into_raw(Box::new(V8Value { value: val }))
}

#[no_mangle]
pub unsafe extern "C" fn v8_script_dispose(script: *mut V8Script) {
    if !script.is_null() {
        drop(Box::from_raw(script));
    }
}

// -----------------------------------------------------------------------------
// Value Creation
// -----------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn v8_value_new_number(_isolate: *mut V8Isolate, value: c_double) -> *mut V8Value {
    let js_val = if value.fract() == 0.0 && value >= (i32::MIN as f64) && value <= (i32::MAX as f64) {
        JSValue::Smi(value as i32)
    } else {
        JSValue::Number(value)
    };
    Box::into_raw(Box::new(V8Value { value: js_val }))
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_new_string(
    _isolate: *mut V8Isolate,
    s: *const c_char,
) -> *mut V8Value {
    if s.is_null() {
        return Box::into_raw(Box::new(V8Value {
            value: JSValue::String(String::new()),
        }));
    }
    let c_str = CStr::from_ptr(s);
    let rust_str = c_str.to_string_lossy().into_owned();
    Box::into_raw(Box::new(V8Value {
        value: JSValue::String(rust_str),
    }))
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_new_boolean(_isolate: *mut V8Isolate, b: bool) -> *mut V8Value {
    Box::into_raw(Box::new(V8Value {
        value: JSValue::Boolean(b),
    }))
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_new_undefined(_isolate: *mut V8Isolate) -> *mut V8Value {
    Box::into_raw(Box::new(V8Value {
        value: JSValue::Undefined,
    }))
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_new_null(_isolate: *mut V8Isolate) -> *mut V8Value {
    Box::into_raw(Box::new(V8Value {
        value: JSValue::Null,
    }))
}

// -----------------------------------------------------------------------------
// Value Conversions & Inspections
// -----------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn v8_value_to_number(val: *const V8Value) -> c_double {
    if val.is_null() {
        return 0.0;
    }
    (*val).value.to_number()
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_to_boolean(val: *const V8Value) -> bool {
    if val.is_null() {
        return false;
    }
    (*val).value.to_boolean()
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_to_string(
    val: *const V8Value,
    buf: *mut c_char,
    max_len: usize,
) -> usize {
    if val.is_null() || buf.is_null() || max_len == 0 {
        return 0;
    }
    let s = (*val).value.to_string_val();
    let bytes = s.as_bytes();
    let copy_len = bytes.len().min(max_len - 1);
    let dst = slice::from_raw_parts_mut(buf as *mut u8, max_len);
    dst[..copy_len].copy_from_slice(&bytes[..copy_len]);
    dst[copy_len] = 0; // null-terminator
    copy_len
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_is_number(val: *const V8Value) -> bool {
    if val.is_null() { return false; }
    matches!((*val).value, JSValue::Smi(_) | JSValue::Number(_))
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_is_string(val: *const V8Value) -> bool {
    if val.is_null() { return false; }
    matches!((*val).value, JSValue::String(_))
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_is_boolean(val: *const V8Value) -> bool {
    if val.is_null() { return false; }
    matches!((*val).value, JSValue::Boolean(_))
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_is_undefined(val: *const V8Value) -> bool {
    if val.is_null() { return false; }
    matches!((*val).value, JSValue::Undefined)
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_is_null(val: *const V8Value) -> bool {
    if val.is_null() { return false; }
    matches!((*val).value, JSValue::Null)
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_is_object(val: *const V8Value) -> bool {
    if val.is_null() { return false; }
    matches!((*val).value, JSValue::Object(_) | JSValue::Array(_))
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_is_function(val: *const V8Value) -> bool {
    if val.is_null() { return false; }
    matches!((*val).value, JSValue::Function(_))
}

#[no_mangle]
pub unsafe extern "C" fn v8_value_dispose(val: *mut V8Value) {
    if !val.is_null() {
        drop(Box::from_raw(val));
    }
}

// -----------------------------------------------------------------------------
// Object Property Operations
// -----------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn v8_object_get(
    obj: *mut V8Value,
    prop: *const c_char,
) -> *mut V8Value {
    if obj.is_null() || prop.is_null() {
        return Box::into_raw(Box::new(V8Value { value: JSValue::Undefined }));
    }
    let prop_name = CStr::from_ptr(prop).to_string_lossy();
    let result = match &(*obj).value {
        JSValue::Object(ref o) | JSValue::Array(ref o) => o.borrow().get_property(&prop_name),
        _ => JSValue::Undefined,
    };
    Box::into_raw(Box::new(V8Value { value: result }))
}

#[no_mangle]
pub unsafe extern "C" fn v8_object_set(
    obj: *mut V8Value,
    prop: *const c_char,
    val: *mut V8Value,
) -> bool {
    if obj.is_null() || prop.is_null() || val.is_null() {
        return false;
    }
    let prop_name = CStr::from_ptr(prop).to_string_lossy();
    let val_to_set = (*val).value.clone();
    match &(*obj).value {
        JSValue::Object(ref o) | JSValue::Array(ref o) => {
            JSObject::set_property(o, &prop_name, val_to_set);
            true
        }
        _ => false,
    }
}

// -----------------------------------------------------------------------------
// Function Invocation
// -----------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn v8_function_call(
    func: *mut V8Value,
    this_val: *mut V8Value,
    argc: usize,
    argv: *const *mut V8Value,
) -> *mut V8Value {
    if func.is_null() {
        return Box::into_raw(Box::new(V8Value { value: JSValue::Undefined }));
    }
    let receiver = if !this_val.is_null() {
        &(*this_val).value
    } else {
        &JSValue::Undefined
    };

    let mut args = Vec::with_capacity(argc);
    if argc > 0 && !argv.is_null() {
        let arg_ptrs = slice::from_raw_parts(argv, argc);
        for &ptr in arg_ptrs {
            if !ptr.is_null() {
                args.push((*ptr).value.clone());
            } else {
                args.push(JSValue::Undefined);
            }
        }
    }

    let result = match &(*func).value {
        JSValue::Function(ref f) => f.call(receiver, &args).unwrap_or(JSValue::Undefined),
        _ => JSValue::Undefined,
    };

    Box::into_raw(Box::new(V8Value { value: result }))
}

// -----------------------------------------------------------------------------
// Engine Version Metadata
// -----------------------------------------------------------------------------

static V8_VERSION_STRING: &[u8] = b"12.4.254.20-rust\0";

#[no_mangle]
pub unsafe extern "C" fn v8_version() -> *const c_char {
    V8_VERSION_STRING.as_ptr() as *const c_char
}
