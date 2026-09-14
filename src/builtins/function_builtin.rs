//! Safe Rust reimplementation of Google V8's ECMAScript `Function` constructor and prototype.
//!
//! Implements `Function.prototype.call`, `apply`, `bind`, `toString`,
//! and dynamic code generation via `new Function(...)`.

use crate::ast::Statement;
use crate::interpreter::bytecode_generator::BytecodeGenerator;
use crate::objects::{JSFunction, JSObject, JSValue};
use crate::parsing::Parser;
use std::cell::RefCell;
use std::rc::Rc;

/// Creates the `Function.prototype` object.
pub fn create_function_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);
    JSObject::set_property(&proto, "name", JSValue::String("Function".to_string()));

    // Function.prototype.call(thisArg, ...args)
    let call_fn = JSFunction::new_native("call", |this, args| {
        let receiver = args.first().unwrap_or(&JSValue::Undefined);
        let call_args = if args.len() > 1 { &args[1..] } else { &[] };

        match this {
            JSValue::Function(f) => f.call(receiver, call_args),
            JSValue::Object(obj) => {
                let call_prop = obj.borrow().get_property("__call__");
                if let JSValue::Function(f) = call_prop {
                    f.call(receiver, call_args)
                } else {
                    Err("TypeError: Function.prototype.call called on non-function".to_string())
                }
            }
            _ => Err("TypeError: Function.prototype.call called on non-function".to_string()),
        }
    });
    JSObject::set_property(&proto, "call", JSValue::Function(call_fn));

    // Function.prototype.apply(thisArg, argArray?)
    let apply_fn = JSFunction::new_native("apply", |this, args| {
        let receiver = args.first().unwrap_or(&JSValue::Undefined);
        let arg_array = args.get(1);

        let call_args: Vec<JSValue> = match arg_array {
            Some(JSValue::Array(arr)) => arr.borrow().elements.clone(),
            Some(JSValue::Object(obj)) => {
                let borrowed = obj.borrow();
                let len = borrowed.get_property("length").to_number() as usize;
                let mut vals = Vec::with_capacity(len);
                for i in 0..len {
                    vals.push(borrowed.get_property(&i.to_string()));
                }
                vals
            }
            Some(JSValue::Undefined) | Some(JSValue::Null) | None => Vec::new(),
            _ => return Err("TypeError: CreateListFromArrayLike called on non-object".to_string()),
        };

        match this {
            JSValue::Function(f) => f.call(receiver, &call_args),
            JSValue::Object(obj) => {
                let call_prop = obj.borrow().get_property("__call__");
                if let JSValue::Function(f) = call_prop {
                    f.call(receiver, &call_args)
                } else {
                    Err("TypeError: Function.prototype.apply called on non-function".to_string())
                }
            }
            _ => Err("TypeError: Function.prototype.apply called on non-function".to_string()),
        }
    });
    JSObject::set_property(&proto, "apply", JSValue::Function(apply_fn));

    // Function.prototype.bind(thisArg, ...boundArgs)
    let bind_fn = JSFunction::new_native("bind", |this, args| {
        let bound_this = args.first().cloned().unwrap_or(JSValue::Undefined);
        let bound_args = if args.len() > 1 { args[1..].to_vec() } else { Vec::new() };

        match this {
            JSValue::Function(f) => {
                let target_fn = f.clone();
                let bound_closure = JSFunction::new_closure("bound", move |_recv, extra_args| {
                    let mut combined = bound_args.clone();
                    combined.extend_from_slice(extra_args);
                    target_fn.call(&bound_this, &combined)
                });
                Ok(JSValue::Function(bound_closure))
            }
            JSValue::Object(obj) => {
                let call_prop = obj.borrow().get_property("__call__");
                if let JSValue::Function(f) = call_prop {
                    let target_fn = f.clone();
                    let bound_closure = JSFunction::new_closure("bound", move |_recv, extra_args| {
                        let mut combined = bound_args.clone();
                        combined.extend_from_slice(extra_args);
                        target_fn.call(&bound_this, &combined)
                    });
                    Ok(JSValue::Function(bound_closure))
                } else {
                    Err("TypeError: Function.prototype.bind called on non-function".to_string())
                }
            }
            _ => Err("TypeError: Function.prototype.bind called on non-function".to_string()),
        }
    });
    JSObject::set_property(&proto, "bind", JSValue::Function(bind_fn));

    // Function.prototype.toString()
    let to_string_fn = JSFunction::new_native("toString", |this, _args| {
        match this {
            JSValue::Function(f) => Ok(JSValue::String(format!("function {}() {{ [native code] }}", f.name))),
            JSValue::Object(obj) => {
                let name = obj.borrow().get_property("name").to_string_val();
                Ok(JSValue::String(format!("function {}() {{ [native code] }}", name)))
            }
            _ => Err("TypeError: Function.prototype.toString requires that 'this' be a Function".to_string()),
        }
    });
    JSObject::set_property(&proto, "toString", JSValue::Function(to_string_fn));

    proto
}

/// Creates the global `Function` constructor object.
pub fn create_function_constructor(prototype: &Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let ctor = JSObject::new_empty(None);
    JSObject::set_property(&ctor, "prototype", JSValue::Object(prototype.clone()));
    JSObject::set_property(prototype, "constructor", JSValue::Object(ctor.clone()));
    JSObject::set_property(&ctor, "name", JSValue::String("Function".to_string()));

    // Invocation: new Function(...args, body)
    let ctor_fn = JSFunction::new_native("Function", |_this, args| {
        let (params_str, body_str) = if args.is_empty() {
            ("".to_string(), "".to_string())
        } else if args.len() == 1 {
            ("".to_string(), args[0].to_string_val())
        } else {
            let params: Vec<String> = args[..args.len() - 1].iter().map(|a| a.to_string_val()).collect();
            let body = args.last().unwrap().to_string_val();
            (params.join(", "), body)
        };

        let func_source = format!("function anonymous({}) {{\n{}\n}}", params_str, body_str);
        let mut parser = Parser::new(&func_source);
        let program = parser.parse_program().map_err(|e| e.to_string())?;

        for stmt in program.statements {
            if let Statement::FunctionDeclaration { name, params, body, .. } = stmt {
                let bc = Rc::new(BytecodeGenerator::compile_named_function(&name, &params, &body));
                let func = JSFunction::new_bytecode(&name, bc);
                return Ok(JSValue::Function(func));
            }
        }

        // Fallback no-op if no function declaration parsed
        let noop = JSFunction::new_native("anonymous", |_this, _args| Ok(JSValue::Undefined));
        Ok(JSValue::Function(noop))
    });
    JSObject::set_property(&ctor, "__call__", JSValue::Function(ctor_fn));

    ctor
}
