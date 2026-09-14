//! Safe Rust reimplementation of WHATWG Streams Standard (`ReadableStream`, `WritableStream`, `TransformStream`).
//!
//! Provides asynchronous, chunk-based streaming primitives for web platform interoperability.

use crate::builtins::new_promise_capability;
use crate::objects::{JSArray, JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

/// Creates a new `ReadableStream` prototype.
pub fn create_readable_stream_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // ReadableStream.prototype.getReader()
    JSObject::set_property(
        &proto,
        "getReader",
        JSValue::Function(JSFunction::new_closure("getReader", |_this, _args| {
            let stream = match _this {
                JSValue::Object(o) => o.clone(),
                _ => return Err("TypeError: getReader called on non-object".to_string()),
            };

            let locked = match JSObject::get_property(&stream.borrow(), "locked") {
                JSValue::Boolean(b) => b,
                _ => false,
            };
            if locked {
                return Err("TypeError: ReadableStream is already locked".to_string());
            }

            JSObject::set_property(&stream, "locked", JSValue::Boolean(true));
            let reader = create_readable_stream_reader(stream);
            Ok(JSValue::Object(reader))
        })),
    );

    // ReadableStream.prototype.cancel(reason)
    JSObject::set_property(
        &proto,
        "cancel",
        JSValue::Function(JSFunction::new_closure("cancel", |_this, args| {
            let stream = match _this {
                JSValue::Object(o) => o.clone(),
                _ => return Err("TypeError: cancel called on non-object".to_string()),
            };
            JSObject::set_property(&stream, "__closed__", JSValue::Boolean(true));
            let (promise, resolve_fn, _) = new_promise_capability(None);
            let reason = args.first().cloned().unwrap_or(JSValue::Undefined);

            let source = JSObject::get_property(&stream.borrow(), "__source__");
            if let JSValue::Object(src) = source {
                let cancel_fn = JSObject::get_property(&src.borrow(), "cancel");
                if let JSValue::Function(f) = cancel_fn {
                    let _ = f.call(&JSValue::Object(src), &[reason]);
                }
            }

            let _ = resolve_fn.call(&JSValue::Undefined, &[JSValue::Undefined]);
            Ok(JSValue::Object(promise))
        })),
    );

    // ReadableStream.prototype.tee()
    JSObject::set_property(
        &proto,
        "tee",
        JSValue::Function(JSFunction::new_closure("tee", |_this, _args| {
            let stream = match _this {
                JSValue::Object(o) => o.clone(),
                _ => return Err("TypeError: tee called on non-object".to_string()),
            };

            let proto = stream.borrow().map.borrow().prototype.clone();
            let b1 = JSObject::new_empty(proto.clone());
            let b2 = JSObject::new_empty(proto);

            let queue_val = JSObject::get_property(&stream.borrow(), "__queue__");
            let chunks = match queue_val {
                JSValue::Array(a) => a.borrow().elements.clone(),
                _ => Vec::new(),
            };

            JSObject::set_property(&b1, "locked", JSValue::Boolean(false));
            JSObject::set_property(&b1, "__closed__", JSValue::Boolean(false));
            JSObject::set_property(&b1, "__queue__", JSValue::Array(JSArray::new_array(chunks.clone())));

            JSObject::set_property(&b2, "locked", JSValue::Boolean(false));
            JSObject::set_property(&b2, "__closed__", JSValue::Boolean(false));
            JSObject::set_property(&b2, "__queue__", JSValue::Array(JSArray::new_array(chunks)));

            let res_arr = JSArray::new_array(vec![JSValue::Object(b1), JSValue::Object(b2)]);
            Ok(JSValue::Array(res_arr))
        })),
    );

    // ReadableStream.prototype.pipeThrough({ writable, readable })
    JSObject::set_property(
        &proto,
        "pipeThrough",
        JSValue::Function(JSFunction::new_closure("pipeThrough", |_this, args| {
            let transform_obj = match args.first() {
                Some(JSValue::Object(o)) => o.clone(),
                _ => return Err("TypeError: pipeThrough requires a pair of { writable, readable }".to_string()),
            };

            let writable = JSObject::get_property(&transform_obj.borrow(), "writable");
            let readable = JSObject::get_property(&transform_obj.borrow(), "readable");

            if let JSValue::Object(ref o) = _this {
                let pipe_to_fn = o.borrow().get_property("pipeTo");
                if let JSValue::Function(f) = pipe_to_fn {
                    let _ = f.call(&_this, &[writable]);
                }
            }

            Ok(readable)
        })),
    );

    // ReadableStream.prototype.pipeTo(dest)
    JSObject::set_property(
        &proto,
        "pipeTo",
        JSValue::Function(JSFunction::new_closure("pipeTo", |_this, args| {
            let (promise, resolve_fn, _) = new_promise_capability(None);
            let dest_writable = match args.first() {
                Some(JSValue::Object(o)) => o.clone(),
                _ => return Err("TypeError: pipeTo requires a WritableStream argument".to_string()),
            };

            if let JSValue::Object(ref src) = _this {
                let q_val = JSObject::get_property(&src.borrow(), "__queue__");
                if let JSValue::Array(arr) = q_val {
                    let chunks = arr.borrow().elements.clone();
                    for chunk in chunks {
                        let sink_val = JSObject::get_property(&dest_writable.borrow(), "__sink__");
                        if let JSValue::Object(sink) = sink_val {
                            let write_fn = JSObject::get_property(&sink.borrow(), "write");
                            if let JSValue::Function(f) = write_fn {
                                let _ = f.call(&JSValue::Object(sink), &[chunk]);
                            }
                        }
                    }
                }
            }

            let _ = resolve_fn.call(&JSValue::Undefined, &[JSValue::Undefined]);
            Ok(JSValue::Object(promise))
        })),
    );

    proto
}

/// Creates a `ReadableStreamDefaultReader` instance wrapping a stream.
pub fn create_readable_stream_reader(stream: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let reader = JSObject::new_empty(None);
    JSObject::set_property(&reader, "__stream__", JSValue::Object(stream.clone()));

    let (closed_promise, closed_resolve, _) = new_promise_capability(None);
    JSObject::set_property(&reader, "closed", JSValue::Object(closed_promise));

    let st_for_read = stream.clone();
    let closed_resolve_clone = closed_resolve.clone();

    // read() -> Promise<{ value, done }>
    JSObject::set_property(
        &reader,
        "read",
        JSValue::Function(JSFunction::new_closure("read", move |_this, _args| {
            let (promise, resolve_fn, _) = new_promise_capability(None);
            let (mut chunks, is_closed, source_val) = {
                let stream_borrow = st_for_read.borrow();
                let queue_val = JSObject::get_property(&stream_borrow, "__queue__");
                let c = match queue_val {
                    JSValue::Array(ref a) => a.borrow().elements.clone(),
                    _ => Vec::new(),
                };
                let closed = match JSObject::get_property(&stream_borrow, "__closed__") {
                    JSValue::Boolean(b) => b,
                    _ => false,
                };
                let src = JSObject::get_property(&stream_borrow, "__source__");
                (c, closed, src)
            };

            if !chunks.is_empty() {
                let first = chunks.remove(0);
                JSObject::set_property(&st_for_read, "__queue__", JSValue::Array(JSArray::new_array(chunks)));

                let result_obj = JSObject::new_empty(None);
                JSObject::set_property(&result_obj, "value", first);
                JSObject::set_property(&result_obj, "done", JSValue::Boolean(false));

                let _ = resolve_fn.call(&JSValue::Undefined, &[JSValue::Object(result_obj)]);
            } else if is_closed {
                let result_obj = JSObject::new_empty(None);
                JSObject::set_property(&result_obj, "value", JSValue::Undefined);
                JSObject::set_property(&result_obj, "done", JSValue::Boolean(true));

                let _ = closed_resolve_clone.call(&JSValue::Undefined, &[JSValue::Undefined]);
                let _ = resolve_fn.call(&JSValue::Undefined, &[JSValue::Object(result_obj)]);
            } else {
                // If underlying source defines pull(), attempt to invoke it
                if let JSValue::Object(ref src) = source_val {
                    let pull_fn = JSObject::get_property(&src.borrow(), "pull");
                    let ctrl_val = JSObject::get_property(&st_for_read.borrow(), "__controller__");
                    if let JSValue::Function(f) = pull_fn {
                        let _ = f.call(&JSValue::Object(src.clone()), &[ctrl_val]);
                    }
                }

                let (mut c2, is_closed_now) = {
                    let stream_reborrow = st_for_read.borrow();
                    let q2_val = JSObject::get_property(&stream_reborrow, "__queue__");
                    let c = match q2_val {
                        JSValue::Array(ref a) => a.borrow().elements.clone(),
                        _ => Vec::new(),
                    };
                    let closed = match JSObject::get_property(&stream_reborrow, "__closed__") {
                        JSValue::Boolean(b) => b,
                        _ => false,
                    };
                    (c, closed)
                };

                if !c2.is_empty() {
                    let first = c2.remove(0);
                    JSObject::set_property(&st_for_read, "__queue__", JSValue::Array(JSArray::new_array(c2)));

                    let result_obj = JSObject::new_empty(None);
                    JSObject::set_property(&result_obj, "value", first);
                    JSObject::set_property(&result_obj, "done", JSValue::Boolean(false));

                    let _ = resolve_fn.call(&JSValue::Undefined, &[JSValue::Object(result_obj)]);
                } else {
                    let result_obj = JSObject::new_empty(None);
                    JSObject::set_property(&result_obj, "value", JSValue::Undefined);
                    JSObject::set_property(&result_obj, "done", JSValue::Boolean(is_closed_now));

                    let _ = resolve_fn.call(&JSValue::Undefined, &[JSValue::Object(result_obj)]);
                }
            }

            Ok(JSValue::Object(promise))
        })),
    );

    // releaseLock()
    let st_for_release = stream.clone();
    JSObject::set_property(
        &reader,
        "releaseLock",
        JSValue::Function(JSFunction::new_closure("releaseLock", move |_this, _args| {
            JSObject::set_property(&st_for_release, "locked", JSValue::Boolean(false));
            Ok(JSValue::Undefined)
        })),
    );

    // cancel(reason)
    let st_for_cancel = stream;
    JSObject::set_property(
        &reader,
        "cancel",
        JSValue::Function(JSFunction::new_closure("cancel", move |_this, args| {
            let cancel_fn = JSObject::get_property(&st_for_cancel.borrow(), "cancel");
            if let JSValue::Function(f) = cancel_fn {
                f.call(&JSValue::Object(st_for_cancel.clone()), args)
            } else {
                let (promise, resolve_fn, _) = new_promise_capability(None);
                let _ = resolve_fn.call(&JSValue::Undefined, &[JSValue::Undefined]);
                Ok(JSValue::Object(promise))
            }
        })),
    );

    reader
}

/// Creates a `ReadableStreamDefaultController` instance.
pub fn create_readable_stream_controller(stream: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let controller = JSObject::new_empty(None);
    let st_for_enqueue = stream.clone();

    // controller.enqueue(chunk)
    JSObject::set_property(
        &controller,
        "enqueue",
        JSValue::Function(JSFunction::new_closure("enqueue", move |_this, args| {
            let chunk = args.first().cloned().unwrap_or(JSValue::Undefined);
            let queue_val = JSObject::get_property(&st_for_enqueue.borrow(), "__queue__");
            let mut chunks = match queue_val {
                JSValue::Array(ref a) => a.borrow().elements.clone(),
                _ => Vec::new(),
            };
            chunks.push(chunk);
            JSObject::set_property(&st_for_enqueue, "__queue__", JSValue::Array(JSArray::new_array(chunks)));
            Ok(JSValue::Undefined)
        })),
    );

    // controller.close()
    let st_for_close = stream.clone();
    JSObject::set_property(
        &controller,
        "close",
        JSValue::Function(JSFunction::new_closure("close", move |_this, _args| {
            JSObject::set_property(&st_for_close, "__closed__", JSValue::Boolean(true));
            Ok(JSValue::Undefined)
        })),
    );

    // controller.error(e)
    let st_for_err = stream.clone();
    JSObject::set_property(
        &controller,
        "error",
        JSValue::Function(JSFunction::new_closure("error", move |_this, args| {
            JSObject::set_property(&st_for_err, "__errored__", JSValue::Boolean(true));
            let err = args.first().cloned().unwrap_or(JSValue::Undefined);
            JSObject::set_property(&st_for_err, "__error__", err);
            Ok(JSValue::Undefined)
        })),
    );

    // controller.desiredSize
    JSObject::set_property(&controller, "desiredSize", JSValue::Smi(1));

    controller
}

/// Creates the `ReadableStream` constructor.
pub fn create_readable_stream_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let proto_clone = prototype.clone();

    let ctor = JSFunction::new_closure("ReadableStream", move |_this, args| {
        let stream = JSObject::new_empty(Some(proto_clone.clone()));
        JSObject::set_property(&stream, "locked", JSValue::Boolean(false));
        JSObject::set_property(&stream, "__closed__", JSValue::Boolean(false));
        JSObject::set_property(&stream, "__errored__", JSValue::Boolean(false));
        JSObject::set_property(&stream, "__queue__", JSValue::Array(JSArray::new_array(Vec::new())));

        let underlying_source = args.first().cloned().unwrap_or(JSValue::Undefined);
        JSObject::set_property(&stream, "__source__", underlying_source.clone());

        let controller = create_readable_stream_controller(stream.clone());
        JSObject::set_property(&stream, "__controller__", JSValue::Object(controller.clone()));

        // Call underlyingSource.start(controller) if provided
        if let JSValue::Object(ref src) = underlying_source {
            let start_fn = JSObject::get_property(&src.borrow(), "start");
            if let JSValue::Function(f) = start_fn {
                let _ = f.call(&underlying_source, &[JSValue::Object(controller)]);
            }
        }

        Ok(JSValue::Object(stream))
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(prototype.clone()));
    JSObject::set_property(&prototype, "constructor", JSValue::Object(ctor_obj.clone()));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor));
    ctor_obj
}

/// Creates the `WritableStream` prototype.
pub fn create_writable_stream_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // WritableStream.prototype.getWriter()
    JSObject::set_property(
        &proto,
        "getWriter",
        JSValue::Function(JSFunction::new_closure("getWriter", |_this, _args| {
            let stream = match _this {
                JSValue::Object(o) => o.clone(),
                _ => return Err("TypeError: getWriter called on non-object".to_string()),
            };

            let locked = match JSObject::get_property(&stream.borrow(), "locked") {
                JSValue::Boolean(b) => b,
                _ => false,
            };
            if locked {
                return Err("TypeError: WritableStream is already locked".to_string());
            }

            JSObject::set_property(&stream, "locked", JSValue::Boolean(true));
            let writer = create_writable_stream_writer(stream);
            Ok(JSValue::Object(writer))
        })),
    );

    // WritableStream.prototype.abort(reason)
    JSObject::set_property(
        &proto,
        "abort",
        JSValue::Function(JSFunction::new_closure("abort", |_this, args| {
            let (promise, resolve_fn, _) = new_promise_capability(None);
            if let JSValue::Object(ref stream) = _this {
                JSObject::set_property(stream, "__closed__", JSValue::Boolean(true));
                let sink_val = JSObject::get_property(&stream.borrow(), "__sink__");
                if let JSValue::Object(sink) = sink_val {
                    let abort_fn = JSObject::get_property(&sink.borrow(), "abort");
                    if let JSValue::Function(f) = abort_fn {
                        let _ = f.call(&JSValue::Object(sink), args);
                    }
                }
            }
            let _ = resolve_fn.call(&JSValue::Undefined, &[JSValue::Undefined]);
            Ok(JSValue::Object(promise))
        })),
    );

    // WritableStream.prototype.close()
    JSObject::set_property(
        &proto,
        "close",
        JSValue::Function(JSFunction::new_closure("close", |_this, _args| {
            let (promise, resolve_fn, _) = new_promise_capability(None);
            if let JSValue::Object(ref stream) = _this {
                JSObject::set_property(stream, "__closed__", JSValue::Boolean(true));
                let sink_val = JSObject::get_property(&stream.borrow(), "__sink__");
                if let JSValue::Object(sink) = sink_val {
                    let close_fn = JSObject::get_property(&sink.borrow(), "close");
                    if let JSValue::Function(f) = close_fn {
                        let _ = f.call(&JSValue::Object(sink), &[]);
                    }
                }
            }
            let _ = resolve_fn.call(&JSValue::Undefined, &[JSValue::Undefined]);
            Ok(JSValue::Object(promise))
        })),
    );

    proto
}

/// Creates a `WritableStreamDefaultWriter` instance.
pub fn create_writable_stream_writer(stream: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let writer = JSObject::new_empty(None);
    JSObject::set_property(&writer, "__stream__", JSValue::Object(stream.clone()));

    let (ready_promise, ready_resolve, _) = new_promise_capability(None);
    let _ = ready_resolve.call(&JSValue::Undefined, &[JSValue::Undefined]);
    JSObject::set_property(&writer, "ready", JSValue::Object(ready_promise));

    let (closed_promise, closed_resolve, _) = new_promise_capability(None);
    JSObject::set_property(&writer, "closed", JSValue::Object(closed_promise));

    // writer.write(chunk)
    let st_for_write = stream.clone();
    JSObject::set_property(
        &writer,
        "write",
        JSValue::Function(JSFunction::new_closure("write", move |_this, args| {
            let (promise, resolve_fn, _) = new_promise_capability(None);
            let chunk = args.first().cloned().unwrap_or(JSValue::Undefined);

            let sink_val = JSObject::get_property(&st_for_write.borrow(), "__sink__");
            if let JSValue::Object(sink) = sink_val {
                let write_fn = JSObject::get_property(&sink.borrow(), "write");
                if let JSValue::Function(f) = write_fn {
                    let ctrl = JSObject::new_empty(None);
                    let _ = f.call(&JSValue::Object(sink), &[chunk, JSValue::Object(ctrl)]);
                }
            }

            let _ = resolve_fn.call(&JSValue::Undefined, &[JSValue::Undefined]);
            Ok(JSValue::Object(promise))
        })),
    );

    // writer.close()
    let st_for_close = stream.clone();
    let closed_resolve_clone = closed_resolve;
    JSObject::set_property(
        &writer,
        "close",
        JSValue::Function(JSFunction::new_closure("close", move |_this, _args| {
            let (promise, resolve_fn, _) = new_promise_capability(None);
            JSObject::set_property(&st_for_close, "__closed__", JSValue::Boolean(true));
            let _ = closed_resolve_clone.call(&JSValue::Undefined, &[JSValue::Undefined]);
            let _ = resolve_fn.call(&JSValue::Undefined, &[JSValue::Undefined]);
            Ok(JSValue::Object(promise))
        })),
    );

    // writer.releaseLock()
    let st_for_release = stream;
    JSObject::set_property(
        &writer,
        "releaseLock",
        JSValue::Function(JSFunction::new_closure("releaseLock", move |_this, _args| {
            JSObject::set_property(&st_for_release, "locked", JSValue::Boolean(false));
            Ok(JSValue::Undefined)
        })),
    );

    writer
}

/// Creates the `WritableStream` constructor.
pub fn create_writable_stream_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let proto_clone = prototype.clone();

    let ctor = JSFunction::new_closure("WritableStream", move |_this, args| {
        let stream = JSObject::new_empty(Some(proto_clone.clone()));
        JSObject::set_property(&stream, "locked", JSValue::Boolean(false));
        JSObject::set_property(&stream, "__closed__", JSValue::Boolean(false));

        let sink = args.first().cloned().unwrap_or(JSValue::Undefined);
        JSObject::set_property(&stream, "__sink__", sink.clone());

        if let JSValue::Object(ref s) = sink {
            let start_fn = JSObject::get_property(&s.borrow(), "start");
            if let JSValue::Function(f) = start_fn {
                let ctrl = JSObject::new_empty(None);
                let _ = f.call(&sink, &[JSValue::Object(ctrl)]);
            }
        }

        Ok(JSValue::Object(stream))
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(prototype.clone()));
    JSObject::set_property(&prototype, "constructor", JSValue::Object(ctor_obj.clone()));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor));
    ctor_obj
}

/// Creates the `TransformStream` prototype.
pub fn create_transform_stream_prototype() -> Rc<RefCell<JSObject>> {
    JSObject::new_empty(None)
}

/// Creates the `TransformStream` constructor.
pub fn create_transform_stream_constructor(
    prototype: Rc<RefCell<JSObject>>,
    readable_proto: Rc<RefCell<JSObject>>,
    writable_proto: Rc<RefCell<JSObject>>,
) -> Rc<RefCell<JSObject>> {
    let proto_clone = prototype.clone();
    let r_proto = readable_proto;
    let w_proto = writable_proto;

    let ctor = JSFunction::new_closure("TransformStream", move |_this, args| {
        let ts = JSObject::new_empty(Some(proto_clone.clone()));
        let transformer = args.first().cloned().unwrap_or(JSValue::Undefined);

        // Underlying readable stream
        let readable = JSObject::new_empty(Some(r_proto.clone()));
        JSObject::set_property(&readable, "locked", JSValue::Boolean(false));
        JSObject::set_property(&readable, "__closed__", JSValue::Boolean(false));
        JSObject::set_property(&readable, "__queue__", JSValue::Array(JSArray::new_array(Vec::new())));

        let r_controller = create_readable_stream_controller(readable.clone());
        JSObject::set_property(&readable, "__controller__", JSValue::Object(r_controller.clone()));

        // Underlying writable stream sink that feeds transformer and enqueues to readable
        let sink = JSObject::new_empty(None);
        let trans_clone = transformer.clone();
        let r_ctrl_clone = r_controller.clone();

        JSObject::set_property(
            &sink,
            "write",
            JSValue::Function(JSFunction::new_closure("write", move |_this, write_args| {
                let chunk = write_args.first().cloned().unwrap_or(JSValue::Undefined);
                if let JSValue::Object(ref t) = trans_clone {
                    let transform_fn = JSObject::get_property(&t.borrow(), "transform");
                    if let JSValue::Function(f) = transform_fn {
                        let _ = f.call(&trans_clone, &[chunk, JSValue::Object(r_ctrl_clone.clone())]);
                    } else {
                        // Default passthrough: enqueue directly
                        let enq_fn = JSObject::get_property(&r_ctrl_clone.borrow(), "enqueue");
                        if let JSValue::Function(f) = enq_fn {
                            let _ = f.call(&JSValue::Object(r_ctrl_clone.clone()), &[chunk]);
                        }
                    }
                } else {
                    let enq_fn = JSObject::get_property(&r_ctrl_clone.borrow(), "enqueue");
                    if let JSValue::Function(f) = enq_fn {
                        let _ = f.call(&JSValue::Object(r_ctrl_clone.clone()), &[chunk]);
                    }
                }
                Ok(JSValue::Undefined)
            })),
        );

        let trans_flush = transformer.clone();
        let r_ctrl_flush = r_controller;
        let r_close = readable.clone();

        JSObject::set_property(
            &sink,
            "close",
            JSValue::Function(JSFunction::new_closure("close", move |_this, _close_args| {
                if let JSValue::Object(ref t) = trans_flush {
                    let flush_fn = JSObject::get_property(&t.borrow(), "flush");
                    if let JSValue::Function(f) = flush_fn {
                        let _ = f.call(&trans_flush, &[JSValue::Object(r_ctrl_flush.clone())]);
                    }
                }
                JSObject::set_property(&r_close, "__closed__", JSValue::Boolean(true));
                Ok(JSValue::Undefined)
            })),
        );

        let writable = JSObject::new_empty(Some(w_proto.clone()));
        JSObject::set_property(&writable, "locked", JSValue::Boolean(false));
        JSObject::set_property(&writable, "__closed__", JSValue::Boolean(false));
        JSObject::set_property(&writable, "__sink__", JSValue::Object(sink));

        JSObject::set_property(&ts, "readable", JSValue::Object(readable));
        JSObject::set_property(&ts, "writable", JSValue::Object(writable));

        Ok(JSValue::Object(ts))
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(prototype.clone()));
    JSObject::set_property(&prototype, "constructor", JSValue::Object(ctor_obj.clone()));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor));
    ctor_obj
}
