//! Multi-architecture backend for the Turbofan / Baseline JIT compiler.
//!
//! Provides target-independent abstractions, instruction selection, register allocation,
//! machine code emitters for x86_64 and AArch64, disassembler, software CPU emulator,
//! and native executable memory management.

pub mod code_allocator;
pub mod disasm;
pub mod emulator;
pub mod instruction_selector;
pub mod label;
pub mod register_allocator;
pub mod registers;

pub mod arm64;
pub mod x64;

pub use code_allocator::NativeExecutable;
pub use disasm::{disassemble_arm64, disassemble_x64};
pub use emulator::{CpuEmulator, ExecutionResult};
pub use instruction_selector::InstructionSelector;
pub use label::{Label, LabelManager};
pub use register_allocator::RegisterAllocator;
pub use registers::{Arm64Register, CallingConvention, X64Register};

/// Supported target CPU architectures in the V8 compiler backend.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Architecture {
    /// 64-bit x86 (AMD64 / Intel 64) - Primary desktop/server backend.
    X64,
    /// 64-bit ARM (AArch64) - Primary mobile / Apple Silicon backend.
    Arm64,
    /// 64-bit RISC-V (RV64GC) - Open standard RISC backend.
    RiscV64,
    /// 32-bit x86 (IA-32) - Legacy 32-bit PC backend.
    Ia32,
    /// 32-bit ARM (ARMv7-A) - Legacy 32-bit mobile/embedded backend.
    Arm32,
}

impl Architecture {
    /// Returns the target architecture of the currently compiling host.
    pub fn host_current() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            Architecture::X64
        }
        #[cfg(target_arch = "aarch64")]
        {
            Architecture::Arm64
        }
        #[cfg(target_arch = "riscv64")]
        {
            Architecture::RiscV64
        }
        #[cfg(target_arch = "x86")]
        {
            Architecture::Ia32
        }
        #[cfg(target_arch = "arm")]
        {
            Architecture::Arm32
        }
        #[cfg(not(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64",
            target_arch = "x86",
            target_arch = "arm"
        )))]
        {
            Architecture::X64
        }
    }

    /// Returns the pointer bitness in bits (32 or 64).
    pub fn bitness(self) -> usize {
        match self {
            Architecture::X64 | Architecture::Arm64 | Architecture::RiscV64 => 64,
            Architecture::Ia32 | Architecture::Arm32 => 32,
        }
    }

    /// Returns the pointer size in bytes (4 or 8).
    pub fn pointer_size(self) -> usize {
        self.bitness() / 8
    }
}

/// Target platform configuration for code generation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TargetConfig {
    pub arch: Architecture,
    pub calling_convention: CallingConvention,
}

impl TargetConfig {
    pub fn host() -> Self {
        Self {
            arch: Architecture::host_current(),
            calling_convention: CallingConvention::host_default(),
        }
    }
}

/// Abstract condition code for branches and conditional moves.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Condition {
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
    Overflow,
    NoOverflow,
    Below,       // Unsigned <
    BelowOrEqual,// Unsigned <=
    Above,       // Unsigned >
    AboveOrEqual,// Unsigned >=
}
