//! R8X - R8 Script & Command Executor (100% Pure Safe Rust).
//!
//! One-off script, command, and expression runner (analogous to npx / bunx).

use std::env;
use std::fs;
use r8::cli::{execute_source, run_wasm_file, setup_cli_builtins};
use r8::runtime::Context;

const VERSION: &str = "r8x 0.1.0 (R8 Script & Command Executor - 100% Pure Safe Rust)";

fn print_help() {
    println!("r8x 0.1.0 - R8 Script & Command Executor (100% Pure Safe Rust)\n");
    println!("Usage: r8x [options] <script.js | script.wasm | -e <code>> [-- [arguments]]\n");
    println!("Options:");
    println!("  -e, --eval <code>     Execute one-liner JavaScript code directly");
    println!("  -p, --print <code>    Evaluate and print result");
    println!("  -v, --version         Print r8x version banner");
    println!("  -h, --help            Print this help message\n");
    println!("Examples:");
    println!("  r8x -e \"console.log(1 + 1)\"");
    println!("  r8x script.js");
}

fn main() {
    let args: Vec<String> = env::args().collect();
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

    if eval_string.is_none() && script_file.is_none() {
        print_help();
        std::process::exit(1);
    }

    let mut ctx = Context::new();
    setup_cli_builtins(&mut ctx, false, VERSION);

    if let Some(code) = eval_string {
        match execute_source(&mut ctx, &code, false, false) {
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
                if let Err(err) = execute_source(&mut ctx, &code, false, false) {
                    eprintln!("Uncaught {}", err);
                    std::process::exit(1);
                }
            }
            Err(err) => {
                eprintln!("Cannot open file '{}': {}", file_path, err);
                std::process::exit(1);
            }
        }
    }
}
