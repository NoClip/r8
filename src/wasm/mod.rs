//! Safe Rust reimplementation of Google V8's `src/wasm/` WebAssembly subsystem.
//!
//! Provides:
//! - `binary_parser`: WebAssembly binary format decoder (MVP sections, LEB128).
//! - `interpreter`: Typed stack-machine bytecode interpreter (full MVP opcode set).
//! - `instance`: Live module instance with linear memory, tables, and globals.
//! - `js_api`: `WebAssembly` JavaScript global namespace (`compile`, `instantiate`).

pub mod binary_parser;
pub mod instance;
pub mod interpreter;
pub mod js_api;

pub use binary_parser::{
    parse as parse_wasm, ExportDesc, FuncType, ImportDesc, MemType, TableType, ValType,
    WasmDataSegment, WasmElementSegment, WasmExport, WasmImport, WasmModule, WasmParseError,
    WasmCode, StructField, StructType, ArrayType, TypeDef, WasmTag,
};
pub use instance::{
    WasmGlobal, WasmInstance, WasmMemory, WasmTable,
    eval_const_i32, wasm_val_to_js, js_val_to_wasm, build_exports_object,
};
pub use interpreter::{WasmInterpreter, WasmTrap, WasmVal, WasmStruct, WasmArray, CatchClause};
pub use js_api::create_webassembly_object;
