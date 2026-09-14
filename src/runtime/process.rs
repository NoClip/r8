//! Safe Rust reimplementation of Host Platform & Process bindings (`process`).
//!
//! Exposes environment variables, CLI arguments, high-resolution timers, process metadata,
//! and standard platform properties for Node.js and D8 compatibility.

use crate::objects::{JSArray, JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

thread_local! {
    static PROCESS_START_TIME: Instant = Instant::now();
}

fn get_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        "linux" => "linux",
        "freebsd" => "freebsd",
        "openbsd" => "openbsd",
        other => other,
    }
}

fn get_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        "arm" => "arm",
        other => other,
    }
}

/// Creates the global `process` host object.
pub fn create_process_object() -> Rc<RefCell<JSObject>> {
    let proc = JSObject::new_empty(None);

    // process.platform
    JSObject::set_property(&proc, "platform", JSValue::String(get_platform().to_string()));

    // process.arch
    JSObject::set_property(&proc, "arch", JSValue::String(get_arch().to_string()));

    // process.version
    JSObject::set_property(&proc, "version", JSValue::String("v22.0.0".to_string()));

    // process.title
    JSObject::set_property(&proc, "title", JSValue::String("d8".to_string()));

    // process.pid
    JSObject::set_property(&proc, "pid", JSValue::Smi(std::process::id() as i32));

    // process.versions
    let versions = JSObject::new_empty(None);
    JSObject::set_property(&versions, "v8", JSValue::String("12.8.0".to_string()));
    JSObject::set_property(&versions, "node", JSValue::String("22.0.0".to_string()));
    JSObject::set_property(&versions, "uv", JSValue::String("1.48.0".to_string()));
    JSObject::set_property(&versions, "rust", JSValue::String("1.85.0".to_string()));
    JSObject::set_property(&versions, "unicode", JSValue::String("16.0.0".to_string()));
    JSObject::set_property(&proc, "versions", JSValue::Object(versions));

    // process.argv
    let args: Vec<JSValue> = std::env::args().map(JSValue::String).collect();
    let argv_arr = JSArray::new_array(args);
    JSObject::set_property(&proc, "argv", JSValue::Array(argv_arr));

    // process.env
    let env_obj = JSObject::new_empty(None);
    for (k, v) in std::env::vars() {
        JSObject::set_property(&env_obj, &k, JSValue::String(v));
    }
    JSObject::set_property(&proc, "env", JSValue::Object(env_obj));

    // process.cwd()
    let cwd_fn = JSFunction::new_native("cwd", |_this, _args| {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string());
        Ok(JSValue::String(cwd))
    });
    JSObject::set_property(&proc, "cwd", JSValue::Function(cwd_fn));

    // process.exit(code?)
    let exit_fn = JSFunction::new_native("exit", |_this, args| {
        let code = args.first().map(|v| v.to_number() as i32).unwrap_or(0);
        std::process::exit(code);
    });
    JSObject::set_property(&proc, "exit", JSValue::Function(exit_fn));

    // process.uptime()
    let uptime_fn = JSFunction::new_native("uptime", |_this, _args| {
        let elapsed = PROCESS_START_TIME.with(|t| t.elapsed().as_secs_f64());
        Ok(JSValue::Number(elapsed))
    });
    JSObject::set_property(&proc, "uptime", JSValue::Function(uptime_fn));

    // process.hrtime(prevTime?) -> [seconds, nanoseconds]
    let hrtime_fn = JSFunction::new_native("hrtime", |_this, args| {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| (d.as_secs(), d.subsec_nanos()))
            .unwrap_or((0, 0));

        if let Some(JSValue::Array(prev_arr)) = args.first() {
            let elems = prev_arr.borrow().elements.clone();
            let prev_sec = elems.first().map(|v| v.to_number() as u64).unwrap_or(0);
            let prev_nano = elems.get(1).map(|v| v.to_number() as u32).unwrap_or(0);

            let mut diff_sec = now.0.saturating_sub(prev_sec);
            let diff_nano = if now.1 >= prev_nano {
                now.1 - prev_nano
            } else {
                diff_sec = diff_sec.saturating_sub(1);
                1_000_000_000 + now.1 - prev_nano
            };

            let res = JSArray::new_array(vec![
                JSValue::Number(diff_sec as f64),
                JSValue::Number(diff_nano as f64),
            ]);
            Ok(JSValue::Array(res))
        } else {
            let res = JSArray::new_array(vec![
                JSValue::Number(now.0 as f64),
                JSValue::Number(now.1 as f64),
            ]);
            Ok(JSValue::Array(res))
        }
    });
    JSObject::set_property(&proc, "hrtime", JSValue::Function(hrtime_fn));

    // process.memoryUsage()
    let mem_fn = JSFunction::new_native("memoryUsage", |_this, _args| {
        let mem = JSObject::new_empty(None);
        JSObject::set_property(&mem, "rss", JSValue::Number(32.0 * 1024.0 * 1024.0));
        JSObject::set_property(&mem, "heapTotal", JSValue::Number(16.0 * 1024.0 * 1024.0));
        JSObject::set_property(&mem, "heapUsed", JSValue::Number(4.0 * 1024.0 * 1024.0));
        JSObject::set_property(&mem, "external", JSValue::Number(1.0 * 1024.0 * 1024.0));
        Ok(JSValue::Object(mem))
    });
    JSObject::set_property(&proc, "memoryUsage", JSValue::Function(mem_fn));

    proc
}
