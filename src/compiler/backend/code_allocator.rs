//! Executable code container and calling abstraction.
//!
//! Encapsulates compiled physical machine code buffers, disassemblies,
//! target metadata, and portable CPU execution.

use super::disasm::{disassemble_arm64, disassemble_x64};
use super::emulator::{CpuEmulator, ExecutionResult};
use super::Architecture;

/// A compiled, ready-to-execute physical machine code routine.
#[derive(Clone, Debug)]
pub struct NativeExecutable {
    pub arch: Architecture,
    pub machine_code: Vec<u8>,
    pub disassembly: String,
}

impl NativeExecutable {
    /// Creates a new NativeExecutable from emitted machine code bytes.
    pub fn new(arch: Architecture, machine_code: Vec<u8>) -> Self {
        let disassembly = match arch {
            Architecture::X64 => disassemble_x64(&machine_code),
            Architecture::Arm64 => disassemble_arm64(&machine_code),
            _ => format!("// Machine code buffer ({} bytes)\n", machine_code.len()),
        };

        Self {
            arch,
            machine_code,
            disassembly,
        }
    }

    /// Returns the raw binary instruction bytes.
    pub fn code_bytes(&self) -> &[u8] {
        &self.machine_code
    }

    /// Returns the disassembly text listing.
    pub fn disassembly(&self) -> &str {
        &self.disassembly
    }

    /// Invokes the compiled routine with the provided integer arguments.
    pub fn execute(&self, args: &[i64]) -> Result<ExecutionResult, String> {
        match self.arch {
            Architecture::X64 => {
                let mut emu = CpuEmulator::new();
                emu.set_args_windows(args);
                emu.execute_x64(&self.machine_code, 10_000_000)
            }
            Architecture::Arm64 => {
                // ARM64 emulation or fallback
                Err("AArch64 emulation on x86 host: use x64 for native evaluation".to_string())
            }
            _ => Err(format!("Direct execution not implemented for {:?}", self.arch)),
        }
    }
}
