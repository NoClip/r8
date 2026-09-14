//! Safe Rust reimplementation of WHATWG DOM Events Standard (`Event`, `CustomEvent`, `EventTarget`).
//!
//! Provides the foundational event-driven notification and listener architecture for the web platform.

use crate::objects::{JSArray, JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

fn current_timestamp() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

/// Creates the `Event.prototype` object.
pub fn create_event_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // Event constants
    JSObject::set_property(&proto, "NONE", JSValue::Smi(0));
    JSObject::set_property(&proto, "CAPTURING_PHASE", JSValue::Smi(1));
    JSObject::set_property(&proto, "AT_TARGET", JSValue::Smi(2));
    JSObject::set_property(&proto, "BUBBLING_PHASE", JSValue::Smi(3));

    // Event.prototype.preventDefault()
    JSObject::set_property(
        &proto,
        "preventDefault",
        JSValue::Function(JSFunction::new_closure("preventDefault", |_this, _args| {
            if let JSValue::Object(ref o) = _this {
                let cancelable = match JSObject::get_property(&o.borrow(), "cancelable") {
                    JSValue::Boolean(b) => b,
                    _ => false,
                };
                if cancelable {
                    JSObject::set_property(o, "defaultPrevented", JSValue::Boolean(true));
                }
            }
            Ok(JSValue::Undefined)
        })),
    );

    // Event.prototype.stopPropagation()
    JSObject::set_property(
        &proto,
        "stopPropagation",
        JSValue::Function(JSFunction::new_closure("stopPropagation", |_this, _args| {
            if let JSValue::Object(ref o) = _this {
                JSObject::set_property(o, "__propagation_stopped__", JSValue::Boolean(true));
            }
            Ok(JSValue::Undefined)
        })),
    );

    // Event.prototype.stopImmediatePropagation()
    JSObject::set_property(
        &proto,
        "stopImmediatePropagation",
        JSValue::Function(JSFunction::new_closure("stopImmediatePropagation", |_this, _args| {
            if let JSValue::Object(ref o) = _this {
                JSObject::set_property(o, "__propagation_stopped__", JSValue::Boolean(true));
                JSObject::set_property(o, "__immediate_propagation_stopped__", JSValue::Boolean(true));
            }
            Ok(JSValue::Undefined)
        })),
    );

    proto
}

/// Creates the `Event` constructor.
pub fn create_event_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let proto_clone = prototype.clone();

    let ctor = JSFunction::new_closure("Event", move |_this, args| {
        let ev = JSObject::new_empty(Some(proto_clone.clone()));
        let event_type = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let init_dict = args.get(1);

        let mut bubbles = false;
        let mut cancelable = false;
        let mut composed = false;

        if let Some(JSValue::Object(dict)) = init_dict {
            if let JSValue::Boolean(b) = JSObject::get_property(&dict.borrow(), "bubbles") {
                bubbles = b;
            }
            if let JSValue::Boolean(b) = JSObject::get_property(&dict.borrow(), "cancelable") {
                cancelable = b;
            }
            if let JSValue::Boolean(b) = JSObject::get_property(&dict.borrow(), "composed") {
                composed = b;
            }
        }

        JSObject::set_property(&ev, "type", JSValue::String(event_type));
        JSObject::set_property(&ev, "bubbles", JSValue::Boolean(bubbles));
        JSObject::set_property(&ev, "cancelable", JSValue::Boolean(cancelable));
        JSObject::set_property(&ev, "composed", JSValue::Boolean(composed));
        JSObject::set_property(&ev, "defaultPrevented", JSValue::Boolean(false));
        JSObject::set_property(&ev, "target", JSValue::Null);
        JSObject::set_property(&ev, "currentTarget", JSValue::Null);
        JSObject::set_property(&ev, "eventPhase", JSValue::Smi(0));
        JSObject::set_property(&ev, "timeStamp", JSValue::Number(current_timestamp()));
        JSObject::set_property(&ev, "__propagation_stopped__", JSValue::Boolean(false));
        JSObject::set_property(&ev, "__immediate_propagation_stopped__", JSValue::Boolean(false));

        Ok(JSValue::Object(ev))
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "NONE", JSValue::Smi(0));
    JSObject::set_property(&ctor_obj, "CAPTURING_PHASE", JSValue::Smi(1));
    JSObject::set_property(&ctor_obj, "AT_TARGET", JSValue::Smi(2));
    JSObject::set_property(&ctor_obj, "BUBBLING_PHASE", JSValue::Smi(3));
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(prototype.clone()));
    JSObject::set_property(&prototype, "constructor", JSValue::Object(ctor_obj.clone()));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor));
    ctor_obj
}

/// Creates the `CustomEvent.prototype` object.
pub fn create_custom_event_prototype(event_proto: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(Some(event_proto));
    JSObject::set_property(&proto, "detail", JSValue::Null);
    proto
}

/// Creates the `CustomEvent` constructor.
pub fn create_custom_event_constructor(
    prototype: Rc<RefCell<JSObject>>,
    event_ctor: Rc<RefCell<JSObject>>,
) -> Rc<RefCell<JSObject>> {
    let proto_clone = prototype.clone();

    let ctor = JSFunction::new_closure("CustomEvent", move |_this, args| {
        let ev = JSObject::new_empty(Some(proto_clone.clone()));
        let event_type = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let init_dict = args.get(1);

        let mut bubbles = false;
        let mut cancelable = false;
        let mut composed = false;
        let mut detail = JSValue::Null;

        if let Some(JSValue::Object(dict)) = init_dict {
            if let JSValue::Boolean(b) = JSObject::get_property(&dict.borrow(), "bubbles") {
                bubbles = b;
            }
            if let JSValue::Boolean(b) = JSObject::get_property(&dict.borrow(), "cancelable") {
                cancelable = b;
            }
            if let JSValue::Boolean(b) = JSObject::get_property(&dict.borrow(), "composed") {
                composed = b;
            }
            detail = JSObject::get_property(&dict.borrow(), "detail");
        }

        JSObject::set_property(&ev, "type", JSValue::String(event_type));
        JSObject::set_property(&ev, "bubbles", JSValue::Boolean(bubbles));
        JSObject::set_property(&ev, "cancelable", JSValue::Boolean(cancelable));
        JSObject::set_property(&ev, "composed", JSValue::Boolean(composed));
        JSObject::set_property(&ev, "defaultPrevented", JSValue::Boolean(false));
        JSObject::set_property(&ev, "detail", detail);
        JSObject::set_property(&ev, "target", JSValue::Null);
        JSObject::set_property(&ev, "currentTarget", JSValue::Null);
        JSObject::set_property(&ev, "eventPhase", JSValue::Smi(0));
        JSObject::set_property(&ev, "timeStamp", JSValue::Number(current_timestamp()));
        JSObject::set_property(&ev, "__propagation_stopped__", JSValue::Boolean(false));
        JSObject::set_property(&ev, "__immediate_propagation_stopped__", JSValue::Boolean(false));

        Ok(JSValue::Object(ev))
    });

    let ctor_obj = JSObject::new_empty(Some(event_ctor));
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(prototype.clone()));
    JSObject::set_property(&prototype, "constructor", JSValue::Object(ctor_obj.clone()));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor));
    ctor_obj
}

/// Creates the `EventTarget.prototype` object.
pub fn create_event_target_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // EventTarget.prototype.addEventListener(type, listener, options?)
    JSObject::set_property(
        &proto,
        "addEventListener",
        JSValue::Function(JSFunction::new_closure("addEventListener", |_this, args| {
            let target = match _this {
                JSValue::Object(ref o) => o.clone(),
                _ => return Err("TypeError: addEventListener called on non-object".to_string()),
            };

            let event_type = args.first().map(|v| v.to_string_val()).unwrap_or_default();
            let listener = match args.get(1) {
                Some(JSValue::Function(f)) => Some(JSValue::Function(f.clone())),
                Some(JSValue::Object(o)) => Some(JSValue::Object(o.clone())),
                _ => None,
            };

            if let Some(l) = listener {
                let listeners_val = JSObject::get_property(&target.borrow(), "__listeners__");
                let listeners_obj = match listeners_val {
                    JSValue::Object(o) => o,
                    _ => {
                        let new_obj = JSObject::new_empty(None);
                        JSObject::set_property(&target, "__listeners__", JSValue::Object(new_obj.clone()));
                        new_obj
                    }
                };

                let type_arr_val = JSObject::get_property(&listeners_obj.borrow(), &event_type);
                let list_arr = match type_arr_val {
                    JSValue::Array(a) => a,
                    _ => {
                        let new_arr = JSArray::new_array(Vec::new());
                        JSObject::set_property(&listeners_obj, &event_type, JSValue::Array(new_arr.clone()));
                        new_arr
                    }
                };

                // Check options: once, capture
                let mut once = false;
                let mut capture = false;
                if let Some(JSValue::Object(opt)) = args.get(2) {
                    if let JSValue::Boolean(b) = JSObject::get_property(&opt.borrow(), "once") {
                        once = b;
                    }
                    if let JSValue::Boolean(b) = JSObject::get_property(&opt.borrow(), "capture") {
                        capture = b;
                    }
                } else if let Some(JSValue::Boolean(b)) = args.get(2) {
                    capture = *b;
                }

                let listener_record = JSObject::new_empty(None);
                JSObject::set_property(&listener_record, "callback", l);
                JSObject::set_property(&listener_record, "once", JSValue::Boolean(once));
                JSObject::set_property(&listener_record, "capture", JSValue::Boolean(capture));

                list_arr.borrow_mut().elements.push(JSValue::Object(listener_record));
            }

            Ok(JSValue::Undefined)
        })),
    );

    // EventTarget.prototype.removeEventListener(type, listener, options?)
    JSObject::set_property(
        &proto,
        "removeEventListener",
        JSValue::Function(JSFunction::new_closure("removeEventListener", |_this, args| {
            let target = match _this {
                JSValue::Object(ref o) => o.clone(),
                _ => return Err("TypeError: removeEventListener called on non-object".to_string()),
            };

            let event_type = args.first().map(|v| v.to_string_val()).unwrap_or_default();
            let listener = args.get(1);

            let listeners_val = JSObject::get_property(&target.borrow(), "__listeners__");
            if let JSValue::Object(listeners_obj) = listeners_val {
                let type_arr_val = JSObject::get_property(&listeners_obj.borrow(), &event_type);
                if let JSValue::Array(list_arr) = type_arr_val {
                    if let Some(target_l) = listener {
                        list_arr.borrow_mut().elements.retain(|item| {
                            if let JSValue::Object(rec) = item {
                                let cb = JSObject::get_property(&rec.borrow(), "callback");
                                &cb != target_l
                            } else {
                                true
                            }
                        });
                    }
                }
            }

            Ok(JSValue::Undefined)
        })),
    );

    // EventTarget.prototype.dispatchEvent(event)
    JSObject::set_property(
        &proto,
        "dispatchEvent",
        JSValue::Function(JSFunction::new_closure("dispatchEvent", |_this, args| {
            let target = match _this {
                JSValue::Object(ref o) => o.clone(),
                _ => return Err("TypeError: dispatchEvent called on non-object".to_string()),
            };

            let event = match args.first() {
                Some(JSValue::Object(ref e)) => e.clone(),
                _ => return Err("TypeError: dispatchEvent requires an Event object".to_string()),
            };

            let event_type = match JSObject::get_property(&event.borrow(), "type") {
                JSValue::String(s) => s,
                _ => String::new(),
            };

            // Set event target and phase
            JSObject::set_property(&event, "target", JSValue::Object(target.clone()));
            JSObject::set_property(&event, "currentTarget", JSValue::Object(target.clone()));
            JSObject::set_property(&event, "eventPhase", JSValue::Smi(2)); // AT_TARGET

            let listeners_val = JSObject::get_property(&target.borrow(), "__listeners__");
            if let JSValue::Object(listeners_obj) = listeners_val {
                let type_arr_val = JSObject::get_property(&listeners_obj.borrow(), &event_type);
                if let JSValue::Array(list_arr) = type_arr_val {
                    let mut elements = list_arr.borrow().elements.clone();
                    let mut surviving = Vec::new();

                    for item in elements.drain(..) {
                        let is_immediate_stopped = match JSObject::get_property(&event.borrow(), "__immediate_propagation_stopped__") {
                            JSValue::Boolean(b) => b,
                            _ => false,
                        };
                        if is_immediate_stopped {
                            surviving.push(item);
                            continue;
                        }

                        if let JSValue::Object(rec) = &item {
                            let cb = JSObject::get_property(&rec.borrow(), "callback");
                            let once = match JSObject::get_property(&rec.borrow(), "once") {
                                JSValue::Boolean(b) => b,
                                _ => false,
                            };

                            match cb {
                                JSValue::Function(f) => {
                                    let _ = f.call(&JSValue::Object(target.clone()), &[JSValue::Object(event.clone())]);
                                }
                                JSValue::Object(handler) => {
                                    let handle_fn = JSObject::get_property(&handler.borrow(), "handleEvent");
                                    if let JSValue::Function(f) = handle_fn {
                                        let _ = f.call(&JSValue::Object(handler), &[JSValue::Object(event.clone())]);
                                    }
                                }
                                _ => {}
                            }

                            if !once {
                                surviving.push(item);
                            }
                        }
                    }

                    list_arr.borrow_mut().elements = surviving;
                }
            }

            // Reset currentTarget and phase
            JSObject::set_property(&event, "currentTarget", JSValue::Null);
            JSObject::set_property(&event, "eventPhase", JSValue::Smi(0));

            let def_prevented = match JSObject::get_property(&event.borrow(), "defaultPrevented") {
                JSValue::Boolean(b) => b,
                _ => false,
            };

            Ok(JSValue::Boolean(!def_prevented))
        })),
    );

    proto
}

/// Creates the `EventTarget` constructor.
pub fn create_event_target_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let proto_clone = prototype.clone();

    let ctor = JSFunction::new_closure("EventTarget", move |_this, _args| {
        let target = JSObject::new_empty(Some(proto_clone.clone()));
        let listeners_map = JSObject::new_empty(None);
        JSObject::set_property(&target, "__listeners__", JSValue::Object(listeners_map));
        Ok(JSValue::Object(target))
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(prototype.clone()));
    JSObject::set_property(&prototype, "constructor", JSValue::Object(ctor_obj.clone()));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor));
    ctor_obj
}
