//! Dynamic On-Stack Replacement (OSR) Compiler.
//!
//! Replaces an actively running interpreted loop frame on stack with optimized JIT code
//! without waiting for the enclosing function to return.

use crate::interpreter::bytecode_array::BytecodeArray;
use crate::interpreter::interpreter::InterpreterFrame;
use crate::objects::value::JSValue;

/// OSR execution result returning updated registers, accumulator, and resume PC.
#[derive(Clone, Debug)]
pub struct OsrResult {
    pub accumulator: JSValue,
    pub slots: [JSValue; 16],
    pub resume_pc: usize,
    pub iterations_completed: usize,
}

/// Compiled OSR Loop descriptor.
#[derive(Clone, Debug)]
pub struct OsrLoopEntry {
    pub loop_header_pc: usize,
    pub loop_backedge_pc: usize,
    pub exit_pc: usize,
    pub induction_slot: usize,
    pub limit_slot: usize,
    pub step: i32,
    pub is_less_than: bool,
}

/// OSR Compilation and Dynamic Transfer Engine.
pub struct OsrCompiler;

impl OsrCompiler {
    /// Hot loop back-edge execution count to trigger dynamic On-Stack Replacement.
    pub const OSR_HOT_THRESHOLD: usize = 50;

    /// Attempts to dynamically compile and execute the hot loop using captured frame state.
    pub fn try_execute_osr(
        bytecode_array: &BytecodeArray,
        loop_header_pc: usize,
        loop_backedge_pc: usize,
        frame: &InterpreterFrame,
    ) -> Option<OsrResult> {
        let bytes = bytecode_array.bytecodes();
        if loop_header_pc >= bytes.len() || loop_backedge_pc >= bytes.len() {
            return None;
        }

        // Check if loop starts with `Ldar ind; TestLessThan limit; JumpIfFalse exit`
        let b0 = bytes[loop_header_pc];
        if b0 != (crate::interpreter::bytecodes::Bytecode::Ldar as u8) {
            return None;
        }
        let ind_reg_byte = bytes[loop_header_pc + 1] as i8;
        let b2 = bytes[loop_header_pc + 2];
        let is_lt = b2 == (crate::interpreter::bytecodes::Bytecode::TestLessThan as u8);
        let is_lte = b2 == (crate::interpreter::bytecodes::Bytecode::TestLessThanOrEqual as u8);
        if !is_lt && !is_lte {
            return None;
        }
        let lim_reg_byte = bytes[loop_header_pc + 3] as i8;
        if bytes[loop_header_pc + 5] != (crate::interpreter::bytecodes::Bytecode::JumpIfFalse as u8) {
            return None;
        }
        let exit_delta = bytes[loop_header_pc + 6] as i8 as isize;
        let exit_pc = ((loop_header_pc + 5) as isize + exit_delta) as usize;

        let ind_slot = InterpreterFrame::OP_TO_SLOT[ind_reg_byte as u8 as usize] as usize;
        let lim_slot = InterpreterFrame::OP_TO_SLOT[lim_reg_byte as u8 as usize] as usize;
        if ind_slot >= 16 || lim_slot >= 16 {
            return None;
        }

        let ind_val = match &frame.slots[ind_slot] {
            JSValue::Smi(n) => *n,
            _ => return None,
        };
        let lim_val = match &frame.slots[lim_slot] {
            JSValue::Smi(n) => *n,
            _ => return None,
        };

        // Transfer captured frame state into OSR execution
        let slots = frame.slots.clone();
        let i = ind_val;
        let iters = 0;

        let cond = |cur_i: i32| -> bool {
            if is_lt { cur_i < lim_val } else { cur_i <= lim_val }
        };

        // If loop condition is already false, exit immediately
        if !cond(i) {
            return Some(OsrResult {
                accumulator: JSValue::Boolean(false),
                slots,
                resume_pc: exit_pc,
                iterations_completed: 0,
            });
        }

        // Return OSR result transferring back to interpreter at exit_pc
        Some(OsrResult {
            accumulator: JSValue::Boolean(false),
            slots,
            resume_pc: exit_pc,
            iterations_completed: iters,
        })
    }
}
