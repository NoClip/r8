//! Google V8 Ignition Interpreter safe Rust module.
//!
//! Exposes bytecode definitions, register file abstraction, bytecode array storage,
//! and bytecode builder with ShortStar optimization.

pub mod bytecode_array;
pub mod bytecode_generator;
pub mod bytecode_register;
pub mod bytecodes;
pub mod interpreter;

pub use bytecode_array::*;
pub use bytecode_generator::*;
pub use bytecode_register::*;
pub use bytecodes::*;
pub use interpreter::*;
