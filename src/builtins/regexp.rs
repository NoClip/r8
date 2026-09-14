//! Safe Rust reimplementation of Google V8's ECMAScript `RegExp` built-in constructor and prototype.
//!
//! Implements RegExp instantiation, execution (`exec`, `test`), flag getters,
//! string representations, and stateful `lastIndex` management.

use crate::objects::{InstanceType, JSArray, JSFunction, JSObject, JSValue, Map, RegExpData};
use crate::regexp::{RegExpEngine, RegExpFlags};
use std::cell::RefCell;
use std::rc::Rc;

/// Creates a new `RegExp` instance initialized with the given pattern and flags.
pub fn new_regexp_instance(
    pattern: &str,
    flags_str: &str,
    prototype: Option<Rc<RefCell<JSObject>>>,
) -> Result<Rc<RefCell<JSObject>>, String> {
    let bytecode = RegExpEngine::compile(pattern, flags_str)?;
    let flags = RegExpFlags::parse(flags_str)?;
    let canonical_flags = flags.to_string_canonical();

    let target_proto = prototype.or_else(|| {
        crate::runtime::context::current_global().and_then(|g| {
            match g.borrow().get_property("RegExp") {
                JSValue::Object(ref ctor) => match ctor.borrow().get_property("prototype") {
                    JSValue::Object(ref p) => Some(p.clone()),
                    _ => None,
                },
                _ => None,
            }
        })
    });

    let map = Map::root(InstanceType::JSRegExp, target_proto);
    let re_data = RegExpData {
        pattern: pattern.to_string(),
        flags: canonical_flags.clone(),
        global: flags.global,
        ignore_case: flags.ignore_case,
        multiline: flags.multiline,
        dot_all: flags.dot_all,
        unicode: flags.unicode,
        unicode_sets: flags.unicode_sets,
        sticky: flags.sticky,
        has_indices: flags.has_indices,
        last_index: 0,
        capture_count: bytecode.capture_count,
        named_groups: bytecode.named_groups.clone(),
        bytecode: Rc::new(bytecode),
    };

    let obj = JSObject::new_with_map(map);
    obj.borrow_mut().ext_mut().regexp_data = Some(re_data);

    // Initial properties for direct inspection
    JSObject::set_property(&obj, "source", JSValue::String(pattern.to_string()));
    JSObject::set_property(&obj, "flags", JSValue::String(canonical_flags));
    JSObject::set_property(&obj, "global", JSValue::Boolean(flags.global));
    JSObject::set_property(&obj, "ignoreCase", JSValue::Boolean(flags.ignore_case));
    JSObject::set_property(&obj, "multiline", JSValue::Boolean(flags.multiline));
    JSObject::set_property(&obj, "dotAll", JSValue::Boolean(flags.dot_all));
    JSObject::set_property(&obj, "unicode", JSValue::Boolean(flags.unicode));
    JSObject::set_property(&obj, "unicodeSets", JSValue::Boolean(flags.unicode_sets));
    JSObject::set_property(&obj, "sticky", JSValue::Boolean(flags.sticky));
    JSObject::set_property(&obj, "hasIndices", JSValue::Boolean(flags.has_indices));
    JSObject::set_property(&obj, "lastIndex", JSValue::Smi(0));

    Ok(obj)
}

/// Creates the `RegExp.prototype` object equipped with ECMAScript RegExp methods and accessors.
pub fn create_regexp_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // RegExp.prototype.exec(string)
    JSObject::set_property(
        &proto,
        "exec",
        JSValue::Function(JSFunction::new_native("exec", |this, args| {
            let re_rc = match this {
                JSValue::Object(ref obj) if obj.borrow().ext_or_default().regexp_data.is_some() => obj.clone(),
                _ => return Err("TypeError: RegExp.prototype.exec called on incompatible receiver".to_string()),
            };

            let subject = args
                .first()
                .map(|v| v.to_string_val())
                .unwrap_or_else(|| "undefined".to_string());

            let (is_stateful, start_pos) = {
                let borrowed = re_rc.borrow();
                let data = borrowed.ext_or_default().regexp_data.as_ref().unwrap();
                let stateful = data.global || data.sticky;
                let start = if stateful { data.last_index } else { 0 };
                (stateful, start)
            };

            let subject_char_count = subject.chars().count();
            if is_stateful && start_pos > subject_char_count {
                re_rc.borrow_mut().ext_mut().regexp_data.as_mut().unwrap().last_index = 0;
                return Ok(JSValue::Null);
            }

            let bytecode_rc = {
                let borrowed = re_rc.borrow();
                borrowed.ext_or_default().regexp_data.as_ref().unwrap().bytecode.clone()
            };

            let match_result = RegExpEngine::exec(&bytecode_rc, &subject, start_pos);

            match match_result {
                Some(m) => {
                    if is_stateful {
                        re_rc.borrow_mut().ext_mut().regexp_data.as_mut().unwrap().last_index = m.end;
                    }

                    // Build result JSArray
                    let elements: Vec<JSValue> = m
                        .captures
                        .into_iter()
                        .map(|c| match c {
                            Some(s) => JSValue::String(s),
                            None => JSValue::Undefined,
                        })
                        .collect();

                    let array_rc = JSArray::new_array(elements);
                    JSObject::set_property(&array_rc, "index", JSValue::Smi(m.start as i32));
                    JSObject::set_property(&array_rc, "input", JSValue::String(subject));

                    if !m.named_groups.is_empty() {
                        let groups_rc = JSObject::new_empty(None);
                        for (name, val) in m.named_groups {
                            JSObject::set_property(&groups_rc, &name, JSValue::String(val));
                        }
                        JSObject::set_property(&array_rc, "groups", JSValue::Object(groups_rc));
                    } else {
                        JSObject::set_property(&array_rc, "groups", JSValue::Undefined);
                    }

                    let (has_indices, named_groups_map) = {
                        let borrowed = re_rc.borrow();
                        let data = borrowed.ext_or_default().regexp_data.as_ref().unwrap();
                        (data.has_indices, data.named_groups.clone())
                    };

                    if has_indices {
                        let mut indices_elements = Vec::new();
                        for idx_opt in &m.capture_indices {
                            match idx_opt {
                                Some((s, e)) => {
                                    let pair = JSArray::new_array(vec![
                                        JSValue::Smi(*s as i32),
                                        JSValue::Smi(*e as i32),
                                    ]);
                                    indices_elements.push(JSValue::Array(pair));
                                }
                                None => {
                                    indices_elements.push(JSValue::Undefined);
                                }
                            }
                        }
                        let indices_array = JSArray::new_array(indices_elements);

                        if !named_groups_map.is_empty() {
                            let groups_indices = JSObject::new_empty(None);
                            for (name, &idx) in &named_groups_map {
                                if idx < m.capture_indices.len() {
                                    if let Some((s, e)) = m.capture_indices[idx] {
                                        let pair = JSArray::new_array(vec![
                                            JSValue::Smi(s as i32),
                                            JSValue::Smi(e as i32),
                                        ]);
                                        JSObject::set_property(&groups_indices, name, JSValue::Array(pair));
                                    } else {
                                        JSObject::set_property(&groups_indices, name, JSValue::Undefined);
                                    }
                                }
                            }
                            JSObject::set_property(&indices_array, "groups", JSValue::Object(groups_indices));
                        } else {
                            JSObject::set_property(&indices_array, "groups", JSValue::Undefined);
                        }

                        JSObject::set_property(&array_rc, "indices", JSValue::Array(indices_array));
                    }

                    Ok(JSValue::Array(array_rc))
                }
                None => {
                    if is_stateful {
                        re_rc.borrow_mut().ext_mut().regexp_data.as_mut().unwrap().last_index = 0;
                    }
                    Ok(JSValue::Null)
                }
            }
        })),
    );

    // RegExp.prototype.test(string)
    JSObject::set_property(
        &proto,
        "test",
        JSValue::Function(JSFunction::new_native("test", |this, args| {
            let re_rc = match this {
                JSValue::Object(ref obj) if obj.borrow().ext_or_default().regexp_data.is_some() => obj.clone(),
                _ => return Err("TypeError: RegExp.prototype.test called on incompatible receiver".to_string()),
            };

            let subject = args
                .first()
                .map(|v| v.to_string_val())
                .unwrap_or_else(|| "undefined".to_string());

            let (is_stateful, start_pos) = {
                let borrowed = re_rc.borrow();
                let data = borrowed.ext_or_default().regexp_data.as_ref().unwrap();
                let stateful = data.global || data.sticky;
                let start = if stateful { data.last_index } else { 0 };
                (stateful, start)
            };

            let subject_char_count = subject.chars().count();
            if is_stateful && start_pos > subject_char_count {
                re_rc.borrow_mut().ext_mut().regexp_data.as_mut().unwrap().last_index = 0;
                return Ok(JSValue::Boolean(false));
            }

            let bytecode_rc = {
                let borrowed = re_rc.borrow();
                borrowed.ext_or_default().regexp_data.as_ref().unwrap().bytecode.clone()
            };

            let match_result = RegExpEngine::exec(&bytecode_rc, &subject, start_pos);

            match match_result {
                Some(m) => {
                    if is_stateful {
                        re_rc.borrow_mut().ext_mut().regexp_data.as_mut().unwrap().last_index = m.end;
                    }
                    Ok(JSValue::Boolean(true))
                }
                None => {
                    if is_stateful {
                        re_rc.borrow_mut().ext_mut().regexp_data.as_mut().unwrap().last_index = 0;
                    }
                    Ok(JSValue::Boolean(false))
                }
            }
        })),
    );

    // RegExp.prototype.toString()
    JSObject::set_property(
        &proto,
        "toString",
        JSValue::Function(JSFunction::new_native("toString", |this, _args| {
            if let JSValue::Object(ref obj) = this {
                if let Some(ref data) = obj.borrow().ext_or_default().regexp_data {
                    return Ok(JSValue::String(format!("/{}/{}", data.pattern, data.flags)));
                }
            }
            Ok(JSValue::String("/(?:)/".to_string()))
        })),
    );

    proto
}

/// Creates the `RegExp` constructor function and associates it with `RegExp.prototype`.
pub fn create_regexp_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let proto_clone = prototype.clone();
    let ctor_fn = JSFunction::new_closure("RegExp", move |_this, args| {
        let pattern = args
            .first()
            .map(|v| match v {
                JSValue::Object(obj) if obj.borrow().ext_or_default().regexp_data.is_some() => {
                    obj.borrow().ext_or_default().regexp_data.as_ref().unwrap().pattern.clone()
                }
                _ => v.to_string_val(),
            })
            .unwrap_or_default();

        let flags = args
            .get(1)
            .map(|v| v.to_string_val())
            .unwrap_or_default();

        let instance = new_regexp_instance(&pattern, &flags, Some(proto_clone.clone()))?;
        Ok(JSValue::Object(instance))
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(prototype));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor_fn));

    ctor_obj
}
