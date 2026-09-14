//! Chrome DevTools Protocol (CDP) JSON-RPC types and serialization.
//!
//! Safe Rust data structures representing JSON-RPC 2.0 requests, responses, and events.

use crate::builtins::json::{parse_json, stringify_json};
use crate::objects::js_object::JSObject;
use crate::objects::value::JSValue;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct CdpRequest {
    pub id: u64,
    pub method: String,
    pub params: Option<Rc<RefCell<JSObject>>>,
}

impl CdpRequest {
    pub fn parse(json_str: &str) -> Result<Self, String> {
        let val = parse_json(json_str)?;
        match val {
            JSValue::Object(obj) => {
                let id = match obj.borrow().get_property("id") {
                    JSValue::Smi(n) => n.max(0) as u64,
                    JSValue::Number(f) => f.max(0.0) as u64,
                    _ => 0,
                };
                let method = match obj.borrow().get_property("method") {
                    JSValue::String(s) => s,
                    _ => return Err("Missing or invalid method in CDP request".to_string()),
                };
                let params = match obj.borrow().get_property("params") {
                    JSValue::Object(p) => Some(p),
                    _ => None,
                };
                Ok(Self { id, method, params })
            }
            _ => Err("CDP request must be a JSON object".to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CdpResponse {
    pub id: u64,
    pub result: Option<JSValue>,
    pub error: Option<String>,
}

impl CdpResponse {
    pub fn ok(id: u64, result: JSValue) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, code: i32, message: &str) -> Self {
        let err_obj = JSObject::new_empty(None);
        JSObject::set_property(&err_obj, "code", JSValue::Smi(code));
        JSObject::set_property(&err_obj, "message", JSValue::String(message.to_string()));
        Self {
            id,
            result: None,
            error: Some(message.to_string()),
        }
    }

    pub fn to_json(&self) -> String {
        let res_obj = JSObject::new_empty(None);
        JSObject::set_property(&res_obj, "id", JSValue::Smi(self.id as i32));

        if let Some(ref res) = self.result {
            JSObject::set_property(&res_obj, "result", res.clone());
        } else if let Some(ref err) = self.error {
            let err_obj = JSObject::new_empty(None);
            JSObject::set_property(&err_obj, "code", JSValue::Smi(-32600));
            JSObject::set_property(&err_obj, "message", JSValue::String(err.clone()));
            JSObject::set_property(&res_obj, "error", JSValue::Object(err_obj));
        } else {
            JSObject::set_property(&res_obj, "result", JSValue::Object(JSObject::new_empty(None)));
        }

        stringify_json(&JSValue::Object(res_obj)).unwrap_or_else(|_| "{}".to_string())
    }
}
