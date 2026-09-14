//! Safe software CPU execution emulator for testing emitted machine code portably.
//!
//! Simulates CPU registers, stack frame memory, arithmetic status flags, and instruction stepping.

use super::registers::X64Register;

/// Result of executing machine code in the CPU emulator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionResult {
    pub return_value: i64,
    pub cycles: usize,
}

/// Simulated CPU state.
#[derive(Clone, Debug)]
pub struct CpuEmulator {
    pub registers: [i64; 16],
    pub stack: Vec<u8>,
    pub zf: bool,
    pub sf: bool,
    pub of: bool,
}

impl Default for CpuEmulator {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuEmulator {
    pub fn new() -> Self {
        let stack_size = 64 * 1024; // 64 KB
        let mut emu = Self {
            registers: [0; 16],
            stack: vec![0; stack_size],
            zf: false,
            sf: false,
            of: false,
        };
        // Initialize RSP to the top of the stack (descending stack)
        emu.registers[X64Register::Rsp as usize] = (stack_size - 16) as i64;
        emu.registers[X64Register::Rbp as usize] = (stack_size - 16) as i64;
        emu
    }

    /// Sets argument values according to the Windows x64 ABI (rcx, rdx, r8, r9).
    pub fn set_args_windows(&mut self, args: &[i64]) {
        if let Some(&a0) = args.first() { self.registers[X64Register::Rcx as usize] = a0; }
        if let Some(&a1) = args.get(1) { self.registers[X64Register::Rdx as usize] = a1; }
        if let Some(&a2) = args.get(2) { self.registers[X64Register::R8  as usize] = a2; }
        if let Some(&a3) = args.get(3) { self.registers[X64Register::R9  as usize] = a3; }
    }

    /// Sets argument values according to the System V AMD64 ABI (rdi, rsi, rdx, rcx, r8, r9).
    pub fn set_args_sysv(&mut self, args: &[i64]) {
        if let Some(&a0) = args.first() { self.registers[X64Register::Rdi as usize] = a0; }
        if let Some(&a1) = args.get(1) { self.registers[X64Register::Rsi as usize] = a1; }
        if let Some(&a2) = args.get(2) { self.registers[X64Register::Rdx as usize] = a2; }
        if let Some(&a3) = args.get(3) { self.registers[X64Register::Rcx as usize] = a3; }
        if let Some(&a4) = args.get(4) { self.registers[X64Register::R8  as usize] = a4; }
        if let Some(&a5) = args.get(5) { self.registers[X64Register::R9  as usize] = a5; }
    }

    /// Runs an x86_64 machine code buffer until `ret` or max cycle limit.
    pub fn execute_x64(&mut self, code: &[u8], max_cycles: usize) -> Result<ExecutionResult, String> {
        let mut pc = 0;
        let mut cycles = 0;
        let mut call_stack: Vec<usize> = Vec::new();

        while pc < code.len() && cycles < max_cycles {
            cycles += 1;
            let mut rex_w = false;
            let mut rex_r = false;
            let mut _rex_x = false;
            let mut rex_b = false;

            let mut b = code[pc];
            pc += 1;

            if (b & 0xF0) == 0x40 {
                rex_w = (b & 0x08) != 0;
                rex_r = (b & 0x04) != 0;
                _rex_x = (b & 0x02) != 0;
                rex_b = (b & 0x01) != 0;
                if pc < code.len() {
                    b = code[pc];
                    pc += 1;
                }
            }

            let reg_idx = |code_val: u8, ext: bool| -> usize {
                ((code_val & 0x7) | if ext { 8 } else { 0 }) as usize
            };

            match b {
                // push reg
                0x50..=0x57 => {
                    let r = reg_idx(b - 0x50, rex_b);
                    let val = self.registers[r];
                    self.registers[X64Register::Rsp as usize] -= 8;
                    let sp = self.registers[X64Register::Rsp as usize] as usize;
                    if sp + 8 <= self.stack.len() {
                        self.stack[sp..sp+8].copy_from_slice(&val.to_le_bytes());
                    }
                }
                // pop reg
                0x58..=0x5F => {
                    let r = reg_idx(b - 0x58, rex_b);
                    let sp = self.registers[X64Register::Rsp as usize] as usize;
                    if sp + 8 <= self.stack.len() {
                        self.registers[r] = i64::from_le_bytes(self.stack[sp..sp+8].try_into().unwrap());
                    }
                    self.registers[X64Register::Rsp as usize] += 8;
                }
                // mov reg, reg OR mov [base + disp], reg
                0x89 => {
                    if pc < code.len() {
                        let modrm = code[pc];
                        pc += 1;
                        let mod_bits = modrm >> 6;
                        let reg_bits = (modrm >> 3) & 0x7;
                        let rm_bits = modrm & 0x7;
                        let src = self.registers[reg_idx(reg_bits, rex_r)];

                        if mod_bits == 0b11 {
                            // mov dst_reg, src_reg
                            self.registers[reg_idx(rm_bits, rex_b)] = src;
                        } else if mod_bits == 0b10 && pc + 4 <= code.len() {
                            // mov [base + disp32], src_reg
                            let disp = i32::from_le_bytes(code[pc..pc+4].try_into().unwrap());
                            pc += 4;
                            let base = self.registers[reg_idx(rm_bits, rex_b)];
                            let addr = (base as isize + disp as isize) as usize;
                            if addr + 8 <= self.stack.len() {
                                self.stack[addr..addr+8].copy_from_slice(&src.to_le_bytes());
                            }
                        }
                    }
                }
                // mov dst_reg, [base + disp]
                0x8B => {
                    if pc < code.len() {
                        let modrm = code[pc];
                        pc += 1;
                        let mod_bits = modrm >> 6;
                        let reg_bits = (modrm >> 3) & 0x7;
                        let rm_bits = modrm & 0x7;
                        let dst_idx = reg_idx(reg_bits, rex_r);

                        if mod_bits == 0b10 && pc + 4 <= code.len() {
                            let disp = i32::from_le_bytes(code[pc..pc+4].try_into().unwrap());
                            pc += 4;
                            let base = self.registers[reg_idx(rm_bits, rex_b)];
                            let addr = (base as isize + disp as isize) as usize;
                            if addr + 8 <= self.stack.len() {
                                self.registers[dst_idx] = i64::from_le_bytes(self.stack[addr..addr+8].try_into().unwrap());
                            }
                        }
                    }
                }
                // mov reg, imm64 / imm32
                0xB8..=0xBF => {
                    let dst_idx = reg_idx(b - 0xB8, rex_b);
                    if rex_w && pc + 8 <= code.len() {
                        let imm = i64::from_le_bytes(code[pc..pc+8].try_into().unwrap());
                        pc += 8;
                        self.registers[dst_idx] = imm;
                    } else if pc + 4 <= code.len() {
                        let imm = i32::from_le_bytes(code[pc..pc+4].try_into().unwrap());
                        pc += 4;
                        self.registers[dst_idx] = imm as i64;
                    }
                }
                // add dst, src
                0x01 => {
                    if pc < code.len() {
                        let modrm = code[pc];
                        pc += 1;
                        let src = self.registers[reg_idx((modrm >> 3) & 7, rex_r)];
                        let dst_idx = reg_idx(modrm & 7, rex_b);
                        let dst = self.registers[dst_idx];
                        let res = dst.wrapping_add(src);
                        self.update_flags(res);
                        self.registers[dst_idx] = res;
                    }
                }
                // sub dst, src
                0x29 => {
                    if pc < code.len() {
                        let modrm = code[pc];
                        pc += 1;
                        let src = self.registers[reg_idx((modrm >> 3) & 7, rex_r)];
                        let dst_idx = reg_idx(modrm & 7, rex_b);
                        let dst = self.registers[dst_idx];
                        let res = dst.wrapping_sub(src);
                        self.update_flags(res);
                        self.registers[dst_idx] = res;
                    }
                }
                // 0x0F extended opcodes
                0x0F => {
                    if pc < code.len() {
                        let b2 = code[pc];
                        pc += 1;
                        match b2 {
                            // imul dst, src
                            0xAF => {
                                if pc < code.len() {
                                    let modrm = code[pc];
                                    pc += 1;
                                    let dst_idx = reg_idx((modrm >> 3) & 7, rex_r);
                                    let src = self.registers[reg_idx(modrm & 7, rex_b)];
                                    let res = self.registers[dst_idx].wrapping_mul(src);
                                    self.update_flags(res);
                                    self.registers[dst_idx] = res;
                                }
                            }
                            // jcc rel32
                            0x80..=0x8F => {
                                let cond_code = b2 & 0xF;
                                if pc + 4 <= code.len() {
                                    let rel = i32::from_le_bytes(code[pc..pc+4].try_into().unwrap());
                                    pc += 4;
                                    if self.eval_condition_code(cond_code) {
                                        pc = (pc as isize + rel as isize) as usize;
                                    }
                                }
                            }
                            // setcc
                            0x90..=0x9F => {
                                let cond_code = b2 & 0xF;
                                if pc < code.len() {
                                    let modrm = code[pc];
                                    pc += 1;
                                    let dst_idx = reg_idx(modrm & 7, rex_b);
                                    self.registers[dst_idx] = if self.eval_condition_code(cond_code) { 1 } else { 0 };
                                }
                            }
                            // movzx dst, src
                            0xB6 => {
                                if pc < code.len() {
                                    let modrm = code[pc];
                                    pc += 1;
                                    let dst_idx = reg_idx((modrm >> 3) & 7, rex_r);
                                    let src = self.registers[reg_idx(modrm & 7, rex_b)] & 0xFF;
                                    self.registers[dst_idx] = src;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                // and dst, src
                0x21 => {
                    if pc < code.len() {
                        let modrm = code[pc];
                        pc += 1;
                        let src = self.registers[reg_idx((modrm >> 3) & 7, rex_r)];
                        let dst_idx = reg_idx(modrm & 7, rex_b);
                        let res = self.registers[dst_idx] & src;
                        self.update_flags(res);
                        self.registers[dst_idx] = res;
                    }
                }
                // or dst, src
                0x09 => {
                    if pc < code.len() {
                        let modrm = code[pc];
                        pc += 1;
                        let src = self.registers[reg_idx((modrm >> 3) & 7, rex_r)];
                        let dst_idx = reg_idx(modrm & 7, rex_b);
                        let res = self.registers[dst_idx] | src;
                        self.update_flags(res);
                        self.registers[dst_idx] = res;
                    }
                }
                // xor dst, src
                0x31 => {
                    if pc < code.len() {
                        let modrm = code[pc];
                        pc += 1;
                        let src = self.registers[reg_idx((modrm >> 3) & 7, rex_r)];
                        let dst_idx = reg_idx(modrm & 7, rex_b);
                        let res = self.registers[dst_idx] ^ src;
                        self.update_flags(res);
                        self.registers[dst_idx] = res;
                    }
                }
                // cmp lhs, rhs
                0x39 => {
                    if pc < code.len() {
                        let modrm = code[pc];
                        pc += 1;
                        let rhs = self.registers[reg_idx((modrm >> 3) & 7, rex_r)];
                        let lhs = self.registers[reg_idx(modrm & 7, rex_b)];
                        let diff = lhs.wrapping_sub(rhs);
                        self.update_flags(diff);
                    }
                }
                // alu reg, imm32
                0x81 => {
                    if pc < code.len() {
                        let modrm = code[pc];
                        pc += 1;
                        let op = (modrm >> 3) & 7;
                        let reg_i = reg_idx(modrm & 7, rex_b);
                        if pc + 4 <= code.len() {
                            let imm = i32::from_le_bytes(code[pc..pc+4].try_into().unwrap()) as i64;
                            pc += 4;
                            match op {
                                0 => { // add
                                    let res = self.registers[reg_i].wrapping_add(imm);
                                    self.update_flags(res);
                                    self.registers[reg_i] = res;
                                }
                                5 => { // sub
                                    let res = self.registers[reg_i].wrapping_sub(imm);
                                    self.update_flags(res);
                                    self.registers[reg_i] = res;
                                }
                                7 => { // cmp
                                    let diff = self.registers[reg_i].wrapping_sub(imm);
                                    self.update_flags(diff);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                // jmp rel32
                0xE9 => {
                    if pc + 4 <= code.len() {
                        let rel = i32::from_le_bytes(code[pc..pc+4].try_into().unwrap());
                        pc = (pc + 4) as usize;
                        pc = (pc as isize + rel as isize) as usize;
                    }
                }
                // ret
                0xC3 => {
                    if let Some(ret_addr) = call_stack.pop() {
                        pc = ret_addr;
                    } else {
                        // Top-level return
                        return Ok(ExecutionResult {
                            return_value: self.registers[X64Register::Rax as usize],
                            cycles,
                        });
                    }
                }
                _ => {
                    return Err(format!("Unsupported emulator opcode 0x{:02x} at pc 0x{:04x}", b, pc - 1));
                }
            }
        }

        if cycles >= max_cycles {
            Err("Execution timed out in CPU emulator".to_string())
        } else {
            Ok(ExecutionResult {
                return_value: self.registers[X64Register::Rax as usize],
                cycles,
            })
        }
    }

    fn update_flags(&mut self, val: i64) {
        self.zf = val == 0;
        self.sf = val < 0;
    }

    fn eval_condition_code(&self, code: u8) -> bool {
        match code {
            0x4 => self.zf,               // je / jz
            0x5 => !self.zf,              // jne / jnz
            0xC => self.sf != self.of,    // jl
            0xD => self.sf == self.of,    // jge
            0xE => self.zf || (self.sf != self.of), // jle
            0xF => !self.zf && (self.sf == self.of), // jg
            _ => false,
        }
    }
}
