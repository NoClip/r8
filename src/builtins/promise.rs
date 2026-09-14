//! Safe Rust reimplementation of Google V8's ECMAScript `Promise` built-in constructor and prototype.
//!
//! Implements Promises/A+ specifications: Pending, Fulfilled, and Rejected states,
//! `.then()`, `.catch()`, `.finally()`, combinatorial utilities (`Promise.all`, `Promise.race`,
//! `Promise.allSettled`, `Promise.any`), static `Promise.resolve` and `Promise.reject`,
//! and global `queueMicrotask()`.

use crate::execution::microtask_queue::{Microtask, MicrotaskQueue};
use crate::objects::{JSArray, JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

pub const PROMISE_PENDING: i32 = 0;
pub const PROMISE_FULFILLED: i32 = 1;
pub const PROMISE_REJECTED: i32 = 2;

/// Allocates a new `Promise` object instance initialized in the Pending state.
pub fn new_promise_instance(mut prototype: Option<Rc<RefCell<JSObject>>>) -> Rc<RefCell<JSObject>> {
    if prototype.is_none() {
        if let Some(global) = crate::runtime::current_global() {
            if let JSValue::Object(ctor) = global.borrow().get_property("Promise") {
                if let JSValue::Object(p) = ctor.borrow().get_property("prototype") {
                    prototype = Some(p);
                }
            }
        }
    }
    let map = crate::objects::Map::root(crate::objects::InstanceType::JSPromise, prototype);
    let obj = JSObject::new_with_map(map);
    JSObject::set_property(&obj, "__promise_state__", JSValue::Smi(PROMISE_PENDING));
    JSObject::set_property(&obj, "__promise_result__", JSValue::Undefined);
    JSObject::set_property(
        &obj,
        "__fulfill_reactions__",
        JSValue::Array(JSArray::new_array(Vec::new())),
    );
    JSObject::set_property(
        &obj,
        "__reject_reactions__",
        JSValue::Array(JSArray::new_array(Vec::new())),
    );
    obj
}

fn create_aggregate_error_instance(errors: Vec<JSValue>, msg: &str) -> Rc<RefCell<JSObject>> {
    let global = crate::runtime::current_global();
    let proto = global.as_ref().and_then(|g| {
        if let JSValue::Object(ctor) = g.borrow().get_property("AggregateError") {
            if let JSValue::Object(p) = ctor.borrow().get_property("prototype") {
                return Some(p);
            }
        }
        None
    });
    let err = crate::builtins::error::new_error_instance(proto, "AggregateError", Some(msg.to_string()), None);
    JSObject::set_property(&err, "errors", JSValue::Array(JSArray::new_array(errors)));
    err
}

/// Retrieves the current `(state, result)` of a Promise object.
pub fn get_promise_state(promise: &Rc<RefCell<JSObject>>) -> (i32, JSValue) {
    let borrowed = promise.borrow();
    let state = match borrowed.get_property("__promise_state__") {
        JSValue::Smi(n) => n,
        _ => PROMISE_PENDING,
    };
    let result = borrowed.get_property("__promise_result__");
    (state, result)
}

/// Internal resolution algorithm for settling a Promise with a value.
pub fn resolve_promise_internal(
    promise: &Rc<RefCell<JSObject>>,
    value: JSValue,
    queue: &Rc<RefCell<MicrotaskQueue>>,
) {
    // 1. Chaining cycle detection: TypeError if resolving with itself
    if let JSValue::Object(ref val_obj) = value {
        if Rc::ptr_eq(val_obj, promise) {
            reject_promise_internal(
                promise,
                JSValue::String("TypeError: Chaining cycle detected for promise".to_string()),
                queue,
            );
            return;
        }

        // 2. If value is already a Promise instance, adopt its state
        if val_obj.borrow().get_property("__promise_state__") != JSValue::Undefined {
            let (other_state, other_res) = get_promise_state(val_obj);
            if other_state == PROMISE_FULFILLED {
                resolve_promise_internal(promise, other_res, queue);
            } else if other_state == PROMISE_REJECTED {
                reject_promise_internal(promise, other_res, queue);
            } else {
                // Pending: register reaction on `val_obj` to resolve/reject `promise`
                let p_clone = promise.clone();
                let q_clone = queue.clone();
                let fulfill_cb = JSFunction::new_closure("resolve", move |_this, args| {
                    let v = args.first().cloned().unwrap_or(JSValue::Undefined);
                    resolve_promise_internal(&p_clone, v, &q_clone);
                    Ok(JSValue::Undefined)
                });

                let p_clone2 = promise.clone();
                let q_clone2 = queue.clone();
                let reject_cb = JSFunction::new_closure("reject", move |_this, args| {
                    let r = args.first().cloned().unwrap_or(JSValue::Undefined);
                    reject_promise_internal(&p_clone2, r, &q_clone2);
                    Ok(JSValue::Undefined)
                });

                add_reaction(val_obj, Some(fulfill_cb), None, false);
                add_reaction(val_obj, Some(reject_cb), None, true);
            }
            return;
        }

        // 3. Thenable adoption: check for callable `then` property
        let then_prop = val_obj.borrow().get_property("then");
        if let JSValue::Function(then_fn) = then_prop {
            let p_clone = promise.clone();
            let q_clone = queue.clone();
            let fulfill_cb = JSFunction::new_closure("resolve", move |_this, args| {
                let v = args.first().cloned().unwrap_or(JSValue::Undefined);
                resolve_promise_internal(&p_clone, v, &q_clone);
                Ok(JSValue::Undefined)
            });

            let p_clone2 = promise.clone();
            let q_clone2 = queue.clone();
            let reject_cb = JSFunction::new_closure("reject", move |_this, args| {
                let r = args.first().cloned().unwrap_or(JSValue::Undefined);
                reject_promise_internal(&p_clone2, r, &q_clone2);
                Ok(JSValue::Undefined)
            });

            if let Err(err) = then_fn.call(&value, &[JSValue::Function(fulfill_cb), JSValue::Function(reject_cb)]) {
                reject_promise_internal(promise, JSValue::String(err), queue);
            }
            return;
        }
    }

    // 4. Settle if still Pending
    let (state, _) = get_promise_state(promise);
    if state != PROMISE_PENDING {
        return; // Already settled
    }

    JSObject::set_property(promise, "__promise_state__", JSValue::Smi(PROMISE_FULFILLED));
    JSObject::set_property(promise, "__promise_result__", value.clone());

    // 5. Drain pending fulfillment reactions into the microtask queue
    drain_reactions(promise, "__fulfill_reactions__", value, false, queue);
}

/// Internal rejection algorithm for settling a Promise with a reason.
pub fn reject_promise_internal(
    promise: &Rc<RefCell<JSObject>>,
    reason: JSValue,
    queue: &Rc<RefCell<MicrotaskQueue>>,
) {
    let (state, _) = get_promise_state(promise);
    if state != PROMISE_PENDING {
        return; // Already settled
    }

    JSObject::set_property(promise, "__promise_state__", JSValue::Smi(PROMISE_REJECTED));
    JSObject::set_property(promise, "__promise_result__", reason.clone());

    // Drain pending rejection reactions into the microtask queue
    drain_reactions(promise, "__reject_reactions__", reason, true, queue);
}

/// Helper to add a reaction record `{ handler, downstream, is_rejection }` to a pending promise.
fn add_reaction(
    promise: &Rc<RefCell<JSObject>>,
    handler: Option<Rc<JSFunction>>,
    downstream: Option<Rc<RefCell<JSObject>>>,
    is_rejection: bool,
) {
    let prop_name = if is_rejection {
        "__reject_reactions__"
    } else {
        "__fulfill_reactions__"
    };

    let record = JSObject::new_empty(None);
    if let Some(h) = handler {
        JSObject::set_property(&record, "handler", JSValue::Function(h));
    }
    if let Some(d) = downstream {
        JSObject::set_property(&record, "downstream", JSValue::Object(d));
    }
    JSObject::set_property(&record, "is_rejection", JSValue::Boolean(is_rejection));

    let reactions = promise.borrow().get_property(prop_name);
    if let JSValue::Array(arr) = reactions {
        arr.borrow_mut().elements.push(JSValue::Object(record));
    }
}

/// Helper to drain all reaction records from a promise and enqueue them as Microtasks.
fn drain_reactions(
    promise: &Rc<RefCell<JSObject>>,
    reactions_prop: &str,
    argument: JSValue,
    is_rejection: bool,
    queue: &Rc<RefCell<MicrotaskQueue>>,
) {
    let reactions_val = promise.borrow().get_property(reactions_prop);
    if let JSValue::Array(arr) = reactions_val {
        let reactions = {
            let mut borrowed = arr.borrow_mut();
            std::mem::take(&mut borrowed.elements)
        };

        let mut q = queue.borrow_mut();
        for r_val in reactions {
            if let JSValue::Object(r) = r_val {
                let handler = match r.borrow().get_property("handler") {
                    JSValue::Function(f) => Some(f),
                    _ => None,
                };
                let downstream = match r.borrow().get_property("downstream") {
                    JSValue::Object(d) => Some(d),
                    _ => None,
                };

                q.enqueue(Microtask::PromiseReaction {
                    handler,
                    argument: argument.clone(),
                    downstream,
                    is_rejection,
                });
            }
        }
    }

    // Clear reaction arrays to release closures
    JSObject::set_property(
        promise,
        "__fulfill_reactions__",
        JSValue::Array(JSArray::new_array(Vec::new())),
    );
    JSObject::set_property(
        promise,
        "__reject_reactions__",
        JSValue::Array(JSArray::new_array(Vec::new())),
    );
}

/// Creates the `Promise.prototype` object containing `.then()`, `.catch()`, and `.finally()`.
pub fn create_promise_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // Promise.prototype.then(onFulfilled, onRejected)
    let proto_clone = proto.clone();
    JSObject::set_property(
        &proto,
        "then",
        JSValue::Function(JSFunction::new_closure("then", move |this, args| {
            let promise = match this {
                JSValue::Object(obj) => obj.clone(),
                _ => return Err("TypeError: Promise.prototype.then called on non-object".to_string()),
            };

            let on_fulfilled = match args.get(0) {
                Some(JSValue::Function(f)) => Some(f.clone()),
                _ => None,
            };
            let on_rejected = match args.get(1) {
                Some(JSValue::Function(f)) => Some(f.clone()),
                _ => None,
            };

            let child = new_promise_instance(Some(proto_clone.clone()));
            let (state, result) = get_promise_state(&promise);

            if let Some(queue_rc) = MicrotaskQueue::current() {
                if state == PROMISE_PENDING {
                    add_reaction(&promise, on_fulfilled, Some(child.clone()), false);
                    add_reaction(&promise, on_rejected, Some(child.clone()), true);
                } else if state == PROMISE_FULFILLED {
                    queue_rc.borrow_mut().enqueue(Microtask::PromiseReaction {
                        handler: on_fulfilled,
                        argument: result,
                        downstream: Some(child.clone()),
                        is_rejection: false,
                    });
                } else if state == PROMISE_REJECTED {
                    queue_rc.borrow_mut().enqueue(Microtask::PromiseReaction {
                        handler: on_rejected,
                        argument: result,
                        downstream: Some(child.clone()),
                        is_rejection: true,
                    });
                }
            } else {
                // If no active queue, record reaction on the promise
                add_reaction(&promise, on_fulfilled, Some(child.clone()), false);
                add_reaction(&promise, on_rejected, Some(child.clone()), true);
            }

            Ok(JSValue::Object(child))
        })),
    );

    // Promise.prototype.catch(onRejected)
    JSObject::set_property(
        &proto,
        "catch",
        JSValue::Function(JSFunction::new_native("catch", |this, args| {
            if let JSValue::Object(obj) = this {
                let then_prop = obj.borrow().get_property("then");
                if let JSValue::Function(then_fn) = then_prop {
                    let on_rejected = args.first().cloned().unwrap_or(JSValue::Undefined);
                    return then_fn.call(this, &[JSValue::Undefined, on_rejected]);
                }
            }
            Err("TypeError: Method Promise.prototype.catch called on incompatible receiver".to_string())
        })),
    );

    // Promise.prototype.finally(onFinally)
    JSObject::set_property(
        &proto,
        "finally",
        JSValue::Function(JSFunction::new_native("finally", |this, args| {
            if let JSValue::Object(obj) = this {
                let on_finally = match args.first() {
                    Some(JSValue::Function(f)) => Some(f.clone()),
                    _ => None,
                };

                let then_prop = obj.borrow().get_property("then");
                if let JSValue::Function(then_fn) = then_prop {
                    let on_finally_fulfill = on_finally.clone();
                    let fulfill_cb = JSFunction::new_closure("finallyCallback", move |_this, args| {
                        let val = args.first().cloned().unwrap_or(JSValue::Undefined);
                        if let Some(ref fin) = on_finally_fulfill {
                            let _ = fin.call(&JSValue::Undefined, &[]);
                        }
                        Ok(val)
                    });

                    let on_finally_reject = on_finally;
                    let reject_cb = JSFunction::new_closure("finallyCallback", move |_this, args| {
                        let reason = args.first().cloned().unwrap_or(JSValue::Undefined);
                        if let Some(ref fin) = on_finally_reject {
                            let _ = fin.call(&JSValue::Undefined, &[]);
                        }
                        // Re-throw the original rejection reason
                        Err(reason.to_string_val())
                    });

                    return then_fn.call(this, &[JSValue::Function(fulfill_cb), JSValue::Function(reject_cb)]);
                }
            }
            Err("TypeError: Method Promise.prototype.finally called on incompatible receiver".to_string())
        })),
    );

    proto
}

/// Creates the `Promise` constructor object and static utilities.
pub fn create_promise_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let promise_ctor = JSObject::new_empty(None);
    JSObject::set_property(&promise_ctor, "prototype", JSValue::Object(prototype.clone()));

    // Promise.resolve(value)
    let proto_for_resolve = prototype.clone();
    JSObject::set_property(
        &promise_ctor,
        "resolve",
        JSValue::Function(JSFunction::new_closure("resolve", move |_this, args| {
            let value = args.first().cloned().unwrap_or(JSValue::Undefined);
            if let JSValue::Object(ref obj) = value {
                if obj.borrow().get_property("__promise_state__") != JSValue::Undefined {
                    return Ok(value);
                }
            }
            let p = new_promise_instance(Some(proto_for_resolve.clone()));
            if let Some(q) = MicrotaskQueue::current() {
                resolve_promise_internal(&p, value, &q);
            } else {
                JSObject::set_property(&p, "__promise_state__", JSValue::Smi(PROMISE_FULFILLED));
                JSObject::set_property(&p, "__promise_result__", value);
            }
            Ok(JSValue::Object(p))
        })),
    );

    // Promise.reject(reason)
    let proto_for_reject = prototype.clone();
    JSObject::set_property(
        &promise_ctor,
        "reject",
        JSValue::Function(JSFunction::new_closure("reject", move |_this, args| {
            let reason = args.first().cloned().unwrap_or(JSValue::Undefined);
            let p = new_promise_instance(Some(proto_for_reject.clone()));
            if let Some(q) = MicrotaskQueue::current() {
                reject_promise_internal(&p, reason, &q);
            } else {
                JSObject::set_property(&p, "__promise_state__", JSValue::Smi(PROMISE_REJECTED));
                JSObject::set_property(&p, "__promise_result__", reason);
            }
            Ok(JSValue::Object(p))
        })),
    );

    // Promise.all(iterable)
    let proto_for_all = prototype.clone();
    JSObject::set_property(
        &promise_ctor,
        "all",
        JSValue::Function(JSFunction::new_closure("all", move |_this, args| {
            let elements = match args.first() {
                Some(JSValue::Array(arr)) => arr.borrow().elements.clone(),
                _ => return Err("TypeError: Promise.all requires an array argument".to_string()),
            };

            let child = new_promise_instance(Some(proto_for_all.clone()));
            let total = elements.len();

            if total == 0 {
                if let Some(q) = MicrotaskQueue::current() {
                    resolve_promise_internal(&child, JSValue::Array(JSArray::new_array(Vec::new())), &q);
                } else {
                    JSObject::set_property(&child, "__promise_state__", JSValue::Smi(PROMISE_FULFILLED));
                    JSObject::set_property(&child, "__promise_result__", JSValue::Array(JSArray::new_array(Vec::new())));
                }
                return Ok(JSValue::Object(child));
            }

            let results = Rc::new(RefCell::new(vec![JSValue::Undefined; total]));
            let remaining = Rc::new(RefCell::new(total));

            for (i, elem) in elements.into_iter().enumerate() {
                let results_clone = results.clone();
                let remaining_clone = remaining.clone();
                let child_fulfill = child.clone();

                let on_fulfilled = JSFunction::new_closure("allResolve", move |_this, args| {
                    let val = args.first().cloned().unwrap_or(JSValue::Undefined);
                    results_clone.borrow_mut()[i] = val;
                    let mut rem = remaining_clone.borrow_mut();
                    *rem -= 1;
                    if *rem == 0 {
                        let final_array = JSValue::Array(JSArray::new_array(results_clone.borrow().clone()));
                        if let Some(q) = MicrotaskQueue::current() {
                            resolve_promise_internal(&child_fulfill, final_array, &q);
                        }
                    }
                    Ok(JSValue::Undefined)
                });

                let child_reject = child.clone();
                let on_rejected = JSFunction::new_closure("allReject", move |_this, args| {
                    let err = args.first().cloned().unwrap_or(JSValue::Undefined);
                    if let Some(q) = MicrotaskQueue::current() {
                        reject_promise_internal(&child_reject, err, &q);
                    }
                    Ok(JSValue::Undefined)
                });

                // Wrap item in Promise.resolve(elem).then(...)
                let item_p = if let JSValue::Object(ref o) = elem {
                    if o.borrow().get_property("__promise_state__") != JSValue::Undefined {
                        o.clone()
                    } else {
                        let p = new_promise_instance(Some(proto_for_all.clone()));
                        if let Some(q) = MicrotaskQueue::current() {
                            resolve_promise_internal(&p, elem, &q);
                        }
                        p
                    }
                } else {
                    let p = new_promise_instance(Some(proto_for_all.clone()));
                    if let Some(q) = MicrotaskQueue::current() {
                        resolve_promise_internal(&p, elem, &q);
                    }
                    p
                };

                let then_prop = item_p.borrow().get_property("then");
                if let JSValue::Function(then_fn) = then_prop {
                    let _ = then_fn.call(&JSValue::Object(item_p), &[JSValue::Function(on_fulfilled), JSValue::Function(on_rejected)]);
                }
            }

            Ok(JSValue::Object(child))
        })),
    );

    // Promise.race(iterable)
    let proto_for_race = prototype.clone();
    JSObject::set_property(
        &promise_ctor,
        "race",
        JSValue::Function(JSFunction::new_closure("race", move |_this, args| {
            let elements = match args.first() {
                Some(JSValue::Array(arr)) => arr.borrow().elements.clone(),
                _ => return Err("TypeError: Promise.race requires an array argument".to_string()),
            };

            let child = new_promise_instance(Some(proto_for_race.clone()));

            for elem in elements {
                let child_fulfill = child.clone();
                let on_fulfilled = JSFunction::new_closure("raceResolve", move |_this, args| {
                    let val = args.first().cloned().unwrap_or(JSValue::Undefined);
                    if let Some(q) = MicrotaskQueue::current() {
                        resolve_promise_internal(&child_fulfill, val, &q);
                    }
                    Ok(JSValue::Undefined)
                });

                let child_reject = child.clone();
                let on_rejected = JSFunction::new_closure("raceReject", move |_this, args| {
                    let err = args.first().cloned().unwrap_or(JSValue::Undefined);
                    if let Some(q) = MicrotaskQueue::current() {
                        reject_promise_internal(&child_reject, err, &q);
                    }
                    Ok(JSValue::Undefined)
                });

                let item_p = if let JSValue::Object(ref o) = elem {
                    if o.borrow().get_property("__promise_state__") != JSValue::Undefined {
                        o.clone()
                    } else {
                        let p = new_promise_instance(Some(proto_for_race.clone()));
                        if let Some(q) = MicrotaskQueue::current() {
                            resolve_promise_internal(&p, elem, &q);
                        }
                        p
                    }
                } else {
                    let p = new_promise_instance(Some(proto_for_race.clone()));
                    if let Some(q) = MicrotaskQueue::current() {
                        resolve_promise_internal(&p, elem, &q);
                    }
                    p
                };

                let then_prop = item_p.borrow().get_property("then");
                if let JSValue::Function(then_fn) = then_prop {
                    let _ = then_fn.call(&JSValue::Object(item_p), &[JSValue::Function(on_fulfilled), JSValue::Function(on_rejected)]);
                }
            }

            Ok(JSValue::Object(child))
        })),
    );

    // Promise.allSettled(iterable)
    let proto_for_settled = prototype.clone();
    JSObject::set_property(
        &promise_ctor,
        "allSettled",
        JSValue::Function(JSFunction::new_closure("allSettled", move |_this, args| {
            let elements = match args.first() {
                Some(JSValue::Array(arr)) => arr.borrow().elements.clone(),
                _ => return Err("TypeError: Promise.allSettled requires an array argument".to_string()),
            };

            let child = new_promise_instance(Some(proto_for_settled.clone()));
            let total = elements.len();

            if total == 0 {
                if let Some(q) = MicrotaskQueue::current() {
                    resolve_promise_internal(&child, JSValue::Array(JSArray::new_array(Vec::new())), &q);
                }
                return Ok(JSValue::Object(child));
            }

            let results = Rc::new(RefCell::new(vec![JSValue::Undefined; total]));
            let remaining = Rc::new(RefCell::new(total));

            for (i, elem) in elements.into_iter().enumerate() {
                let results_clone = results.clone();
                let remaining_clone = remaining.clone();
                let child_fulfill = child.clone();

                let on_fulfilled = JSFunction::new_closure("settledFulfill", move |_this, args| {
                    let val = args.first().cloned().unwrap_or(JSValue::Undefined);
                    let obj = JSObject::new_empty(None);
                    JSObject::set_property(&obj, "status", JSValue::String("fulfilled".to_string()));
                    JSObject::set_property(&obj, "value", val);
                    results_clone.borrow_mut()[i] = JSValue::Object(obj);

                    let mut rem = remaining_clone.borrow_mut();
                    *rem -= 1;
                    if *rem == 0 {
                        let final_array = JSValue::Array(JSArray::new_array(results_clone.borrow().clone()));
                        if let Some(q) = MicrotaskQueue::current() {
                            resolve_promise_internal(&child_fulfill, final_array, &q);
                        }
                    }
                    Ok(JSValue::Undefined)
                });

                let results_clone2 = results.clone();
                let remaining_clone2 = remaining.clone();
                let child_reject = child.clone();

                let on_rejected = JSFunction::new_closure("settledReject", move |_this, args| {
                    let reason = args.first().cloned().unwrap_or(JSValue::Undefined);
                    let obj = JSObject::new_empty(None);
                    JSObject::set_property(&obj, "status", JSValue::String("rejected".to_string()));
                    JSObject::set_property(&obj, "reason", reason);
                    results_clone2.borrow_mut()[i] = JSValue::Object(obj);

                    let mut rem = remaining_clone2.borrow_mut();
                    *rem -= 1;
                    if *rem == 0 {
                        let final_array = JSValue::Array(JSArray::new_array(results_clone2.borrow().clone()));
                        if let Some(q) = MicrotaskQueue::current() {
                            resolve_promise_internal(&child_reject, final_array, &q);
                        }
                    }
                    Ok(JSValue::Undefined)
                });

                let item_p = if let JSValue::Object(ref o) = elem {
                    if o.borrow().get_property("__promise_state__") != JSValue::Undefined {
                        o.clone()
                    } else {
                        let p = new_promise_instance(Some(proto_for_settled.clone()));
                        if let Some(q) = MicrotaskQueue::current() {
                            resolve_promise_internal(&p, elem, &q);
                        }
                        p
                    }
                } else {
                    let p = new_promise_instance(Some(proto_for_settled.clone()));
                    if let Some(q) = MicrotaskQueue::current() {
                        resolve_promise_internal(&p, elem, &q);
                    }
                    p
                };

                let then_prop = item_p.borrow().get_property("then");
                if let JSValue::Function(then_fn) = then_prop {
                    let _ = then_fn.call(&JSValue::Object(item_p), &[JSValue::Function(on_fulfilled), JSValue::Function(on_rejected)]);
                }
            }

            Ok(JSValue::Object(child))
        })),
    );

    // Promise.any(iterable) (ES2021)
    let proto_for_any = prototype.clone();
    JSObject::set_property(
        &promise_ctor,
        "any",
        JSValue::Function(JSFunction::new_closure("any", move |_this, args| {
            let elements = match args.first() {
                Some(JSValue::Array(arr)) => arr.borrow().elements.clone(),
                _ => return Err("TypeError: Promise.any requires an array argument".to_string()),
            };

            let child = new_promise_instance(Some(proto_for_any.clone()));
            let total = elements.len();

            if total == 0 {
                let agg_err = create_aggregate_error_instance(Vec::new(), "All promises were rejected");
                if let Some(q) = MicrotaskQueue::current() {
                    reject_promise_internal(&child, JSValue::Object(agg_err), &q);
                } else {
                    JSObject::set_property(&child, "__promise_state__", JSValue::Smi(PROMISE_REJECTED));
                    JSObject::set_property(&child, "__promise_result__", JSValue::Object(agg_err));
                }
                return Ok(JSValue::Object(child));
            }

            let errors = Rc::new(RefCell::new(vec![JSValue::Undefined; total]));
            let remaining = Rc::new(RefCell::new(total));

            for (i, elem) in elements.into_iter().enumerate() {
                let child_fulfill = child.clone();
                let on_fulfilled = JSFunction::new_closure("anyResolve", move |_this, args| {
                    let val = args.first().cloned().unwrap_or(JSValue::Undefined);
                    if let Some(q) = MicrotaskQueue::current() {
                        resolve_promise_internal(&child_fulfill, val, &q);
                    }
                    Ok(JSValue::Undefined)
                });

                let errors_clone = errors.clone();
                let remaining_clone = remaining.clone();
                let child_reject = child.clone();
                let on_rejected = JSFunction::new_closure("anyReject", move |_this, args| {
                    let err = args.first().cloned().unwrap_or(JSValue::Undefined);
                    errors_clone.borrow_mut()[i] = err;
                    let mut rem = remaining_clone.borrow_mut();
                    *rem -= 1;
                    if *rem == 0 {
                        let agg_err = create_aggregate_error_instance(errors_clone.borrow().clone(), "All promises were rejected");
                        if let Some(q) = MicrotaskQueue::current() {
                            reject_promise_internal(&child_reject, JSValue::Object(agg_err), &q);
                        }
                    }
                    Ok(JSValue::Undefined)
                });

                let item_p = if let JSValue::Object(ref o) = elem {
                    if o.borrow().get_property("__promise_state__") != JSValue::Undefined {
                        o.clone()
                    } else {
                        let p = new_promise_instance(Some(proto_for_any.clone()));
                        if let Some(q) = MicrotaskQueue::current() {
                            resolve_promise_internal(&p, elem, &q);
                        }
                        p
                    }
                } else {
                    let p = new_promise_instance(Some(proto_for_any.clone()));
                    if let Some(q) = MicrotaskQueue::current() {
                        resolve_promise_internal(&p, elem, &q);
                    }
                    p
                };

                let then_prop = item_p.borrow().get_property("then");
                if let JSValue::Function(then_fn) = then_prop {
                    let _ = then_fn.call(&JSValue::Object(item_p), &[JSValue::Function(on_fulfilled), JSValue::Function(on_rejected)]);
                }
            }

            Ok(JSValue::Object(child))
        })),
    );

    // Direct invocation `new Promise(executor)`
    let proto_for_ctor = prototype.clone();
    JSObject::set_property(
        &promise_ctor,
        "__call__",
        JSValue::Function(JSFunction::new_closure("Promise", move |_this, args| {
            let executor = match args.first() {
                Some(JSValue::Function(f)) => f.clone(),
                _ => return Err("TypeError: Promise resolver undefined is not a function".to_string()),
            };

            let promise = new_promise_instance(Some(proto_for_ctor.clone()));

            let p_resolve = promise.clone();
            let resolve_fn = JSFunction::new_closure("resolve", move |_this, args| {
                let val = args.first().cloned().unwrap_or(JSValue::Undefined);
                if let Some(q) = MicrotaskQueue::current() {
                    resolve_promise_internal(&p_resolve, val, &q);
                } else {
                    JSObject::set_property(&p_resolve, "__promise_state__", JSValue::Smi(PROMISE_FULFILLED));
                    JSObject::set_property(&p_resolve, "__promise_result__", val);
                }
                Ok(JSValue::Undefined)
            });

            let p_reject = promise.clone();
            let reject_fn = JSFunction::new_closure("reject", move |_this, args| {
                let reason = args.first().cloned().unwrap_or(JSValue::Undefined);
                if let Some(q) = MicrotaskQueue::current() {
                    reject_promise_internal(&p_reject, reason, &q);
                } else {
                    JSObject::set_property(&p_reject, "__promise_state__", JSValue::Smi(PROMISE_REJECTED));
                    JSObject::set_property(&p_reject, "__promise_result__", reason);
                }
                Ok(JSValue::Undefined)
            });

            // Call executor(resolve, reject)
            let call_res = executor.call(
                &JSValue::Undefined,
                &[JSValue::Function(resolve_fn), JSValue::Function(reject_fn.clone())],
            );

            if let Err(err) = call_res {
                let _ = reject_fn.call(&JSValue::Undefined, &[JSValue::String(err)]);
            }

            Ok(JSValue::Object(promise))
        })),
    );

    // Promise.withResolvers() (ES2024)
    let proto_for_resolvers = prototype.clone();
    JSObject::set_property(
        &promise_ctor,
        "withResolvers",
        JSValue::Function(JSFunction::new_closure("withResolvers", move |_this, _args| {
            let (promise, resolve_fn, reject_fn) = new_promise_capability(Some(proto_for_resolvers.clone()));
            let res_obj = JSObject::new_empty(None);
            JSObject::set_property(&res_obj, "promise", JSValue::Object(promise));
            JSObject::set_property(&res_obj, "resolve", JSValue::Function(resolve_fn));
            JSObject::set_property(&res_obj, "reject", JSValue::Function(reject_fn));
            Ok(JSValue::Object(res_obj))
        })),
    );

    promise_ctor
}

/// Creates the global `queueMicrotask(callback)` function.
pub fn create_queue_microtask_function() -> Rc<JSFunction> {
    JSFunction::new_native("queueMicrotask", |_this, args| {
        let callback = match args.first() {
            Some(JSValue::Function(cb)) => cb.clone(),
            _ => return Err("TypeError: queueMicrotask requires a function argument".to_string()),
        };

        if let Some(q) = MicrotaskQueue::current() {
            q.borrow_mut().enqueue(Microtask::Callable {
                func: callback,
                args: Vec::new(),
            });
            Ok(JSValue::Undefined)
        } else {
            Err("TypeError: No active microtask queue available".to_string())
        }
    })
}

/// Creates a new Promise capability: returns `(promise, resolve_fn, reject_fn)`.
pub fn new_promise_capability(
    prototype: Option<Rc<RefCell<JSObject>>>,
) -> (Rc<RefCell<JSObject>>, Rc<JSFunction>, Rc<JSFunction>) {
    let promise = new_promise_instance(prototype);
    let p_resolve = promise.clone();
    let resolve_fn = JSFunction::new_closure("resolve", move |_this, args| {
        let val = args.first().cloned().unwrap_or(JSValue::Undefined);
        if let Some(q) = MicrotaskQueue::current() {
            resolve_promise_internal(&p_resolve, val, &q);
        } else {
            JSObject::set_property(&p_resolve, "__promise_state__", JSValue::Smi(PROMISE_FULFILLED));
            JSObject::set_property(&p_resolve, "__promise_result__", val);
        }
        Ok(JSValue::Undefined)
    });

    let p_reject = promise.clone();
    let reject_fn = JSFunction::new_closure("reject", move |_this, args| {
        let reason = args.first().cloned().unwrap_or(JSValue::Undefined);
        if let Some(q) = MicrotaskQueue::current() {
            reject_promise_internal(&p_reject, reason, &q);
        } else {
            JSObject::set_property(&p_reject, "__promise_state__", JSValue::Smi(PROMISE_REJECTED));
            JSObject::set_property(&p_reject, "__promise_result__", reason);
        }
        Ok(JSValue::Undefined)
    });

    (promise, resolve_fn, reject_fn)
}
