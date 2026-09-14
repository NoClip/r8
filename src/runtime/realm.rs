//! Safe Rust reimplementation of Google V8's D8 `Realm` API.
//!
//! Provides multi-realm isolation, cross-realm evaluation, and global environment management.

use super::context::Context;
use crate::objects::function::JSFunction;
use crate::objects::js_object::JSObject;
use crate::objects::value::JSValue;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

thread_local! {
    static REALM_MANAGER: RefCell<RealmManager> = RefCell::new(RealmManager::new());
}

/// Manager tracking active execution realms.
pub struct RealmManager {
    realms: HashMap<usize, Context>,
    next_id: usize,
    current_id: usize,
}

impl RealmManager {
    pub fn new() -> Self {
        Self {
            realms: HashMap::new(),
            next_id: 1,
            current_id: 0,
        }
    }

    pub fn create_realm(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let ctx = Context::new();
        self.realms.insert(id, ctx);
        id
    }

    pub fn current(&self) -> usize {
        self.current_id
    }

    pub fn set_current(&mut self, id: usize) {
        self.current_id = id;
    }

    pub fn get_realm(&mut self, id: usize) -> Option<&mut Context> {
        self.realms.get_mut(&id)
    }

    pub fn global(&self, id: usize) -> Option<Rc<RefCell<JSObject>>> {
        if id == 0 {
            crate::runtime::current_global()
        } else {
            self.realms.get(&id).map(|c| c.global_object.clone())
        }
    }

    pub fn eval(&mut self, id: usize, code: &str) -> Result<JSValue, String> {
        if id == 0 {
            if let Some(glob) = crate::runtime::current_global() {
                let mut ctx = Context::new();
                ctx.global_object = glob;
                ctx.eval(code)
            } else {
                let mut ctx = Context::new();
                ctx.eval(code)
            }
        } else if let Some(realm) = self.realms.get_mut(&id) {
            let prev_id = self.current_id;
            self.current_id = id;
            let res = realm.eval(code);
            self.current_id = prev_id;
            res
        } else {
            Err(format!("Invalid realm index: {}", id))
        }
    }

    pub fn dispose(&mut self, id: usize) -> bool {
        self.realms.remove(&id).is_some()
    }

    pub fn navigate(&mut self, id: usize) -> bool {
        if let Some(realm) = self.realms.get_mut(&id) {
            *realm = Context::new();
            true
        } else {
            false
        }
    }
}

/// Creates the standard D8 `Realm` object.
pub fn create_realm_object() -> Rc<RefCell<JSObject>> {
    let realm_obj = JSObject::new_empty(None);

    // Realm.create()
    let create_fn = JSFunction::new_native("create", |_this, _args| {
        let id = REALM_MANAGER.with(|m| m.borrow_mut().create_realm());
        Ok(JSValue::Smi(id as i32))
    });
    JSObject::set_property(&realm_obj, "create", JSValue::Function(create_fn));

    // Realm.createAllowCrossOrigin()
    let create_cross_fn = JSFunction::new_native("createAllowCrossOrigin", |_this, _args| {
        let id = REALM_MANAGER.with(|m| m.borrow_mut().create_realm());
        Ok(JSValue::Smi(id as i32))
    });
    JSObject::set_property(&realm_obj, "createAllowCrossOrigin", JSValue::Function(create_cross_fn));

    // Realm.current()
    let current_fn = JSFunction::new_native("current", |_this, _args| {
        let id = REALM_MANAGER.with(|m| m.borrow().current());
        Ok(JSValue::Smi(id as i32))
    });
    JSObject::set_property(&realm_obj, "current", JSValue::Function(current_fn));

    // Realm.global(id)
    let global_fn = JSFunction::new_native("global", |_this, args| {
        let id = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
        let glob = REALM_MANAGER.with(|m| m.borrow().global(id));
        match glob {
            Some(g) => Ok(JSValue::Object(g)),
            None => Ok(JSValue::Undefined),
        }
    });
    JSObject::set_property(&realm_obj, "global", JSValue::Function(global_fn));

    // Realm.eval(id, code)
    let eval_fn = JSFunction::new_native("eval", |_this, args| {
        let id = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
        let code = args.get(1).map(|v| v.to_string_val()).unwrap_or_default();
        REALM_MANAGER.with(|m| m.borrow_mut().eval(id, &code))
    });
    JSObject::set_property(&realm_obj, "eval", JSValue::Function(eval_fn));

    // Realm.dispose(id)
    let dispose_fn = JSFunction::new_native("dispose", |_this, args| {
        let id = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
        REALM_MANAGER.with(|m| m.borrow_mut().dispose(id));
        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&realm_obj, "dispose", JSValue::Function(dispose_fn));

    // Realm.navigate(id)
    let navigate_fn = JSFunction::new_native("navigate", |_this, args| {
        let id = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
        REALM_MANAGER.with(|m| m.borrow_mut().navigate(id));
        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&realm_obj, "navigate", JSValue::Function(navigate_fn));

    // Realm.owner(obj)
    let owner_fn = JSFunction::new_native("owner", |_this, _args| {
        Ok(JSValue::Smi(0))
    });
    JSObject::set_property(&realm_obj, "owner", JSValue::Function(owner_fn));

    realm_obj
}
