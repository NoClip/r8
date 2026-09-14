//! Full x86_64 Machine Code Assembler (`MacroAssembler`).
//!
//! Encodes 64-bit AMD64 binary machine code instructions with REX prefixes, ModR/M bytes,
//! SIB bytes, signed displacements, and 2-pass label backpatching.

use crate::compiler::backend::label::{Label, LabelManager};
use crate::compiler::backend::registers::X64Register;
use crate::compiler::backend::Condition;

/// Emits raw x86_64 machine code instructions into a byte buffer.
#[derive(Clone, Debug, Default)]
pub struct X64Assembler {
    code: Vec<u8>,
    label_manager: LabelManager,
}

impl X64Assembler {
    pub fn new() -> Self {
        Self {
            code: Vec::with_capacity(256),
            label_manager: LabelManager::new(),
        }
    }

    /// Returns the current code buffer length (byte offset).
    #[inline]
    pub fn offset(&self) -> usize {
        self.code.len()
    }

    /// Consumes the assembler and returns the generated machine code bytes.
    pub fn finalize(self) -> Vec<u8> {
        self.code
    }

    /// Emits a single byte.
    #[inline]
    pub fn emit_u8(&mut self, b: u8) {
        self.code.push(b);
    }

    /// Emits a 16-bit little-endian integer.
    pub fn emit_u16(&mut self, w: u16) {
        self.code.extend_from_slice(&w.to_le_bytes());
    }

    /// Emits a 32-bit little-endian integer.
    pub fn emit_u32(&mut self, d: u32) {
        self.code.extend_from_slice(&d.to_le_bytes());
    }

    /// Emits a 64-bit little-endian integer.
    pub fn emit_u64(&mut self, q: u64) {
        self.code.extend_from_slice(&q.to_le_bytes());
    }

    /// Creates a new unbound label.
    pub fn create_label(&mut self) -> Label {
        self.label_manager.create_label()
    }

    /// Binds a label to the current offset and patches all forward references.
    pub fn bind(&mut self, label: Label) {
        let current = self.offset();
        let pending = self.label_manager.bind(label, current);
        for patch_pos in pending {
            let next_pc = patch_pos + 4;
            let rel = (current as isize - next_pc as isize) as i32;
            let bytes = rel.to_le_bytes();
            self.code[patch_pos..patch_pos + 4].copy_from_slice(&bytes);
        }
    }

    // -------------------------------------------------------------------------
    // Prefix and Addressing Helpers
    // -------------------------------------------------------------------------

    /// Emits a REX prefix byte: `0100 W R X B`.
    /// - `w`: 64-bit operand width
    /// - `r`: extends ModR/M `reg` field
    /// - `x`: extends SIB `index` field
    /// - `b`: extends ModR/M `r/m` or opcode `reg` field
    pub fn emit_rex(&mut self, w: bool, r: bool, x: bool, b: bool) {
        let mut rex = 0x40;
        if w { rex |= 0x08; }
        if r { rex |= 0x04; }
        if x { rex |= 0x02; }
        if b { rex |= 0x01; }
        self.emit_u8(rex);
    }

    /// Emits a ModR/M byte: `(mod << 6) | (reg << 3) | rm`.
    #[inline]
    pub fn emit_modrm(&mut self, mod_bits: u8, reg_bits: u8, rm_bits: u8) {
        let byte = ((mod_bits & 0x03) << 6) | ((reg_bits & 0x07) << 3) | (rm_bits & 0x07);
        self.emit_u8(byte);
    }

    // -------------------------------------------------------------------------
    // Function Framing
    // -------------------------------------------------------------------------

    /// Standard x86_64 function prologue:
    /// `push rbp; mov rbp, rsp; sub rsp, <aligned_stack_size>`
    pub fn emit_prologue(&mut self, stack_size: u32) {
        // push rbp
        self.push(X64Register::Rbp);
        // mov rbp, rsp
        self.mov_reg_reg(X64Register::Rbp, X64Register::Rsp);
        // sub rsp, stack_size (if > 0, aligned to 16 bytes)
        let aligned = (stack_size + 15) & !15;
        if aligned > 0 {
            self.sub_reg_imm32(X64Register::Rsp, aligned as i32);
        }
    }

    /// Standard x86_64 function epilogue:
    /// `mov rsp, rbp; pop rbp; ret`
    pub fn emit_epilogue(&mut self) {
        self.mov_reg_reg(X64Register::Rsp, X64Register::Rbp);
        self.pop(X64Register::Rbp);
        self.ret();
    }

    // -------------------------------------------------------------------------
    // Stack Operations
    // -------------------------------------------------------------------------

    /// `push reg`
    pub fn push(&mut self, reg: X64Register) {
        if reg.is_extended() {
            self.emit_rex(false, false, false, true);
        }
        self.emit_u8(0x50 + reg.code());
    }

    /// `pop reg`
    pub fn pop(&mut self, reg: X64Register) {
        if reg.is_extended() {
            self.emit_rex(false, false, false, true);
        }
        self.emit_u8(0x58 + reg.code());
    }

    // -------------------------------------------------------------------------
    // Move Instructions
    // -------------------------------------------------------------------------

    /// `mov dst, src` (64-bit register-to-register)
    pub fn mov_reg_reg(&mut self, dst: X64Register, src: X64Register) {
        self.emit_rex(true, src.is_extended(), false, dst.is_extended());
        self.emit_u8(0x89);
        self.emit_modrm(0b11, src.code(), dst.code());
    }

    /// `mov dst, imm64` (load 64-bit constant)
    pub fn mov_reg_imm64(&mut self, dst: X64Register, imm: i64) {
        self.emit_rex(true, false, false, dst.is_extended());
        self.emit_u8(0xB8 + dst.code());
        self.emit_u64(imm as u64);
    }

    /// `mov dst, imm32` (sign-extended 32-bit constant)
    pub fn mov_reg_imm32(&mut self, dst: X64Register, imm: i32) {
        self.emit_rex(true, false, false, dst.is_extended());
        self.emit_u8(0xC7);
        self.emit_modrm(0b11, 0, dst.code());
        self.emit_u32(imm as u32);
    }

    /// `mov dst, [base + disp32]` (load from stack or memory)
    pub fn mov_reg_mem(&mut self, dst: X64Register, base: X64Register, disp: i32) {
        self.emit_rex(true, dst.is_extended(), false, base.is_extended());
        self.emit_u8(0x8B);
        self.emit_modrm(0b10, dst.code(), base.code());
        self.emit_u32(disp as u32);
    }

    /// `mov [base + disp32], src` (store to stack or memory)
    pub fn mov_mem_reg(&mut self, base: X64Register, disp: i32, src: X64Register) {
        self.emit_rex(true, src.is_extended(), false, base.is_extended());
        self.emit_u8(0x89);
        self.emit_modrm(0b10, src.code(), base.code());
        self.emit_u32(disp as u32);
    }

    // -------------------------------------------------------------------------
    // Arithmetic & Bitwise
    // -------------------------------------------------------------------------

    /// `add dst, src`
    pub fn add_reg_reg(&mut self, dst: X64Register, src: X64Register) {
        self.emit_rex(true, src.is_extended(), false, dst.is_extended());
        self.emit_u8(0x01);
        self.emit_modrm(0b11, src.code(), dst.code());
    }

    /// `add dst, imm32`
    pub fn add_reg_imm32(&mut self, dst: X64Register, imm: i32) {
        self.emit_rex(true, false, false, dst.is_extended());
        self.emit_u8(0x81);
        self.emit_modrm(0b11, 0, dst.code());
        self.emit_u32(imm as u32);
    }

    /// `sub dst, src`
    pub fn sub_reg_reg(&mut self, dst: X64Register, src: X64Register) {
        self.emit_rex(true, src.is_extended(), false, dst.is_extended());
        self.emit_u8(0x29);
        self.emit_modrm(0b11, src.code(), dst.code());
    }

    /// `sub dst, imm32`
    pub fn sub_reg_imm32(&mut self, dst: X64Register, imm: i32) {
        self.emit_rex(true, false, false, dst.is_extended());
        self.emit_u8(0x81);
        self.emit_modrm(0b11, 5, dst.code());
        self.emit_u32(imm as u32);
    }

    /// `imul dst, src` (signed 64-bit multiply)
    pub fn imul_reg_reg(&mut self, dst: X64Register, src: X64Register) {
        self.emit_rex(true, dst.is_extended(), false, src.is_extended());
        self.emit_u8(0x0F);
        self.emit_u8(0xAF);
        self.emit_modrm(0b11, dst.code(), src.code());
    }

    /// `and dst, src`
    pub fn and_reg_reg(&mut self, dst: X64Register, src: X64Register) {
        self.emit_rex(true, src.is_extended(), false, dst.is_extended());
        self.emit_u8(0x21);
        self.emit_modrm(0b11, src.code(), dst.code());
    }

    /// `or dst, src`
    pub fn or_reg_reg(&mut self, dst: X64Register, src: X64Register) {
        self.emit_rex(true, src.is_extended(), false, dst.is_extended());
        self.emit_u8(0x09);
        self.emit_modrm(0b11, src.code(), dst.code());
    }

    /// `xor dst, src`
    pub fn xor_reg_reg(&mut self, dst: X64Register, src: X64Register) {
        self.emit_rex(true, src.is_extended(), false, dst.is_extended());
        self.emit_u8(0x31);
        self.emit_modrm(0b11, src.code(), dst.code());
    }

    /// `neg dst` (two's complement negate)
    pub fn neg_reg(&mut self, dst: X64Register) {
        self.emit_rex(true, false, false, dst.is_extended());
        self.emit_u8(0xF7);
        self.emit_modrm(0b11, 3, dst.code());
    }

    /// `not dst` (one's complement bitwise not)
    pub fn not_reg(&mut self, dst: X64Register) {
        self.emit_rex(true, false, false, dst.is_extended());
        self.emit_u8(0xF7);
        self.emit_modrm(0b11, 2, dst.code());
    }

    /// `shl dst, cl` (shift left by cl register)
    pub fn shl_reg_cl(&mut self, dst: X64Register) {
        self.emit_rex(true, false, false, dst.is_extended());
        self.emit_u8(0xD3);
        self.emit_modrm(0b11, 4, dst.code());
    }

    /// `shr dst, cl` (logical shift right by cl register)
    pub fn shr_reg_cl(&mut self, dst: X64Register) {
        self.emit_rex(true, false, false, dst.is_extended());
        self.emit_u8(0xD3);
        self.emit_modrm(0b11, 5, dst.code());
    }

    /// `sar dst, cl` (arithmetic shift right by cl register)
    pub fn sar_reg_cl(&mut self, dst: X64Register) {
        self.emit_rex(true, false, false, dst.is_extended());
        self.emit_u8(0xD3);
        self.emit_modrm(0b11, 7, dst.code());
    }

    // -------------------------------------------------------------------------
    // Comparisons & Condition Codes
    // -------------------------------------------------------------------------

    /// `cmp lhs, rhs`
    pub fn cmp_reg_reg(&mut self, lhs: X64Register, rhs: X64Register) {
        self.emit_rex(true, rhs.is_extended(), false, lhs.is_extended());
        self.emit_u8(0x39);
        self.emit_modrm(0b11, rhs.code(), lhs.code());
    }

    /// `cmp lhs, imm32`
    pub fn cmp_reg_imm32(&mut self, lhs: X64Register, imm: i32) {
        self.emit_rex(true, false, false, lhs.is_extended());
        self.emit_u8(0x81);
        self.emit_modrm(0b11, 7, lhs.code());
        self.emit_u32(imm as u32);
    }

    /// `test lhs, rhs`
    pub fn test_reg_reg(&mut self, lhs: X64Register, rhs: X64Register) {
        self.emit_rex(true, rhs.is_extended(), false, lhs.is_extended());
        self.emit_u8(0x85);
        self.emit_modrm(0b11, rhs.code(), lhs.code());
    }

    /// `setcc dst` (sets byte to 1 if condition met, 0 otherwise, then zero-extends to 64-bit)
    pub fn setcc(&mut self, cond: Condition, dst: X64Register) {
        let cond_byte = Self::condition_code(cond);
        // setcc byte
        if dst.is_extended() {
            self.emit_rex(false, false, false, true);
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x90 + cond_byte);
        self.emit_modrm(0b11, 0, dst.code());

        // movzx dst, dst byte (zero-extend 8-bit to 64-bit)
        self.emit_rex(true, dst.is_extended(), false, dst.is_extended());
        self.emit_u8(0x0F);
        self.emit_u8(0xB6);
        self.emit_modrm(0b11, dst.code(), dst.code());
    }

    // -------------------------------------------------------------------------
    // Control Flow: Jumps, Calls, Returns
    // -------------------------------------------------------------------------

    /// `jmp label` (unconditional relative jump with 32-bit displacement)
    pub fn jmp(&mut self, label: Label) {
        self.emit_u8(0xE9);
        let patch_pos = self.offset();
        self.emit_u32(0); // placeholder
        if let Some(target) = self.label_manager.record_use(label, patch_pos) {
            let next_pc = patch_pos + 4;
            let rel = (target as isize - next_pc as isize) as i32;
            self.code[patch_pos..patch_pos + 4].copy_from_slice(&rel.to_le_bytes());
        }
    }

    /// `jcc cond, label` (conditional relative jump with 32-bit displacement)
    pub fn jcc(&mut self, cond: Condition, label: Label) {
        let cond_byte = Self::condition_code(cond);
        self.emit_u8(0x0F);
        self.emit_u8(0x80 + cond_byte);
        let patch_pos = self.offset();
        self.emit_u32(0); // placeholder
        if let Some(target) = self.label_manager.record_use(label, patch_pos) {
            let next_pc = patch_pos + 4;
            let rel = (target as isize - next_pc as isize) as i32;
            self.code[patch_pos..patch_pos + 4].copy_from_slice(&rel.to_le_bytes());
        }
    }

    /// `ret`
    pub fn ret(&mut self) {
        self.emit_u8(0xC3);
    }

    /// Maps abstract `Condition` to x86_64 4-bit condition code (0x0..0xF).
    pub fn condition_code(cond: Condition) -> u8 {
        match cond {
            Condition::Overflow => 0x0,
            Condition::NoOverflow => 0x1,
            Condition::Below => 0x2,        // Unsigned <
            Condition::AboveOrEqual => 0x3, // Unsigned >=
            Condition::Equal => 0x4,        // Zero / Equal
            Condition::NotEqual => 0x5,     // Not Zero / Not Equal
            Condition::BelowOrEqual => 0x6, // Unsigned <=
            Condition::Above => 0x7,        // Unsigned >
            Condition::LessThan => 0xC,     // Signed <
            Condition::GreaterThanOrEqual => 0xD, // Signed >=
            Condition::LessThanOrEqual => 0xE,    // Signed <=
            Condition::GreaterThan => 0xF,        // Signed >
        }
    }
}
