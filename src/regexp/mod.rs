//! Safe Rust reimplementation of Google V8's Irregexp Regular Expression Subsystem (`src/regexp/`).
//!
//! Provides the complete regular expression compilation and execution pipeline:
//! AST nodes, syntax parser, Irregexp bytecodes, bytecode compiler, and backtracking VM interpreter.

pub mod ast;
pub mod bytecodes;
pub mod compiler;
pub mod interpreter;
pub mod parser;

pub use ast::{AssertionType, RegExpFlags, RegExpNode};
pub use bytecodes::{RegExpBytecode, RegExpOpcode};
pub use compiler::RegExpCompiler;
pub use interpreter::{RegExpInterpreter, RegExpMatch};
pub use parser::RegExpParser;

/// High-level engine interface for compiling and executing regular expressions.
pub struct RegExpEngine;

impl RegExpEngine {
    /// Compiles a regular expression pattern and flags into an executable Irregexp bytecode program.
    pub fn compile(pattern: &str, flags_str: &str) -> Result<RegExpBytecode, String> {
        let flags = RegExpFlags::parse(flags_str)?;
        let mut parser = RegExpParser::new(pattern, flags);
        let ast = parser.parse()?;
        let capture_count = parser.capture_count;
        let named_groups = parser.named_groups;

        let compiler = RegExpCompiler::new(
            pattern.to_string(),
            flags,
            capture_count,
            named_groups,
        );
        compiler.compile(&ast)
    }

    /// Executes a compiled Irregexp bytecode program against a subject string from `start_pos`.
    pub fn exec(bytecode: &RegExpBytecode, subject: &str, start_pos: usize) -> Option<RegExpMatch> {
        let interp = RegExpInterpreter::new(bytecode, subject);
        interp.execute(start_pos)
    }
}
