//! Safe Rust reimplementation of the W3C High Resolution Time Level 2 `performance` API.
//!
//! Provides `performance.now()` with sub-millisecond precision and `performance.timeOrigin`.

use crate::objects::{JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Global performance timing state.
pub struct PerformanceState {
    pub start_instant: Instant,
    pub time_origin: f64,
}

thread_local! {
    static PERF_STATE: PerformanceState = {
        let now = SystemTime::now();
        let ms = now.duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0);
        PerformanceState {
            start_instant: Instant::now(),
            time_origin: ms,
        }
    };
}

/// Returns high-resolution milliseconds elapsed since context startup.
pub fn performance_now() -> f64 {
    PERF_STATE.with(|state| {
        let elapsed = state.start_instant.elapsed();
        elapsed.as_secs_f64() * 1000.0
    })
}

/// Returns context creation time in epoch milliseconds.
pub fn performance_time_origin() -> f64 {
    PERF_STATE.with(|state| state.time_origin)
}

/// Creates the global `performance` object.
pub fn create_performance_object() -> Rc<RefCell<JSObject>> {
    let perf = JSObject::new_empty(None);

    // performance.now()
    let now_fn = JSFunction::new_native("now", |_this, _args| {
        Ok(JSValue::Number(performance_now()))
    });
    JSObject::set_property(&perf, "now", JSValue::Function(now_fn));

    // performance.timeOrigin
    JSObject::set_property(&perf, "timeOrigin", JSValue::Number(performance_time_origin()));

    perf
}
