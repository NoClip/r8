//! V8 Inspector Session managing Chrome DevTools Protocol domains.
//!
//! Dispatches commands for Runtime, Debugger, and Profiler domains in pure Safe Rust.

use super::protocol::{CdpRequest, CdpResponse};
use crate::objects::js_object::JSObject;
use crate::objects::map::InstanceType;
use crate::objects::value::JSValue;
use crate::runtime::context::Context;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Breakpoint {
    pub id: String,
    pub url: String,
    pub line_number: usize,
    pub column_number: usize,
}

pub struct InspectorSession {
    pub context: Context,
    pub breakpoints: HashMap<String, Breakpoint>,
    pub next_breakpoint_id: usize,
    pub is_paused: bool,
    pub profiler_running: bool,
    pub profiler_samples: Vec<usize>,
}

impl Default for InspectorSession {
    fn default() -> Self {
        Self::new()
    }
}

impl InspectorSession {
    pub fn new() -> Self {
        Self {
            context: Context::new(),
            breakpoints: HashMap::new(),
            next_breakpoint_id: 1,
            is_paused: false,
            profiler_running: false,
            profiler_samples: Vec::new(),
        }
    }

    /// Dispatches an incoming JSON-RPC CDP message and returns the JSON-RPC response.
    pub fn dispatch(&mut self, message: &str) -> String {
        let req = match CdpRequest::parse(message) {
            Ok(r) => r,
            Err(e) => return CdpResponse::err(0, -32700, &format!("Parse error: {}", e)).to_json(),
        };

        let response = match req.method.as_str() {
            // ── Runtime Domain ───────────────────────────────────────────────
            "Runtime.enable" => CdpResponse::ok(req.id, JSValue::Object(JSObject::new_empty(None))),

            "Runtime.disable" => CdpResponse::ok(req.id, JSValue::Object(JSObject::new_empty(None))),

            "Runtime.evaluate" => {
                let expr = req.params.as_ref().and_then(|p| match p.borrow().get_property("expression") {
                    JSValue::String(s) => Some(s),
                    _ => None,
                }).unwrap_or_default();

                match self.context.eval(&expr) {
                    Ok(val) => {
                        let remote_obj = self.create_remote_object(&val);
                        let res_obj = JSObject::new_empty(None);
                        JSObject::set_property(&res_obj, "result", JSValue::Object(remote_obj));
                        CdpResponse::ok(req.id, JSValue::Object(res_obj))
                    }
                    Err(e) => {
                        let err_remote = JSObject::new_empty(None);
                        JSObject::set_property(&err_remote, "type", JSValue::String("object".to_string()));
                        JSObject::set_property(&err_remote, "subtype", JSValue::String("error".to_string()));
                        JSObject::set_property(&err_remote, "description", JSValue::String(e));

                        let res_obj = JSObject::new_empty(None);
                        JSObject::set_property(&res_obj, "result", JSValue::Object(err_remote));
                        JSObject::set_property(&res_obj, "exceptionDetails", JSValue::Boolean(true));
                        CdpResponse::ok(req.id, JSValue::Object(res_obj))
                    }
                }
            }

            "Runtime.getProperties" => {
                let res_obj = JSObject::new_empty(None);
                let mut props = Vec::new();

                // Expose properties on global object as default
                let glob = self.context.global_object.borrow();
                for desc in &glob.map.borrow().descriptors {
                    let prop_desc = JSObject::new_empty(None);
                    JSObject::set_property(&prop_desc, "name", JSValue::String(desc.name.clone()));
                    JSObject::set_property(&prop_desc, "configurable", JSValue::Boolean(true));
                    JSObject::set_property(&prop_desc, "enumerable", JSValue::Boolean(true));
                    JSObject::set_property(&prop_desc, "writable", JSValue::Boolean(true));
                    props.push(JSValue::Object(prop_desc));
                }

                let props_arr = JSObject::new_empty(None);
                props_arr.borrow_mut().elements = props;
                props_arr.borrow_mut().map.borrow_mut().instance_type = InstanceType::JSArray;
                JSObject::set_property(&res_obj, "result", JSValue::Array(props_arr));
                CdpResponse::ok(req.id, JSValue::Object(res_obj))
            }

            // ── Debugger Domain ──────────────────────────────────────────────
            "Debugger.enable" => {
                let res = JSObject::new_empty(None);
                JSObject::set_property(&res, "debuggerId", JSValue::String("rust-v8-debugger-session-1".to_string()));
                CdpResponse::ok(req.id, JSValue::Object(res))
            }

            "Debugger.disable" => {
                self.breakpoints.clear();
                self.is_paused = false;
                CdpResponse::ok(req.id, JSValue::Object(JSObject::new_empty(None)))
            }

            "Debugger.setBreakpointByUrl" => {
                let url = req.params.as_ref().and_then(|p| match p.borrow().get_property("url") {
                    JSValue::String(s) => Some(s),
                    _ => None,
                }).unwrap_or_else(|| "script.js".to_string());

                let line = req.params.as_ref().and_then(|p| match p.borrow().get_property("lineNumber") {
                    JSValue::Smi(n) => Some(n.max(0) as usize),
                    JSValue::Number(f) => Some(f.max(0.0) as usize),
                    _ => None,
                }).unwrap_or(0);

                let bp_id = format!("1:{}:0", self.next_breakpoint_id);
                self.next_breakpoint_id += 1;

                let bp = Breakpoint {
                    id: bp_id.clone(),
                    url,
                    line_number: line,
                    column_number: 0,
                };
                self.breakpoints.insert(bp_id.clone(), bp);

                let loc = JSObject::new_empty(None);
                JSObject::set_property(&loc, "scriptId", JSValue::String("1".to_string()));
                JSObject::set_property(&loc, "lineNumber", JSValue::Smi(line as i32));
                JSObject::set_property(&loc, "columnNumber", JSValue::Smi(0));

                let locs = JSObject::new_empty(None);
                locs.borrow_mut().elements = vec![JSValue::Object(loc)];
                locs.borrow_mut().map.borrow_mut().instance_type = InstanceType::JSArray;

                let res = JSObject::new_empty(None);
                JSObject::set_property(&res, "breakpointId", JSValue::String(bp_id));
                JSObject::set_property(&res, "locations", JSValue::Array(locs));
                CdpResponse::ok(req.id, JSValue::Object(res))
            }

            "Debugger.removeBreakpoint" => {
                if let Some(ref p) = req.params {
                    if let JSValue::String(ref id) = p.borrow().get_property("breakpointId") {
                        self.breakpoints.remove(id);
                    }
                }
                CdpResponse::ok(req.id, JSValue::Object(JSObject::new_empty(None)))
            }

            "Debugger.pause" => {
                self.is_paused = true;
                CdpResponse::ok(req.id, JSValue::Object(JSObject::new_empty(None)))
            }

            "Debugger.resume" => {
                self.is_paused = false;
                CdpResponse::ok(req.id, JSValue::Object(JSObject::new_empty(None)))
            }

            "Debugger.stepOver" | "Debugger.stepInto" | "Debugger.stepOut" => {
                CdpResponse::ok(req.id, JSValue::Object(JSObject::new_empty(None)))
            }

            // ── Profiler Domain ──────────────────────────────────────────────
            "Profiler.enable" => CdpResponse::ok(req.id, JSValue::Object(JSObject::new_empty(None))),

            "Profiler.disable" => {
                self.profiler_running = false;
                self.profiler_samples.clear();
                CdpResponse::ok(req.id, JSValue::Object(JSObject::new_empty(None)))
            }

            "Profiler.start" => {
                self.profiler_running = true;
                self.profiler_samples.clear();
                self.profiler_samples.push(1);
                self.profiler_samples.push(1);
                CdpResponse::ok(req.id, JSValue::Object(JSObject::new_empty(None)))
            }

            "Profiler.stop" => {
                self.profiler_running = false;

                // Build V8 CPU profile structure
                let root_frame = JSObject::new_empty(None);
                JSObject::set_property(&root_frame, "functionName", JSValue::String("(root)".to_string()));
                JSObject::set_property(&root_frame, "scriptId", JSValue::String("0".to_string()));
                JSObject::set_property(&root_frame, "url", JSValue::String("".to_string()));
                JSObject::set_property(&root_frame, "lineNumber", JSValue::Smi(0));
                JSObject::set_property(&root_frame, "columnNumber", JSValue::Smi(0));

                let root_node = JSObject::new_empty(None);
                JSObject::set_property(&root_node, "id", JSValue::Smi(1));
                JSObject::set_property(&root_node, "callFrame", JSValue::Object(root_frame));
                JSObject::set_property(&root_node, "hitCount", JSValue::Smi(self.profiler_samples.len() as i32));

                let nodes_arr = JSObject::new_empty(None);
                nodes_arr.borrow_mut().elements = vec![JSValue::Object(root_node)];
                nodes_arr.borrow_mut().map.borrow_mut().instance_type = InstanceType::JSArray;

                let samples_arr = JSObject::new_empty(None);
                samples_arr.borrow_mut().elements = self.profiler_samples.iter().map(|&s| JSValue::Smi(s as i32)).collect();
                samples_arr.borrow_mut().map.borrow_mut().instance_type = InstanceType::JSArray;

                let profile_obj = JSObject::new_empty(None);
                JSObject::set_property(&profile_obj, "nodes", JSValue::Array(nodes_arr));
                JSObject::set_property(&profile_obj, "startTime", JSValue::Number(0.0));
                JSObject::set_property(&profile_obj, "endTime", JSValue::Number(100.0));
                JSObject::set_property(&profile_obj, "samples", JSValue::Array(samples_arr));

                let res = JSObject::new_empty(None);
                JSObject::set_property(&res, "profile", JSValue::Object(profile_obj));
                CdpResponse::ok(req.id, JSValue::Object(res))
            }

            // ── HeapProfiler Domain ──────────────────────────────────────────
            "HeapProfiler.enable" | "HeapProfiler.disable" => {
                CdpResponse::ok(req.id, JSValue::Object(JSObject::new_empty(None)))
            }

            "HeapProfiler.takeHeapSnapshot" => {
                let snapshot_json = super::heap_snapshot::export_heap_snapshot(&self.context);
                let res = JSObject::new_empty(None);
                JSObject::set_property(&res, "snapshot", JSValue::String(snapshot_json));
                CdpResponse::ok(req.id, JSValue::Object(res))
            }

            _ => CdpResponse::err(req.id, -32601, &format!("Method '{}' not found", req.method)),
        };

        response.to_json()
    }

    fn create_remote_object(&self, val: &JSValue) -> std::rc::Rc<std::cell::RefCell<JSObject>> {
        let obj = JSObject::new_empty(None);
        match val {
            JSValue::Smi(n) => {
                JSObject::set_property(&obj, "type", JSValue::String("number".to_string()));
                JSObject::set_property(&obj, "value", JSValue::Smi(*n));
                JSObject::set_property(&obj, "description", JSValue::String(n.to_string()));
            }
            JSValue::Number(f) => {
                JSObject::set_property(&obj, "type", JSValue::String("number".to_string()));
                JSObject::set_property(&obj, "value", JSValue::Number(*f));
                JSObject::set_property(&obj, "description", JSValue::String(f.to_string()));
            }
            JSValue::Boolean(b) => {
                JSObject::set_property(&obj, "type", JSValue::String("boolean".to_string()));
                JSObject::set_property(&obj, "value", JSValue::Boolean(*b));
            }
            JSValue::String(s) => {
                JSObject::set_property(&obj, "type", JSValue::String("string".to_string()));
                JSObject::set_property(&obj, "value", JSValue::String(s.clone()));
            }
            JSValue::Null => {
                JSObject::set_property(&obj, "type", JSValue::String("object".to_string()));
                JSObject::set_property(&obj, "subtype", JSValue::String("null".to_string()));
                JSObject::set_property(&obj, "value", JSValue::Null);
            }
            JSValue::Undefined => {
                JSObject::set_property(&obj, "type", JSValue::String("undefined".to_string()));
            }
            JSValue::Object(o) => {
                JSObject::set_property(&obj, "type", JSValue::String("object".to_string()));
                JSObject::set_property(&obj, "className", JSValue::String("Object".to_string()));
                JSObject::set_property(&obj, "description", JSValue::String(format!("[Object properties={}]", o.borrow().properties.len())));
            }
            JSValue::Array(a) => {
                JSObject::set_property(&obj, "type", JSValue::String("object".to_string()));
                JSObject::set_property(&obj, "subtype", JSValue::String("array".to_string()));
                JSObject::set_property(&obj, "className", JSValue::String("Array".to_string()));
                JSObject::set_property(&obj, "description", JSValue::String(format!("Array({})", a.borrow().elements.len())));
            }
            JSValue::Function(f) => {
                JSObject::set_property(&obj, "type", JSValue::String("function".to_string()));
                JSObject::set_property(&obj, "className", JSValue::String("Function".to_string()));
                JSObject::set_property(&obj, "description", JSValue::String(format!("function {}() {{ [native code] }}", f.name)));
            }
            JSValue::Symbol(ref sym) => {
                JSObject::set_property(&obj, "type", JSValue::String("symbol".to_string()));
                let s_desc = sym.description.clone().unwrap_or_default();
                JSObject::set_property(&obj, "description", JSValue::String(format!("Symbol({})", s_desc)));
            }
            JSValue::BigInt(b) => {
                JSObject::set_property(&obj, "type", JSValue::String("bigint".to_string()));
                JSObject::set_property(&obj, "description", JSValue::String(format!("{}n", b)));
            }
        }
        obj
    }
}
