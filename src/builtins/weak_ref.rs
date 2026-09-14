//! Safe Rust reimplementation of ECMAScript WeakRef and FinalizationRegistry built-ins.
//!
//! Provides non-leaking weak references using `std::rc::Weak` and cleanup callback registries.

use crate::objects::js_object::JSObject;
use crate::objects::value::JSValue;
use crate::objects::JSFunction;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

thread_local! {
    static WEAK_REF_STORAGE: RefCell<Vec<(usize, Weak<RefCell<JSObject>>)>> = const { RefCell::new(Vec::new()) };
    static NEXT_WEAK_ID: RefCell<usize> = const { RefCell::new(1) };
}

fn register_weak(obj: &Rc<RefCell<JSObject>>) -> usize {
    NEXT_WEAK_ID.with(|id_cell| {
        let mut id = id_cell.borrow_mut();
        let cur = *id;
        *id += 1;
        WEAK_REF_STORAGE.with(|storage| {
            storage.borrow_mut().push((cur, Rc::downgrade(obj)));
        });
        cur
    })
}

fn deref_weak(id: usize) -> Option<Rc<RefCell<JSObject>>> {
    WEAK_REF_STORAGE.with(|storage| {
        let borrowed = storage.borrow();
        for (w_id, weak) in borrowed.iter() {
            if *w_id == id {
                return weak.upgrade();
            }
        }
        None
    })
}

/// Creates the `WeakRef` constructor and prototype.
pub fn create_weak_ref_constructor() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // WeakRef.prototype.deref()
    let deref_fn = JSFunction::new_native("deref", |this, _args| {
        if let JSValue::Object(ref o) = this {
            let id_val = o.borrow().get_property("__weak_id__");
            if let JSValue::Smi(id) = id_val {
                if let Some(target) = deref_weak(id as usize) {
                    return Ok(JSValue::Object(target));
                }
            }
        }
        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&proto, "deref", JSValue::Function(deref_fn));

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(proto.clone()));

    let proto_clone = proto.clone();
    let ctor_fn = JSFunction::new_closure("WeakRef", move |_this, args| {
        let target = match args.first() {
            Some(JSValue::Object(o)) => o.clone(),
            _ => return Err("TypeError: WeakRef: target must be an object".to_string()),
        };

        let weak_id = register_weak(&target);
        let instance = JSObject::new_empty(Some(proto_clone.clone()));
        JSObject::set_property(&instance, "__weak_id__", JSValue::Smi(weak_id as i32));
        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor_fn));
    JSObject::set_property(&proto, "constructor", JSValue::Object(ctor_obj.clone()));

    ctor_obj
}

/// Creates the `FinalizationRegistry` constructor and prototype.
pub fn create_finalization_registry_constructor() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // FinalizationRegistry.prototype.register(target, heldValue, unregisterToken)
    let register_fn = JSFunction::new_native("register", |this, args| {
        let target_val = args.first().cloned().unwrap_or(JSValue::Undefined);
        if !matches!(target_val, JSValue::Object(_)) {
            return Err("TypeError: FinalizationRegistry.register: target must be an object".to_string());
        }

        let held_value = args.get(1).cloned().unwrap_or(JSValue::Undefined);
        let token = args.get(2).cloned().unwrap_or(JSValue::Undefined);

        if let JSValue::Object(ref o) = this {
            let cells_prop = o.borrow().get_property("__cells__");
            if let JSValue::Array(ref arr) = cells_prop {
                let entry = crate::objects::js_array::JSArray::new_array(vec![
                    target_val,
                    held_value,
                    token,
                ]);
                arr.borrow_mut().elements.push(JSValue::Array(entry));
            }
        }

        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&proto, "register", JSValue::Function(register_fn));

    // FinalizationRegistry.prototype.unregister(unregisterToken)
    let unregister_fn = JSFunction::new_native("unregister", |this, args| {
        let token = args.first().cloned().unwrap_or(JSValue::Undefined);
        if matches!(token, JSValue::Undefined | JSValue::Null) {
            return Err("TypeError: FinalizationRegistry.unregister: token must be an object or symbol".to_string());
        }

        let mut removed = false;
        if let JSValue::Object(ref o) = this {
            let cells_prop = o.borrow().get_property("__cells__");
            if let JSValue::Array(ref arr) = cells_prop {
                let mut borrowed = arr.borrow_mut();
                let initial_len = borrowed.elements.len();
                borrowed.elements.retain(|item| {
                    if let JSValue::Array(ref entry) = item {
                        let t = entry.borrow().elements.get(2).cloned().unwrap_or(JSValue::Undefined);
                        t != token
                    } else {
                        true
                    }
                });
                removed = borrowed.elements.len() < initial_len;
            }
        }

        Ok(JSValue::Boolean(removed))
    });
    JSObject::set_property(&proto, "unregister", JSValue::Function(unregister_fn));

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(proto.clone()));

    let proto_clone = proto.clone();
    let ctor_fn = JSFunction::new_closure("FinalizationRegistry", move |_this, args| {
        let callback = match args.first() {
            Some(JSValue::Function(f)) => f.clone(),
            _ => return Err("TypeError: FinalizationRegistry: cleanup callback must be a function".to_string()),
        };

        let instance = JSObject::new_empty(Some(proto_clone.clone()));
        JSObject::set_property(&instance, "__cleanup_callback__", JSValue::Function(callback));
        JSObject::set_property(
            &instance,
            "__cells__",
            JSValue::Array(crate::objects::js_array::JSArray::new_array(Vec::new())),
        );
        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor_fn));
    JSObject::set_property(&proto, "constructor", JSValue::Object(ctor_obj.clone()));

    ctor_obj
}
