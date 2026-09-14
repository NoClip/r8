//! Safe Rust reimplementation of Google V8's `Isolate` and `Context` execution environment.
//!
//! Maintains the root global scope, prototypes, and built-in objects.

use crate::ast::{Program, Statement};
use crate::builtins::{
    create_aggregate_error_constructor, create_base_error_constructor,
    create_suppressed_error_constructor,
    create_array_buffer_constructor, create_array_buffer_prototype,
    create_array_constructor, create_array_prototype, create_atob_function, create_atomics_object,
    create_async_disposable_stack_constructor, create_async_disposable_stack_prototype,
    create_bigint_constructor, create_bigint_prototype,
    create_boolean_constructor, create_boolean_prototype, create_btoa_function,
    create_console_object, create_crypto_object,
    create_data_view_constructor, create_data_view_prototype,
    create_custom_event_constructor, create_custom_event_prototype,
    create_date_constructor, create_date_prototype,
    create_disposable_stack_constructor, create_disposable_stack_prototype,
    create_error_constructor, create_error_prototype,
    create_event_constructor, create_event_prototype,
    create_event_target_constructor, create_event_target_prototype,
    create_finalization_registry_constructor,
    create_function_constructor, create_function_prototype,
    create_intl_object, create_iterator_constructor, create_iterator_prototype, create_json_object,
    create_map_constructor, create_map_prototype, create_math_object,
    create_number_constructor, create_number_prototype,
    create_object_constructor, create_performance_object, create_promise_constructor, create_promise_prototype,
    create_proxy_constructor, create_queue_microtask_function,
    create_readable_stream_constructor, create_readable_stream_prototype,
    create_reflect_object,
    create_regexp_constructor, create_regexp_prototype, create_set_constructor,
    create_set_prototype, create_shared_array_buffer_constructor, create_shared_array_buffer_prototype,
    create_string_constructor, create_string_prototype, create_structured_clone_function,
    create_symbol_constructor, create_symbol_prototype,
    create_text_decoder_constructor, create_text_encoder_constructor,
    create_transform_stream_constructor, create_transform_stream_prototype,
    create_typed_array_constructor, create_typed_array_prototype,
    create_url_constructor, create_url_search_params_constructor,
    create_weak_map_constructor, create_weak_map_prototype,
    create_weak_ref_constructor, create_weak_set_constructor, create_weak_set_prototype,
    create_writable_stream_constructor, create_writable_stream_prototype,
    decode_uri_component_function, decode_uri_function,
    encode_uri_component_function, encode_uri_function,
    escape_function, is_finite_function, is_nan_function,
    new_promise_capability, parse_float_function, parse_int_function, unescape_function,
};
use crate::wasm::js_api::create_webassembly_object;
use crate::execution::{
    microtask_queue::MicrotaskQueue, timer_queue::TimerQueue, create_worker_constructor,
};
use crate::interpreter::bytecode_generator::BytecodeGenerator;
use crate::interpreter::interpreter::InterpreterVM;
use crate::objects::{JSFunction, JSObject, JSValue, TypedArrayKind};
use crate::parsing::Parser;
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static CURRENT_GLOBAL: RefCell<Option<Rc<RefCell<JSObject>>>> = const { RefCell::new(None) };
    static CURRENT_SUPER_STACK: RefCell<Vec<JSValue>> = const { RefCell::new(Vec::new()) };
}

/// Sets the active global object for the current thread.
pub fn set_current_global(global: Rc<RefCell<JSObject>>) {
    CURRENT_GLOBAL.with(|g| {
        *g.borrow_mut() = Some(global);
    });
}

/// Retrieves the active global object for the current thread if available.
pub fn current_global() -> Option<Rc<RefCell<JSObject>>> {
    CURRENT_GLOBAL.with(|g| g.borrow().clone())
}

/// Pushes a super reference onto the thread-local super stack.
pub fn push_current_super(val: JSValue) {
    CURRENT_SUPER_STACK.with(|s| s.borrow_mut().push(val));
}

/// Pops the top super reference from the thread-local super stack.
pub fn pop_current_super() -> Option<JSValue> {
    CURRENT_SUPER_STACK.with(|s| s.borrow_mut().pop())
}

/// Retrieves the active super reference for the current execution frame.
pub fn current_super() -> Option<JSValue> {
    CURRENT_SUPER_STACK.with(|s| s.borrow().last().cloned())
}

/// Execution realm containing the global environment, isolate microtask queue, and standard library prototypes.
#[derive(Clone)]
pub struct Context {
    pub global_object: Rc<RefCell<JSObject>>,
    pub array_prototype: Rc<RefCell<JSObject>>,
    pub string_prototype: Rc<RefCell<JSObject>>,
    pub date_prototype: Rc<RefCell<JSObject>>,
    pub promise_prototype: Rc<RefCell<JSObject>>,
    pub map_prototype: Rc<RefCell<JSObject>>,
    pub set_prototype: Rc<RefCell<JSObject>>,
    pub weak_map_prototype: Rc<RefCell<JSObject>>,
    pub weak_set_prototype: Rc<RefCell<JSObject>>,
    pub symbol_prototype: Rc<RefCell<JSObject>>,
    pub regexp_prototype: Rc<RefCell<JSObject>>,
    pub generator_prototype: Rc<RefCell<JSObject>>,
    pub microtask_queue: Rc<RefCell<MicrotaskQueue>>,
    pub timer_queue: Rc<RefCell<TimerQueue>>,
}

/// Alias matching V8's `Isolate` runtime container.
pub type Isolate = Context;

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    /// Creates a new execution context initialized with all standard built-ins.
    pub fn new() -> Self {
        let array_prototype = create_array_prototype();
        let string_prototype = create_string_prototype();
        let date_prototype = create_date_prototype();
        let promise_prototype = create_promise_prototype();
        let map_prototype = create_map_prototype();
        let set_prototype = create_set_prototype();
        let weak_map_prototype = create_weak_map_prototype();
        let weak_set_prototype = create_weak_set_prototype();
        let symbol_prototype = create_symbol_prototype();
        let regexp_prototype = create_regexp_prototype();
        let microtask_queue = Rc::new(RefCell::new(MicrotaskQueue::new()));
        let global_object = JSObject::new_empty(None);

        // globalThis self-reference
        JSObject::set_property(&global_object, "globalThis", JSValue::Object(global_object.clone()));

        // Special values
        JSObject::set_property(&global_object, "undefined", JSValue::Undefined);
        JSObject::set_property(&global_object, "NaN", JSValue::Number(f64::NAN));
        JSObject::set_property(&global_object, "Infinity", JSValue::Number(f64::INFINITY));

        // Built-in objects
        JSObject::set_property(&global_object, "Math", JSValue::Object(create_math_object()));
        JSObject::set_property(&global_object, "Object", JSValue::Object(create_object_constructor()));

        // Number, Boolean, Function
        let number_prototype = create_number_prototype();
        let number_ctor = create_number_constructor(&number_prototype);
        JSObject::set_property(&global_object, "Number", JSValue::Object(number_ctor));

        let boolean_prototype = create_boolean_prototype();
        let boolean_ctor = create_boolean_constructor(&boolean_prototype);
        JSObject::set_property(&global_object, "Boolean", JSValue::Object(boolean_ctor));

        let function_prototype = create_function_prototype();
        let function_ctor = create_function_constructor(&function_prototype);
        JSObject::set_property(&global_object, "Function", JSValue::Object(function_ctor));

        // Error hierarchy
        let error_prototype = create_error_prototype();
        let error_ctor = create_base_error_constructor(error_prototype.clone());
        JSObject::set_property(&global_object, "Error", JSValue::Object(error_ctor));

        for err_name in &["TypeError", "RangeError", "ReferenceError", "SyntaxError", "URIError", "EvalError"] {
            let p = JSObject::new_empty(Some(error_prototype.clone()));
            JSObject::set_property(&p, "name", JSValue::String(err_name.to_string()));
            JSObject::set_property(&p, "message", JSValue::String("".to_string()));
            let c = create_error_constructor(err_name, p);
            JSObject::set_property(&global_object, err_name, JSValue::Object(c));
        }

        let (agg_ctor, _agg_proto) = create_aggregate_error_constructor(error_prototype.clone());
        JSObject::set_property(&global_object, "AggregateError", JSValue::Object(agg_ctor));
        let (sup_ctor, _sup_proto) = create_suppressed_error_constructor(error_prototype.clone());
        JSObject::set_property(&global_object, "SuppressedError", JSValue::Object(sup_ctor));
        JSObject::set_property(
            &global_object,
            "Array",
            JSValue::Object(create_array_constructor(array_prototype.clone())),
        );
        JSObject::set_property(
            &global_object,
            "String",
            JSValue::Object(create_string_constructor(string_prototype.clone())),
        );
        JSObject::set_property(
            &global_object,
            "Date",
            JSValue::Object(create_date_constructor(date_prototype.clone())),
        );
        JSObject::set_property(&global_object, "JSON", JSValue::Object(create_json_object()));
        JSObject::set_property(&global_object, "console", JSValue::Object(create_console_object()));
        JSObject::set_property(
            &global_object,
            "Promise",
            JSValue::Object(create_promise_constructor(promise_prototype.clone())),
        );
        JSObject::set_property(
            &global_object,
            "Map",
            JSValue::Object(create_map_constructor(map_prototype.clone())),
        );
        JSObject::set_property(
            &global_object,
            "Set",
            JSValue::Object(create_set_constructor(set_prototype.clone())),
        );
        JSObject::set_property(
            &global_object,
            "WeakMap",
            JSValue::Object(create_weak_map_constructor(weak_map_prototype.clone())),
        );
        JSObject::set_property(
            &global_object,
            "WeakSet",
            JSValue::Object(create_weak_set_constructor(weak_set_prototype.clone())),
        );
        JSObject::set_property(
            &global_object,
            "Symbol",
            JSValue::Object(create_symbol_constructor(symbol_prototype.clone())),
        );
        JSObject::set_property(
            &global_object,
            "RegExp",
            JSValue::Object(create_regexp_constructor(regexp_prototype.clone())),
        );

        let disp_proto = create_disposable_stack_prototype();
        let disp_ctor = create_disposable_stack_constructor(disp_proto);
        JSObject::set_property(&global_object, "DisposableStack", JSValue::Object(disp_ctor));

        let async_disp_proto = create_async_disposable_stack_prototype();
        let async_disp_ctor = create_async_disposable_stack_constructor(async_disp_proto);
        JSObject::set_property(&global_object, "AsyncDisposableStack", JSValue::Object(async_disp_ctor));

        JSObject::set_property(
            &global_object,
            "WebAssembly",
            JSValue::Object(create_webassembly_object()),
        );

        // Phase 20: ArrayBuffer, DataView, and TypedArrays
        let array_buffer_prototype = create_array_buffer_prototype();
        let data_view_prototype = create_data_view_prototype();

        JSObject::set_property(
            &global_object,
            "ArrayBuffer",
            JSValue::Object(create_array_buffer_constructor(&array_buffer_prototype)),
        );
        JSObject::set_property(
            &global_object,
            "DataView",
            JSValue::Object(create_data_view_constructor(&data_view_prototype)),
        );

        let typed_array_kinds = [
            TypedArrayKind::Int8,
            TypedArrayKind::Uint8,
            TypedArrayKind::Uint8Clamped,
            TypedArrayKind::Int16,
            TypedArrayKind::Uint16,
            TypedArrayKind::Int32,
            TypedArrayKind::Uint32,
            TypedArrayKind::Float16,
            TypedArrayKind::Float32,
            TypedArrayKind::Float64,
            TypedArrayKind::BigInt64,
            TypedArrayKind::BigUint64,
        ];

        for kind in typed_array_kinds {
            let proto = create_typed_array_prototype(kind);
            let ctor = create_typed_array_constructor(kind, &proto);
            JSObject::set_property(&global_object, kind.name(), JSValue::Object(ctor));
        }

        // Phase 28: BigInt subsystem
        let bigint_proto = create_bigint_prototype();
        JSObject::set_property(
            &global_object,
            "BigInt",
            JSValue::Object(create_bigint_constructor(&bigint_proto)),
        );

        // Phase 21: Proxy and Reflect
        JSObject::set_property(
            &global_object,
            "Proxy",
            JSValue::Object(create_proxy_constructor()),
        );
        JSObject::set_property(
            &global_object,
            "Reflect",
            JSValue::Object(create_reflect_object()),
        );
        // Phase 23: Intl Internationalization API
        JSObject::set_property(
            &global_object,
            "Intl",
            JSValue::Object(create_intl_object()),
        );

        // Phase 26: SharedArrayBuffer, Atomics, and Web Workers
        let shared_array_buffer_proto = create_shared_array_buffer_prototype();
        JSObject::set_property(
            &global_object,
            "SharedArrayBuffer",
            JSValue::Object(create_shared_array_buffer_constructor(&shared_array_buffer_proto)),
        );
        JSObject::set_property(
            &global_object,
            "Atomics",
            JSValue::Object(create_atomics_object()),
        );
        JSObject::set_property(
            &global_object,
            "Worker",
            JSValue::Object(create_worker_constructor()),
        );

        // Global functions
        JSObject::set_property(&global_object, "isNaN", JSValue::Function(is_nan_function()));
        JSObject::set_property(&global_object, "isFinite", JSValue::Function(is_finite_function()));
        JSObject::set_property(&global_object, "parseInt", JSValue::Function(parse_int_function()));
        JSObject::set_property(&global_object, "parseFloat", JSValue::Function(parse_float_function()));
        JSObject::set_property(
            &global_object,
            "queueMicrotask",
            JSValue::Function(create_queue_microtask_function()),
        );
        JSObject::set_property(&global_object, "escape", JSValue::Function(escape_function()));
        JSObject::set_property(&global_object, "unescape", JSValue::Function(unescape_function()));

        let eval_fn = JSFunction::new_native("eval", |_this, args| {
            if let Some(first) = args.first() {
                if let JSValue::String(code) = first {
                    if let Some(global) = current_global() {
                        let mut parser = Parser::new(code);
                        let program = parser.parse_program().map_err(|e| e.to_string())?;
                        Context::hoist_declarations(&global, &JSObject::new_empty(None), &program.statements);
                        let bytecode = BytecodeGenerator::compile_program(&program);
                        InterpreterVM::execute_with_context(&bytecode, &[], Some(&global)).map_err(|e| e.message)
                    } else {
                        Err("No active execution realm for eval".to_string())
                    }
                } else {
                    Ok(first.clone())
                }
            } else {
                Ok(JSValue::Undefined)
            }
        });
        JSObject::set_property(&global_object, "eval", JSValue::Function(eval_fn));

        // Phase 29: Generators & Iterators
        let generator_prototype = crate::builtins::create_generator_prototype();
        let generator_function_proto = crate::builtins::create_generator_function_prototype(generator_prototype.clone());
        let generator_function_ctor = crate::builtins::create_generator_function_constructor(generator_function_proto.clone());
        JSObject::set_property(
            &global_object,
            "GeneratorFunction",
            JSValue::Object(generator_function_ctor),
        );

        // Phase 31: Host Runtime APIs & Modern Built-ins
        let timer_queue = Rc::new(RefCell::new(TimerQueue::new()));

        // Cryptography API
        JSObject::set_property(&global_object, "crypto", JSValue::Object(create_crypto_object()));

        // Encoding API
        JSObject::set_property(&global_object, "TextEncoder", JSValue::Object(create_text_encoder_constructor()));
        JSObject::set_property(&global_object, "TextDecoder", JSValue::Object(create_text_decoder_constructor()));

        // URL & URLSearchParams APIs
        let url_sp_ctor = create_url_search_params_constructor();
        let url_ctor = create_url_constructor(url_sp_ctor.clone());
        JSObject::set_property(&global_object, "URLSearchParams", JSValue::Object(url_sp_ctor));
        JSObject::set_property(&global_object, "URL", JSValue::Object(url_ctor));

        // Global URI utilities
        JSObject::set_property(&global_object, "encodeURI", JSValue::Function(encode_uri_function()));
        JSObject::set_property(&global_object, "decodeURI", JSValue::Function(decode_uri_function()));
        JSObject::set_property(&global_object, "encodeURIComponent", JSValue::Function(encode_uri_component_function()));
        JSObject::set_property(&global_object, "decodeURIComponent", JSValue::Function(decode_uri_component_function()));

        // Weak References API
        JSObject::set_property(&global_object, "WeakRef", JSValue::Object(create_weak_ref_constructor()));
        JSObject::set_property(&global_object, "FinalizationRegistry", JSValue::Object(create_finalization_registry_constructor()));

        // Host Timers
        let tq_for_timeout = timer_queue.clone();
        let set_timeout_fn = JSFunction::new_closure("setTimeout", move |_this, args| {
            let cb = match args.first() {
                Some(JSValue::Function(f)) => f.clone(),
                _ => return Err("TypeError: setTimeout requires a callback function".to_string()),
            };
            let delay = args.get(1).map(|v| v.to_number().max(0.0) as u64).unwrap_or(0);
            let extra_args = if args.len() > 2 { args[2..].to_vec() } else { Vec::new() };
            let id = tq_for_timeout.borrow_mut().set_timeout(cb, delay, extra_args);
            Ok(JSValue::Smi(id as i32))
        });
        JSObject::set_property(&global_object, "setTimeout", JSValue::Function(set_timeout_fn));

        let tq_for_cleartimeout = timer_queue.clone();
        let clear_timeout_fn = JSFunction::new_closure("clearTimeout", move |_this, args| {
            let id = args.first().map(|v| v.to_number() as u32).unwrap_or(0);
            tq_for_cleartimeout.borrow_mut().clear_timer(id);
            Ok(JSValue::Undefined)
        });
        JSObject::set_property(&global_object, "clearTimeout", JSValue::Function(clear_timeout_fn));

        let tq_for_interval = timer_queue.clone();
        let set_interval_fn = JSFunction::new_closure("setInterval", move |_this, args| {
            let cb = match args.first() {
                Some(JSValue::Function(f)) => f.clone(),
                _ => return Err("TypeError: setInterval requires a callback function".to_string()),
            };
            let interval = args.get(1).map(|v| v.to_number().max(1.0) as u64).unwrap_or(1);
            let extra_args = if args.len() > 2 { args[2..].to_vec() } else { Vec::new() };
            let id = tq_for_interval.borrow_mut().set_interval(cb, interval, extra_args);
            Ok(JSValue::Smi(id as i32))
        });
        JSObject::set_property(&global_object, "setInterval", JSValue::Function(set_interval_fn));

        let tq_for_clearinterval = timer_queue.clone();
        let clear_interval_fn = JSFunction::new_closure("clearInterval", move |_this, args| {
            let id = args.first().map(|v| v.to_number() as u32).unwrap_or(0);
            tq_for_clearinterval.borrow_mut().clear_timer(id);
            Ok(JSValue::Undefined)
        });
        JSObject::set_property(&global_object, "clearInterval", JSValue::Function(clear_interval_fn));

        // Phase 35: Universal Host & Web Platform APIs
        JSObject::set_property(
            &global_object,
            "structuredClone",
            JSValue::Function(create_structured_clone_function()),
        );
        JSObject::set_property(
            &global_object,
            "btoa",
            JSValue::Function(create_btoa_function()),
        );
        JSObject::set_property(
            &global_object,
            "atob",
            JSValue::Function(create_atob_function()),
        );
        JSObject::set_property(
            &global_object,
            "performance",
            JSValue::Object(create_performance_object()),
        );

        // Phase 36: Iterator and Iterator Helpers
        let iterator_prototype = create_iterator_prototype();
        let iterator_ctor = create_iterator_constructor(iterator_prototype);
        JSObject::set_property(&global_object, "Iterator", JSValue::Object(iterator_ctor));

        // Phase 41: D8 Realm API
        let realm_obj = crate::runtime::realm::create_realm_object();
        JSObject::set_property(&global_object, "Realm", JSValue::Object(realm_obj));

        // Phase 42: WHATWG Streams
        let readable_stream_proto = create_readable_stream_prototype();
        let readable_stream_ctor = create_readable_stream_constructor(readable_stream_proto.clone());
        JSObject::set_property(&global_object, "ReadableStream", JSValue::Object(readable_stream_ctor));

        let writable_stream_proto = create_writable_stream_prototype();
        let writable_stream_ctor = create_writable_stream_constructor(writable_stream_proto.clone());
        JSObject::set_property(&global_object, "WritableStream", JSValue::Object(writable_stream_ctor));

        let transform_stream_proto = create_transform_stream_prototype();
        let transform_stream_ctor = create_transform_stream_constructor(
            transform_stream_proto,
            readable_stream_proto,
            writable_stream_proto,
        );
        JSObject::set_property(&global_object, "TransformStream", JSValue::Object(transform_stream_ctor));

        // Phase 42: WHATWG Events
        let event_proto = create_event_prototype();
        let event_ctor = create_event_constructor(event_proto.clone());
        JSObject::set_property(&global_object, "Event", JSValue::Object(event_ctor.clone()));

        let custom_event_proto = create_custom_event_prototype(event_proto);
        let custom_event_ctor = create_custom_event_constructor(custom_event_proto, event_ctor);
        JSObject::set_property(&global_object, "CustomEvent", JSValue::Object(custom_event_ctor));

        let event_target_proto = create_event_target_prototype();
        let event_target_ctor = create_event_target_constructor(event_target_proto);
        JSObject::set_property(&global_object, "EventTarget", JSValue::Object(event_target_ctor));

        // Phase 42: Host Process Platform Bindings
        let process_obj = crate::runtime::process::create_process_object();
        JSObject::set_property(&global_object, "process", JSValue::Object(process_obj));

        Self {
            global_object,
            array_prototype,
            string_prototype,
            date_prototype,
            promise_prototype,
            map_prototype,
            set_prototype,
            weak_map_prototype,
            weak_set_prototype,
            symbol_prototype,
            regexp_prototype,
            generator_prototype,
            microtask_queue,
            timer_queue,
        }
    }

    /// Hoists top-level function and ES6 class declarations into the global scope.
    pub fn hoist_declarations(
        global_object: &Rc<RefCell<JSObject>>,
        promise_prototype: &Rc<RefCell<JSObject>>,
        statements: &[Statement],
    ) {
        for stmt in statements {
            match stmt {
                Statement::FunctionDeclaration {
                    name,
                    params,
                    body,
                    is_async,
                    is_generator,
                } => {
                    if *is_generator {
                        let bc = Rc::new(BytecodeGenerator::compile_named_function(name, params, body));
                        let is_as = *is_async;
                        let func = JSFunction::new_closure(name, move |this, args| {
                            let global = crate::runtime::current_global();
                            let gen_proto = global.as_ref().and_then(|g| {
                                match g.borrow().get_property("GeneratorFunction") {
                                    JSValue::Object(gf) => {
                                        match gf.borrow().get_property("prototype") {
                                            JSValue::Object(gfp) => {
                                                match gfp.borrow().get_property("prototype") {
                                                    JSValue::Object(gp) => Some(gp),
                                                    _ => None,
                                                }
                                            }
                                            _ => None,
                                        }
                                    }
                                    _ => None,
                                }
                            });
                            let gen_obj = JSObject::new_generator(bc.clone(), this.clone(), args.to_vec(), gen_proto, is_as);
                            Ok(JSValue::Object(gen_obj))
                        });
                        JSObject::set_property(global_object, name, JSValue::Function(func));
                    } else if *is_async {
                        let bc = Rc::new(BytecodeGenerator::compile_named_function(name, params, body));
                        let proto = promise_prototype.clone();
                        let func = JSFunction::new_closure(name, move |_this, args| {
                            let (promise, resolve_fn, reject_fn) =
                                new_promise_capability(Some(proto.clone()));
                            match InterpreterVM::execute(&bc, args) {
                                Ok(val) => {
                                    let _ = resolve_fn.call(&JSValue::Undefined, &[val]);
                                }
                                Err(e) => {
                                    let _ = reject_fn
                                        .call(&JSValue::Undefined, &[JSValue::String(e.message)]);
                                }
                            }
                            Ok(JSValue::Object(promise))
                        });
                        JSObject::set_property(global_object, name, JSValue::Function(func));
                    } else {
                        let bc = BytecodeGenerator::compile_named_function(name, params, body);
                        let func = JSFunction::new_bytecode(name, Rc::new(bc));
                        JSObject::set_property(global_object, name, JSValue::Function(func));
                    }
                }

                Statement::ClassDeclaration {
                    name,
                    super_class,
                    constructor,
                    methods,
                    fields,
                    static_blocks,
                } => {
                    let super_ctor_opt = if let Some(ref super_name) = super_class {
                        let val = JSObject::get_property(&global_object.borrow(), super_name);
                        match val {
                            JSValue::Object(ref o) => Some(o.clone()),
                            _ => None,
                        }
                    } else {
                        None
                    };

                    let super_proto_opt = if let Some(ref super_ctor) = super_ctor_opt {
                        let p = JSObject::get_property(&super_ctor.borrow(), "prototype");
                        match p {
                            JSValue::Object(ref o) => Some(o.clone()),
                            _ => None,
                        }
                    } else {
                        let obj_ctor = JSObject::get_property(&global_object.borrow(), "Object");
                        if let JSValue::Object(ref o) = obj_ctor {
                            if let JSValue::Object(ref p) = JSObject::get_property(&o.borrow(), "prototype") {
                                Some(p.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    let class_proto = JSObject::new_empty(super_proto_opt.clone());
                    let class_ctor = JSObject::new_empty(super_ctor_opt.clone());

                    JSObject::set_property(&class_ctor, "prototype", JSValue::Object(class_proto.clone()));
                    JSObject::set_property(&class_ctor, "name", JSValue::String(name.clone()));
                    JSObject::set_property(&class_ctor, "__is_class__", JSValue::Boolean(true));
                    JSObject::set_property(&class_proto, "constructor", JSValue::Object(class_ctor.clone()));

                    for method in methods {
                        let method_key = if method.is_getter {
                            format!("__get_{}__", method.name)
                        } else if method.is_setter {
                            format!("__set_{}__", method.name)
                        } else {
                            method.name.clone()
                        };

                        let method_bc = Rc::new(BytecodeGenerator::compile_function(&method.params, &method.body));
                        let super_proto_c = super_proto_opt.clone();
                        let super_ctor_c = super_ctor_opt.clone();
                        let is_static = method.is_static;

                        let method_fn = JSFunction::new_closure(&method.name, move |this, args| {
                            let super_val = if is_static {
                                super_ctor_c.as_ref().map(|o| JSValue::Object(o.clone())).unwrap_or(JSValue::Undefined)
                            } else {
                                super_proto_c.as_ref().map(|o| JSValue::Object(o.clone())).unwrap_or(JSValue::Undefined)
                            };

                            push_current_super(super_val);
                            let res = InterpreterVM::execute_with_receiver(&method_bc, this, args, None)
                                .map_err(|e| e.message);
                            pop_current_super();
                            res
                        });

                        if method.is_static {
                            JSObject::set_property(&class_ctor, &method_key, JSValue::Function(method_fn));
                        } else {
                            JSObject::set_property(&class_proto, &method_key, JSValue::Function(method_fn));
                        }
                    }

                    // Static fields initialization
                    for field in fields {
                        if field.is_static {
                            let val = if let Some(ref init) = field.initializer {
                                let init_prog = Program::new(vec![Statement::Return(Some(init.clone()))]);
                                let init_bc = BytecodeGenerator::compile_program(&init_prog);
                                InterpreterVM::execute_with_receiver(&init_bc, &JSValue::Object(class_ctor.clone()), &[], None)
                                    .unwrap_or(JSValue::Undefined)
                            } else {
                                JSValue::Undefined
                            };
                            JSObject::set_property(&class_ctor, &field.name, val);
                        }
                    }

                    // Static initialization blocks execution
                    for block in static_blocks {
                        let block_prog = Program::new(block.body.clone());
                        let block_bc = BytecodeGenerator::compile_program(&block_prog);
                        let _ = InterpreterVM::execute_with_receiver(&block_bc, &JSValue::Object(class_ctor.clone()), &[], None);
                    }

                    let ctor_params;
                    let ctor_body;
                    if let Some(ref ctor_method) = constructor {
                        ctor_params = ctor_method.params.clone();
                        ctor_body = ctor_method.body.clone();
                    } else {
                        ctor_params = Vec::new();
                        ctor_body = Vec::new();
                    }

                    let ctor_bc = Rc::new(BytecodeGenerator::compile_function(&ctor_params, &ctor_body));
                    let super_ctor_c = super_ctor_opt.clone();
                    let instance_fields = fields.iter().filter(|f| !f.is_static).cloned().collect::<Vec<_>>();

                    let ctor_fn = JSFunction::new_closure(name, move |this, args| {
                        // Initialize instance fields on `this` before running constructor body
                        for field in &instance_fields {
                            let field_val = if let Some(ref init) = field.initializer {
                                let init_prog = Program::new(vec![Statement::Return(Some(init.clone()))]);
                                let init_bc = BytecodeGenerator::compile_program(&init_prog);
                                InterpreterVM::execute_with_receiver(&init_bc, this, &[], None).unwrap_or(JSValue::Undefined)
                            } else {
                                JSValue::Undefined
                            };
                            if let JSValue::Object(ref o) = this {
                                JSObject::set_property(o, &field.name, field_val);
                            }
                        }

                        let super_val = super_ctor_c.as_ref().map(|o| JSValue::Object(o.clone())).unwrap_or(JSValue::Undefined);
                        push_current_super(super_val);
                        let res = InterpreterVM::execute_with_receiver(&ctor_bc, this, args, None)
                            .map_err(|e| e.message);
                        pop_current_super();
                        res
                    });

                    JSObject::set_property(&class_ctor, "__call__", JSValue::Function(ctor_fn));
                    JSObject::set_property(global_object, name, JSValue::Object(class_ctor));
                }
                _ => {}
            }
        }
    }

    /// Evaluates JavaScript source code in this execution context.
    pub fn eval(&mut self, source: &str) -> Result<JSValue, String> {
        MicrotaskQueue::set_current(Some(self.microtask_queue.clone()));
        TimerQueue::set_current(Some(self.timer_queue.clone()));
        set_current_global(self.global_object.clone());

        let mut parser = Parser::new(source);
        let program = parser.parse_program().map_err(|e| e.to_string())?;

        // Hoist top-level function and class declarations into global scope
        Self::hoist_declarations(&self.global_object, &self.promise_prototype, &program.statements);

        let bytecode_array = BytecodeGenerator::compile_program(&program);
        let result =
            InterpreterVM::execute_with_context(&bytecode_array, &[], Some(&self.global_object))
                .map_err(|e| e.message);

        // V8 MicrotasksPolicy::kAuto: drain microtasks after script execution
        self.run_microtasks();

        result
    }

    /// Runs all currently scheduled microtasks until the microtask queue is empty.
    pub fn run_microtasks(&self) -> usize {
        MicrotaskQueue::set_current(Some(self.microtask_queue.clone()));
        MicrotaskQueue::run_microtasks(&self.microtask_queue).unwrap_or(0)
    }

    /// Advances virtual clock by `ms` milliseconds, executing all due timers and draining microtasks.
    pub fn advance_timers(&self, ms: u64) -> usize {
        TimerQueue::set_current(Some(self.timer_queue.clone()));
        MicrotaskQueue::set_current(Some(self.microtask_queue.clone()));
        set_current_global(self.global_object.clone());
        TimerQueue::advance_time(&self.timer_queue, ms)
    }

    /// Runs all scheduled timers to completion, draining microtasks after each turn.
    pub fn run_all_timers(&self) -> usize {
        TimerQueue::set_current(Some(self.timer_queue.clone()));
        MicrotaskQueue::set_current(Some(self.microtask_queue.clone()));
        set_current_global(self.global_object.clone());
        TimerQueue::run_all(&self.timer_queue)
    }

    /// Serializes this initialized context into a compact binary heap snapshot payload.
    pub fn create_snapshot(&self) -> Vec<u8> {
        crate::snapshot::SnapshotSerializer::new().serialize(self)
    }

    /// Deserializes and reconstitutes an execution context from a pre-compiled binary snapshot.
    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, String> {
        let mut deserializer = crate::snapshot::SnapshotDeserializer::new(bytes);
        deserializer.deserialize()
    }

    /// Evaluates an ECMAScript module from source string with the given specifier.
    pub fn eval_module(&mut self, specifier: &str, source: &str) -> Result<JSValue, String> {
        TimerQueue::set_current(Some(self.timer_queue.clone()));
        set_current_global(self.global_object.clone());
        crate::runtime::module::register_module(specifier, source)?;
        let result = crate::runtime::module::link_and_evaluate_module(specifier, Some(&self.global_object));
        self.run_microtasks();
        result
    }
}
