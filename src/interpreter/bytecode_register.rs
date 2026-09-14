//! Safe Rust reimplementation of Google V8's `src/interpreter/bytecode-register.h`.
//!
//! Provides the Register and RegisterList representations used in V8's Ignition interpreter,
//! including parameter indexing, operand bias math, ShortStar mapping, and stack frame slot layout.

use super::bytecodes::Bytecode;
use std::fmt;

/// An interpreter Register located in the function's register file in its stack frame.
///
/// Registers hold parameters, `this`, and expression/local values.
///
/// Layout in Ignition:
/// - Parameters have negative indices (`< 0`).
/// - Local registers (`r0`, `r1`, ...) have non-negative indices (`>= 0`).
/// - Stack-frame fixed slots (closure, context, bytecode array, etc.) have dedicated negative indices.
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Register {
    index: i32,
}

impl Register {
    pub const INVALID_INDEX: i32 = i32::MAX;

    // Stack frame slot offsets relative to FP divided by pointer size
    pub const REGISTER_FILE_START_OFFSET: i32 = -6;
    pub const FIRST_PARAM_REGISTER_INDEX: i32 = -9;
    pub const FUNCTION_CLOSURE_REGISTER_INDEX: i32 = -5;
    pub const CURRENT_CONTEXT_REGISTER_INDEX: i32 = -4;
    pub const BYTECODE_ARRAY_REGISTER_INDEX: i32 = -3;
    pub const BYTECODE_OFFSET_REGISTER_INDEX: i32 = -2;
    pub const FEEDBACK_VECTOR_REGISTER_INDEX: i32 = -1;
    pub const CALLER_PC_OFFSET_REGISTER_INDEX: i32 = -7;
    pub const ARGUMENT_COUNT_REGISTER_INDEX: i32 = -8;

    /// Creates a new register with the given raw index.
    #[inline(always)]
    pub const fn new(index: i32) -> Self {
        Self { index }
    }

    /// Creates an invalid register instance.
    pub const fn invalid_value() -> Self {
        Self {
            index: Self::INVALID_INDEX,
        }
    }

    /// Returns the raw internal register index.
    #[inline(always)]
    pub const fn index(self) -> i32 {
        self.index
    }

    /// Returns true if this register holds a valid index.
    #[inline(always)]
    pub const fn is_valid(self) -> bool {
        self.index != Self::INVALID_INDEX
    }

    /// Returns true if this register represents a function parameter.
    #[inline(always)]
    pub const fn is_parameter(self) -> bool {
        self.index < 0
    }

    /// Returns a Register corresponding to parameter index (0 for receiver `this`, 1 for first argument, etc.).
    pub const fn from_parameter_index(param_index: i32) -> Self {
        assert!(param_index >= 0);
        Self {
            index: Self::FIRST_PARAM_REGISTER_INDEX - param_index,
        }
    }

    /// Converts this register to its parameter index.
    pub const fn to_parameter_index(self) -> i32 {
        assert!(self.is_parameter());
        Self::FIRST_PARAM_REGISTER_INDEX - self.index
    }

    /// Returns the register representing the receiver (`this`, parameter 0).
    pub const fn receiver() -> Self {
        Self::from_parameter_index(0)
    }

    /// Returns true if this register is the receiver (`this`).
    pub const fn is_receiver(self) -> bool {
        self.is_parameter() && self.to_parameter_index() == 0
    }

    /// Returns the register for the function's closure object.
    pub const fn function_closure() -> Self {
        Self::new(Self::FUNCTION_CLOSURE_REGISTER_INDEX)
    }

    pub const fn is_function_closure(self) -> bool {
        self.index == Self::FUNCTION_CLOSURE_REGISTER_INDEX
    }

    /// Returns the register for the current context object.
    pub const fn current_context() -> Self {
        Self::new(Self::CURRENT_CONTEXT_REGISTER_INDEX)
    }

    pub const fn is_current_context(self) -> bool {
        self.index == Self::CURRENT_CONTEXT_REGISTER_INDEX
    }

    /// Returns the register for the bytecode array.
    pub const fn bytecode_array() -> Self {
        Self::new(Self::BYTECODE_ARRAY_REGISTER_INDEX)
    }

    pub const fn is_bytecode_array(self) -> bool {
        self.index == Self::BYTECODE_ARRAY_REGISTER_INDEX
    }

    /// Returns the register for the saved bytecode offset.
    pub const fn bytecode_offset() -> Self {
        Self::new(Self::BYTECODE_OFFSET_REGISTER_INDEX)
    }

    pub const fn is_bytecode_offset(self) -> bool {
        self.index == Self::BYTECODE_OFFSET_REGISTER_INDEX
    }

    /// Returns the register for the cached feedback vector.
    pub const fn feedback_vector() -> Self {
        Self::new(Self::FEEDBACK_VECTOR_REGISTER_INDEX)
    }

    pub const fn is_feedback_vector(self) -> bool {
        self.index == Self::FEEDBACK_VECTOR_REGISTER_INDEX
    }

    /// Returns the virtual accumulator register.
    pub const fn virtual_accumulator() -> Self {
        Self::new(Self::CALLER_PC_OFFSET_REGISTER_INDEX)
    }

    /// Returns the argument count register.
    pub const fn argument_count() -> Self {
        Self::new(Self::ARGUMENT_COUNT_REGISTER_INDEX)
    }

    /// Converts this register to the biased 32-bit integer operand value encoded in bytecodes.
    #[inline(always)]
    pub const fn to_operand(self) -> i32 {
        Self::REGISTER_FILE_START_OFFSET - self.index
    }

    /// Reconstructs a Register from its encoded operand integer.
    #[inline(always)]
    pub const fn from_operand(operand: i32) -> Self {
        Self::new(Self::REGISTER_FILE_START_OFFSET - operand)
    }

    /// If this register is a local register in the range `r0`..`r15`, returns the corresponding ShortStar bytecode.
    pub fn try_to_short_star(self) -> Option<Bytecode> {
        if self.index >= 0 && self.index < 16 {
            Bytecode::from_short_star_index(self.index as u8)
        } else {
            None
        }
    }

    /// Reconstructs a local register from a ShortStar bytecode (`Star0`..`Star15`).
    pub fn from_short_star(bytecode: Bytecode) -> Self {
        let idx = bytecode
            .short_star_index()
            .expect("Bytecode must be a ShortStar variant");
        Self::new(idx as i32)
    }

    /// Returns the string representation (e.g. `r0`, `a0`, `<this>`, `<context>`).
    pub fn to_string_name(&self) -> String {
        if !self.is_valid() {
            return "<invalid>".to_string();
        }
        if self.is_receiver() {
            return "<this>".to_string();
        }
        if self.is_parameter() {
            let param_idx = self.to_parameter_index();
            if param_idx >= 0 {
                return format!("a{}", param_idx);
            }
            if self.is_function_closure() {
                return "<closure>".to_string();
            }
            if self.is_current_context() {
                return "<context>".to_string();
            }
            if self.is_bytecode_array() {
                return "<bytecode_array>".to_string();
            }
            if self.is_bytecode_offset() {
                return "<bytecode_offset>".to_string();
            }
            if self.is_feedback_vector() {
                return "<feedback_vector>".to_string();
            }
            return format!("reg({})", self.index);
        }
        format!("r{}", self.index)
    }
}

impl fmt::Debug for Register {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Register({})", self.to_string_name())
    }
}

impl fmt::Display for Register {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_name())
    }
}

/// A contiguous sequence of registers used for arguments and call outputs.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RegisterList {
    first_reg_index: i32,
    register_count: usize,
}

impl RegisterList {
    /// Creates an empty register list.
    pub const fn new() -> Self {
        Self {
            first_reg_index: Register::INVALID_INDEX,
            register_count: 0,
        }
    }

    /// Creates a single-element register list.
    pub const fn from_register(r: Register) -> Self {
        Self {
            first_reg_index: r.index(),
            register_count: 1,
        }
    }

    /// Creates a register list covering `count` contiguous registers starting at `first_reg`.
    pub const fn from_range(first_reg: Register, count: usize) -> Self {
        Self {
            first_reg_index: first_reg.index(),
            register_count: count,
        }
    }

    /// Returns the number of registers in this list.
    pub const fn register_count(&self) -> usize {
        self.register_count
    }

    /// Returns the first register, or `Register(0)` if empty.
    pub fn first_register(&self) -> Register {
        if self.register_count == 0 {
            Register::new(0)
        } else {
            Register::new(self.first_reg_index)
        }
    }

    /// Returns the last register, or `Register(0)` if empty.
    pub fn last_register(&self) -> Register {
        if self.register_count == 0 {
            Register::new(0)
        } else {
            Register::new(self.first_reg_index + (self.register_count as i32 - 1))
        }
    }

    /// Returns the i-th register in the list.
    pub fn get(&self, i: usize) -> Register {
        assert!(i < self.register_count, "RegisterList index out of bounds");
        Register::new(self.first_reg_index + i as i32)
    }

    /// Returns a new RegisterList truncated to `new_count` registers.
    pub fn truncate(&self, new_count: usize) -> Self {
        assert!(new_count <= self.register_count);
        Self {
            first_reg_index: self.first_reg_index,
            register_count: new_count,
        }
    }

    /// Returns a new RegisterList with the first register removed.
    pub fn pop_left(&self) -> Self {
        assert!(self.register_count > 0);
        Self {
            first_reg_index: self.first_reg_index + 1,
            register_count: self.register_count - 1,
        }
    }
}

impl Default for RegisterList {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RegisterListIter {
    list: RegisterList,
    current: usize,
}

impl Iterator for RegisterListIter {
    type Item = Register;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current < self.list.register_count {
            let reg = self.list.get(self.current);
            self.current += 1;
            Some(reg)
        } else {
            None
        }
    }
}

impl IntoIterator for RegisterList {
    type Item = Register;
    type IntoIter = RegisterListIter;
    fn into_iter(self) -> Self::IntoIter {
        RegisterListIter {
            list: self,
            current: 0,
        }
    }
}
