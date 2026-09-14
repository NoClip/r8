//! Safe Rust reimplementation of Google V8's ECMAScript `Symbol` built-in and primitive.
//!
//! Implements unique primitive symbol values, global symbol registry (`Symbol.for`, `Symbol.keyFor`),
//! and well-known symbols (`Symbol.iterator`, `Symbol.toStringTag`, etc.).

use crate::objects::{JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_SYMBOL_ID: AtomicU32 = AtomicU32::new(1);

thread_local! {
    static REGISTRY: RefCell<HashMap<String, u32>> = RefCell::new(HashMap::new());
}

/// Allocates a new unique symbol primitive value.
pub fn new_symbol(description: Option<String>) -> JSValue {
    let id = NEXT_SYMBOL_ID.fetch_add(1, Ordering::Relaxed);
    JSValue::Symbol(Rc::new(crate::objects::value::SymbolData { id, description }))
}

/// Creates the `Symbol.prototype` object.
pub fn create_symbol_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // Symbol.prototype.toString()
    JSObject::set_property(
        &proto,
        "toString",
        JSValue::Function(JSFunction::new_native("toString", |this, _args| {
            if let JSValue::Symbol(ref sym) = this {
                match &sym.description {
                    Some(d) => Ok(JSValue::String(format!("Symbol({})", d))),
                    None => Ok(JSValue::String("Symbol()".to_string())),
                }
            } else {
                Err("TypeError: Symbol.prototype.toString requires that 'this' be a Symbol".to_string())
            }
        })),
    );

    // Symbol.prototype.description getter
    JSObject::set_property(
        &proto,
        "__get_description__",
        JSValue::Function(JSFunction::new_native("description", |this, _args| {
            if let JSValue::Symbol(ref sym) = this {
                match &sym.description {
                    Some(d) => Ok(JSValue::String(d.clone())),
                    None => Ok(JSValue::Undefined),
                }
            } else {
                Err("TypeError: Symbol.prototype.description requires that 'this' be a Symbol".to_string())
            }
        })),
    );

    // Symbol.prototype.valueOf()
    JSObject::set_property(
        &proto,
        "valueOf",
        JSValue::Function(JSFunction::new_native("valueOf", |this, _args| {
            if let JSValue::Symbol(_) = this {
                Ok(this.clone())
            } else {
                Err("TypeError: Symbol.prototype.valueOf requires that 'this' be a Symbol".to_string())
            }
        })),
    );

    proto
}

/// Allocates the global `Symbol` constructor and factory object.
pub fn create_symbol_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let sym_ctor = JSObject::new_empty(None);
    JSObject::set_property(&sym_ctor, "prototype", JSValue::Object(prototype));
    JSObject::set_property(&sym_ctor, "name", JSValue::String("Symbol".to_string()));
    JSObject::set_property(&sym_ctor, "__not_constructor__", JSValue::Boolean(true));

    // Symbol.for(key)
    JSObject::set_property(
        &sym_ctor,
        "for",
        JSValue::Function(JSFunction::new_native("for", |_this, args| {
            let key = args.first().map(|v| v.to_string_val()).unwrap_or_else(|| "undefined".to_string());
            let id = REGISTRY.with(|r| {
                let mut map = r.borrow_mut();
                if let Some(&existing_id) = map.get(&key) {
                    existing_id
                } else {
                    let new_id = NEXT_SYMBOL_ID.fetch_add(1, Ordering::Relaxed);
                    map.insert(key.clone(), new_id);
                    new_id
                }
            });
            Ok(JSValue::Symbol(Rc::new(crate::objects::value::SymbolData { id, description: Some(key) })))
        })),
    );

    // Symbol.keyFor(sym)
    JSObject::set_property(
        &sym_ctor,
        "keyFor",
        JSValue::Function(JSFunction::new_native("keyFor", |_this, args| {
            if let Some(JSValue::Symbol(sym)) = args.first() {
                let key_opt = REGISTRY.with(|r| {
                    let map = r.borrow();
                    for (k, &sym_id) in map.iter() {
                        if sym_id == sym.id {
                            return Some(k.clone());
                        }
                    }
                    None
                });
                match key_opt {
                    Some(k) => Ok(JSValue::String(k)),
                    None => Ok(JSValue::Undefined),
                }
            } else {
                Err("TypeError: Symbol.keyFor requires a symbol argument".to_string())
            }
        })),
    );

    // Well-known symbols
    let iterator_sym = new_symbol(Some("Symbol.iterator".to_string()));
    let to_string_tag_sym = new_symbol(Some("Symbol.toStringTag".to_string()));
    let has_instance_sym = new_symbol(Some("Symbol.hasInstance".to_string()));
    let to_primitive_sym = new_symbol(Some("Symbol.toPrimitive".to_string()));
    let is_concat_spreadable_sym = new_symbol(Some("Symbol.isConcatSpreadable".to_string()));
    let species_sym = new_symbol(Some("Symbol.species".to_string()));
    let dispose_sym = new_symbol(Some("Symbol.dispose".to_string()));
    let async_dispose_sym = new_symbol(Some("Symbol.asyncDispose".to_string()));

    JSObject::set_property(&sym_ctor, "iterator", iterator_sym);
    JSObject::set_property(&sym_ctor, "toStringTag", to_string_tag_sym);
    JSObject::set_property(&sym_ctor, "hasInstance", has_instance_sym);
    JSObject::set_property(&sym_ctor, "toPrimitive", to_primitive_sym);
    JSObject::set_property(&sym_ctor, "isConcatSpreadable", is_concat_spreadable_sym);
    JSObject::set_property(&sym_ctor, "species", species_sym);
    JSObject::set_property(&sym_ctor, "dispose", dispose_sym);
    JSObject::set_property(&sym_ctor, "asyncDispose", async_dispose_sym);

    // Callable Symbol([desc]) function
    let sym_fn = JSFunction::new_native("Symbol", |this, args| {
        if let JSValue::Object(ref obj) = this {
            if obj.borrow().get_property("prototype") != JSValue::Undefined {
                return Err("TypeError: Symbol is not a constructor".to_string());
            }
        }
        let desc = args.first().and_then(|v| {
            if *v == JSValue::Undefined {
                None
            } else {
                Some(v.to_string_val())
            }
        });
        Ok(new_symbol(desc))
    });

    JSObject::set_property(&sym_ctor, "__call__", JSValue::Function(sym_fn));
    sym_ctor
}
