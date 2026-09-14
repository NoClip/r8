//! Safe Rust reimplementation of Google V8's `src/base/logging.h`.
//!
//! Provides fatal crash handlers, assertion reporting hooks, and thread-safe
//! diagnostics integration matching V8 logging semantics.

use std::sync::RwLock;

/// Safe Rust log handler callback type.
pub type LogHandler = fn(file: &str, line: i32, message: &str);

static FATAL_HANDLER: RwLock<Option<LogHandler>> = RwLock::new(None);
static DCHECK_HANDLER: RwLock<Option<LogHandler>> = RwLock::new(None);

/// Registers a custom fatal handler function.
pub fn set_fatal_handler(handler: Option<LogHandler>) {
    if let Ok(mut guard) = FATAL_HANDLER.write() {
        *guard = handler;
    }
}

/// Registers a custom debug check handler function.
pub fn set_dcheck_handler(handler: Option<LogHandler>) {
    if let Ok(mut guard) = DCHECK_HANDLER.write() {
        *guard = handler;
    }
}

/// Dispatches a fatal error, invoking the custom handler if registered, or printing to stderr.
pub fn fatal(file: &str, line: i32, message: &str) {
    if let Ok(guard) = FATAL_HANDLER.read() {
        if let Some(handler) = *guard {
            handler(file, line, message);
            return;
        }
    }
    eprintln!("\n#\n# Fatal error in {}, line {}\n# {}\n#\n", file, line, message);
}

/// Dispatches a debug check failure.
pub fn dcheck(file: &str, line: i32, message: &str) {
    if let Ok(guard) = DCHECK_HANDLER.read() {
        if let Some(handler) = *guard {
            handler(file, line, message);
            return;
        }
    }
    eprintln!("\n#\n# Debug check failed in {}, line {}: {}\n#\n", file, line, message);
}
