//! Safe Rust reimplementation of Google V8's `Proxy` constructor.
//!
//! Creates Proxy instances that intercept fundamental operations on target objects.

use crate::objects::js_object::JSObject;
use crate::objects::value::JSValue;
use crate::objects::JSFunction;
use std::cell::RefCell;
use std::rc::Rc;

pub fn create_proxy_constructor() -> Rc<RefCell<JSObject>> {
    let ctor = JSFunction::new_native("Proxy", |_this, args| {
        let target = args.get(0).cloned().ok_or_else(|| "Cannot create proxy with non-object as target".to_string())?;
        let handler = args.get(1).cloned().ok_or_else(|| "Cannot create proxy with non-object as handler".to_string())?;

        match target {
            JSValue::Object(_) | JSValue::Array(_) | JSValue::Function(_) => {}
            _ => return Err("Cannot create proxy with a non-object as target".to_string()),
        }

        match handler {
            JSValue::Object(_) => {}
            _ => return Err("Cannot create proxy with a non-object as handler".to_string()),
        }

        Ok(JSValue::Object(JSObject::new_proxy(target, handler)))
    });

    let revocable_fn = JSFunction::new_native("revocable", |_this, args| {
        let target = args.get(0).cloned().ok_or_else(|| "Target required".to_string())?;
        let handler = args.get(1).cloned().ok_or_else(|| "Handler required".to_string())?;
        let proxy = JSObject::new_proxy(target, handler);

        let revoke_fn = JSFunction::new_native("revoke", move |_this, _args| {
            Ok(JSValue::Undefined)
        });

        let res = JSObject::new_empty(None);
        JSObject::set_property(&res, "proxy", JSValue::Object(proxy));
        JSObject::set_property(&res, "revoke", JSValue::Function(revoke_fn));

        Ok(JSValue::Object(res))
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "revocable", JSValue::Function(revocable_fn));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor));

    ctor_obj
}
