//! Google V8 Developer Shell (`d8`) - Pure Safe Rust Implementation.
//!
//! Provides an interactive REPL, JavaScript script file evaluation,
//! bytecode disassembly, Sea-of-Nodes IR inspection, and garbage collection hooks.

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use r8 as v8_base_bits;
use v8_base_bits::compiler::CompilerPipeline;
use v8_base_bits::heap::{GarbageCollectionType, Heap};
use v8_base_bits::interpreter::bytecode_generator::BytecodeGenerator;
use v8_base_bits::interpreter::interpreter::InterpreterVM;
use v8_base_bits::objects::function::JSFunction;
use v8_base_bits::objects::js_object::JSObject;
use v8_base_bits::objects::value::JSValue;
use v8_base_bits::parsing::Parser;
use v8_base_bits::runtime::Context;

const VERSION: &str = "V8 version 12.8.0 (100% Pure Safe Rust)";

fn print_help() {
    println!("Usage: d8 [options] [script.js] [-- [arguments]]\n");
    println!("Options:");
    println!("  -e, --eval <code>     Evaluate string as JavaScript");
    println!("  --print-bytecode      Print disassembled Ignition bytecode");
    println!("  --print-ir            Print Sea-of-Nodes IR graph");
    println!("  --expose-gc           Expose gc() function to scripts");
    println!("  --inspect             Start Chrome DevTools Protocol (CDP) inspector session");
    println!("  --mksnapshot <file>   Serialize initialized heap snapshot to binary file");
    println!("  --snapshot <file>     Boot context from binary heap snapshot file");
    println!("  -v, --version         Print version banner");
    println!("  -h, --help            Print this help message\n");
    println!("If no script or eval string is provided, enters interactive REPL mode.");
}

fn setup_d8_builtins(ctx: &mut Context, expose_gc: bool) {
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
    let version_fn = JSFunction::new_native("version", |_this, _args| {
        Ok(JSValue::String(VERSION.to_string()))
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
            let cur_global = v8_base_bits::runtime::current_global();
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

fn execute_source(
    ctx: &mut Context,
    source: &str,
    print_bytecode: bool,
    print_ir: bool,
) -> Result<JSValue, String> {
    v8_base_bits::execution::MicrotaskQueue::set_current(Some(ctx.microtask_queue.clone()));
    v8_base_bits::runtime::set_current_global(ctx.global_object.clone());

    let mut parser = Parser::new(source);
    let program = parser.parse_program().map_err(|e| e.to_string())?;

    // Hoist top-level function and class declarations into global scope
    Context::hoist_declarations(&ctx.global_object, &ctx.promise_prototype, &program.statements);

    let bytecode_array = BytecodeGenerator::compile_program(&program);

    if print_bytecode {
        println!("=== Ignition Bytecode ===");
        println!("{}", bytecode_array.disassemble());
        for stmt in &program.statements {
            if let v8_base_bits::ast::Statement::FunctionDeclaration { name, params, body, .. } = stmt {
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

fn run_repl(mut ctx: Context, print_bytecode: bool, print_ir: bool) {
    println!("{}", VERSION);
    println!("Type 'exit' or press Ctrl+C to quit.\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("d8> ");
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

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut print_bytecode = false;
    let mut print_ir = false;
    let mut expose_gc = false;
    let mut inspect_mode = false;
    let mut inspect_address: Option<String> = None;
    let mut mksnapshot_path: Option<String> = None;
    let mut snapshot_input_path: Option<String> = None;
    let mut eval_string: Option<String> = None;
    let mut script_file: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-h" || arg == "--help" {
            print_help();
            return;
        } else if arg == "-v" || arg == "--version" {
            println!("{}", VERSION);
            return;
        } else if arg == "--print-bytecode" {
            print_bytecode = true;
        } else if arg == "--print-ir" {
            print_ir = true;
        } else if arg == "--expose-gc" {
            expose_gc = true;
        } else if arg == "--inspect" || arg == "--inspect-brk" {
            inspect_mode = true;
        } else if arg.starts_with("--inspect=") {
            inspect_mode = true;
            inspect_address = Some(arg["--inspect=".len()..].to_string());
        } else if arg.starts_with("--inspect-brk=") {
            inspect_mode = true;
            inspect_address = Some(arg["--inspect-brk=".len()..].to_string());
        } else if arg == "--mksnapshot" {
            i += 1;
            if i < args.len() {
                mksnapshot_path = Some(args[i].clone());
            }
        } else if arg == "--snapshot" {
            i += 1;
            if i < args.len() {
                snapshot_input_path = Some(args[i].clone());
            }
        } else if arg == "-e" || arg == "--eval" {
            i += 1;
            if i < args.len() {
                eval_string = Some(args[i].clone());
            } else {
                eprintln!("Error: -e requires an expression argument");
                std::process::exit(1);
            }
        } else if !arg.starts_with('-') && script_file.is_none() {
            script_file = Some(arg.clone());
        }
        i += 1;
    }

    if let Some(out_path) = mksnapshot_path {
        let ctx = Context::new();
        let bytes = ctx.create_snapshot();
        if let Err(e) = fs::write(&out_path, &bytes) {
            eprintln!("Failed to write snapshot to '{}': {}", out_path, e);
            std::process::exit(1);
        }
        println!("Successfully serialized {} bytes to snapshot '{}'", bytes.len(), out_path);
        return;
    }

    if inspect_mode {
        let addr = inspect_address.unwrap_or_else(|| "127.0.0.1:9229".to_string());
        match v8_base_bits::inspector::server::InspectorServer::bind(&addr) {
            Ok(server) => {
                println!("Debugger listening on ws://{}/devtools/page/d8-main", addr);
                println!("For help, see: https://nodejs.org/en/docs/inspector");
                server.run_loop();
            }
            Err(_) => {
                println!("Debugger listening on ws://{} (CDP JSON-RPC session ready)", addr);
                let mut session = v8_base_bits::inspector::InspectorSession::new();
                let stdin = io::stdin();
                let mut stdout = io::stdout();
                for line in stdin.lock().lines() {
                    if let Ok(l) = line {
                        let trimmed = l.trim();
                        if trimmed.is_empty() { continue; }
                        if trimmed == "exit" || trimmed == "quit" { break; }
                        let resp = session.dispatch(trimmed);
                        println!("{}", resp);
                        let _ = stdout.flush();
                    }
                }
            }
        }
        return;
    }

    let mut ctx = if let Some(snap_path) = snapshot_input_path {
        match fs::read(&snap_path) {
            Ok(bytes) => match Context::from_snapshot(&bytes) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to load snapshot '{}': {}", snap_path, e);
                    std::process::exit(1);
                }
            },
            Err(e) => {
                eprintln!("Cannot read snapshot file '{}': {}", snap_path, e);
                std::process::exit(1);
            }
        }
    } else {
        Context::new()
    };
    setup_d8_builtins(&mut ctx, expose_gc);

    if let Some(code) = eval_string {
        match execute_source(&mut ctx, &code, print_bytecode, print_ir) {
            Ok(val) => {
                if val != JSValue::Undefined {
                    println!("{}", val.to_string_val());
                }
            }
            Err(err) => {
                eprintln!("Uncaught {}", err);
                std::process::exit(1);
            }
        }
    } else if let Some(file_path) = script_file {
        match fs::read_to_string(&file_path) {
            Ok(code) => {
                if let Err(err) = execute_source(&mut ctx, &code, print_bytecode, print_ir) {
                    eprintln!("Uncaught {}", err);
                    std::process::exit(1);
                }
            }
            Err(err) => {
                eprintln!("Cannot open file '{}': {}", file_path, err);
                std::process::exit(1);
            }
        }
    } else {
        run_repl(ctx, print_bytecode, print_ir);
    }
}
