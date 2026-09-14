//! Safe Rust reimplementation of Google V8's ECMAScript Proxy (`JSProxy`).
//!
//! Encapsulates target and handler objects with support for standard trap interception:
//! get, set, has, deleteProperty, apply, and construct.

use super::value::JSValue;

#[derive(Clone, Debug)]
pub struct ProxyData {
    pub target: JSValue,
    pub handler: JSValue,
}

impl ProxyData {
    pub fn new(target: JSValue, handler: JSValue) -> Self {
        Self { target, handler }
    }
}
