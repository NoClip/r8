//! Fast Baseline Machine-Code JIT Compiler (Sparkplug).
//!
//! Directly compiles Ignition bytecode into native x86_64 machine code in a single linear pass
//! without constructing intermediate Sea-of-Nodes graphs, achieving 10-100x faster compilation.

use crate::compiler::backend::code_allocator::NativeExecutable;
use crate::compiler::backend::label::Label;
use crate::compiler::backend::registers::X64Register;
use crate::compiler::backend::x64::X64Assembler;
use crate::compiler::backend::{Architecture, Condition};
use crate::interpreter::bytecode_array::{BytecodeArray, ConstantValue};
use crate::interpreter::bytecodes::Bytecode;
use std::collections::HashMap;

/// Sparkplug Baseline 1-pass JIT Compiler.
pub struct SparkplugCompiler<'a> {
    bytecode_array: &'a BytecodeArray,
    masm: X64Assembler,
    labels: HashMap<usize, Label>,
    stack_frame_size: u32,
}

impl<'a> SparkplugCompiler<'a> {
    pub fn new(bytecode_array: &'a BytecodeArray) -> Self {
        Self {
            bytecode_array,
            masm: X64Assembler::new(),
            labels: HashMap::new(),
            stack_frame_size: 0,
        }
    }

    /// Checks whether this bytecode array can be compiled by Sparkplug.
    pub fn can_compile(bytecode_array: &BytecodeArray) -> bool {
        let bytes = bytecode_array.bytecodes();
        let mut pc = 0;
        while pc < bytes.len() {
            let op = bytes[pc];
            pc += 1;
            match op {
                x if x == Bytecode::LdaZero as u8
                    || x == Bytecode::LdaUndefined as u8
                    || x == Bytecode::LdaNull as u8
                    || x == Bytecode::LdaTrue as u8
                    || x == Bytecode::LdaFalse as u8 => {}
                x if x == Bytecode::LdaSmi as u8 => {
                    if pc >= bytes.len() { return false; }
                    pc += 1;
                }
                x if x == Bytecode::LdaConstant as u8 => {
                    if pc >= bytes.len() { return false; }
                    let idx = bytes[pc] as usize;
                    pc += 1;
                    match bytecode_array.get_constant(idx) {
                        Some(ConstantValue::Smi(_)) | Some(ConstantValue::Number(_)) | Some(ConstantValue::Boolean(_)) => {}
                        _ => return false,
                    }
                }
                x if x == Bytecode::Ldar as u8 => {
                    if pc >= bytes.len() { return false; }
                    pc += 1;
                }
                x if x == Bytecode::Star as u8 => {
                    if pc >= bytes.len() { return false; }
                    pc += 1;
                }
                x if x >= Bytecode::Star0 as u8 && x <= Bytecode::Star15 as u8 => {}
                x if x == Bytecode::Mov as u8 => {
                    if pc + 1 >= bytes.len() { return false; }
                    pc += 2;
                }
                x if x == Bytecode::Add as u8
                    || x == Bytecode::Sub as u8
                    || x == Bytecode::Mul as u8
                    || x == Bytecode::BitwiseAnd as u8
                    || x == Bytecode::BitwiseOr as u8
                    || x == Bytecode::BitwiseXor as u8 => {
                    if pc + 1 >= bytes.len() { return false; }
                    pc += 2;
                }
                x if x == Bytecode::AddSmi as u8
                    || x == Bytecode::SubSmi as u8
                    || x == Bytecode::MulSmi as u8
                    || x == Bytecode::BitwiseAndSmi as u8
                    || x == Bytecode::BitwiseOrSmi as u8
                    || x == Bytecode::BitwiseXorSmi as u8 => {
                    if pc + 1 >= bytes.len() { return false; }
                    pc += 2;
                }
                x if x == Bytecode::Inc as u8
                    || x == Bytecode::Dec as u8
                    || x == Bytecode::Negate as u8
                    || x == Bytecode::BitwiseNot as u8 => {}
                x if x == Bytecode::TestEqual as u8
                    || x == Bytecode::TestEqualStrict as u8
                    || x == Bytecode::TestLessThan as u8
                    || x == Bytecode::TestGreaterThan as u8
                    || x == Bytecode::TestLessThanOrEqual as u8
                    || x == Bytecode::TestGreaterThanOrEqual as u8 => {
                    if pc + 1 >= bytes.len() { return false; }
                    pc += 2;
                }
                x if x == Bytecode::Jump as u8
                    || x == Bytecode::JumpIfTrue as u8
                    || x == Bytecode::JumpIfFalse as u8
                    || x == Bytecode::JumpLoop as u8 => {
                    if pc >= bytes.len() { return false; }
                    pc += 1;
                }
                x if x == Bytecode::Return as u8 => {}
                _ => return false,
            }
        }
        true
    }

    /// Compiles the bytecode array into native x86_64 machine code.
    pub fn compile(bytecode_array: &'a BytecodeArray) -> NativeExecutable {
        let mut compiler = SparkplugCompiler::new(bytecode_array);
        compiler.generate_code()
    }

    /// Maps an interpreter register operand to a stack offset from RBP:
    /// Slots 0..15 (parameters and locals) are mapped to [RBP - 8 * (slot + 1)].
    #[inline(always)]
    fn slot_offset(slot: usize) -> i32 {
        -8 * (slot as i32 + 1)
    }

    fn operand_to_slot(reg_byte: i8) -> usize {
        crate::interpreter::interpreter::InterpreterFrame::OP_TO_SLOT[reg_byte as u8 as usize] as usize
    }

    fn generate_code(&mut self) -> NativeExecutable {
        let bytes = self.bytecode_array.bytecodes();

        // Pass 1: scan jump targets and preallocate labels
        let mut pc = 0;
        while pc < bytes.len() {
            let op = bytes[pc];
            let inst_pc = pc;
            pc += 1;
            match op {
                x if x == Bytecode::Jump as u8
                    || x == Bytecode::JumpIfTrue as u8
                    || x == Bytecode::JumpIfFalse as u8
                    || x == Bytecode::JumpLoop as u8 => {
                    if pc < bytes.len() {
                        let delta = bytes[pc] as i8 as isize;
                        pc += 1;
                        let target = (inst_pc as isize + delta) as usize;
                        if !self.labels.contains_key(&target) {
                            let lbl = self.masm.create_label();
                            self.labels.insert(target, lbl);
                        }
                    }
                }
                x if x == Bytecode::LdaSmi as u8 || x == Bytecode::LdaConstant as u8 || x == Bytecode::Ldar as u8 || x == Bytecode::Star as u8 => {
                    pc += 1;
                }
                x if x == Bytecode::Mov as u8 => {
                    pc += 2;
                }
                x if x == Bytecode::Add as u8
                    || x == Bytecode::Sub as u8
                    || x == Bytecode::Mul as u8
                    || x == Bytecode::BitwiseAnd as u8
                    || x == Bytecode::BitwiseOr as u8
                    || x == Bytecode::BitwiseXor as u8
                    || x == Bytecode::AddSmi as u8
                    || x == Bytecode::SubSmi as u8
                    || x == Bytecode::MulSmi as u8
                    || x == Bytecode::BitwiseAndSmi as u8
                    || x == Bytecode::BitwiseOrSmi as u8
                    || x == Bytecode::BitwiseXorSmi as u8
                    || x == Bytecode::TestEqual as u8
                    || x == Bytecode::TestEqualStrict as u8
                    || x == Bytecode::TestLessThan as u8
                    || x == Bytecode::TestGreaterThan as u8
                    || x == Bytecode::TestLessThanOrEqual as u8
                    || x == Bytecode::TestGreaterThanOrEqual as u8 => {
                    pc += 2;
                }
                _ => {}
            }
        }

        // Emit standard function prologue
        self.stack_frame_size = 128; // 16 slots * 8 bytes
        self.masm.emit_prologue(self.stack_frame_size);

        // Copy incoming arguments into parameter slots:
        // Under Windows x64 ABI: RCX = this (slot 0), RDX = arg0 (slot 1), R8 = arg1 (slot 2), R9 = arg2 (slot 3)
        self.masm.mov_mem_reg(X64Register::Rbp, Self::slot_offset(0), X64Register::Rcx);
        self.masm.mov_mem_reg(X64Register::Rbp, Self::slot_offset(1), X64Register::Rdx);
        self.masm.mov_mem_reg(X64Register::Rbp, Self::slot_offset(2), X64Register::R8);
        self.masm.mov_mem_reg(X64Register::Rbp, Self::slot_offset(3), X64Register::R9);

        // Pass 2: Direct linear machine code generation
        pc = 0;
        while pc < bytes.len() {
            let inst_pc = pc;
            if let Some(&lbl) = self.labels.get(&inst_pc) {
                self.masm.bind(lbl);
            }

            let op = bytes[pc];
            pc += 1;

            // Direct ShortStar handling (Star0..Star15)
            if op >= (Bytecode::Star0 as u8) && op <= (Bytecode::Star15 as u8) {
                let star_idx = (op - (Bytecode::Star0 as u8)) as usize;
                let slot = 4 + star_idx;
                self.masm.mov_mem_reg(X64Register::Rbp, Self::slot_offset(slot), X64Register::Rax);
                continue;
            }

            let bc = unsafe { Bytecode::from_byte_unchecked(op) };
            match bc {
                Bytecode::LdaZero => {
                    self.masm.mov_reg_imm32(X64Register::Rax, 0);
                }
                Bytecode::LdaUndefined | Bytecode::LdaNull => {
                    self.masm.mov_reg_imm32(X64Register::Rax, 0);
                }
                Bytecode::LdaTrue => {
                    self.masm.mov_reg_imm32(X64Register::Rax, 1);
                }
                Bytecode::LdaFalse => {
                    self.masm.mov_reg_imm32(X64Register::Rax, 0);
                }
                Bytecode::LdaSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 1;
                    self.masm.mov_reg_imm32(X64Register::Rax, imm);
                }
                Bytecode::LdaConstant => {
                    let idx = bytes[pc] as usize;
                    pc += 1;
                    if let Some(c) = self.bytecode_array.get_constant(idx) {
                        match c {
                            ConstantValue::Smi(n) => self.masm.mov_reg_imm32(X64Register::Rax, *n),
                            ConstantValue::Number(f) => self.masm.mov_reg_imm64(X64Register::Rax, *f as i64),
                            ConstantValue::Boolean(b) => self.masm.mov_reg_imm32(X64Register::Rax, if *b { 1 } else { 0 }),
                            _ => self.masm.mov_reg_imm32(X64Register::Rax, 0),
                        }
                    }
                }
                Bytecode::Ldar => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_reg_mem(X64Register::Rax, X64Register::Rbp, Self::slot_offset(slot));
                }
                Bytecode::Star => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_mem_reg(X64Register::Rbp, Self::slot_offset(slot), X64Register::Rax);
                }
                Bytecode::Mov => {
                    let src_byte = bytes[pc] as i8;
                    let dst_byte = bytes[pc + 1] as i8;
                    pc += 2;
                    let src_slot = Self::operand_to_slot(src_byte);
                    let dst_slot = Self::operand_to_slot(dst_byte);
                    self.masm.mov_reg_mem(X64Register::R10, X64Register::Rbp, Self::slot_offset(src_slot));
                    self.masm.mov_mem_reg(X64Register::Rbp, Self::slot_offset(dst_slot), X64Register::R10);
                }
                Bytecode::Add => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 2; // reg + feedback
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_reg_mem(X64Register::R10, X64Register::Rbp, Self::slot_offset(slot));
                    self.masm.add_reg_reg(X64Register::Rax, X64Register::R10);
                }
                Bytecode::Sub => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 2;
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_reg_mem(X64Register::R10, X64Register::Rbp, Self::slot_offset(slot));
                    self.masm.sub_reg_reg(X64Register::Rax, X64Register::R10);
                }
                Bytecode::Mul => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 2;
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_reg_mem(X64Register::R10, X64Register::Rbp, Self::slot_offset(slot));
                    self.masm.imul_reg_reg(X64Register::Rax, X64Register::R10);
                }
                Bytecode::BitwiseAnd => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 2;
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_reg_mem(X64Register::R10, X64Register::Rbp, Self::slot_offset(slot));
                    self.masm.and_reg_reg(X64Register::Rax, X64Register::R10);
                }
                Bytecode::BitwiseOr => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 2;
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_reg_mem(X64Register::R10, X64Register::Rbp, Self::slot_offset(slot));
                    self.masm.or_reg_reg(X64Register::Rax, X64Register::R10);
                }
                Bytecode::BitwiseXor => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 2;
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_reg_mem(X64Register::R10, X64Register::Rbp, Self::slot_offset(slot));
                    self.masm.xor_reg_reg(X64Register::Rax, X64Register::R10);
                }
                Bytecode::AddSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 2;
                    self.masm.add_reg_imm32(X64Register::Rax, imm);
                }
                Bytecode::SubSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 2;
                    self.masm.sub_reg_imm32(X64Register::Rax, imm);
                }
                Bytecode::MulSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 2;
                    self.masm.mov_reg_imm32(X64Register::R10, imm);
                    self.masm.imul_reg_reg(X64Register::Rax, X64Register::R10);
                }
                Bytecode::BitwiseAndSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 2;
                    self.masm.mov_reg_imm32(X64Register::R10, imm);
                    self.masm.and_reg_reg(X64Register::Rax, X64Register::R10);
                }
                Bytecode::BitwiseOrSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 2;
                    self.masm.mov_reg_imm32(X64Register::R10, imm);
                    self.masm.or_reg_reg(X64Register::Rax, X64Register::R10);
                }
                Bytecode::BitwiseXorSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 2;
                    self.masm.mov_reg_imm32(X64Register::R10, imm);
                    self.masm.xor_reg_reg(X64Register::Rax, X64Register::R10);
                }
                Bytecode::Inc => {
                    self.masm.add_reg_imm32(X64Register::Rax, 1);
                }
                Bytecode::Dec => {
                    self.masm.sub_reg_imm32(X64Register::Rax, 1);
                }
                Bytecode::Negate => {
                    self.masm.neg_reg(X64Register::Rax);
                }
                Bytecode::BitwiseNot => {
                    self.masm.not_reg(X64Register::Rax);
                }
                Bytecode::TestEqual | Bytecode::TestEqualStrict => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 2;
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_reg_mem(X64Register::R10, X64Register::Rbp, Self::slot_offset(slot));
                    self.masm.cmp_reg_reg(X64Register::Rax, X64Register::R10);
                    self.masm.setcc(Condition::Equal, X64Register::Rax);
                }
                Bytecode::TestLessThan => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 2;
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_reg_mem(X64Register::R10, X64Register::Rbp, Self::slot_offset(slot));
                    self.masm.cmp_reg_reg(X64Register::Rax, X64Register::R10);
                    self.masm.setcc(Condition::LessThan, X64Register::Rax);
                }
                Bytecode::TestGreaterThan => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 2;
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_reg_mem(X64Register::R10, X64Register::Rbp, Self::slot_offset(slot));
                    self.masm.cmp_reg_reg(X64Register::Rax, X64Register::R10);
                    self.masm.setcc(Condition::GreaterThan, X64Register::Rax);
                }
                Bytecode::TestLessThanOrEqual => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 2;
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_reg_mem(X64Register::R10, X64Register::Rbp, Self::slot_offset(slot));
                    self.masm.cmp_reg_reg(X64Register::Rax, X64Register::R10);
                    self.masm.setcc(Condition::LessThanOrEqual, X64Register::Rax);
                }
                Bytecode::TestGreaterThanOrEqual => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 2;
                    let slot = Self::operand_to_slot(reg_byte);
                    self.masm.mov_reg_mem(X64Register::R10, X64Register::Rbp, Self::slot_offset(slot));
                    self.masm.cmp_reg_reg(X64Register::Rax, X64Register::R10);
                    self.masm.setcc(Condition::GreaterThanOrEqual, X64Register::Rax);
                }
                Bytecode::Jump | Bytecode::JumpLoop => {
                    let delta = bytes[pc] as i8 as isize;
                    pc += 1;
                    let target = (inst_pc as isize + delta) as usize;
                    if let Some(&lbl) = self.labels.get(&target) {
                        self.masm.jmp(lbl);
                    }
                }
                Bytecode::JumpIfTrue => {
                    let delta = bytes[pc] as i8 as isize;
                    pc += 1;
                    let target = (inst_pc as isize + delta) as usize;
                    self.masm.test_reg_reg(X64Register::Rax, X64Register::Rax);
                    if let Some(&lbl) = self.labels.get(&target) {
                        self.masm.jcc(Condition::NotEqual, lbl);
                    }
                }
                Bytecode::JumpIfFalse => {
                    let delta = bytes[pc] as i8 as isize;
                    pc += 1;
                    let target = (inst_pc as isize + delta) as usize;
                    self.masm.test_reg_reg(X64Register::Rax, X64Register::Rax);
                    if let Some(&lbl) = self.labels.get(&target) {
                        self.masm.jcc(Condition::Equal, lbl);
                    }
                }
                Bytecode::Return => {
                    self.masm.emit_epilogue();
                }
                _ => {
                    // Unsupported in Sparkplug baseline
                    self.masm.emit_epilogue();
                }
            }
        }

        // Final safety epilogue if return fell through
        self.masm.emit_epilogue();
        let code_bytes = self.masm.clone().finalize();
        NativeExecutable::new(Architecture::X64, code_bytes)
    }
}
