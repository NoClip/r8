//! R8 - Safe Rust V8 JavaScript & WebAssembly Engine.
//!
//! Universal execution engine, REPL, and script runner.

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use r8::cli::{execute_source, run_repl, run_wasm_file, setup_cli_builtins};
use r8::inspector::server::InspectorServer;
use r8::inspector::InspectorSession;
use r8::runtime::Context;

const VERSION: &str = "r8 12.8.0 (based on Google V8 12.8)";

fn print_help() {
    println!("r8 12.8.0 (based on Google V8 12.8)\n");
    println!("Usage: r8 [options] [script.js | script.wasm] [-- [arguments]]\n");
    println!("Commands & Options:");
    println!("  -e, --eval <code>     Evaluate string as JavaScript");
    println!("  -p, --print <code>    Evaluate string as JavaScript and print result");
    println!("  --jitless             Run in interpreter-only mode");
    println!("  --expose-gc           Expose gc() function to scripts");
    println!("  --inspect[=addr]      Start Chrome DevTools Protocol (CDP) inspector session");
    println!("  --mksnapshot <file>   Serialize initialized heap snapshot to binary file");
    println!("  --snapshot <file>     Boot context from binary heap snapshot file");
    println!("  -v, --version         Print version banner");
    println!("  -h, --help            Print this help message\n");
    println!("If no script or eval string is provided, enters interactive REPL mode.");
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
    let mut print_result = false;
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
        } else if arg == "--jitless" {
            // R8's tiering allows interpreter-only execution
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
        } else if arg == "-p" || arg == "--print" {
            i += 1;
            if i < args.len() {
                eval_string = Some(args[i].clone());
                print_result = true;
            } else {
                eprintln!("Error: -p requires an expression argument");
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
        match InspectorServer::bind(&addr) {
            Ok(server) => {
                println!("Debugger listening on ws://{}/devtools/page/r8-main", addr);
                println!("For help, see: https://nodejs.org/en/docs/inspector");
                server.run_loop();
            }
            Err(_) => {
                println!("Debugger listening on ws://{} (CDP JSON-RPC session ready)", addr);
                let mut session = InspectorSession::new();
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
    setup_cli_builtins(&mut ctx, expose_gc, VERSION);

    if let Some(code) = eval_string {
        match execute_source(&mut ctx, &code, print_bytecode, print_ir) {
            Ok(val) => {
                if print_result || val != r8::objects::value::JSValue::Undefined {
                    println!("{}", val.to_string_val());
                }
            }
            Err(err) => {
                eprintln!("Uncaught {}", err);
                std::process::exit(1);
            }
        }
    } else if let Some(file_path) = script_file {
        if file_path.ends_with(".wasm") {
            if let Err(e) = run_wasm_file(&file_path) {
                eprintln!("Error executing WebAssembly: {}", e);
                std::process::exit(1);
            }
            return;
        }

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
        run_repl(ctx, "r8> ", VERSION, print_bytecode, print_ir);
    }
}
