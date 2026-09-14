//! Safe Rust reimplementation of Google V8's ECMAScript `console` built-in object.
//!
//! Implements `console.log`, `console.error`, `console.warn`, `console.time`,
//! `console.timeEnd`, `console.assert`, `console.table`, `console.trace`,
//! `console.count`, and `console.countReset` with in-memory capture
//! for differential testing and assertion verification.

use crate::objects::{JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Instant;

static LOG_BUFFER: Mutex<Vec<String>> = Mutex::new(Vec::new());
static TIMERS: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
static COUNTERS: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

/// Retrieves the most recently logged message (for testing).
pub fn get_last_log() -> Option<String> {
    LOG_BUFFER.lock().unwrap().last().cloned()
}

/// Clears the recorded log buffer and counters.
pub fn clear_logs() {
    LOG_BUFFER.lock().unwrap().clear();
    if let Ok(mut timers) = TIMERS.lock() {
        if let Some(ref mut map) = *timers {
            map.clear();
        }
    }
    if let Ok(mut counters) = COUNTERS.lock() {
        if let Some(ref mut map) = *counters {
            map.clear();
        }
    }
}

pub fn create_console_object() -> Rc<RefCell<JSObject>> {
    let console = JSObject::new_empty(None);

    // console.log(...args)
    JSObject::set_property(
        &console,
        "log",
        JSValue::Function(JSFunction::new_native("log", |_this, args| {
            let parts: Vec<String> = args.iter().map(|a| a.to_string_val()).collect();
            let line = parts.join(" ");
            println!("{}", line);
            LOG_BUFFER.lock().unwrap().push(line);
            Ok(JSValue::Undefined)
        })),
    );

    // console.error(...args)
    JSObject::set_property(
        &console,
        "error",
        JSValue::Function(JSFunction::new_native("error", |_this, args| {
            let parts: Vec<String> = args.iter().map(|a| a.to_string_val()).collect();
            let line = format!("[ERROR] {}", parts.join(" "));
            eprintln!("{}", line);
            LOG_BUFFER.lock().unwrap().push(line);
            Ok(JSValue::Undefined)
        })),
    );

    // console.warn(...args)
    JSObject::set_property(
        &console,
        "warn",
        JSValue::Function(JSFunction::new_native("warn", |_this, args| {
            let parts: Vec<String> = args.iter().map(|a| a.to_string_val()).collect();
            let line = format!("[WARN] {}", parts.join(" "));
            eprintln!("{}", line);
            LOG_BUFFER.lock().unwrap().push(line);
            Ok(JSValue::Undefined)
        })),
    );

    // console.time(label?)
    JSObject::set_property(
        &console,
        "time",
        JSValue::Function(JSFunction::new_native("time", |_this, args| {
            let label = args.first().map(|v| v.to_string_val()).unwrap_or_else(|| "default".to_string());
            let mut guard = TIMERS.lock().unwrap();
            let timers = guard.get_or_insert_with(HashMap::new);
            timers.insert(label, Instant::now());
            Ok(JSValue::Undefined)
        })),
    );

    // console.timeEnd(label?)
    JSObject::set_property(
        &console,
        "timeEnd",
        JSValue::Function(JSFunction::new_native("timeEnd", |_this, args| {
            let label = args.first().map(|v| v.to_string_val()).unwrap_or_else(|| "default".to_string());
            let mut guard = TIMERS.lock().unwrap();
            if let Some(ref mut timers) = *guard {
                if let Some(start) = timers.remove(&label) {
                    let elapsed = start.elapsed();
                    let msg = format!("{}: {:.3}ms", label, elapsed.as_secs_f64() * 1000.0);
                    println!("{}", msg);
                    LOG_BUFFER.lock().unwrap().push(msg);
                    return Ok(JSValue::Undefined);
                }
            }
            let msg = format!("Timer '{}' does not exist", label);
            eprintln!("{}", msg);
            LOG_BUFFER.lock().unwrap().push(msg);
            Ok(JSValue::Undefined)
        })),
    );

    // console.assert(condition, ...data)
    JSObject::set_property(
        &console,
        "assert",
        JSValue::Function(JSFunction::new_native("assert", |_this, args| {
            let condition = args.first().map(|v| v.to_boolean()).unwrap_or(false);
            if !condition {
                let parts: Vec<String> = args.iter().skip(1).map(|a| a.to_string_val()).collect();
                let detail = if parts.is_empty() { "console.assert".to_string() } else { parts.join(" ") };
                let msg = format!("Assertion failed: {}", detail);
                eprintln!("{}", msg);
                LOG_BUFFER.lock().unwrap().push(msg);
            }
            Ok(JSValue::Undefined)
        })),
    );

    // console.table(data)
    JSObject::set_property(
        &console,
        "table",
        JSValue::Function(JSFunction::new_native("table", |_this, args| {
            let line = match args.first() {
                Some(JSValue::Array(arr)) => {
                    let elems: Vec<String> = arr.borrow().elements.iter().map(|e| e.to_string_val()).collect();
                    format!("[Table: Array({}) [{}] ]", elems.len(), elems.join(", "))
                }
                Some(JSValue::Object(obj)) => {
                    format!("[Table: Object {:?}]", obj.borrow().map.borrow().descriptors)
                }
                Some(other) => other.to_string_val(),
                None => "".to_string(),
            };
            println!("{}", line);
            LOG_BUFFER.lock().unwrap().push(line);
            Ok(JSValue::Undefined)
        })),
    );

    // console.trace(...data)
    JSObject::set_property(
        &console,
        "trace",
        JSValue::Function(JSFunction::new_native("trace", |_this, args| {
            let parts: Vec<String> = args.iter().map(|a| a.to_string_val()).collect();
            let label = if parts.is_empty() { "Trace".to_string() } else { format!("Trace: {}", parts.join(" ")) };
            let line = format!("{}\n    at console.trace (native)", label);
            println!("{}", line);
            LOG_BUFFER.lock().unwrap().push(line);
            Ok(JSValue::Undefined)
        })),
    );

    // console.count(label?)
    JSObject::set_property(
        &console,
        "count",
        JSValue::Function(JSFunction::new_native("count", |_this, args| {
            let label = args.first().map(|v| v.to_string_val()).unwrap_or_else(|| "default".to_string());
            let mut guard = COUNTERS.lock().unwrap();
            let counters = guard.get_or_insert_with(HashMap::new);
            let count = counters.entry(label.clone()).or_insert(0);
            *count += 1;
            let line = format!("{}: {}", label, count);
            println!("{}", line);
            LOG_BUFFER.lock().unwrap().push(line);
            Ok(JSValue::Undefined)
        })),
    );

    // console.countReset(label?)
    JSObject::set_property(
        &console,
        "countReset",
        JSValue::Function(JSFunction::new_native("countReset", |_this, args| {
            let label = args.first().map(|v| v.to_string_val()).unwrap_or_else(|| "default".to_string());
            let mut guard = COUNTERS.lock().unwrap();
            if let Some(ref mut counters) = *guard {
                if counters.remove(&label).is_some() {
                    return Ok(JSValue::Undefined);
                }
            }
            let msg = format!("Count for '{}' does not exist", label);
            eprintln!("{}", msg);
            LOG_BUFFER.lock().unwrap().push(msg);
            Ok(JSValue::Undefined)
        })),
    );

    console
}
