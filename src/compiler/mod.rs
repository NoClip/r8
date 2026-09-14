//! Safe Rust reimplementation of Google V8's Sea-of-Nodes Optimizing Compiler (`src/compiler/`).

pub mod backend;
pub mod graph;
pub mod graph_builder;
pub mod node;
pub mod optimizer;
pub mod osr;
pub mod pipeline;
pub mod sparkplug;

pub use backend::{
    arm64::Arm64Assembler,
    disasm::{disassemble_arm64, disassemble_x64},
    emulator::{CpuEmulator, ExecutionResult},
    instruction_selector::InstructionSelector,
    label::Label,
    registers::{Arm64Register, CallingConvention, X64Register},
    x64::X64Assembler,
    Architecture, Condition, NativeExecutable, TargetConfig,
};
pub use graph::Graph;
pub use graph_builder::BytecodeGraphBuilder;
pub use node::{Node, NodeId, NodeOp};
pub use optimizer::{OptimizationStats, Optimizer};
pub use osr::{OsrCompiler, OsrLoopEntry, OsrResult};
pub use pipeline::CompilerPipeline;
pub use sparkplug::SparkplugCompiler;
