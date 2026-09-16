//! Shared CLI Infrastructure for R8, RD8, R8X, and D8 binaries.
//!
//! Provides common runtime setup, builtins, REPL, snapshot serialization,
//! Chrome DevTools Protocol inspector dispatch, and script execution.

use std::fs;
use std::io::{self, BufRead, Write};
use crate::compiler::CompilerPipeline;
use crate::heap::{GarbageCollectionType, Heap};
use crate::interpreter::bytecode_generator::BytecodeGenerator;
use crate::interpreter::interpreter::InterpreterVM;
use crate::objects::function::JSFunction;
use crate::objects::js_object::JSObject;
use crate::objects::value::JSValue;
use crate::parsing::Parser;
use crate::runtime::Context;

pub fn setup_cli_builtins(ctx: &mut Context, expose_gc: bool, version: &str) {
    let version_str = version.to_string();

    // 1. print(...) builtin
    let print_fn = JSFunction::new_native("print", |_this, args| {
        let output: Vec<String> = args.iter().map(|a| a.to_string_val()).collect();
        println!("{}", output.join(" "));
        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&ctx.global_object, "print", JSValue::Function(print_fn));

    // 2. read(filename) builtin
    let read_fn = JSFunction::new_native("read", |_this, args| {
        if let Some(path_val) = args.first() {
            let path = path_val.to_string_val();
            match fs::read_to_string(&path) {
                Ok(content) => Ok(JSValue::String(content)),
                Err(e) => Err(format!("Cannot read file '{}': {}", path, e)),
            }
        } else {
            Err("read() requires a file path argument".to_string())
        }
    });
    JSObject::set_property(&ctx.global_object, "read", JSValue::Function(read_fn));

    // 3. version() builtin
    let version_fn = JSFunction::new_closure("version", move |_this, _args| {
        Ok(JSValue::String(version_str.clone()))
    });
    JSObject::set_property(&ctx.global_object, "version", JSValue::Function(version_fn));

    // 4. readbuffer(filename) builtin
    let readbuffer_fn = JSFunction::new_native("readbuffer", |_this, args| {
        if let Some(path_val) = args.first() {
            let path = path_val.to_string_val();
            match fs::read(&path) {
                Ok(bytes) => {
                    let buf = JSObject::new_array_buffer_from_bytes(bytes, None);
                    Ok(JSValue::Object(buf))
                }
                Err(e) => Err(format!("Cannot read buffer from file '{}': {}", path, e)),
            }
        } else {
            Err("readbuffer() requires a file path argument".to_string())
        }
    });
    JSObject::set_property(&ctx.global_object, "readbuffer", JSValue::Function(readbuffer_fn));

    // 5. readline() builtin
    let readline_fn = JSFunction::new_native("readline", |_this, _args| {
        let mut line = String::new();
        let stdin = io::stdin();
        let mut handle = stdin.lock();
        match handle.read_line(&mut line) {
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(JSValue::String(line))
            }
            Err(e) => Err(format!("readline() failed: {}", e)),
        }
    });
    JSObject::set_property(&ctx.global_object, "readline", JSValue::Function(readline_fn));

    // 6. load(filename) builtin
    let load_fn = JSFunction::new_closure("load", move |_this, args| {
        if let Some(path_val) = args.first() {
            let path = path_val.to_string_val();
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Cannot load script file '{}': {}", path, e))?;
            let mut parser = Parser::new(&content);
            let program = parser.parse_program().map_err(|e| e.to_string())?;
            let cur_global = crate::runtime::current_global();
            if let Some(ref g) = cur_global {
                let dummy_proto = JSObject::new_empty(None);
                Context::hoist_declarations(g, &dummy_proto, &program.statements);
            }
            let bc = BytecodeGenerator::compile_program(&program);
            InterpreterVM::execute(&bc, &[]).map_err(|e| e.message)
        } else {
            Err("load() requires a file path argument".to_string())
        }
    });
    JSObject::set_property(&ctx.global_object, "load", JSValue::Function(load_fn));

    // 7. quit(status?) builtin
    let quit_fn = JSFunction::new_native("quit", |_this, args| {
        let code = args.first().map(|v| v.to_number() as i32).unwrap_or(0);
        std::process::exit(code);
    });
    JSObject::set_property(&ctx.global_object, "quit", JSValue::Function(quit_fn));

    // 8. gc() builtin if requested
    if expose_gc {
        let gc_fn = JSFunction::new_native("gc", |_this, _args| {
            let mut heap = Heap::new(1024, 4096);
            let reclaimed = heap.collect_garbage(GarbageCollectionType::MarkSweep);
            println!("[GC: reclaimed {} objects]", reclaimed);
            Ok(JSValue::Undefined)
        });
        JSObject::set_property(&ctx.global_object, "gc", JSValue::Function(gc_fn));
    }
}

pub fn execute_source(
    ctx: &mut Context,
    source: &str,
    print_bytecode: bool,
    print_ir: bool,
) -> Result<JSValue, String> {
    crate::execution::MicrotaskQueue::set_current(Some(ctx.microtask_queue.clone()));
    crate::runtime::set_current_global(ctx.global_object.clone());

    let mut parser = Parser::new(source);
    let program = parser.parse_program().map_err(|e| e.to_string())?;

    // Hoist top-level function and class declarations into global scope
    Context::hoist_declarations(&ctx.global_object, &ctx.promise_prototype, &program.statements);

    let bytecode_array = BytecodeGenerator::compile_program(&program);

    if print_bytecode {
        println!("=== Ignition Bytecode ===");
        println!("{}", bytecode_array.disassemble());
        for stmt in &program.statements {
            if let crate::ast::Statement::FunctionDeclaration { name, params, body, .. } = stmt {
                let func_bc = BytecodeGenerator::compile_named_function(name, params, body);
                println!("=== Ignition Bytecode (Function: {}) ===", name);
                println!("{}", func_bc.disassemble());
            }
        }
    }

    if print_ir {
        println!("=== Sea-of-Nodes IR Graph ===");
        let (graph, stats) = CompilerPipeline::compile(&bytecode_array);
        println!("Nodes count: {}", graph.len());
        println!("Optimizations: {:?}", stats);
        for node in &graph.nodes {
            if !node.is_dead() {
                println!("  Node {:?}: {:?} (inputs: {:?})", node.id, node.op, node.inputs);
            }
        }
    }

    let res = InterpreterVM::execute_with_context(&bytecode_array, &[], Some(&ctx.global_object))
        .map_err(|e| e.message);
    ctx.run_microtasks();
    res
}

pub fn run_repl(mut ctx: Context, prompt: &str, version_banner: &str, print_bytecode: bool, print_ir: bool) {
    println!("{}", version_banner);
    println!("Type 'exit' or press Ctrl+C to quit.\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("{}", prompt);
        let _ = stdout.flush();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "exit" || trimmed == "quit" {
                    break;
                }

                match execute_source(&mut ctx, trimmed, print_bytecode, print_ir) {
                    Ok(val) => {
                        if val != JSValue::Undefined {
                            println!("{}", val.to_string_val());
                        }
                    }
                    Err(err) => {
                        eprintln!("Uncaught {}", err);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }
    }
}

pub fn run_wasm_file(path: &str) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|e| format!("Cannot read WebAssembly file '{}': {}", path, e))?;
    let module = crate::wasm::parse_wasm(&bytes).map_err(|e| format!("WebAssembly parse error: {:?}", e))?;
    println!("[wasm] Successfully parsed module '{}'", path);
    println!("[wasm] Types: {}, Functions: {}, Tables: {}, Memories: {}, Globals: {}, Exports: {}",
        module.types.len(),
        module.functions.len(),
        module.tables.len(),
        module.memories.len(),
        module.globals.len(),
        module.exports.len()
    );
    for exp in &module.exports {
        println!("  - Export '{}': {:?}", exp.name, exp.desc);
    }
    let mut instance = crate::wasm::WasmInstance::instantiate(&module, Vec::new());

    if let Some(exp) = module.exports.iter().find(|e| e.name == "_start" || e.name == "main") {
        println!("[wasm] Invoking entry function '{}'...", exp.name);
        match crate::wasm::WasmInterpreter::call_export(&module, &mut instance, &exp.name, Vec::new()) {
            Ok(results) => {
                println!("[wasm] Result: {:?}", results);
            }
            Err(trap) => {
                eprintln!("[wasm trap] {:?}", trap);
            }
        }
    }
    Ok(())
}
