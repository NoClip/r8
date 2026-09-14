//! Safe Rust reimplementation of Google V8's Multi-Isolate Web Worker architecture.
//!
//! Spawns real OS threads running independent `Context` execution isolates, communicating
//! across thread boundaries via thread-safe message queues.

use crate::objects::{JSFunction, JSObject, JSValue};
use crate::runtime::Context;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub enum WorkerCommand {
    Execute(String),
    PostMessage(String),
    Terminate,
}

pub struct WorkerHandle {
    sender: Sender<WorkerCommand>,
    receiver: Arc<Mutex<Receiver<String>>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl WorkerHandle {
    pub fn new(initial_code: String) -> Self {
        let (to_worker_tx, to_worker_rx) = channel::<WorkerCommand>();
        let (from_worker_tx, from_worker_rx) = channel::<String>();

        let thread_handle = thread::spawn(move || {
            let mut worker_ctx = Context::new();

            // Register worker's self postMessage
            let tx_clone = from_worker_tx.clone();
            let post_message_fn = JSFunction::new_closure("postMessage", move |_this, args| {
                let msg = args.first().map(|a| a.to_string_val()).unwrap_or_default();
                let _ = tx_clone.send(msg);
                Ok(JSValue::Undefined)
            });
            JSObject::set_property(&worker_ctx.global_object, "postMessage", JSValue::Function(post_message_fn));

            // Execute initial script
            if !initial_code.is_empty() {
                let _ = worker_ctx.eval(&initial_code);
            }

            // Command loop
            while let Ok(cmd) = to_worker_rx.recv() {
                match cmd {
                    WorkerCommand::Execute(code) => {
                        let _ = worker_ctx.eval(&code);
                    }
                    WorkerCommand::PostMessage(msg) => {
                        // Dispatch onmessage if registered
                        let onmessage_val = worker_ctx.global_object.borrow().get_property("onmessage");
                        if let JSValue::Function(f) = onmessage_val {
                            let arg = JSValue::String(msg);
                            let _ = f.call(&JSValue::Undefined, &[arg]);
                        }
                    }
                    WorkerCommand::Terminate => {
                        break;
                    }
                }
            }
        });

        Self {
            sender: to_worker_tx,
            receiver: Arc::new(Mutex::new(from_worker_rx)),
            thread_handle: Some(thread_handle),
        }
    }

    pub fn post_message(&self, msg: String) -> Result<(), String> {
        self.sender.send(WorkerCommand::PostMessage(msg)).map_err(|e| e.to_string())
    }

    pub fn receive_message(&self, timeout_ms: u64) -> Result<Option<String>, String> {
        let rx = self.receiver.lock().map_err(|e| e.to_string())?;
        if timeout_ms == 0 {
            match rx.try_recv() {
                Ok(msg) => Ok(Some(msg)),
                Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("Worker channel disconnected".to_string()),
            }
        } else {
            match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
                Ok(msg) => Ok(Some(msg)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err("Worker channel disconnected".to_string()),
            }
        }
    }

    pub fn terminate(&mut self) {
        let _ = self.sender.send(WorkerCommand::Terminate);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Creates the global `Worker` constructor object.
pub fn create_worker_constructor() -> Rc<RefCell<JSObject>> {
    let worker_ctor = JSFunction::new_native("Worker", |_this, args| {
        let script = args.first().map(|a| a.to_string_val()).unwrap_or_default();
        let handle = Arc::new(Mutex::new(WorkerHandle::new(script)));

        let instance = JSObject::new_empty(None);

        // postMessage(msg)
        let h1 = handle.clone();
        let post_fn = JSFunction::new_closure("postMessage", move |_this, p_args| {
            let msg = p_args.first().map(|a| a.to_string_val()).unwrap_or_default();
            let h = h1.lock().unwrap();
            h.post_message(msg).map_err(|e| e.to_string())?;
            Ok(JSValue::Undefined)
        });
        JSObject::set_property(&instance, "postMessage", JSValue::Function(post_fn));

        // receiveMessage(timeout_ms)
        let h2 = handle.clone();
        let recv_fn = JSFunction::new_closure("receiveMessage", move |_this, r_args| {
            let timeout = r_args.first().map(|a| a.to_number() as u64).unwrap_or(1000);
            let h = h2.lock().unwrap();
            match h.receive_message(timeout) {
                Ok(Some(msg)) => Ok(JSValue::String(msg)),
                Ok(None) => Ok(JSValue::Null),
                Err(e) => Err(e),
            }
        });
        JSObject::set_property(&instance, "receiveMessage", JSValue::Function(recv_fn));

        // terminate()
        let h3 = handle.clone();
        let term_fn = JSFunction::new_closure("terminate", move |_this, _t_args| {
            let mut h = h3.lock().unwrap();
            h.terminate();
            Ok(JSValue::Undefined)
        });
        JSObject::set_property(&instance, "terminate", JSValue::Function(term_fn));

        Ok(JSValue::Object(instance))
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(worker_ctor));
    ctor_obj
}
