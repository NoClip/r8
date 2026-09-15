//! Executable code container and calling abstraction.
//!
//! Encapsulates compiled physical machine code buffers, disassemblies,
//! target metadata, and portable CPU execution.

use super::disasm::{disassemble_arm64, disassemble_x64};
use super::emulator::{CpuEmulator, ExecutionResult};
use super::Architecture;

#[cfg(all(windows, target_arch = "x86_64"))]
pub mod native_exec {
    const MEM_COMMIT: u32 = 0x1000;
    const MEM_RESERVE: u32 = 0x2000;
    const MEM_RELEASE: u32 = 0x8000;
    const PAGE_EXECUTE_READWRITE: u32 = 0x40;

    extern "system" {
        fn VirtualAlloc(
            lpAddress: *const u8,
            dwSize: usize,
            flAllocationType: u32,
            flProtect: u32,
        ) -> *mut u8;
        fn VirtualFree(lpAddress: *mut u8, dwSize: usize, dwFreeType: u32) -> i32;
    }

    #[derive(Debug)]
    pub struct JitMemory {
        pub ptr: *mut u8,
        #[allow(dead_code)]
        pub size: usize,
    }

    unsafe impl Send for JitMemory {}
    unsafe impl Sync for JitMemory {}

    impl JitMemory {
        pub fn new(code: &[u8]) -> Option<Self> {
            if code.is_empty() {
                return None;
            }
            unsafe {
                let ptr = VirtualAlloc(
                    std::ptr::null(),
                    code.len(),
                    MEM_COMMIT | MEM_RESERVE,
                    PAGE_EXECUTE_READWRITE,
                );
                if ptr.is_null() {
                    return None;
                }
                std::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len());
                Some(Self {
                    ptr,
                    size: code.len(),
                })
            }
        }

        #[inline(always)]
        pub unsafe fn call_x64(&self, a0: i64, a1: i64, a2: i64, a3: i64) -> i64 {
            let func: extern "C" fn(i64, i64, i64, i64) -> i64 = std::mem::transmute(self.ptr);
            func(a0, a1, a2, a3)
        }
    }

    impl Drop for JitMemory {
        fn drop(&mut self) {
            if !self.ptr.is_null() {
                unsafe {
                    VirtualFree(self.ptr, 0, MEM_RELEASE);
                }
            }
        }
    }
}

/// A compiled, ready-to-execute physical machine code routine.
#[derive(Clone, Debug)]
pub struct NativeExecutable {
    pub arch: Architecture,
    pub machine_code: Vec<u8>,
    pub disassembly: String,
    #[cfg(all(windows, target_arch = "x86_64"))]
    jit_mem: Option<std::sync::Arc<native_exec::JitMemory>>,
}

impl NativeExecutable {
    /// Creates a new NativeExecutable from emitted machine code bytes.
    pub fn new(arch: Architecture, machine_code: Vec<u8>) -> Self {
        let disassembly = match arch {
            Architecture::X64 => disassemble_x64(&machine_code),
            Architecture::Arm64 => disassemble_arm64(&machine_code),
            _ => format!("// Machine code buffer ({} bytes)\n", machine_code.len()),
        };

        #[cfg(all(windows, target_arch = "x86_64"))]
        let jit_mem = if arch == Architecture::X64 {
            native_exec::JitMemory::new(&machine_code).map(std::sync::Arc::new)
        } else {
            None
        };

        Self {
            arch,
            machine_code,
            disassembly,
            #[cfg(all(windows, target_arch = "x86_64"))]
            jit_mem,
        }
    }

    /// Returns the raw binary instruction bytes.
    pub fn code_bytes(&self) -> &[u8] {
        &self.machine_code
    }

    /// Returns the raw executable function pointer if compiled to physical hardware memory.
    #[inline(always)]
    pub fn raw_fn_ptr(&self) -> Option<unsafe extern "C" fn(i64, i64, i64, i64) -> i64> {
        #[cfg(all(windows, target_arch = "x86_64"))]
        {
            self.jit_mem.as_ref().map(|jit| unsafe { std::mem::transmute(jit.ptr) })
        }
        #[cfg(not(all(windows, target_arch = "x86_64")))]
        {
            None
        }
    }

    /// Returns the disassembly text listing.
    pub fn disassembly(&self) -> &str {
        &self.disassembly
    }

    /// Invokes the compiled routine with the provided integer arguments.
    pub fn execute(&self, args: &[i64]) -> Result<ExecutionResult, String> {
        match self.arch {
            Architecture::X64 => {
                #[cfg(all(windows, target_arch = "x86_64"))]
                if let Some(ref jit) = self.jit_mem {
                    if args.len() <= 4 {
                        let a0 = args.get(0).copied().unwrap_or(0);
                        let a1 = args.get(1).copied().unwrap_or(0);
                        let a2 = args.get(2).copied().unwrap_or(0);
                        let a3 = args.get(3).copied().unwrap_or(0);
                        let ret = unsafe { jit.call_x64(a0, a1, a2, a3) };
                        return Ok(ExecutionResult {
                            return_value: ret,
                            cycles: 0,
                        });
                    }
                }
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
