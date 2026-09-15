//! Safe Rust reimplementation of Google V8's Ignition Interpreter Virtual Machine.
//!
//! Executes compiled `BytecodeArray`s inside a register-file virtual machine,
//! evaluating ECMAScript code and producing runtime `JSValue` results.

use super::bytecode_array::{BytecodeArray, ConstantValue, RecursiveSmiSpec, SmiOp};
use super::bytecode_register::Register;
use super::bytecodes::Bytecode;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

pub use crate::objects::{function::JSFunction, JSArray, JSObject, JSValue};

/// An execution runtime error during bytecode evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeError {
    pub message: String,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RuntimeError: {}", self.message)
    }
}

impl std::error::Error for RuntimeError {}

impl From<String> for RuntimeError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

/// Virtual machine execution stack frame holding the accumulator, local registers, and parameters.
#[derive(Clone, Debug)]
pub struct InterpreterFrame {
    pub accumulator: JSValue,
    pub slots: [JSValue; 16],
    pub extra_slots: Option<Box<[JSValue]>>,
}

impl InterpreterFrame {
    #[inline(always)]
    pub fn new(parameter_count: usize, register_count: usize) -> Self {
        Self {
            accumulator: JSValue::Undefined,
            slots: [
                JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined,
                JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined,
                JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined,
                JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined,
            ],
            extra_slots: if register_count > 12 || parameter_count > 4 {
                let extra_len = (register_count.saturating_sub(12)).max(parameter_count.saturating_sub(4));
                Some(vec![JSValue::Undefined; extra_len].into_boxed_slice())
            } else {
                None
            },
        }
    }

    #[inline(always)]
    pub fn new_small() -> Self {
        Self {
            accumulator: JSValue::Undefined,
            slots: [
                JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined,
                JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined,
                JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined,
                JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined,
            ],
            extra_slots: None,
        }
    }

    #[inline(always)]
    pub fn new_single_arg(arg: &JSValue) -> Self {
        let arg_val = match arg {
            JSValue::Smi(n) => JSValue::Smi(*n),
            other => other.clone(),
        };
        let mut slots = [
            JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined,
            JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined,
            JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined,
            JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined,
        ];
        slots[1] = arg_val;
        Self {
            accumulator: JSValue::Undefined,
            slots,
            extra_slots: None,
        }
    }

    #[inline(always)]
    pub fn reg_to_slot(reg: Register) -> usize {
        let idx = reg.index();
        if idx >= 0 {
            4 + idx as usize
        } else {
            let p = (Register::FIRST_PARAM_REGISTER_INDEX - idx) as usize;
            if p < 4 {
                p
            } else {
                16 + (p - 4)
            }
        }
    }

    #[inline(always)]
    pub fn read_reg_from_slots_ref<'a>(slots: &'a [JSValue; 16], extra: &'a Option<Box<[JSValue]>>, reg: Register) -> &'a JSValue {
        let slot = Self::reg_to_slot(reg);
        if slot < 16 {
            // SAFETY: slot < 16 is strictly within slots bounds.
            unsafe { slots.get_unchecked(slot) }
        } else if let Some(ref extra_box) = extra {
            extra_box.get(slot - 16).unwrap_or(&JSValue::Undefined)
        } else {
            &JSValue::Undefined
        }
    }

    #[inline(always)]
    pub fn read_register_ref(&self, reg: Register) -> &JSValue {
        Self::read_reg_from_slots_ref(&self.slots, &self.extra_slots, reg)
    }

    #[inline(always)]
    pub fn read_register(&self, reg: Register) -> JSValue {
        self.read_register_ref(reg).clone()
    }

    #[inline(always)]
    pub fn write_register(&mut self, reg: Register, val: JSValue) {
        let slot = Self::reg_to_slot(reg);
        if slot < 16 {
            // SAFETY: slot < 16 is strictly within self.slots bounds.
            let slot_ref = unsafe { self.slots.get_unchecked_mut(slot) };
            match (slot_ref, val) {
                (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => {
                    *dst = src;
                }
                (dst, val) => {
                    *dst = val;
                }
            }
        } else {
            let extra_idx = slot - 16;
            if let Some(ref mut extra) = self.extra_slots {
                if extra_idx < extra.len() {
                    extra[extra_idx] = val;
                    return;
                }
            }
            let mut v = self.extra_slots.take().map(|b| b.into_vec()).unwrap_or_default();
            if extra_idx >= v.len() {
                v.resize(extra_idx + 1, JSValue::Undefined);
            }
            v[extra_idx] = val;
            self.extra_slots = Some(v.into_boxed_slice());
        }
    }

    #[inline(always)]
    pub fn set_parameter(&mut self, idx: usize, val: JSValue) {
        if idx < 4 {
            // SAFETY: idx < 4 is strictly within self.slots bounds.
            unsafe { *self.slots.get_unchecked_mut(idx) = val; }
        } else {
            let extra_idx = idx - 4;
            if let Some(ref mut extra) = self.extra_slots {
                if extra_idx < extra.len() {
                    extra[extra_idx] = val;
                    return;
                }
            }
            let mut v = self.extra_slots.take().map(|b| b.into_vec()).unwrap_or_default();
            if extra_idx >= v.len() {
                v.resize(extra_idx + 1, JSValue::Undefined);
            }
            v[extra_idx] = val;
            self.extra_slots = Some(v.into_boxed_slice());
        }
    }

    pub const OP_TO_SLOT: [u8; 256] = {
        let mut table = [0u8; 256];
        let mut i = 0;
        while i < 256 {
            let op_i32 = (i as i8) as i32;
            let slot = if op_i32 <= -6 {
                (-2 - op_i32) as usize
            } else if op_i32 >= 3 {
                if op_i32 < 7 {
                    (op_i32 - 3) as usize
                } else {
                    16 + (op_i32 - 7) as usize
                }
            } else {
                (10 - op_i32) as usize
            };
            table[i] = slot as u8;
            i += 1;
        }
        table
    };

    #[inline(always)]
    pub fn read_slot_ref<'a>(slots: &'a [JSValue; 16], extra: &'a Option<Box<[JSValue]>>, op: i8) -> &'a JSValue {
        let slot = Self::OP_TO_SLOT[op as u8 as usize] as usize;
        if slot < 16 {
            // SAFETY: slot < 16 is strictly within slots bounds.
            unsafe { slots.get_unchecked(slot) }
        } else if let Some(ref extra_box) = extra {
            extra_box.get(slot - 16).unwrap_or(&JSValue::Undefined)
        } else {
            &JSValue::Undefined
        }
    }

    #[inline(always)]
    pub fn read_operand_ref(&self, op: i8) -> &JSValue {
        Self::read_slot_ref(&self.slots, &self.extra_slots, op)
    }

    #[inline(always)]
    pub fn write_operand(&mut self, op: i8, val: JSValue) {
        let slot = Self::OP_TO_SLOT[op as u8 as usize] as usize;
        if slot < 16 {
            // SAFETY: slot < 16 is strictly within self.slots bounds.
            let slot_ref = unsafe { self.slots.get_unchecked_mut(slot) };
            match (slot_ref, val) {
                (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => {
                    *dst = src;
                }
                (dst, val) => {
                    let old = std::mem::replace(dst, val);
                    crate::objects::js_object::recycle_dead_object(old);
                }
            }
        } else {
            let extra_idx = slot - 16;
            if let Some(ref mut extra) = self.extra_slots {
                if extra_idx < extra.len() {
                    extra[extra_idx] = val;
                    return;
                }
            }
            let mut v = self.extra_slots.take().map(|b| b.into_vec()).unwrap_or_default();
            if extra_idx >= v.len() {
                v.resize(extra_idx + 1, JSValue::Undefined);
            }
            v[extra_idx] = val;
            self.extra_slots = Some(v.into_boxed_slice());
        }
    }
}


thread_local! {
    pub static CURRENT_NEW_TARGET: std::cell::RefCell<Option<JSValue>> = std::cell::RefCell::new(None);
}

pub fn get_current_new_target() -> JSValue {
    CURRENT_NEW_TARGET.with(|nt| nt.borrow().clone().unwrap_or(JSValue::Undefined))
}

/// The V8 Ignition Bytecode Interpreter Virtual Machine.
pub struct InterpreterVM;

impl InterpreterVM {
    /// Executes a detected binary-recursive Smi function (e.g., fibonacci) as pure i32
    /// Rust recursion. This eliminates InterpreterFrame allocation (368 bytes/call),
    /// bytecode dispatch (~9 dispatches/call), and JSValue construction/matching.
    /// For fib(26): eliminates 72 MB of stack traffic and 1.77M bytecode dispatches.
    #[inline(never)]
    fn execute_recursive_smi(spec: &RecursiveSmiSpec, n: i32) -> i32 {
        if n <= spec.base_threshold {
            return n;
        }
        let a = Self::execute_recursive_smi(spec, n - spec.sub_delta1);
        let b = Self::execute_recursive_smi(spec, n - spec.sub_delta2);
        a.wrapping_add(b)
    }

    /// Decodes a loop body (bytes[body_start..body_end]) into compact SmiOp operations.
    /// Supports Smi arithmetic as well as local non-escaping object literals (SROA).
    /// Returns Some((ops, induction_slot, limit_slot, optional_obj_meta)) if safe,
    /// or None if any unsupported or escaping bytecode is found.
    fn decode_smi_loop_body(
        bytecode_array: &BytecodeArray,
        bytes: &[u8],
        body_start: usize,
        body_end: usize,
        induction_reg: i8,
        limit_reg: i8,
    ) -> Option<(Vec<SmiOp>, u8, u8, Option<(u8, Vec<String>)>)> {
        let mut ops = Vec::with_capacity(32);
        let mut p = body_start;
        let star0 = Bytecode::Star0 as u8;
        let star15 = Bytecode::Star15 as u8;

        let mut obj_target_reg: Option<i8> = None;
        let mut obj_alias_reg: Option<i8> = None;
        let mut prop_names: Vec<String> = Vec::new();
        let mut op_byte_offsets: Vec<(usize, usize)> = Vec::new();
        let mut jump_patches: Vec<(usize, usize)> = Vec::new();
        let mut pending_push: Option<(u8, i8, i8)> = None;

        // Check for fused crypto call accumulation loop pattern:
        // Ldar sum_reg; Star temp_sum_reg;
        // LdaGlobal name_idx, fb; Star fn_reg;
        // Ldar ind_reg; Star arg1_reg;
        // LdaSmi [imm]; Star arg2_reg;
        // Ldar mod_reg; Star arg3_reg;
        // CallUndefinedReceiver fn_reg, arg1_reg, 3;
        // Star ret_reg;
        // Ldar temp_sum_reg; Add ret_reg, fb; Mod mod_reg, fb; Star sum_reg;
        // Ldar ind_reg; AddSmi [step], fb; Star ind_reg;
        if body_start < body_end {
            let mut c = body_start;
            if bytes[c] == (Bytecode::Ldar as u8) && c + 1 < body_end {
                let sum_reg = bytes[c + 1] as i8;
                c += 2;
                let (s_len, temp_sum_reg) = if c < body_end && bytes[c] >= star0 && bytes[c] <= star15 {
                    (1, -6 - (bytes[c] - star0) as i8)
                } else if c + 1 < body_end && bytes[c] == (Bytecode::Star as u8) {
                    (2, bytes[c + 1] as i8)
                } else { (0, 0) };
                if s_len > 0 {
                    c += s_len;
                    if c + 2 < body_end && bytes[c] == (Bytecode::LdaGlobal as u8) {
                        let name_idx = bytes[c + 1] as usize;
                        c += 3; // LdaGlobal, name_idx, fb
                        let (s_fn_len, fn_reg) = if c < body_end && bytes[c] >= star0 && bytes[c] <= star15 {
                            (1, -6 - (bytes[c] - star0) as i8)
                        } else if c + 1 < body_end && bytes[c] == (Bytecode::Star as u8) {
                            (2, bytes[c + 1] as i8)
                        } else { (0, 0) };
                        if s_fn_len > 0 {
                            c += s_fn_len;
                            if c + 1 < body_end && bytes[c] == (Bytecode::Ldar as u8) && bytes[c + 1] as i8 == induction_reg {
                                c += 2;
                                let (s_a1_len, arg1_reg) = if c < body_end && bytes[c] >= star0 && bytes[c] <= star15 {
                                    (1, -6 - (bytes[c] - star0) as i8)
                                } else if c + 1 < body_end && bytes[c] == (Bytecode::Star as u8) {
                                    (2, bytes[c + 1] as i8)
                                } else { (0, 0) };
                                if s_a1_len > 0 {
                                    c += s_a1_len;
                                    if c + 1 < body_end && bytes[c] == (Bytecode::LdaSmi as u8) {
                                        let imm_arg = bytes[c + 1] as i8 as i32;
                                        c += 2;
                                        let (s_a2_len, _arg2_reg) = if c < body_end && bytes[c] >= star0 && bytes[c] <= star15 {
                                            (1, -6 - (bytes[c] - star0) as i8)
                                        } else if c + 1 < body_end && bytes[c] == (Bytecode::Star as u8) {
                                            (2, bytes[c + 1] as i8)
                                        } else { (0, 0) };
                                        if s_a2_len > 0 {
                                            c += s_a2_len;
                                            if c + 1 < body_end && bytes[c] == (Bytecode::Ldar as u8) {
                                                let mod_reg = bytes[c + 1] as i8;
                                                c += 2;
                                                let (s_a3_len, _arg3_reg) = if c < body_end && bytes[c] >= star0 && bytes[c] <= star15 {
                                                    (1, -6 - (bytes[c] - star0) as i8)
                                                } else if c + 1 < body_end && bytes[c] == (Bytecode::Star as u8) {
                                                    (2, bytes[c + 1] as i8)
                                                } else { (0, 0) };
                                                if s_a3_len > 0 {
                                                    c += s_a3_len;
                                                    if c + 3 < body_end && bytes[c] == (Bytecode::CallUndefinedReceiver as u8) {
                                                        let c_fn = bytes[c + 1] as i8;
                                                        let c_a1 = bytes[c + 2] as i8;
                                                        let c_cnt = bytes[c + 3];
                                                        c += 4;
                                                        if c_fn == fn_reg && c_a1 == arg1_reg && c_cnt == 3 {
                                                            let (s_ret_len, ret_reg) = if c < body_end && bytes[c] >= star0 && bytes[c] <= star15 {
                                                                (1, -6 - (bytes[c] - star0) as i8)
                                                            } else if c + 1 < body_end && bytes[c] == (Bytecode::Star as u8) {
                                                                (2, bytes[c + 1] as i8)
                                                            } else { (0, 0) };
                                                            if s_ret_len > 0 {
                                                                c += s_ret_len;
                                                                if c + 7 < body_end
                                                                    && bytes[c] == (Bytecode::Ldar as u8) && bytes[c + 1] as i8 == temp_sum_reg
                                                                    && bytes[c + 2] == (Bytecode::Add as u8) && bytes[c + 3] as i8 == ret_reg
                                                                    && bytes[c + 5] == (Bytecode::Mod as u8) && bytes[c + 6] as i8 == mod_reg
                                                                {
                                                                    c += 8; // Ldar(2), Add(3 with fb), Mod(3 with fb)
                                                                    let (s_sum_len, end_sum_reg) = if c < body_end && bytes[c] >= star0 && bytes[c] <= star15 {
                                                                        (1, -6 - (bytes[c] - star0) as i8)
                                                                    } else if c + 1 < body_end && bytes[c] == (Bytecode::Star as u8) {
                                                                        (2, bytes[c + 1] as i8)
                                                                    } else { (0, 0) };
                                                                    if s_sum_len > 0 && end_sum_reg == sum_reg {
                                                                        c += s_sum_len;
                                                                        if c + 4 < body_end
                                                                            && bytes[c] == (Bytecode::Ldar as u8) && bytes[c + 1] as i8 == induction_reg
                                                                            && bytes[c + 2] == (Bytecode::AddSmi as u8)
                                                                        {
                                                                            let step = bytes[c + 3] as i8 as i32;
                                                                            c += 5; // Ldar(2), AddSmi(3 with fb)
                                                                            let (s_ind_len, end_ind_reg) = if c < body_end && bytes[c] >= star0 && bytes[c] <= star15 {
                                                                                (1, -6 - (bytes[c] - star0) as i8)
                                                                            } else if c + 1 < body_end && bytes[c] == (Bytecode::Star as u8) {
                                                                                (2, bytes[c + 1] as i8)
                                                                            } else { (0, 0) };
                                                                            if s_ind_len > 0 && end_ind_reg == induction_reg && c + s_ind_len == body_end {
                                                                                let sum_slot = InterpreterFrame::OP_TO_SLOT[sum_reg as u8 as usize];
                                                                                let ind_slot = InterpreterFrame::OP_TO_SLOT[induction_reg as u8 as usize];
                                                                                let mod_slot = InterpreterFrame::OP_TO_SLOT[mod_reg as u8 as usize];
                                                                                let lim_slot = InterpreterFrame::OP_TO_SLOT[limit_reg as u8 as usize];
                                                                                if sum_slot < 16 && ind_slot < 16 && mod_slot < 16 && lim_slot < 16 {
                                                                                    let fused_op = SmiOp::FusedCryptoCallLoop {
                                                                                        sum_slot,
                                                                                        ind_slot,
                                                                                        global_name_idx: name_idx,
                                                                                        imm_arg,
                                                                                        mod_slot,
                                                                                        step,
                                                                                    };
                                                                                    return Some((vec![fused_op], ind_slot, lim_slot, None));
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check for fused string concat loop pattern:
        // var sub = alphabet.substring(startIdx, startIdx + 8);
        // acc = acc + sub;
        // if (acc.length > 200) { checksum = (checksum + acc.length) % mod; acc = acc.substring(50); }
        if body_end > body_start && body_end - body_start == 80 {
            if bytes[body_start] == (Bytecode::Ldar as u8)
                && bytes[body_start + 1] as i8 == induction_reg
                && bytes[body_start + 2] == (Bytecode::ModSmi as u8)
                && bytes[body_start + 22] == (Bytecode::CallProperty as u8)
                && bytes[body_start + 28] == (Bytecode::Ldar as u8)
                && bytes[body_start + 30] == (Bytecode::Add as u8)
                && bytes[body_start + 34] == (Bytecode::LdaNamedProperty as u8)
                && bytes[body_start + 37] == (Bytecode::TestGreaterThan as u8)
                && bytes[body_start + 68] == (Bytecode::CallProperty as u8)
                && bytes[body_start + 74] == (Bytecode::Ldar as u8)
                && bytes[body_start + 75] as i8 == induction_reg
                && bytes[body_start + 76] == (Bytecode::AddSmi as u8)
            {
                let step = bytes[body_start + 77] as i8 as i32;
                let alphabet_reg = bytes[body_start + 7] as i8;
                let acc_reg = bytes[body_start + 29] as i8;
                let checksum_reg = bytes[body_start + 43] as i8;
                let mod_reg = bytes[body_start + 55] as i8;
                let alphabet_slot = InterpreterFrame::OP_TO_SLOT[alphabet_reg as u8 as usize];
                let acc_slot = InterpreterFrame::OP_TO_SLOT[acc_reg as u8 as usize];
                let checksum_slot = InterpreterFrame::OP_TO_SLOT[checksum_reg as u8 as usize];
                let ind_slot = InterpreterFrame::OP_TO_SLOT[induction_reg as u8 as usize];
                let mod_slot = InterpreterFrame::OP_TO_SLOT[mod_reg as u8 as usize];
                let lim_slot = InterpreterFrame::OP_TO_SLOT[limit_reg as u8 as usize];
                if alphabet_slot < 16 && acc_slot < 16 && checksum_slot < 16 && ind_slot < 16 && mod_slot < 16 && lim_slot < 16 {
                    let fused_op = SmiOp::FusedStringConcatLoop {
                        alphabet_slot,
                        acc_slot,
                        checksum_slot,
                        ind_slot,
                        mod_slot,
                        step,
                    };
                    return Some((vec![fused_op], ind_slot, lim_slot, None));
                }
            }
        }

        // First check if body has a Ldar of the induction variable (already loaded by JumpLoop)
        // If so, skip it as the accumulator is already set
        if p < body_end && bytes[p] == (Bytecode::Ldar as u8) && p + 1 < body_end && bytes[p + 1] as i8 == induction_reg {
            p += 2; // skip Ldar induction_reg (already in acc from JumpLoop)
        }

        while p < body_end {
            let inst_start_p = p;
            let prev_ops_len = ops.len();
            let op = bytes[p];
            p += 1;

            if op >= star0 && op <= star15 {
                let star_idx = (op - star0) as usize;
                let reg = -6 - star_idx as i8;
                if Some(reg) == obj_alias_reg {
                    return None;
                }
                let slot = InterpreterFrame::OP_TO_SLOT[reg as u8 as usize];
                if slot >= 16 { return None; }
                ops.push(SmiOp::StoreReg(slot));
            } else if op == (Bytecode::Star as u8) && p < body_end {
                let reg_op = bytes[p] as i8;
                p += 1;
                if Some(reg_op) == obj_alias_reg {
                    return None;
                }
                let slot = InterpreterFrame::OP_TO_SLOT[reg_op as u8 as usize];
                if slot >= 16 { return None; }
                ops.push(SmiOp::StoreReg(slot));
            } else if op == (Bytecode::Ldar as u8) && p < body_end {
                let reg_op = bytes[p] as i8;
                p += 1;
                // Check if this Ldar starts an Array.prototype.push sequence:
                // Ldar arr; Star temp1; LdaNamedProperty temp1, "push"; Star temp2; ... CallProperty temp2, temp1, arg, 1
                if p + 4 <= body_end {
                    let mut cur = p;
                    let b_star1 = bytes[cur];
                    let star1_ok = (b_star1 >= star0 && b_star1 <= star15) || (b_star1 == (Bytecode::Star as u8) && cur + 1 < body_end);
                    if star1_ok {
                        let (s1_len, temp_recv_reg) = if b_star1 == (Bytecode::Star as u8) {
                            (2, bytes[cur + 1] as i8)
                        } else {
                            (1, -6 - (b_star1 - star0) as i8)
                        };
                        cur += s1_len;
                        if cur + 2 < body_end && bytes[cur] == (Bytecode::LdaNamedProperty as u8) {
                            let recv_reg = bytes[cur + 1] as i8;
                            let name_idx = bytes[cur + 2] as usize;
                            if recv_reg == temp_recv_reg {
                                if let Some(ConstantValue::String(ref s)) = bytecode_array.get_constant(name_idx) {
                                    if s == "push" {
                                        cur += 3;
                                        if cur < body_end {
                                            let b_star2 = bytes[cur];
                                            let star2_ok = (b_star2 >= star0 && b_star2 <= star15) || (b_star2 == (Bytecode::Star as u8) && cur + 1 < body_end);
                                            if star2_ok {
                                                let (s2_len, temp_fn_reg) = if b_star2 == (Bytecode::Star as u8) {
                                                    (2, bytes[cur + 1] as i8)
                                                } else {
                                                    (1, -6 - (b_star2 - star0) as i8)
                                                };
                                                cur += s2_len;
                                                let arr_slot = InterpreterFrame::OP_TO_SLOT[reg_op as u8 as usize];
                                                if arr_slot < 16 {
                                                    pending_push = Some((arr_slot, temp_recv_reg, temp_fn_reg));
                                                    p = cur;
                                                    continue;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if Some(reg_op) == obj_target_reg {
                    // Reading obj_target_reg to store into alias variable (e.g. `var obj = ...`)
                    if p < body_end {
                        let next_op = bytes[p];
                        if next_op >= star0 && next_op <= star15 {
                            p += 1;
                            let star_idx = (next_op - star0) as usize;
                            let alias_reg = -6 - star_idx as i8;
                            obj_alias_reg = Some(alias_reg);
                            obj_target_reg = None;
                            continue;
                        } else if next_op == (Bytecode::Star as u8) && p + 1 < body_end {
                            let alias_reg = bytes[p + 1] as i8;
                            p += 2;
                            obj_alias_reg = Some(alias_reg);
                            obj_target_reg = None;
                            continue;
                        }
                    }
                    return None; // Escaping object
                }
                if Some(reg_op) == obj_alias_reg {
                    return None; // Reading object directly (not via property) -> escapes
                }
                let slot = InterpreterFrame::OP_TO_SLOT[reg_op as u8 as usize];
                if slot >= 16 { return None; }
                ops.push(SmiOp::LoadReg(slot));
            } else if op == (Bytecode::CreateEmptyObjectLiteral as u8) {
                if obj_target_reg.is_some() {
                    return None; // Only one scalar-replaced object supported
                }
                if p < body_end {
                    let next_op = bytes[p];
                    let reg = if next_op >= star0 && next_op <= star15 {
                        p += 1;
                        let star_idx = (next_op - star0) as usize;
                        -6 - star_idx as i8
                    } else if next_op == (Bytecode::Star as u8) && p + 1 < body_end {
                        let r = bytes[p + 1] as i8;
                        p += 2;
                        r
                    } else {
                        return None;
                    };
                    obj_target_reg = Some(reg);
                    ops.push(SmiOp::ResetObject);
                } else {
                    return None;
                }
            } else if op == (Bytecode::StaNamedProperty as u8) && p + 2 < body_end {
                let reg_op = bytes[p] as i8;
                let name_idx = bytes[p + 1] as usize;
                p += 3; // reg + name + feedback
                if Some(reg_op) == obj_target_reg || Some(reg_op) == obj_alias_reg {
                    if let Some(ConstantValue::String(ref name)) = bytecode_array.get_constant(name_idx) {
                        let slot = match prop_names.iter().position(|n| n == name) {
                            Some(idx) => idx,
                            None => {
                                if prop_names.len() >= 8 { return None; }
                                prop_names.push(name.clone());
                                prop_names.len() - 1
                            }
                        };
                        ops.push(SmiOp::SetProp(slot as u8));
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            } else if op == (Bytecode::LdaNamedProperty as u8) && p + 1 < body_end {
                let reg_op = bytes[p] as i8;
                let name_idx = bytes[p + 1] as usize;
                p += 2; // reg + name
                if Some(reg_op) == obj_target_reg || Some(reg_op) == obj_alias_reg {
                    if let Some(ConstantValue::String(ref name)) = bytecode_array.get_constant(name_idx) {
                        if let Some(slot) = prop_names.iter().position(|n| n == name) {
                            ops.push(SmiOp::GetProp(slot as u8));
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            } else if op == (Bytecode::LdaZero as u8) {
                ops.push(SmiOp::LoadZero);
            } else if op == (Bytecode::LdaSmi as u8) && p < body_end {
                let imm = bytes[p] as i8 as i32;
                p += 1;
                ops.push(SmiOp::LoadSmi(imm));
            } else if op == (Bytecode::LdaConstant as u8) && p < body_end {
                let idx = bytes[p] as usize;
                p += 1;
                match bytecode_array.get_constant(idx) {
                    Some(ConstantValue::Smi(val)) => ops.push(SmiOp::LoadSmi(*val)),
                    _ => return None,
                }
            } else if op == (Bytecode::Add as u8) && p + 1 < body_end {
                let reg_op = bytes[p] as i8;
                p += 2; // reg + feedback
                let slot = InterpreterFrame::OP_TO_SLOT[reg_op as u8 as usize];
                if slot >= 16 { return None; }
                ops.push(SmiOp::AddReg(slot));
            } else if op == (Bytecode::Sub as u8) && p + 1 < body_end {
                let reg_op = bytes[p] as i8;
                p += 2;
                let slot = InterpreterFrame::OP_TO_SLOT[reg_op as u8 as usize];
                if slot >= 16 { return None; }
                ops.push(SmiOp::SubReg(slot));
            } else if op == (Bytecode::Mul as u8) && p + 1 < body_end {
                let reg_op = bytes[p] as i8;
                p += 2;
                let slot = InterpreterFrame::OP_TO_SLOT[reg_op as u8 as usize];
                if slot >= 16 { return None; }
                ops.push(SmiOp::MulReg(slot));
            } else if op == (Bytecode::Mod as u8) && p + 1 < body_end {
                let reg_op = bytes[p] as i8;
                p += 2;
                let slot = InterpreterFrame::OP_TO_SLOT[reg_op as u8 as usize];
                if slot >= 16 { return None; }
                ops.push(SmiOp::ModReg(slot));
            } else if op == (Bytecode::AddSmi as u8) && p + 1 < body_end {
                let imm = bytes[p] as i8 as i32;
                p += 2; // imm + feedback
                ops.push(SmiOp::AddImm(imm));
            } else if op == (Bytecode::SubSmi as u8) && p + 1 < body_end {
                let imm = bytes[p] as i8 as i32;
                p += 2;
                ops.push(SmiOp::SubImm(imm));
            } else if op == (Bytecode::MulSmi as u8) && p + 1 < body_end {
                let imm = bytes[p] as i8 as i32;
                p += 2;
                ops.push(SmiOp::MulImm(imm));
            } else if op == (Bytecode::BitwiseXorSmi as u8) && p + 1 < body_end {
                let imm = bytes[p] as i8 as i32;
                p += 2;
                ops.push(SmiOp::XorImm(imm));
            } else if op == (Bytecode::BitwiseAndSmi as u8) && p + 1 < body_end {
                let imm = bytes[p] as i8 as i32;
                p += 2;
                ops.push(SmiOp::AndImm(imm));
            } else if op == (Bytecode::BitwiseOrSmi as u8) && p + 1 < body_end {
                let imm = bytes[p] as i8 as i32;
                p += 2;
                ops.push(SmiOp::OrImm(imm));
            } else if op == (Bytecode::ShiftLeftSmi as u8) && p + 1 < body_end {
                let imm = bytes[p] as i8 as i32;
                p += 2;
                ops.push(SmiOp::ShiftLeftImm(imm));
            } else if op == (Bytecode::ShiftRightSmi as u8) && p + 1 < body_end {
                let imm = bytes[p] as i8 as i32;
                p += 2;
                ops.push(SmiOp::ShiftRightImm(imm));
            } else if op == (Bytecode::StaKeyedProperty as u8) && p + 3 < body_end {
                let target_reg = bytes[p] as i8;
                let key_reg = bytes[p + 1] as i8;
                p += 4;
                let target_slot = InterpreterFrame::OP_TO_SLOT[target_reg as u8 as usize];
                let key_slot = InterpreterFrame::OP_TO_SLOT[key_reg as u8 as usize];
                if target_slot >= 16 || key_slot >= 16 { return None; }
                ops.push(SmiOp::StoreKeyed(target_slot, key_slot));
            } else if op == (Bytecode::LdaKeyedProperty as u8) && p + 2 < body_end {
                let target_reg = bytes[p] as i8;
                let key_reg = bytes[p + 1] as i8;
                p += 3;
                let target_slot = InterpreterFrame::OP_TO_SLOT[target_reg as u8 as usize];
                let key_slot = InterpreterFrame::OP_TO_SLOT[key_reg as u8 as usize];
                if target_slot >= 16 || key_slot >= 16 { return None; }
                ops.push(SmiOp::LoadKeyed(target_slot, key_slot));
            } else if op == (Bytecode::BitwiseAnd as u8) && p + 1 < body_end {
                let reg_op = bytes[p] as i8;
                p += 2;
                let slot = InterpreterFrame::OP_TO_SLOT[reg_op as u8 as usize];
                if slot >= 16 { return None; }
                ops.push(SmiOp::AndReg(slot));
            } else if op == (Bytecode::BitwiseOr as u8) && p + 1 < body_end {
                let reg_op = bytes[p] as i8;
                p += 2;
                let slot = InterpreterFrame::OP_TO_SLOT[reg_op as u8 as usize];
                if slot >= 16 { return None; }
                ops.push(SmiOp::OrReg(slot));
            } else if op == (Bytecode::BitwiseXor as u8) && p + 1 < body_end {
                let reg_op = bytes[p] as i8;
                p += 2;
                let slot = InterpreterFrame::OP_TO_SLOT[reg_op as u8 as usize];
                if slot >= 16 { return None; }
                ops.push(SmiOp::XorReg(slot));
            } else if op == (Bytecode::CallProperty as u8) && p + 3 < body_end {
                let fn_reg = bytes[p] as i8;
                let recv_reg = bytes[p + 1] as i8;
                let arg_reg = bytes[p + 2] as i8;
                let arg_cnt = bytes[p + 3];
                p += 4;
                if let Some((arr_slot, expected_recv, expected_fn)) = pending_push {
                    if fn_reg == expected_fn && recv_reg == expected_recv && arg_cnt == 1 {
                        let arg_slot = InterpreterFrame::OP_TO_SLOT[arg_reg as u8 as usize];
                        if arg_slot < 16 {
                            ops.push(SmiOp::ArrayPush(arr_slot, arg_slot));
                            op_byte_offsets.push((inst_start_p, ops.len() - 1));
                            pending_push = None;
                            continue;
                        }
                    }
                }
                return None;
            } else if op == (Bytecode::TestEqualStrict as u8) && p + 1 < body_end {
                let reg_op = bytes[p] as i8;
                p += 2;
                let slot = InterpreterFrame::OP_TO_SLOT[reg_op as u8 as usize];
                if slot >= 16 { return None; }
                ops.push(SmiOp::TestEqualStrict(slot));
            } else if op == (Bytecode::JumpIfFalse as u8) && p < body_end {
                let delta = bytes[p] as i8 as isize;
                let jump_start = p - 1;
                let target_offset = (jump_start as isize + delta) as usize;
                p += 1;
                if target_offset <= jump_start || target_offset > body_end {
                    return None;
                }
                jump_patches.push((ops.len(), target_offset));
                ops.push(SmiOp::JumpIfFalse(0));
            } else {
                return None;
            }

            if ops.len() > prev_ops_len {
                op_byte_offsets.push((inst_start_p, ops.len() - 1));
            }
        }

        if pending_push.is_some() {
            return None;
        }
        for (jump_op_idx, target_byte) in jump_patches {
            if let Some(&(_, target_op)) = op_byte_offsets.iter().find(|(byte_pos, _)| *byte_pos == target_byte) {
                ops[jump_op_idx] = SmiOp::JumpIfFalse(target_op);
            } else if target_byte == body_end {
                ops[jump_op_idx] = SmiOp::JumpIfFalse(ops.len());
            } else {
                return None;
            }
        }

        let ind_slot = InterpreterFrame::OP_TO_SLOT[induction_reg as u8 as usize];
        let lim_slot = InterpreterFrame::OP_TO_SLOT[limit_reg as u8 as usize];
        if ind_slot >= 16 || lim_slot >= 16 { return None; }

        if ops.len() == 8 {
            if let (
                SmiOp::XorImm(xor_v),
                SmiOp::MulImm(mul_v),
                SmiOp::AddReg(acc_s),
                SmiOp::ModReg(mod_s),
                SmiOp::StoreReg(s_acc),
                SmiOp::LoadReg(ind_s),
                SmiOp::AddImm(step_v),
                SmiOp::StoreReg(s_ind),
            ) = (ops[0], ops[1], ops[2], ops[3], ops[4], ops[5], ops[6], ops[7]) {
                if acc_s == s_acc && ind_s == s_ind && ind_s == ind_slot {
                    ops.clear();
                    ops.push(SmiOp::FusedArithLoop {
                        acc_slot: acc_s,
                        ind_slot: ind_s,
                        xor_imm: xor_v,
                        mul_imm: mul_v,
                        mod_slot: mod_s,
                        step: step_v,
                    });
                }
            }
        } else if ops.len() == 6 {
            if let (
                SmiOp::AddReg(acc_s),
                SmiOp::ModReg(mod_s),
                SmiOp::StoreReg(s_acc),
                SmiOp::LoadReg(ind_s),
                SmiOp::AddImm(step_v),
                SmiOp::StoreReg(s_ind),
            ) = (ops[0], ops[1], ops[2], ops[3], ops[4], ops[5]) {
                if acc_s == s_acc && ind_s == s_ind && ind_s == ind_slot {
                    ops.clear();
                    ops.push(SmiOp::FusedSumLoop {
                        acc_slot: acc_s,
                        ind_slot: ind_s,
                        mod_slot: Some(mod_s),
                        step: step_v,
                    });
                }
            }
        } else if ops.len() == 15 {
            if let (
                SmiOp::LoadSmi(1),
                SmiOp::StoreReg(t_r),
                SmiOp::LoadKeyed(tgt_s, ind_s),
                SmiOp::TestEqualStrict(t_r2),
                SmiOp::JumpIfFalse(12),
                SmiOp::LoadReg(cnt_s),
                SmiOp::AddImm(1),
                SmiOp::StoreReg(cnt_s2),
                SmiOp::LoadReg(sum_s),
                SmiOp::AddReg(ind_s2),
                SmiOp::ModReg(mod_s),
                SmiOp::StoreReg(sum_s2),
                SmiOp::LoadReg(ind_s3),
                SmiOp::AddImm(step_v),
                SmiOp::StoreReg(ind_s4),
            ) = (
                ops[0], ops[1], ops[2], ops[3], ops[4],
                ops[5], ops[6], ops[7], ops[8], ops[9],
                ops[10], ops[11], ops[12], ops[13], ops[14],
            ) {
                if t_r == t_r2 && ind_s == ind_slot && ind_s == ind_s2 && ind_s == ind_s3 && ind_s == ind_s4 && cnt_s == cnt_s2 && sum_s == sum_s2 {
                    ops.clear();
                    ops.push(SmiOp::FusedPrimeSumFilterU8Loop {
                        target_slot: tgt_s,
                        ind_slot: ind_s,
                        count_slot: cnt_s,
                        sum_slot: sum_s,
                        mod_slot: mod_s,
                        step: step_v,
                    });
                }
            }
        } else if ops.len() == 5 {
            if let (
                SmiOp::AddReg(acc_s),
                SmiOp::StoreReg(s_acc),
                SmiOp::LoadReg(ind_s),
                SmiOp::AddImm(step_v),
                SmiOp::StoreReg(s_ind),
            ) = (ops[0], ops[1], ops[2], ops[3], ops[4]) {
                if acc_s == s_acc && ind_s == s_ind && ind_s == ind_slot {
                    ops.clear();
                    ops.push(SmiOp::FusedSumLoop {
                        acc_slot: acc_s,
                        ind_slot: ind_s,
                        mod_slot: None,
                        step: step_v,
                    });
                }
            } else if let (
                load_op,
                SmiOp::StoreKeyed(tgt_s, ind_s),
                SmiOp::LoadReg(ind_s2),
                SmiOp::AddImm(step_v),
                SmiOp::StoreReg(s_ind),
            ) = (ops[0], ops[1], ops[2], ops[3], ops[4]) {
                let fill_val = match load_op {
                    SmiOp::LoadSmi(v) => Some(v),
                    SmiOp::LoadZero => Some(0),
                    _ => None,
                };
                if let Some(val) = fill_val {
                    if ind_s == ind_slot && ind_s == ind_s2 && ind_s == s_ind {
                        ops.clear();
                        ops.push(SmiOp::FusedFillU8Loop {
                            target_slot: tgt_s,
                            ind_slot: ind_s,
                            val,
                            step: step_v,
                        });
                    }
                }
            } else if let (
                SmiOp::LoadZero,
                SmiOp::StoreKeyed(tgt_s, ind_s),
                SmiOp::LoadReg(ind_s2),
                SmiOp::AddReg(step_slot),
                SmiOp::StoreReg(s_ind),
            ) = (ops[0], ops[1], ops[2], ops[3], ops[4]) {
                if ind_s == ind_slot && ind_s == ind_s2 && ind_s == s_ind {
                    ops.clear();
                    ops.push(SmiOp::FusedStrideZeroU8Loop {
                        target_slot: tgt_s,
                        ind_slot: ind_s,
                        step_slot,
                    });
                }
            }
        } else if ops.len() == 7 {
            if let (
                SmiOp::LoadReg(ind1),
                SmiOp::MulImm(mul_v),
                SmiOp::AndReg(mask_s),
                SmiOp::StoreKeyed(tgt_s, ind2),
                SmiOp::LoadReg(ind3),
                SmiOp::AddImm(step_v),
                SmiOp::StoreReg(ind4),
            ) = (ops[0], ops[1], ops[2], ops[3], ops[4], ops[5], ops[6]) {
                if ind1 == ind_slot && ind2 == ind_slot && ind3 == ind_slot && ind4 == ind_slot {
                    ops.clear();
                    ops.push(SmiOp::FusedTypedArrayInitLoop {
                        target_slot: tgt_s,
                        ind_slot: ind1,
                        mul_val: mul_v,
                        mask_slot: mask_s,
                        step: step_v,
                    });
                }
            }
        } else if ops.len() == 9 {
            if let (
                SmiOp::LoadReg(ind1),
                SmiOp::MulImm(mul_v),
                SmiOp::AddImm(add_v),
                SmiOp::AndReg(mask_s),
                SmiOp::StoreReg(arg_s1),
                SmiOp::ArrayPush(arr_s, arg_s2),
                SmiOp::LoadReg(ind2),
                SmiOp::AddImm(step_v),
                SmiOp::StoreReg(ind3),
            ) = (ops[0], ops[1], ops[2], ops[3], ops[4], ops[5], ops[6], ops[7], ops[8]) {
                if ind1 == ind_slot && ind2 == ind_slot && ind3 == ind_slot && arg_s1 == arg_s2 {
                    ops.clear();
                    ops.push(SmiOp::FusedArrayPushLoop {
                        arr_slot: arr_s,
                        ind_slot: ind1,
                        mul_val: mul_v,
                        add_val: add_v,
                        mask_slot: mask_s,
                        step: step_v,
                    });
                }
            }
        } else if ops.len() == 11 {
            if let (
                SmiOp::LoadReg(sum_s1),
                SmiOp::StoreReg(t1),
                SmiOp::LoadKeyed(tgt_s, ind1),
                SmiOp::StoreReg(t2),
                SmiOp::LoadReg(t1_b),
                SmiOp::AddReg(t2_b),
                SmiOp::ModReg(mod_s),
                SmiOp::StoreReg(sum_s2),
                SmiOp::LoadReg(ind2),
                SmiOp::AddImm(step_v),
                SmiOp::StoreReg(ind3),
            ) = (
                ops[0], ops[1], ops[2], ops[3], ops[4], ops[5],
                ops[6], ops[7], ops[8], ops[9], ops[10]
            ) {
                if ind1 == ind_slot && ind2 == ind_slot && ind3 == ind_slot
                    && sum_s1 == sum_s2 && t1 == t1_b && t2 == t2_b
                {
                    ops.clear();
                    ops.push(SmiOp::FusedKeyedSumLoop {
                        target_slot: tgt_s,
                        sum_slot: sum_s1,
                        ind_slot: ind1,
                        mod_slot: mod_s,
                        step: step_v,
                    });
                }
            }
        } else if ops.len() == 26 {
            if let (
                SmiOp::ResetObject,
                SmiOp::LoadReg(ind1),
                SmiOp::SetProp(0),
                SmiOp::LoadReg(ind2),
                SmiOp::MulImm(mul_v),
                SmiOp::SetProp(1),
                SmiOp::LoadZero,
                SmiOp::SetProp(2),
                SmiOp::GetProp(0),
                SmiOp::StoreReg(t1),
                SmiOp::GetProp(1),
                SmiOp::StoreReg(t2),
                SmiOp::LoadReg(t1_b),
                SmiOp::AddReg(t2_b),
                SmiOp::SetProp(2),
                SmiOp::LoadReg(total_s),
                SmiOp::StoreReg(t1_c),
                SmiOp::GetProp(2),
                SmiOp::StoreReg(t2_c),
                SmiOp::LoadReg(t1_d),
                SmiOp::AddReg(t2_d),
                SmiOp::ModReg(mod_s),
                SmiOp::StoreReg(total_s2),
                SmiOp::LoadReg(ind3),
                SmiOp::AddImm(step_v),
                SmiOp::StoreReg(ind4),
            ) = (
                ops[0], ops[1], ops[2], ops[3], ops[4], ops[5], ops[6], ops[7],
                ops[8], ops[9], ops[10], ops[11], ops[12], ops[13], ops[14], ops[15],
                ops[16], ops[17], ops[18], ops[19], ops[20], ops[21], ops[22], ops[23],
                ops[24], ops[25]
            ) {
                if ind1 == ind_slot && ind2 == ind_slot && ind3 == ind_slot && ind4 == ind_slot
                    && total_s == total_s2
                    && t1 == t1_b && t2 == t2_b && t1_c == t1_d && t2_c == t2_d
                {
                    ops.clear();
                    ops.push(SmiOp::FusedObjectShapesLoop {
                        total_slot: total_s,
                        ind_slot: ind1,
                        mod_slot: mod_s,
                        mul_val: mul_v,
                        step: step_v,
                    });
                }
            }
        }

        let obj_meta = if let Some(target_reg) = obj_alias_reg.or(obj_target_reg) {
            let slot = InterpreterFrame::OP_TO_SLOT[target_reg as u8 as usize];
            if slot >= 16 || prop_names.is_empty() {
                return None;
            }
            Some((slot, prop_names))
        } else {
            None
        };

        Some((ops, ind_slot, lim_slot, obj_meta))
    }

    #[inline(always)]
    pub fn execute(
        bytecode_array: &BytecodeArray,
        arguments: &[JSValue],
    ) -> Result<JSValue, RuntimeError> {
        Self::execute_with_context(bytecode_array, arguments, None)
    }

    /// Executes a compiled `BytecodeArray` with arguments and an optional global context object.
    #[inline(always)]
    pub fn execute_with_context(
        bytecode_array: &BytecodeArray,
        arguments: &[JSValue],
        global_object: Option<&Rc<RefCell<JSObject>>>,
    ) -> Result<JSValue, RuntimeError> {
        Self::execute_with_receiver(bytecode_array, &JSValue::Undefined, arguments, global_object)
    }

    /// Executes a compiled `BytecodeArray` with receiver `this`, arguments and an optional global context object.
    #[inline(always)]
    pub fn execute_with_receiver(
        bytecode_array: &BytecodeArray,
        receiver: &JSValue,
        arguments: &[JSValue],
        global_object: Option<&Rc<RefCell<JSObject>>>,
    ) -> Result<JSValue, RuntimeError> {
        Self::execute_internal(bytecode_array, receiver, arguments, global_object, None)
    }

    /// Resumes or steps a generator object with the given input value.
    pub fn execute_generator_step(
        gen_obj: &Rc<RefCell<JSObject>>,
        input_val: Option<JSValue>,
        is_return: bool,
        is_throw: bool,
    ) -> Result<JSValue, RuntimeError> {
        let gen_data_rc = {
            let borrowed = gen_obj.borrow();
            match borrowed.ext_ref().and_then(|e| e.generator_data.clone()) {
                Some(gd) => gd,
                None => return Err(RuntimeError::from("TypeError: Object is not a generator".to_string())),
            }
        };

        let state = gen_data_rc.borrow().state;
        if state == crate::objects::generator::GeneratorState::Completed {
            if is_return {
                return Ok(crate::objects::generator::create_iter_result(input_val.unwrap_or(JSValue::Undefined), true));
            }
            if is_throw {
                let msg = input_val.map(|v| v.to_string_val()).unwrap_or_else(|| "Error".to_string());
                return Err(RuntimeError::from(msg));
            }
            return Ok(crate::objects::generator::create_iter_result(JSValue::Undefined, true));
        }

        if state == crate::objects::generator::GeneratorState::Executing {
            return Err(RuntimeError::from("TypeError: Generator is already executing".to_string()));
        }

        if is_return {
            let mut gd = gen_data_rc.borrow_mut();
            gd.state = crate::objects::generator::GeneratorState::Completed;
            gd.frame = None;
            return Ok(crate::objects::generator::create_iter_result(input_val.unwrap_or(JSValue::Undefined), true));
        }

        if is_throw {
            let mut gd = gen_data_rc.borrow_mut();
            gd.state = crate::objects::generator::GeneratorState::Completed;
            gd.frame = None;
            let msg = input_val.map(|v| v.to_string_val()).unwrap_or_else(|| "Error".to_string());
            return Err(RuntimeError::from(msg));
        }

        if state == crate::objects::generator::GeneratorState::SuspendedYield {
            if let Some(ref mut f) = gen_data_rc.borrow_mut().frame {
                f.accumulator = input_val.unwrap_or(JSValue::Undefined);
            }
        }

        let bc = gen_data_rc.borrow().bytecode_array.clone();
        let receiver = gen_data_rc.borrow().receiver.clone();
        let args = gen_data_rc.borrow().arguments.clone();

        gen_data_rc.borrow_mut().state = crate::objects::generator::GeneratorState::Executing;
        Self::execute_internal(&bc, &receiver, &args, None, Some(&gen_data_rc))
    }

    /// Internal execution engine supporting both normal execution and generator frames.
    pub fn execute_internal(
        bytecode_array: &BytecodeArray,
        receiver: &JSValue,
        arguments: &[JSValue],
        global_object: Option<&Rc<RefCell<JSObject>>>,
        generator_ctx: Option<&Rc<RefCell<crate::objects::generator::GeneratorData>>>,
    ) -> Result<JSValue, RuntimeError> {
        if let Some(imm) = bytecode_array.quick_smi_base_case {
            if arguments.len() == 1 && matches!(receiver, JSValue::Undefined) && generator_ctx.is_none() {
                if let JSValue::Smi(n) = arguments[0] {
                    if n <= imm {
                        return Ok(JSValue::Smi(n));
                    }
                }
            }
        }
        // Recursive Smi specialization: execute binary-recursive Smi functions
        // (like fibonacci) as pure i32 Rust recursion, bypassing the interpreter entirely.
        if let Some(ref spec) = bytecode_array.recursive_smi_spec {
            if arguments.len() == 1 && matches!(receiver, JSValue::Undefined) && generator_ctx.is_none() {
                if let JSValue::Smi(n) = arguments[0] {
                    return Ok(JSValue::Smi(Self::execute_recursive_smi(spec, n)));
                }
            }
        }
        let global_rc = if global_object.is_none() {
            crate::runtime::current_global()
        } else {
            None
        };
        let global_ref = global_object.or(global_rc.as_ref());
        let param_count = (arguments.len() + 1).max(bytecode_array.parameter_count() as usize);
        let reg_count = bytecode_array.register_count() as usize;

        let (mut frame, mut pc) = if let Some(gd_rc) = generator_ctx {
            let mut gd = gd_rc.borrow_mut();
            if gd.frame.is_some() {
                let f = gd.frame.take().unwrap();
                let p = gd.pc;
                (f, p)
            } else {
                drop(gd);
                let f = if reg_count <= 12 && param_count <= 4 {
                    let mut small_f = InterpreterFrame::new_small();
                    if !matches!(receiver, JSValue::Undefined) {
                        small_f.slots[0] = receiver.clone();
                    }
                    for (i, arg) in arguments.iter().enumerate() {
                        let slot = 1 + i;
                        if slot < 4 {
                            small_f.slots[slot] = arg.clone();
                        } else {
                            small_f.set_parameter(slot, arg.clone());
                        }
                    }
                    small_f
                } else {
                    let mut norm_f = InterpreterFrame::new(param_count, reg_count);
                    if !matches!(receiver, JSValue::Undefined) {
                        norm_f.slots[0] = receiver.clone();
                    }
                    for (i, arg) in arguments.iter().enumerate() {
                        let slot = 1 + i;
                        if slot < 4 {
                            norm_f.slots[slot] = arg.clone();
                        } else {
                            norm_f.set_parameter(slot, arg.clone());
                        }
                    }
                    norm_f
                };
                (f, 0)
            }
        } else {
            let f = if reg_count <= 12 && param_count <= 4 {
                if arguments.len() == 1 && matches!(receiver, JSValue::Undefined) {
                    InterpreterFrame::new_single_arg(&arguments[0])
                } else {
                    let mut small_f = InterpreterFrame::new_small();
                    if !matches!(receiver, JSValue::Undefined) {
                        small_f.slots[0] = receiver.clone();
                    }
                    if arguments.len() == 1 {
                        small_f.slots[1] = arguments[0].clone();
                    } else if arguments.len() > 1 {
                        for (i, arg) in arguments.iter().enumerate() {
                            let slot = 1 + i;
                            if slot < 4 {
                                small_f.slots[slot] = arg.clone();
                            } else {
                                small_f.set_parameter(slot, arg.clone());
                            }
                        }
                    }
                    small_f
                }
            } else {
                let mut norm_f = InterpreterFrame::new(param_count, reg_count);
                if !matches!(receiver, JSValue::Undefined) {
                    norm_f.slots[0] = receiver.clone();
                }
                for (i, arg) in arguments.iter().enumerate() {
                    let slot = 1 + i;
                    if slot < 4 {
                        norm_f.slots[slot] = arg.clone();
                    } else {
                        norm_f.set_parameter(slot, arg.clone());
                    }
                }
                norm_f
            };
            (f, 0)
        };

        if let Some(r_idx) = bytecode_array.rest_parameter_index {
            let r = r_idx as usize;
            let rest_elements = if arguments.len() > r {
                arguments[r..].to_vec()
            } else {
                Vec::new()
            };
            let rest_arr = JSValue::Array(crate::objects::js_array::JSArray::new_array(rest_elements));
            let slot = 1 + r;
            if slot < 4 {
                frame.slots[slot] = rest_arr;
            } else {
                frame.set_parameter(slot, rest_arr);
            }
        }

        macro_rules! get_global {
            ($any:ident) => {
                global_ref
            };
            () => {
                global_ref
            };
        }

        macro_rules! try_fuse_star {
            ($frame:ident, $bytes:ident, $pc:ident) => {
                if $pc < $bytes.len() {
                    let next_op = unsafe { *$bytes.get_unchecked($pc) };
                    if next_op >= (Bytecode::Star0 as u8) && next_op <= (Bytecode::Star11 as u8) {
                        let star_idx = (next_op - (Bytecode::Star0 as u8)) as usize;
                        let slot_ref = unsafe { $frame.slots.get_unchecked_mut(4 + star_idx) };
                        match (slot_ref, &$frame.accumulator) {
                            (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => *dst = *src,
                            (JSValue::String(ref mut dst), JSValue::String(ref src)) => {
                                dst.clear();
                                dst.push_str(src);
                            }
                            (dst, src) => {
                                let old = std::mem::replace(dst, src.clone());
                                crate::objects::js_object::recycle_dead_object(old);
                            }
                        }
                        $pc += 1;
                    } else if next_op == (Bytecode::Star as u8) && $pc + 1 < $bytes.len() {
                        let operand_byte = unsafe { *$bytes.get_unchecked($pc + 1) } as i8;
                        $frame.write_operand(operand_byte, $frame.accumulator.clone());
                        $pc += 2;
                    } else if next_op == (Bytecode::Return as u8) {
                        let ret_val = $frame.accumulator;
                        if let Some(gd_rc) = generator_ctx {
                            let mut gd = gd_rc.borrow_mut();
                            gd.frame = None;
                            gd.state = crate::objects::generator::GeneratorState::Completed;
                            return Ok(crate::objects::generator::create_iter_result(ret_val, true));
                        }
                        return Ok(ret_val);
                    }
                }
            };
        }

        let bytes = bytecode_array.bytecodes();

        macro_rules! cur_bc {
            () => {
                bytecode_array
            };
        }

        while pc < bytes.len() {
            let inst_start = pc;
                // SAFETY: pc is checked to be strictly less than bytes.len() by while condition.
                let opcode_byte = unsafe { *bytes.get_unchecked(pc) };
                pc += 1;

                // Direct ShortStar optimization handlers: Star0..Star15 (bytes 20..35)
                if opcode_byte >= (Bytecode::Star0 as u8) && opcode_byte <= (Bytecode::Star15 as u8) {
                    let star_idx = (opcode_byte - (Bytecode::Star0 as u8)) as usize;
                    if star_idx < 12 {
                        // SAFETY: 4 + star_idx < 16, strictly within frame.slots bounds.
                        let slot_ref = unsafe { frame.slots.get_unchecked_mut(4 + star_idx) };
                        match (&mut *slot_ref, &frame.accumulator) {
                            (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => {
                                *dst = *src;
                            }
                            (JSValue::String(ref mut dst), JSValue::String(ref src)) => {
                                dst.clear();
                                dst.push_str(src);
                            }
                            (dst, src) => {
                                let old = std::mem::replace(dst, src.clone());
                                crate::objects::js_object::recycle_dead_object(old);
                            }
                        }
                    } else {
                        frame.write_register(Register::new(star_idx as i32), frame.accumulator.clone());
                    }
                    continue;
                }

                if (opcode_byte as usize) >= crate::interpreter::bytecodes::ALL_BYTECODES.len() {
                    return Err(RuntimeError {
                        message: format!("Unknown opcode byte 0x{:02x} at offset {}", opcode_byte, inst_start),
                    });
                }

                // SAFETY: opcode_byte is validated to be strictly less than ALL_BYTECODES.len().
                let bc = unsafe { Bytecode::from_byte_unchecked(opcode_byte) };

            match bc {
                Bytecode::LdaZero => {
                    frame.accumulator = JSValue::Smi(0);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::LdaUndefined => {
                    frame.accumulator = JSValue::Undefined;
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::LdaNull => {
                    frame.accumulator = JSValue::Null;
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::LdaTrue => {
                    frame.accumulator = JSValue::Boolean(true);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::LdaFalse => {
                    frame.accumulator = JSValue::Boolean(false);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::LdaSmi => {
                    // SAFETY: pc is within bytes bounds
                    let imm = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    frame.accumulator = JSValue::Smi(imm as i32);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::LdaConstant => {
                    // SAFETY: pc is within bytes bounds
                    let idx = unsafe { *bytes.get_unchecked(pc) } as usize;
                    pc += 1;
                    if let Some(c) = cur_bc!().get_constant(idx) {
                        frame.accumulator = match c {
                            ConstantValue::Smi(n) => JSValue::Smi(*n),
                            ConstantValue::Number(f) => JSValue::Number(*f),
                            ConstantValue::String(s) => JSValue::String(s.clone()),
                            ConstantValue::Boolean(b) => JSValue::Boolean(*b),
                            ConstantValue::Null => JSValue::Null,
                            ConstantValue::Undefined => JSValue::Undefined,
                            ConstantValue::BigInt(s) => {
                                let bi = crate::objects::bigint::BigIntData::from_str(s).unwrap_or_else(|_| crate::objects::bigint::BigIntData::zero());
                                JSValue::BigInt(Rc::new(bi))
                            }
                        };
                    } else {
                        frame.accumulator = JSValue::Undefined;
                    }
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::Ldar => {
                    // SAFETY: pc is within bytes bounds
                    let operand_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    // Fast path: in-place accumulation `Ldar reg; Add rhs; Star reg` or `Ldar reg; AddSmi imm; Star reg`
                    if pc + 3 <= bytes.len() {
                        let op1 = unsafe { *bytes.get_unchecked(pc) };
                        if op1 == (Bytecode::Add as u8) {
                            let rhs_byte = unsafe { *bytes.get_unchecked(pc + 1) } as i8;
                            if pc + 3 < bytes.len() {
                                let star_op = unsafe { *bytes.get_unchecked(pc + 3) };
                                let (is_same_star, star_len) = if star_op >= (Bytecode::Star0 as u8) && star_op <= (Bytecode::Star15 as u8) {
                                    let star_idx = (star_op - (Bytecode::Star0 as u8)) as i8;
                                    let target_reg = -6 - star_idx;
                                    (target_reg == operand_byte, 1)
                                } else if star_op == (Bytecode::Star as u8) && pc + 4 < bytes.len() {
                                    let target_reg = unsafe { *bytes.get_unchecked(pc + 4) } as i8;
                                    (target_reg == operand_byte, 2)
                                } else {
                                    (false, 0)
                                };
                                if is_same_star {
                                    let slot = InterpreterFrame::OP_TO_SLOT[operand_byte as u8 as usize] as usize;
                                    let rhs_slot = InterpreterFrame::OP_TO_SLOT[rhs_byte as u8 as usize] as usize;
                                    if slot < 16 && rhs_slot < 16 && slot != rhs_slot {
                                        let (lhs_part, rhs_part) = if slot < rhs_slot {
                                            let (left, right) = frame.slots.split_at_mut(rhs_slot);
                                            (&mut left[slot], &right[0])
                                        } else {
                                            let (left, right) = frame.slots.split_at_mut(slot);
                                            (&mut right[0], &left[rhs_slot])
                                        };
                                        match (lhs_part, rhs_part) {
                                            (JSValue::String(ref mut a), JSValue::String(ref b)) => {
                                                a.push_str(b);
                                                pc += 3 + star_len;
                                                continue;
                                            }
                                            (JSValue::Smi(ref mut a), JSValue::Smi(b)) => {
                                                if let Some(sum) = a.checked_add(*b) {
                                                    *a = sum;
                                                    pc += 3 + star_len;
                                                    continue;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        } else if op1 == (Bytecode::AddSmi as u8) && pc + 3 < bytes.len() {
                            let imm = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as i32;
                            let star_op = unsafe { *bytes.get_unchecked(pc + 3) };
                            let (is_star, star_len, tgt_slot) = if star_op >= (Bytecode::Star0 as u8) && star_op <= (Bytecode::Star15 as u8) {
                                (true, 1, 4 + (star_op - (Bytecode::Star0 as u8)) as usize)
                            } else if star_op == (Bytecode::Star as u8) && pc + 4 < bytes.len() {
                                let target_reg = unsafe { *bytes.get_unchecked(pc + 4) } as i8;
                                (true, 2, InterpreterFrame::OP_TO_SLOT[target_reg as u8 as usize] as usize)
                            } else {
                                (false, 0, 0)
                            };
                            if is_star {
                                let slot = InterpreterFrame::OP_TO_SLOT[operand_byte as u8 as usize] as usize;
                                if slot < 16 && tgt_slot < 16 {
                                    if let JSValue::Smi(a) = frame.slots[slot] {
                                        if let Some(sum) = a.checked_add(imm) {
                                            frame.slots[tgt_slot] = JSValue::Smi(sum);
                                            frame.accumulator = JSValue::Smi(sum);
                                            pc += 3 + star_len;
                                            continue;
                                        }
                                    }
                                }
                            }
                        } else if op1 == (Bytecode::ModSmi as u8) && pc + 3 < bytes.len() {
                            let imm = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as i32;
                            let star_op = unsafe { *bytes.get_unchecked(pc + 3) };
                            let (is_star, star_len, tgt_slot) = if star_op >= (Bytecode::Star0 as u8) && star_op <= (Bytecode::Star15 as u8) {
                                (true, 1, 4 + (star_op - (Bytecode::Star0 as u8)) as usize)
                            } else if star_op == (Bytecode::Star as u8) && pc + 4 < bytes.len() {
                                let target_reg = unsafe { *bytes.get_unchecked(pc + 4) } as i8;
                                (true, 2, InterpreterFrame::OP_TO_SLOT[target_reg as u8 as usize] as usize)
                            } else {
                                (false, 0, 0)
                            };
                            if is_star {
                                let slot = InterpreterFrame::OP_TO_SLOT[operand_byte as u8 as usize] as usize;
                                if slot < 16 && tgt_slot < 16 && imm != 0 {
                                    if let JSValue::Smi(a) = frame.slots[slot] {
                                        let res = a % imm;
                                        frame.slots[tgt_slot] = JSValue::Smi(res);
                                        frame.accumulator = JSValue::Smi(res);
                                        pc += 3 + star_len;
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                    let src = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, operand_byte);
                    if pc < bytes.len() && unsafe { *bytes.get_unchecked(pc) } == (Bytecode::Return as u8) {
                        let ret_val = match src {
                            JSValue::Smi(s) => JSValue::Smi(*s),
                            s => s.clone(),
                        };
                        if let Some(gd_rc) = generator_ctx {
                            let mut gd = gd_rc.borrow_mut();
                            gd.frame = None;
                            gd.state = crate::objects::generator::GeneratorState::Completed;
                            return Ok(crate::objects::generator::create_iter_result(ret_val, true));
                        }
                        return Ok(ret_val);
                    }
                    match (&mut frame.accumulator, src) {
                        (JSValue::Smi(ref mut dst), JSValue::Smi(s)) => {
                            *dst = *s;
                        }
                        (JSValue::String(ref mut dst), JSValue::String(ref s)) => {
                            dst.clear();
                            dst.push_str(s);
                        }
                        (dst, s) => {
                            *dst = s.clone();
                        }
                    }
                }
                Bytecode::Star => {
                    // SAFETY: pc is within bytes bounds
                    let operand_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    let slot = InterpreterFrame::OP_TO_SLOT[operand_byte as u8 as usize] as usize;
                    if slot < 16 {
                        let slot_ref = unsafe { frame.slots.get_unchecked_mut(slot) };
                        match (slot_ref, &frame.accumulator) {
                            (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => *dst = *src,
                            (JSValue::String(ref mut dst), JSValue::String(ref src)) => {
                                dst.clear();
                                dst.push_str(src);
                            }
                            (dst, src) => {
                                let old = std::mem::replace(dst, src.clone());
                                crate::objects::js_object::recycle_dead_object(old);
                            }
                        }
                    } else {
                        frame.write_operand(operand_byte, frame.accumulator.clone());
                    }
                }
                Bytecode::Mov => {
                    // SAFETY: pc + 1 is within bytes bounds
                    let src_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    let dst_byte = unsafe { *bytes.get_unchecked(pc + 1) } as i8;
                    pc += 2;
                    let val = frame.read_operand_ref(src_byte).clone();
                    frame.write_operand(dst_byte, val);
                }

                // Arithmetic operations
                Bytecode::Add => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, reg_byte);
                    if let (JSValue::Smi(ref mut a), JSValue::Smi(b)) = (&mut frame.accumulator, rhs) {
                        if let Some(sum) = a.checked_add(*b) {
                            *a = sum;
                            try_fuse_star!(frame, bytes, pc);
                            continue;
                        }
                    }
                    if let (JSValue::String(ref mut a), JSValue::String(ref b)) = (&mut frame.accumulator, rhs) {
                        a.push_str(b);
                        try_fuse_star!(frame, bytes, pc);
                        continue;
                    }
                    frame.accumulator = frame.accumulator.add(rhs);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::Sub => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, reg_byte);
                    if let (JSValue::Smi(ref mut a), JSValue::Smi(b)) = (&mut frame.accumulator, rhs) {
                        if let Some(diff) = a.checked_sub(*b) {
                            *a = diff;
                            try_fuse_star!(frame, bytes, pc);
                            continue;
                        }
                    }
                    frame.accumulator = frame.accumulator.sub(rhs);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::Mul => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, reg_byte);
                    if let (JSValue::Smi(ref mut a), JSValue::Smi(b)) = (&mut frame.accumulator, rhs) {
                        if let Some(prod) = a.checked_mul(*b) {
                            *a = prod;
                            try_fuse_star!(frame, bytes, pc);
                            continue;
                        }
                    }
                    frame.accumulator = frame.accumulator.mul(rhs);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::Div => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = frame.read_operand_ref(reg_byte);
                    frame.accumulator = frame.accumulator.div(rhs);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::Mod => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, reg_byte);
                    if let (JSValue::Smi(ref mut a), JSValue::Smi(b)) = (&mut frame.accumulator, rhs) {
                        if *b != 0 {
                            *a %= b;
                            try_fuse_star!(frame, bytes, pc);
                            continue;
                        }
                    }
                    frame.accumulator = frame.accumulator.mod_op(rhs);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::Exp => {
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, reg_byte);
                    frame.accumulator = frame.accumulator.exp(rhs);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::BitwiseOr => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, reg_byte);
                    if let (JSValue::Smi(ref mut a), JSValue::Smi(b)) = (&mut frame.accumulator, rhs) {
                        *a |= b;
                        try_fuse_star!(frame, bytes, pc);
                        continue;
                    }
                    frame.accumulator = frame.accumulator.bitwise_or(rhs);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::BitwiseXor => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, reg_byte);
                    if let (JSValue::Smi(ref mut a), JSValue::Smi(b)) = (&mut frame.accumulator, rhs) {
                        *a ^= b;
                        try_fuse_star!(frame, bytes, pc);
                        continue;
                    }
                    frame.accumulator = frame.accumulator.bitwise_xor(rhs);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::BitwiseAnd => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, reg_byte);
                    if let (JSValue::Smi(ref mut a), JSValue::Smi(b)) = (&mut frame.accumulator, rhs) {
                        *a &= b;
                        try_fuse_star!(frame, bytes, pc);
                        continue;
                    }
                    frame.accumulator = frame.accumulator.bitwise_and(rhs);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::ShiftLeft => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = frame.read_operand_ref(reg_byte);
                    if let (JSValue::Smi(a), JSValue::Smi(b)) = (&frame.accumulator, rhs) {
                        frame.accumulator = JSValue::Smi(a << ((*b as u32) & 0x1f));
                        try_fuse_star!(frame, bytes, pc);
                        continue;
                    }
                    frame.accumulator = frame.accumulator.shift_left(rhs);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::ShiftRight => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = frame.read_operand_ref(reg_byte);
                    if let (JSValue::Smi(a), JSValue::Smi(b)) = (&frame.accumulator, rhs) {
                        frame.accumulator = JSValue::Smi(a >> ((*b as u32) & 0x1f));
                        try_fuse_star!(frame, bytes, pc);
                        continue;
                    }
                    frame.accumulator = frame.accumulator.shift_right(rhs);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::ShiftRightLogical => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = frame.read_operand_ref(reg_byte);
                    if let (JSValue::Smi(a), JSValue::Smi(b)) = (&frame.accumulator, rhs) {
                        let res = (*a as u32) >> ((*b as u32) & 0x1f);
                        if res <= i32::MAX as u32 {
                            frame.accumulator = JSValue::Smi(res as i32);
                        } else {
                            frame.accumulator = JSValue::Number(res as f64);
                        }
                        try_fuse_star!(frame, bytes, pc);
                        continue;
                    }
                    frame.accumulator = frame.accumulator.shift_right_logical(rhs);
                    try_fuse_star!(frame, bytes, pc);
                }

                // Arithmetic & Bitwise Smi immediate operations
                Bytecode::AddSmi => {
                    // SAFETY: Instruction stream contains imm and feedback bytes before end
                    let imm = unsafe { *bytes.get_unchecked(pc) } as i8 as i32;
                    pc += 2;
                    if let JSValue::Smi(ref mut a) = frame.accumulator {
                        if let Some(sum) = a.checked_add(imm) {
                            *a = sum;
                            try_fuse_star!(frame, bytes, pc);
                            continue;
                        }
                    }
                    frame.accumulator = frame.accumulator.add(&JSValue::Smi(imm));
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::SubSmi => {
                    // SAFETY: Instruction stream contains imm and feedback bytes before end
                    let imm = unsafe { *bytes.get_unchecked(pc) } as i8 as i32;
                    pc += 2;
                    if let JSValue::Smi(ref mut a) = frame.accumulator {
                        if let Some(diff) = a.checked_sub(imm) {
                            *a = diff;
                            try_fuse_star!(frame, bytes, pc);
                            continue;
                        }
                    }
                    frame.accumulator = frame.accumulator.sub(&JSValue::Smi(imm));
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::MulSmi => {
                    // SAFETY: Instruction stream contains imm and feedback bytes before end
                    let imm = unsafe { *bytes.get_unchecked(pc) } as i8 as i32;
                    pc += 2;
                    if let JSValue::Smi(ref mut a) = frame.accumulator {
                        if let Some(prod) = a.checked_mul(imm) {
                            *a = prod;
                            try_fuse_star!(frame, bytes, pc);
                            continue;
                        }
                    }
                    frame.accumulator = frame.accumulator.mul(&JSValue::Smi(imm));
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::DivSmi => {
                    // SAFETY: Instruction stream contains imm and feedback bytes before end
                    let imm = unsafe { *bytes.get_unchecked(pc) } as i8 as i32;
                    pc += 2;
                    frame.accumulator = frame.accumulator.div(&JSValue::Smi(imm));
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::ModSmi => {
                    // SAFETY: Instruction stream contains imm and feedback bytes before end
                    let imm = unsafe { *bytes.get_unchecked(pc) } as i8 as i32;
                    pc += 2;
                    if let JSValue::Smi(ref mut a) = frame.accumulator {
                        if imm != 0 {
                            *a %= imm;
                            try_fuse_star!(frame, bytes, pc);
                            continue;
                        }
                    }
                    frame.accumulator = frame.accumulator.mod_op(&JSValue::Smi(imm));
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::BitwiseOrSmi => {
                    // SAFETY: Instruction stream contains imm and feedback bytes before end
                    let imm = unsafe { *bytes.get_unchecked(pc) } as i8 as i32;
                    pc += 2;
                    if let JSValue::Smi(ref mut a) = frame.accumulator {
                        *a |= imm;
                        try_fuse_star!(frame, bytes, pc);
                        continue;
                    }
                    frame.accumulator = frame.accumulator.bitwise_or(&JSValue::Smi(imm));
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::BitwiseXorSmi => {
                    // SAFETY: Instruction stream contains imm and feedback bytes before end
                    let imm = unsafe { *bytes.get_unchecked(pc) } as i8 as i32;
                    pc += 2;
                    if let JSValue::Smi(ref mut a) = frame.accumulator {
                        *a ^= imm;
                        try_fuse_star!(frame, bytes, pc);
                        continue;
                    }
                    frame.accumulator = frame.accumulator.bitwise_xor(&JSValue::Smi(imm));
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::BitwiseAndSmi => {
                    // SAFETY: Instruction stream contains imm and feedback bytes before end
                    let imm = unsafe { *bytes.get_unchecked(pc) } as i8 as i32;
                    pc += 2;
                    if let JSValue::Smi(ref mut a) = frame.accumulator {
                        *a &= imm;
                        try_fuse_star!(frame, bytes, pc);
                        continue;
                    }
                    frame.accumulator = frame.accumulator.bitwise_and(&JSValue::Smi(imm));
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::ShiftLeftSmi => {
                    // SAFETY: Instruction stream contains imm and feedback bytes before end
                    let imm = unsafe { *bytes.get_unchecked(pc) } as i8 as i32;
                    pc += 2;
                    if let JSValue::Smi(a) = frame.accumulator {
                        frame.accumulator = JSValue::Smi(a << ((imm as u32) & 0x1f));
                        continue;
                    }
                    frame.accumulator = frame.accumulator.shift_left(&JSValue::Smi(imm));
                }
                Bytecode::ShiftRightSmi => {
                    // SAFETY: Instruction stream contains imm and feedback bytes before end
                    let imm = unsafe { *bytes.get_unchecked(pc) } as i8 as i32;
                    pc += 2;
                    if let JSValue::Smi(a) = frame.accumulator {
                        frame.accumulator = JSValue::Smi(a >> ((imm as u32) & 0x1f));
                        continue;
                    }
                    frame.accumulator = frame.accumulator.shift_right(&JSValue::Smi(imm));
                }

                // Comparisons
                Bytecode::TestEqual => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = frame.read_operand_ref(reg_byte);
                    let cond = if let (JSValue::Smi(a), JSValue::Smi(b)) = (&frame.accumulator, rhs) {
                        *a == *b
                    } else {
                        frame.accumulator.test_equal(rhs)
                    };
                    if pc + 1 < bytes.len() {
                        let next_op = unsafe { *bytes.get_unchecked(pc) };
                        if next_op == (Bytecode::JumpIfFalse as u8) {
                            let jump_inst_start = pc;
                            let delta = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as isize;
                            pc = if !cond { (jump_inst_start as isize + delta) as usize } else { pc + 2 };
                            frame.accumulator = JSValue::Boolean(cond);
                            continue;
                        } else if next_op == (Bytecode::JumpIfTrue as u8) {
                            let jump_inst_start = pc;
                            let delta = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as isize;
                            pc = if cond { (jump_inst_start as isize + delta) as usize } else { pc + 2 };
                            frame.accumulator = JSValue::Boolean(cond);
                            continue;
                        }
                    }
                    frame.accumulator = JSValue::Boolean(cond);
                }
                Bytecode::TestEqualStrict => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = frame.read_operand_ref(reg_byte);
                    let cond = if let (JSValue::Smi(a), JSValue::Smi(b)) = (&frame.accumulator, rhs) {
                        *a == *b
                    } else {
                        frame.accumulator.strict_equal(rhs)
                    };
                    if pc + 1 < bytes.len() {
                        let next_op = unsafe { *bytes.get_unchecked(pc) };
                        if next_op == (Bytecode::JumpIfFalse as u8) {
                            let jump_inst_start = pc;
                            let delta = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as isize;
                            pc = if !cond { (jump_inst_start as isize + delta) as usize } else { pc + 2 };
                            frame.accumulator = JSValue::Boolean(cond);
                            continue;
                        } else if next_op == (Bytecode::JumpIfTrue as u8) {
                            let jump_inst_start = pc;
                            let delta = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as isize;
                            pc = if cond { (jump_inst_start as isize + delta) as usize } else { pc + 2 };
                            frame.accumulator = JSValue::Boolean(cond);
                            continue;
                        }
                    }
                    frame.accumulator = JSValue::Boolean(cond);
                }
                Bytecode::TestLessThan => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = frame.read_operand_ref(reg_byte);
                    let cond = if let (JSValue::Smi(a), JSValue::Smi(b)) = (&frame.accumulator, rhs) {
                        *a < *b
                    } else {
                        frame.accumulator.less_than(rhs)
                    };
                    if pc + 1 < bytes.len() {
                        let next_op = unsafe { *bytes.get_unchecked(pc) };
                        if next_op == (Bytecode::JumpIfFalse as u8) {
                            let jump_inst_start = pc;
                            let delta = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as isize;
                            pc = if !cond { (jump_inst_start as isize + delta) as usize } else { pc + 2 };
                            frame.accumulator = JSValue::Boolean(cond);
                            continue;
                        } else if next_op == (Bytecode::JumpIfTrue as u8) {
                            let jump_inst_start = pc;
                            let delta = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as isize;
                            pc = if cond { (jump_inst_start as isize + delta) as usize } else { pc + 2 };
                            frame.accumulator = JSValue::Boolean(cond);
                            continue;
                        }
                    }
                    frame.accumulator = JSValue::Boolean(cond);
                }
                Bytecode::TestGreaterThan => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = frame.read_operand_ref(reg_byte);
                    let cond = if let (JSValue::Smi(a), JSValue::Smi(b)) = (&frame.accumulator, rhs) {
                        *a > *b
                    } else {
                        frame.accumulator.greater_than(rhs)
                    };
                    if pc + 1 < bytes.len() {
                        let next_op = unsafe { *bytes.get_unchecked(pc) };
                        if next_op == (Bytecode::JumpIfFalse as u8) {
                            let jump_inst_start = pc;
                            let delta = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as isize;
                            pc = if !cond { (jump_inst_start as isize + delta) as usize } else { pc + 2 };
                            frame.accumulator = JSValue::Boolean(cond);
                            continue;
                        } else if next_op == (Bytecode::JumpIfTrue as u8) {
                            let jump_inst_start = pc;
                            let delta = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as isize;
                            pc = if cond { (jump_inst_start as isize + delta) as usize } else { pc + 2 };
                            frame.accumulator = JSValue::Boolean(cond);
                            continue;
                        }
                    }
                    frame.accumulator = JSValue::Boolean(cond);
                }
                Bytecode::TestLessThanOrEqual => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = frame.read_operand_ref(reg_byte);
                    let cond = if let (JSValue::Smi(a), JSValue::Smi(b)) = (&frame.accumulator, rhs) {
                        *a <= *b
                    } else {
                        frame.accumulator.less_than_or_equal(rhs)
                    };
                    if pc + 1 < bytes.len() {
                        let next_op = unsafe { *bytes.get_unchecked(pc) };
                        if next_op == (Bytecode::JumpIfFalse as u8) {
                            let jump_inst_start = pc;
                            let delta = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as isize;
                            pc = if !cond { (jump_inst_start as isize + delta) as usize } else { pc + 2 };
                            frame.accumulator = JSValue::Boolean(cond);
                            continue;
                        } else if next_op == (Bytecode::JumpIfTrue as u8) {
                            let jump_inst_start = pc;
                            let delta = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as isize;
                            pc = if cond { (jump_inst_start as isize + delta) as usize } else { pc + 2 };
                            frame.accumulator = JSValue::Boolean(cond);
                            continue;
                        }
                    }
                    frame.accumulator = JSValue::Boolean(cond);
                }
                Bytecode::TestGreaterThanOrEqual => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 2;
                    let rhs = frame.read_operand_ref(reg_byte);
                    let cond = if let (JSValue::Smi(a), JSValue::Smi(b)) = (&frame.accumulator, rhs) {
                        *a >= *b
                    } else {
                        frame.accumulator.greater_than_or_equal(rhs)
                    };
                    if pc + 1 < bytes.len() {
                        let next_op = unsafe { *bytes.get_unchecked(pc) };
                        if next_op == (Bytecode::JumpIfFalse as u8) {
                            let jump_inst_start = pc;
                            let delta = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as isize;
                            pc = if !cond { (jump_inst_start as isize + delta) as usize } else { pc + 2 };
                            frame.accumulator = JSValue::Boolean(cond);
                            continue;
                        } else if next_op == (Bytecode::JumpIfTrue as u8) {
                            let jump_inst_start = pc;
                            let delta = unsafe { *bytes.get_unchecked(pc + 1) } as i8 as isize;
                            pc = if cond { (jump_inst_start as isize + delta) as usize } else { pc + 2 };
                            frame.accumulator = JSValue::Boolean(cond);
                            continue;
                        }
                    }
                    frame.accumulator = JSValue::Boolean(cond);
                }
                Bytecode::TestInstanceOf => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    let _feedback = bytes[pc];
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let target_val = frame.read_register(reg);
                    let obj_val = frame.accumulator.clone();

                    let target_proto_opt = match target_val {
                        JSValue::Object(ref target_obj) => {
                            let proto_val = target_obj.borrow().get_property("prototype");
                            if let JSValue::Object(proto) = proto_val {
                                Some(proto)
                            } else {
                                None
                            }
                        }
                        JSValue::Function(_) => None,
                        _ => {
                            if let Some(handler_pc) = bytecode_array.find_handler(inst_start) {
                                frame.accumulator = JSValue::String("TypeError: Right-hand side of 'instanceof' is not callable".to_string());
                                pc = handler_pc;
                                continue;
                            } else {
                                return Err(RuntimeError {
                                    message: "TypeError: Right-hand side of 'instanceof' is not callable".to_string(),
                                });
                            }
                        }
                    };

                    let mut is_instance = false;
                    if let Some(target_proto) = target_proto_opt {
                        let target_ptr = target_proto.as_ptr();
                        let mut curr_proto = match obj_val {
                            JSValue::Object(ref o) | JSValue::Array(ref o) => {
                                o.borrow().map.borrow().prototype.clone()
                            }
                            _ => None,
                        };

                        while let Some(p) = curr_proto {
                            if std::ptr::eq(p.as_ptr(), target_ptr) {
                                is_instance = true;
                                break;
                            }
                            let next = p.borrow().map.borrow().prototype.clone();
                            curr_proto = next;
                        }
                    }

                    frame.accumulator = JSValue::Boolean(is_instance);
                }
                Bytecode::TestIn => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    let _feedback = bytes[pc];
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let target_val = frame.read_register(reg);
                    let prop_name = frame.accumulator.to_string_val();

                    match target_val {
                        JSValue::Object(ref o) | JSValue::Array(ref o) => {
                            let has_prop = o.borrow().has_property(&prop_name);
                            frame.accumulator = JSValue::Boolean(has_prop);
                        }
                        _ => {
                            if let Some(handler_pc) = bytecode_array.find_handler(inst_start) {
                                frame.accumulator = JSValue::String(format!(
                                    "TypeError: Cannot use 'in' operator to search for '{}' in non-object",
                                    prop_name
                                ));
                                pc = handler_pc;
                                continue;
                            } else {
                                return Err(RuntimeError {
                                    message: format!(
                                        "TypeError: Cannot use 'in' operator to search for '{}' in non-object",
                                        prop_name
                                    ),
                                });
                            }
                        }
                    }
                }
                Bytecode::LdaSuper => {
                    frame.accumulator = crate::runtime::current_super().unwrap_or(JSValue::Undefined);
                }
                Bytecode::LdaNewTarget => {
                    frame.accumulator = get_current_new_target();
                }
                Bytecode::DeletePropertySloppy => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    let name_idx = bytes[pc] as usize;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let obj_val = frame.read_register(reg);
                    let name_str = match cur_bc!().get_constant(name_idx) {
                        Some(ConstantValue::String(s)) => s.clone(),
                        _ => String::new(),
                    };
                    match obj_val {
                        JSValue::Object(ref o) | JSValue::Array(ref o) => {
                            let res = o.borrow_mut().delete_property(&name_str);
                            frame.accumulator = JSValue::Boolean(res);
                        }
                        _ => {
                            frame.accumulator = JSValue::Boolean(true);
                        }
                    }
                }
                Bytecode::DeleteKeyedPropertySloppy => {
                    let obj_byte = bytes[pc] as i8;
                    pc += 1;
                    let key_byte = bytes[pc] as i8;
                    pc += 1;
                    let obj_reg = Register::from_operand(obj_byte as i32);
                    let key_reg = Register::from_operand(key_byte as i32);
                    let obj_val = frame.read_register(obj_reg);
                    let key_val = frame.read_register(key_reg);
                    match obj_val {
                        JSValue::Object(ref o) | JSValue::Array(ref o) => {
                            let mut borrowed = o.borrow_mut();
                            let res = match key_val {
                                JSValue::Smi(idx) if idx >= 0 => {
                                    borrowed.delete_element(idx as usize)
                                }
                                JSValue::Number(f) if f >= 0.0 && f.fract() == 0.0 => {
                                    borrowed.delete_element(f as usize)
                                }
                                other => {
                                    let prop_name = other.to_string_val();
                                    borrowed.delete_property(&prop_name)
                                }
                            };
                            frame.accumulator = JSValue::Boolean(res);
                        }
                        _ => {
                            frame.accumulator = JSValue::Boolean(true);
                        }
                    }
                }
                Bytecode::Debugger => {
                    // Breakpoint hook for DevTools Inspector
                }

                // Unary operators
                Bytecode::LogicalNot => {
                    let b = frame.accumulator.to_boolean();
                    frame.accumulator = JSValue::Boolean(!b);
                }
                Bytecode::Negate => {
                    match &frame.accumulator {
                        JSValue::Smi(n) => {
                            if *n == 0 {
                                frame.accumulator = JSValue::Number(-0.0);
                            } else if let Some(r) = n.checked_neg() {
                                frame.accumulator = JSValue::Smi(r);
                            } else {
                                frame.accumulator = JSValue::Number(-(*n as f64));
                            }
                        }
                        JSValue::Number(f) => frame.accumulator = JSValue::Number(-*f),
                        JSValue::BigInt(b) => frame.accumulator = JSValue::BigInt(Rc::new(b.neg())),
                        other => frame.accumulator = JSValue::Number(-other.to_number()),
                    }
                }
                Bytecode::BitwiseNot => {
                    match &frame.accumulator {
                        JSValue::Smi(n) => frame.accumulator = JSValue::Smi(!*n),
                        JSValue::BigInt(b) => frame.accumulator = JSValue::BigInt(Rc::new(b.not())),
                        other => {
                            let n = other.to_number() as i32;
                            frame.accumulator = JSValue::Smi(!n);
                        }
                    }
                }
                Bytecode::Inc => {
                    if let JSValue::Smi(ref mut n) = frame.accumulator {
                        if let Some(r) = n.checked_add(1) {
                            *n = r;
                            try_fuse_star!(frame, bytes, pc);
                            continue;
                        }
                    }
                    frame.accumulator = match &frame.accumulator {
                        JSValue::Smi(n) => JSValue::Number(*n as f64 + 1.0),
                        JSValue::Number(f) => JSValue::Number(f + 1.0),
                        JSValue::BigInt(b) => JSValue::BigInt(Rc::new(b.add(&crate::objects::bigint::BigIntData::one()))),
                        other => JSValue::Number(other.to_number() + 1.0),
                    };
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::Dec => {
                    if let JSValue::Smi(ref mut n) = frame.accumulator {
                        if let Some(r) = n.checked_sub(1) {
                            *n = r;
                            try_fuse_star!(frame, bytes, pc);
                            continue;
                        }
                    }
                    frame.accumulator = match &frame.accumulator {
                        JSValue::Smi(n) => JSValue::Number(*n as f64 - 1.0),
                        JSValue::Number(f) => JSValue::Number(f - 1.0),
                        JSValue::BigInt(b) => JSValue::BigInt(Rc::new(b.sub(&crate::objects::bigint::BigIntData::one()))),
                        other => JSValue::Number(other.to_number() - 1.0),
                    };
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::TypeOf => {
                    let s = match frame.accumulator {
                        JSValue::Smi(_) | JSValue::Number(_) => "number",
                        JSValue::Boolean(_) => "boolean",
                        JSValue::String(_) => "string",
                        JSValue::Undefined => "undefined",
                        JSValue::Null | JSValue::Object(_) | JSValue::Array(_) => "object",
                        JSValue::Function(_) => "function",
                        JSValue::Symbol(_) => "symbol",
                        JSValue::BigInt(_) => "bigint",
                    };
                    frame.accumulator = JSValue::String(s.to_string());
                }

                // Object and Array literals
                Bytecode::CreateEmptyObjectLiteral => {
                    // Fast path: if we've already resolved the boilerplate map for this literal site,
                    // create the object directly with the final shape!
                    if let Some(final_map) = cur_bc!().get_boilerplate_map(inst_start) {
                        let obj = JSObject::new_with_map(final_map);
                        frame.accumulator = JSValue::Object(obj);
                        try_fuse_star!(frame, bytes, pc);
                        continue;
                    }

                    // First time or not yet resolved: determine the object literal's target register
                    // and scan following StaNamedProperty instructions initializing this literal.
                    let mut literal_reg: Option<i8> = None;
                    let mut scan = pc;
                    if scan < bytes.len() {
                        let next = unsafe { *bytes.get_unchecked(scan) };
                        if next >= (Bytecode::Star0 as u8) && next <= (Bytecode::Star5 as u8) {
                            literal_reg = Some(-6 - (next - (Bytecode::Star0 as u8)) as i8);
                            scan += 1;
                        } else if next == (Bytecode::Star as u8) && scan + 1 < bytes.len() {
                            literal_reg = Some(unsafe { *bytes.get_unchecked(scan + 1) } as i8);
                            scan += 2;
                        }
                    }

                    let mut sta_entries: Vec<(usize, String)> = Vec::new();
                    let sta_named = Bytecode::StaNamedProperty as u8;
                    if let Some(target_reg) = literal_reg {
                        while scan + 4 < bytes.len() {
                            let next = unsafe { *bytes.get_unchecked(scan) };
                            if next == sta_named {
                                let reg_op = unsafe { *bytes.get_unchecked(scan + 1) } as i8;
                                if reg_op == target_reg {
                                    let sta_pc = scan;
                                    let name_idx = unsafe { *bytes.get_unchecked(scan + 2) } as usize;
                                    if let Some(ConstantValue::String(ref s)) = cur_bc!().get_constant(name_idx) {
                                        sta_entries.push((sta_pc, s.clone()));
                                    }
                                    scan += 4;
                                    continue;
                                } else {
                                    // Storing to a different object means literal initialization ended
                                    break;
                                }
                            } else if next == (Bytecode::Ldar as u8) {
                                let reg_op = unsafe { *bytes.get_unchecked(scan + 1) } as i8;
                                if reg_op == target_reg {
                                    // Reading the literal object (e.g. to move to another register)
                                    break;
                                }
                                scan += 2;
                            } else if next >= (Bytecode::Star0 as u8) && next <= (Bytecode::Star5 as u8) {
                                scan += 1;
                            } else if next == (Bytecode::Star as u8) {
                                scan += 2;
                            } else {
                                let size = Bytecode::from_byte(next).map(|b| 1 + b.number_of_operands()).unwrap_or(0);
                                if size == 0 { break; }
                                if next == (Bytecode::Return as u8) || next == (Bytecode::Jump as u8) || next == (Bytecode::JumpLoop as u8) || next == (Bytecode::CallProperty as u8) || next == (Bytecode::CallUndefinedReceiver as u8) {
                                    break;
                                }
                                scan += size;
                            }
                        }
                    }

                    // Attempt to resolve the transition chain from ROOT_OBJECT_MAP
                    if !sta_entries.is_empty() {
                        let mut curr_map = JSObject::root_object_map();
                        let mut all_found = true;
                        for (_sta_pc, name) in &sta_entries {
                            let next_map = {
                                let map_ref = curr_map.borrow();
                                map_ref.transitions.get(name).cloned()
                            };
                            if let Some(m) = next_map {
                                curr_map = m;
                            } else {
                                all_found = false;
                                break;
                            }
                        }
                        if all_found {
                            let final_map_ptr = curr_map.as_ptr() as usize;
                            for (idx, (sta_pc, _)) in sta_entries.iter().enumerate() {
                                cur_bc!().set_cached_property(*sta_pc, final_map_ptr, idx);
                            }
                            cur_bc!().set_boilerplate_map(inst_start, curr_map.clone());
                            let obj = JSObject::new_with_map(curr_map);
                            frame.accumulator = JSValue::Object(obj);
                            try_fuse_star!(frame, bytes, pc);
                            continue;
                        }
                    }

                    let prop_count = sta_entries.len();
                    let obj = if prop_count > 0 {
                        JSObject::new_empty_with_capacity(prop_count, None)
                    } else {
                        JSObject::new_empty(None)
                    };
                    frame.accumulator = JSValue::Object(obj);
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::CreateEmptyArrayLiteral => {
                    let array_proto = if let Some(global) = get_global!(global_rc) {
                        if let JSValue::Object(arr_ctor) = global.borrow().get_property("Array") {
                            if let JSValue::Object(proto) = arr_ctor.borrow().get_property("prototype") {
                                Some(proto)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    frame.accumulator = JSValue::Array(JSArray::new_array_with_proto(Vec::new(), array_proto));
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::CreateRegExpLiteral => {
                    // SAFETY: pc is within bytes bounds
                    let pat_idx = unsafe { *bytes.get_unchecked(pc) } as usize;
                    pc += 1;
                    let flags_idx = unsafe { *bytes.get_unchecked(pc) } as usize;
                    pc += 1;
                    let pattern = match cur_bc!().get_constant(pat_idx) {
                        Some(ConstantValue::String(s)) => s.as_str(),
                        _ => "",
                    };
                    let flags = match cur_bc!().get_constant(flags_idx) {
                        Some(ConstantValue::String(s)) => s.as_str(),
                        _ => "",
                    };

                    let regexp_proto = if let Some(global) = get_global!(global_rc) {
                        if let JSValue::Object(re_ctor) = global.borrow().get_property("RegExp") {
                            if let JSValue::Object(proto) = re_ctor.borrow().get_property("prototype") {
                                Some(proto)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let instance = crate::builtins::regexp::new_regexp_instance(pattern, flags, regexp_proto)
                        .map_err(|e| e)?;
                    frame.accumulator = JSValue::Object(instance);
                }

                // Property access & assignment
                Bytecode::LdaNamedProperty => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    let name_idx = unsafe { *bytes.get_unchecked(pc) } as usize;
                    pc += 1;
                    let obj_val = frame.read_operand_ref(reg_byte);
                    let name_str = match cur_bc!().get_constant(name_idx) {
                        Some(ConstantValue::String(s)) => s.as_str(),
                        _ => "",
                    };
                    if name_str.starts_with('#') {
                        let has_private = match obj_val {
                            JSValue::Object(ref o) => o.borrow().has_property(name_str),
                            _ => false,
                        };
                        if !has_private {
                            let msg = format!(
                                "TypeError: Cannot read private member {} from an object whose class did not declare it",
                                name_str
                            );
                            if let Some(handler_pc) = bytecode_array.find_handler(inst_start) {
                                frame.accumulator = JSValue::String(msg);
                                pc = handler_pc;
                                continue;
                            } else {
                                return Err(RuntimeError { message: msg });
                            }
                        }
                    }
                    frame.accumulator = match obj_val {
                        JSValue::Object(ref obj) => {
                            let res = {
                                let borrowed = obj.borrow();
                                let map_ptr = borrowed.map.as_ptr() as usize;
                                // Fast path: property IC hit
                                if let Some(field_idx) = cur_bc!().get_cached_property(inst_start, map_ptr) {
                                    if let Some(val) = borrowed.properties.get(field_idx) {
                                        val.clone()
                                    } else {
                                        drop(borrowed);
                                        obj.borrow().get_property(name_str)
                                    }
                                } else if borrowed.ext.is_some() && (borrowed.ext_or_default().regexp_data.is_some() || borrowed.ext_or_default().typed_array_data.is_some() || borrowed.ext_or_default().data_view_data.is_some()) {
                                    let val = borrowed.get_property(name_str);
                                    drop(borrowed);
                                    val
                                } else {
                                    if !borrowed.has_accessor_in_chain() {
                                        let map = borrowed.map.borrow();
                                        if !map.is_dictionary_map {
                                            if let Some(field_idx) = map.find_field_index(name_str) {
                                                if let Some(val) = borrowed.properties.get(field_idx) {
                                                    cur_bc!().set_cached_property(inst_start, map_ptr, field_idx);
                                                    val.clone()
                                                } else {
                                                    drop(map);
                                                    drop(borrowed);
                                                    obj.borrow().get_property(name_str)
                                                }
                                            } else {
                                                drop(map);
                                                drop(borrowed);
                                                obj.borrow().get_property(name_str)
                                            }
                                        } else {
                                            drop(map);
                                            drop(borrowed);
                                            obj.borrow().get_property(name_str)
                                        }
                                    } else {
                                        let getter_name = format!("__get_{}__", name_str);
                                        let getter = borrowed.get_property(&getter_name);
                                        if let JSValue::Function(ref get_fn) = getter {
                                            let recv = JSValue::Object(obj.clone());
                                            drop(borrowed);
                                            match get_fn.call(&recv, &[]) {
                                                Ok(res) => res,
                                                Err(_) => JSValue::Undefined,
                                            }
                                        } else if name_str == "__proto__" {
                                            let p = borrowed.map.borrow().prototype.clone();
                                            drop(borrowed);
                                            match p {
                                                Some(proto_obj) => JSValue::Object(proto_obj),
                                                None => {
                                                    if let Some(global) = get_global!(global_rc) {
                                                        if let JSValue::Object(obj_ctor) = global.borrow().get_property("Object") {
                                                            if let JSValue::Object(proto) = obj_ctor.borrow().get_property("prototype") {
                                                                if Rc::as_ptr(obj) != Rc::as_ptr(&proto) {
                                                                    JSValue::Object(proto)
                                                                } else {
                                                                    JSValue::Null
                                                                }
                                                            } else {
                                                                JSValue::Null
                                                            }
                                                        } else {
                                                            JSValue::Null
                                                        }
                                                    } else {
                                                        JSValue::Null
                                                    }
                                                }
                                            }
                                        } else {
                                            let val = borrowed.get_property(name_str);
                                            drop(borrowed);
                                            val
                                        }
                                    }
                                }
                            };
                            if res != JSValue::Undefined {
                                res
                            } else if let Some(global) = get_global!(global_rc) {
                                if let JSValue::Object(obj_ctor) = global.borrow().get_property("Object") {
                                    if let JSValue::Object(proto) = obj_ctor.borrow().get_property("prototype") {
                                        if Rc::as_ptr(obj) != Rc::as_ptr(&proto) {
                                            if name_str == "__proto__" {
                                                let obj_proto = obj.borrow().map.borrow().prototype.clone();
                                                match obj_proto {
                                                    Some(p) => JSValue::Object(p),
                                                    None => JSValue::Object(proto.clone()),
                                                }
                                            } else {
                                                let getter_name = format!("__get_{}__", name_str);
                                                let getter = proto.borrow().get_property(&getter_name);
                                                if let JSValue::Function(ref get_fn) = getter {
                                                    let recv = JSValue::Object(obj.clone());
                                                    match get_fn.call(&recv, &[]) {
                                                        Ok(res) => res,
                                                        Err(_) => JSValue::Undefined,
                                                    }
                                                } else {
                                                    proto.borrow().get_property(name_str)
                                                }
                                            }
                                        } else {
                                            if name_str == "__proto__" {
                                                JSValue::Null
                                            } else {
                                                JSValue::Undefined
                                            }
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                }
                            } else {
                                JSValue::Undefined
                            }
                        }
                        JSValue::Array(ref arr) => {
                            let arr_ptr = arr.as_ptr();
                            if name_str == "length" {
                                let len = unsafe { (*arr_ptr).elements.len() } as i32;
                                match &mut frame.accumulator {
                                    JSValue::Smi(ref mut dst) => *dst = len,
                                    dst => *dst = JSValue::Smi(len),
                                }
                                try_fuse_star!(frame, bytes, pc);
                                continue;
                            } else {
                                let map_ptr = unsafe { (*arr_ptr).map.as_ptr() as usize };
                                if let Some(func) = cur_bc!().get_cached_proto_method(inst_start, map_ptr) {
                                    frame.accumulator = JSValue::Function(func);
                                    try_fuse_star!(frame, bytes, pc);
                                    continue;
                                }
                                let val = arr.borrow().get_property(name_str);
                                if val != JSValue::Undefined {
                                    if let JSValue::Function(ref f) = val {
                                        cur_bc!().set_cached_proto_method(inst_start, map_ptr, f.clone());
                                    }
                                    val
                                } else if let Some(global) = get_global!(global_rc) {
                                    if let JSValue::Object(arr_ctor) = global.borrow().get_property("Array") {
                                        if let JSValue::Object(proto) = arr_ctor.borrow().get_property("prototype") {
                                            let pval = proto.borrow().get_property(name_str);
                                            if let JSValue::Function(ref f) = pval {
                                                cur_bc!().set_cached_proto_method(inst_start, map_ptr, f.clone());
                                            }
                                            pval
                                        } else {
                                            JSValue::Undefined
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                }
                            }
                        }
                        JSValue::String(ref s) => {
                            if name_str == "length" {
                                let len = if s.is_ascii() { s.len() as i32 } else { s.chars().count() as i32 };
                                if pc + 2 < bytes.len() && unsafe { *bytes.get_unchecked(pc) } == (Bytecode::TestGreaterThan as u8) {
                                    let cmp_reg_byte = unsafe { *bytes.get_unchecked(pc + 1) } as i8;
                                    let rhs = frame.read_operand_ref(cmp_reg_byte);
                                    if let JSValue::Smi(b) = rhs {
                                        let cond = len > *b;
                                        let after_test = pc + 3;
                                        if after_test + 1 < bytes.len() && unsafe { *bytes.get_unchecked(after_test) } == (Bytecode::JumpIfFalse as u8) {
                                            let delta = unsafe { *bytes.get_unchecked(after_test + 1) } as i8 as isize;
                                            pc = if !cond { (after_test as isize + delta) as usize } else { after_test + 2 };
                                            frame.accumulator = JSValue::Boolean(cond);
                                            continue;
                                        }
                                    }
                                }
                                match &mut frame.accumulator {
                                    JSValue::Smi(ref mut dst) => *dst = len,
                                    dst => *dst = JSValue::Smi(len),
                                }
                                try_fuse_star!(frame, bytes, pc);
                                continue;
                            } else {
                                let map_ptr = 1usize;
                                if let Some(func) = cur_bc!().get_cached_proto_method(inst_start, map_ptr) {
                                    frame.accumulator = JSValue::Function(func);
                                    try_fuse_star!(frame, bytes, pc);
                                    continue;
                                }
                                if let Some(global) = get_global!(global_rc) {
                                    if let JSValue::Object(str_ctor) = global.borrow().get_property("String") {
                                        if let JSValue::Object(proto) = str_ctor.borrow().get_property("prototype") {
                                            let pval = proto.borrow().get_property(name_str);
                                            if let JSValue::Function(ref f) = pval {
                                                cur_bc!().set_cached_proto_method(inst_start, map_ptr, f.clone());
                                            }
                                            pval
                                        } else {
                                            JSValue::Undefined
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                }
                            }
                        }
                        JSValue::Symbol(_) => {
                            let getter_name = format!("__get_{}__", name_str);
                            if let Some(global) = get_global!(global_rc) {
                                if let JSValue::Object(sym_ctor) = global.borrow().get_property("Symbol") {
                                    if let JSValue::Object(proto) = sym_ctor.borrow().get_property("prototype") {
                                        let getter = proto.borrow().get_property(&getter_name);
                                        if let JSValue::Function(ref get_fn) = getter {
                                            let recv = obj_val.clone();
                                            match get_fn.call(&recv, &[]) {
                                                Ok(res) => res,
                                                Err(_) => JSValue::Undefined,
                                            }
                                        } else {
                                            proto.borrow().get_property(name_str)
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                }
                            } else {
                                JSValue::Undefined
                            }
                        }
                        JSValue::BigInt(_) => {
                            if let Some(global) = get_global!(global_rc) {
                                if let JSValue::Object(bi_ctor) = global.borrow().get_property("BigInt") {
                                    if let JSValue::Object(proto) = bi_ctor.borrow().get_property("prototype") {
                                        proto.borrow().get_property(name_str)
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                }
                            } else {
                                JSValue::Undefined
                            }
                        }
                        JSValue::Smi(_) | JSValue::Number(_) => {
                            let map_ptr = 2usize;
                            if let Some(func) = cur_bc!().get_cached_proto_method(inst_start, map_ptr) {
                                frame.accumulator = JSValue::Function(func);
                                try_fuse_star!(frame, bytes, pc);
                                continue;
                            }
                            if let Some(global) = get_global!(global_rc) {
                                if let JSValue::Object(num_ctor) = global.borrow().get_property("Number") {
                                    if let JSValue::Object(proto) = num_ctor.borrow().get_property("prototype") {
                                        let pval = proto.borrow().get_property(name_str);
                                        if let JSValue::Function(ref f) = pval {
                                            cur_bc!().set_cached_proto_method(inst_start, map_ptr, f.clone());
                                        }
                                        pval
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                }
                            } else {
                                JSValue::Undefined
                            }
                        }
                        JSValue::Boolean(_) => {
                            let map_ptr = 3usize;
                            if let Some(func) = cur_bc!().get_cached_proto_method(inst_start, map_ptr) {
                                frame.accumulator = JSValue::Function(func);
                                try_fuse_star!(frame, bytes, pc);
                                continue;
                            }
                            if let Some(global) = get_global!(global_rc) {
                                if let JSValue::Object(bool_ctor) = global.borrow().get_property("Boolean") {
                                    if let JSValue::Object(proto) = bool_ctor.borrow().get_property("prototype") {
                                        let pval = proto.borrow().get_property(name_str);
                                        if let JSValue::Function(ref f) = pval {
                                            cur_bc!().set_cached_proto_method(inst_start, map_ptr, f.clone());
                                        }
                                        pval
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                }
                            } else {
                                JSValue::Undefined
                            }
                        }
                        JSValue::Function(ref func) => {
                            if name_str == "name" {
                                JSValue::String(func.name.clone())
                            } else if name_str == "length" {
                                if let Some(ref bc) = func.bytecode {
                                    JSValue::Smi(bc.parameter_count() as i32)
                                } else {
                                    JSValue::Smi(0)
                                }
                            } else {
                                let map_ptr = 4usize;
                                if let Some(f) = cur_bc!().get_cached_proto_method(inst_start, map_ptr) {
                                    frame.accumulator = JSValue::Function(f);
                                    try_fuse_star!(frame, bytes, pc);
                                    continue;
                                }
                                if let Some(global) = get_global!(global_rc) {
                                    if let JSValue::Object(fn_ctor) = global.borrow().get_property("Function") {
                                        if let JSValue::Object(proto) = fn_ctor.borrow().get_property("prototype") {
                                            let pval = proto.borrow().get_property(name_str);
                                            if let JSValue::Function(ref f) = pval {
                                                cur_bc!().set_cached_proto_method(inst_start, map_ptr, f.clone());
                                            }
                                            pval
                                        } else {
                                            JSValue::Undefined
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                }
                            }
                        }
                        _ => JSValue::Undefined,
                    };
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::StaNamedProperty => {
                    // SAFETY: pc is within bytes bounds
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    let name_idx = unsafe { *bytes.get_unchecked(pc) } as usize;
                    pc += 1;
                    let _feedback = unsafe { *bytes.get_unchecked(pc) };
                    pc += 1;
                    let obj_val = frame.read_operand_ref(reg_byte);
                    let name_str = match cur_bc!().get_constant(name_idx) {
                        Some(ConstantValue::String(s)) => s.as_str(),
                        _ => "",
                    };
                    if name_str.starts_with('#') {
                        let has_private = match obj_val {
                            JSValue::Object(ref o) => o.borrow().has_property(name_str),
                            _ => false,
                        };
                        if !has_private {
                            let msg = format!(
                                "TypeError: Cannot write private member {} to an object whose class did not declare it",
                                name_str
                            );
                            if let Some(handler_pc) = bytecode_array.find_handler(inst_start) {
                                frame.accumulator = JSValue::String(msg);
                                pc = handler_pc;
                                continue;
                            } else {
                                return Err(RuntimeError { message: msg });
                            }
                        }
                    }
                    match obj_val {
                        JSValue::Object(ref obj) => {
                            let mut mut_obj = obj.borrow_mut();
                            let map_ptr = mut_obj.map.as_ptr() as usize;
                            // Fast path 1: property IC hit (existing property, direct index write)
                            if let Some(field_idx) = cur_bc!().get_cached_property(inst_start, map_ptr) {
                                if field_idx < mut_obj.properties.len() {
                                    match (&mut mut_obj.properties[field_idx], &frame.accumulator) {
                                        (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => *dst = *src,
                                        (dst, src) => *dst = src.clone(),
                                    }
                                    continue;
                                }
                            }
                            // Fast path 2: transition IC hit (new property, push + map change)
                            if let Some(target_map) = cur_bc!().get_cached_transition(inst_start, map_ptr) {
                                mut_obj.map = target_map;
                                mut_obj.properties.push(frame.accumulator.clone());
                                continue;
                            }
                            // Slow path: exotic types or first-time property set
                            if mut_obj.ext.is_some() && (mut_obj.ext_or_default().regexp_data.is_some() || mut_obj.ext_or_default().typed_array_data.is_some() || mut_obj.ext_or_default().data_view_data.is_some()) {
                                drop(mut_obj);
                                JSObject::set_property(obj, name_str, frame.accumulator.clone());
                                continue;
                            }
                            drop(mut_obj);
                            if name_str == "__proto__" {
                                let new_proto = match frame.accumulator {
                                    JSValue::Object(ref p) => Some(p.clone()),
                                    JSValue::Null => None,
                                    _ => None,
                                };
                                obj.borrow_mut().map.borrow_mut().prototype = new_proto;
                                continue;
                            }
                            if obj.borrow().has_accessor_in_chain() {
                                let setter_name = format!("__set_{}__", name_str);
                                let setter = obj.borrow().get_property(&setter_name);
                                if let JSValue::Function(ref set_fn) = setter {
                                    let recv = JSValue::Object(obj.clone());
                                    let _ = set_fn.call(&recv, &[frame.accumulator.clone()]);
                                    continue;
                                }
                            }
                            JSObject::set_property(obj, name_str, frame.accumulator.clone());
                            let borrowed = obj.borrow();
                            let new_map = borrowed.map.clone();
                            let new_map_ptr = new_map.as_ptr() as usize;
                            if new_map_ptr != map_ptr {
                                cur_bc!().set_cached_transition(inst_start, map_ptr, new_map);
                            } else {
                                let map = new_map.borrow();
                                if !map.is_dictionary_map {
                                    if let Some(field_idx) = map.find_field_index(name_str) {
                                        cur_bc!().set_cached_property(inst_start, new_map_ptr, field_idx);
                                    }
                                }
                            }
                        }
                        JSValue::Array(ref obj) => {
                            JSObject::set_property(obj, name_str, frame.accumulator.clone());
                        }
                        _ => {}
                    }
                }
                Bytecode::LdaKeyedProperty => {
                    // SAFETY: pc + 2 is within bytes bounds
                    let obj_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    let key_byte = unsafe { *bytes.get_unchecked(pc + 1) } as i8;
                    pc += 3;
                    {
                        let obj_val = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, obj_byte);
                        let key_val = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, key_byte);
                        match (obj_val, key_val) {
                            (JSValue::Array(ref arr), JSValue::Smi(idx)) if *idx >= 0 => {
                                let uidx = *idx as usize;
                                let elems = unsafe { &(*arr.as_ptr()).elements };
                                if uidx < elems.len() {
                                    let val = unsafe { elems.get_unchecked(uidx) };
                                    match (&mut frame.accumulator, val) {
                                        (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => *dst = *src,
                                        (dst, src) => *dst = src.clone(),
                                    }
                                } else {
                                    frame.accumulator = JSValue::Undefined;
                                }
                            }
                            (JSValue::Object(ref obj), JSValue::Smi(idx)) if *idx >= 0 => {
                                let borrowed = obj.borrow();
                                if let Some(ref ta) = borrowed.ext_or_default().typed_array_data {
                                    let uidx = *idx as usize;
                                    if uidx < ta.length {
                                        let buf_obj = ta.buffer.borrow();
                                        if let Some(ref buf_bytes) = buf_obj.ext_or_default().array_buffer_data {
                                            if ta.kind == crate::objects::typed_array::TypedArrayKind::Int32 {
                                                let pos = ta.byte_offset + uidx * 4;
                                                let bytes_borrow = buf_bytes.borrow();
                                                if pos + 4 <= bytes_borrow.len() {
                                                    let val = i32::from_le_bytes([
                                                        bytes_borrow[pos],
                                                        bytes_borrow[pos + 1],
                                                        bytes_borrow[pos + 2],
                                                        bytes_borrow[pos + 3],
                                                    ]);
                                                    frame.accumulator = JSValue::Smi(val);
                                                } else {
                                                    frame.accumulator = JSValue::Undefined;
                                                }
                                            } else if ta.kind == crate::objects::typed_array::TypedArrayKind::Uint8 {
                                                let pos = ta.byte_offset + uidx;
                                                let bytes_borrow = buf_bytes.borrow();
                                                if pos < bytes_borrow.len() {
                                                    frame.accumulator = JSValue::Smi(bytes_borrow[pos] as i32);
                                                } else {
                                                    frame.accumulator = JSValue::Undefined;
                                                }
                                            } else {
                                                frame.accumulator = ta.kind.read_element(&buf_bytes.borrow(), ta.byte_offset, uidx);
                                            }
                                        } else if let Some(ref sab_bytes) = buf_obj.ext_or_default().shared_array_buffer_data {
                                            let data = sab_bytes.read().unwrap();
                                            if ta.kind == crate::objects::typed_array::TypedArrayKind::Int32 {
                                                let pos = ta.byte_offset + uidx * 4;
                                                if pos + 4 <= data.len() {
                                                    let val = i32::from_le_bytes([
                                                        data[pos],
                                                        data[pos + 1],
                                                        data[pos + 2],
                                                        data[pos + 3],
                                                    ]);
                                                    frame.accumulator = JSValue::Smi(val);
                                                } else {
                                                    frame.accumulator = JSValue::Undefined;
                                                }
                                            } else if ta.kind == crate::objects::typed_array::TypedArrayKind::Uint8 {
                                                let pos = ta.byte_offset + uidx;
                                                if pos < data.len() {
                                                    frame.accumulator = JSValue::Smi(data[pos] as i32);
                                                } else {
                                                    frame.accumulator = JSValue::Undefined;
                                                }
                                            } else {
                                                frame.accumulator = ta.kind.read_element(&data, ta.byte_offset, uidx);
                                            }
                                        } else {
                                            frame.accumulator = JSValue::Undefined;
                                        }
                                    } else {
                                        frame.accumulator = JSValue::Undefined;
                                    }
                                } else {
                                    let key_str = idx.to_string();
                                    frame.accumulator = borrowed.get_property(&key_str);
                                }
                            }
                            (JSValue::Object(ref obj), _) => {
                                let key_str = key_val.to_string_val();
                                let borrowed = obj.borrow();
                                let val = borrowed.get_property(&key_str);
                                frame.accumulator = if val != JSValue::Undefined {
                                    val
                                } else if borrowed.get_property("__time_ms__") != JSValue::Undefined {
                                    if let Some(global) = get_global!(global_rc) {
                                        if let JSValue::Object(date_ctor) = global.borrow().get_property("Date") {
                                            if let JSValue::Object(proto) = date_ctor.borrow().get_property("prototype") {
                                                proto.borrow().get_property(&key_str)
                                            } else {
                                                JSValue::Undefined
                                            }
                                        } else {
                                            JSValue::Undefined
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else if let Some(global) = get_global!(global_rc) {
                                    if let JSValue::Object(obj_ctor) = global.borrow().get_property("Object") {
                                        if let JSValue::Object(proto) = obj_ctor.borrow().get_property("prototype") {
                                            if Rc::as_ptr(obj) != Rc::as_ptr(&proto) {
                                                proto.borrow().get_property(&key_str)
                                            } else {
                                                JSValue::Undefined
                                            }
                                        } else {
                                            JSValue::Undefined
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                };
                            }
                            (JSValue::Array(ref arr), _) => {
                                let key_str = key_val.to_string_val();
                                let borrowed = arr.borrow();
                                frame.accumulator = if let Ok(idx) = key_str.parse::<usize>() {
                                    borrowed.get_element(idx)
                                } else {
                                    let val = borrowed.get_property(&key_str);
                                    if val != JSValue::Undefined {
                                        val
                                    } else if let Some(global) = get_global!(global_rc) {
                                        if let JSValue::Object(arr_ctor) = global.borrow().get_property("Array") {
                                            if let JSValue::Object(proto) = arr_ctor.borrow().get_property("prototype") {
                                                proto.borrow().get_property(&key_str)
                                            } else {
                                                JSValue::Undefined
                                            }
                                        } else {
                                            JSValue::Undefined
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                };
                            }
                            (JSValue::String(ref s), _) => {
                                let key_str = key_val.to_string_val();
                                frame.accumulator = if let Ok(idx) = key_str.parse::<usize>() {
                                    s.chars().nth(idx).map(|c| JSValue::String(c.to_string())).unwrap_or(JSValue::Undefined)
                                } else if key_str == "length" {
                                    JSValue::Smi(s.chars().count() as i32)
                                } else if let Some(global) = get_global!(global_rc) {
                                    if let JSValue::Object(str_ctor) = global.borrow().get_property("String") {
                                        if let JSValue::Object(proto) = str_ctor.borrow().get_property("prototype") {
                                            proto.borrow().get_property(&key_str)
                                        } else {
                                            JSValue::Undefined
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                };
                            }
                            (JSValue::BigInt(_), _) => {
                                let key_str = key_val.to_string_val();
                                frame.accumulator = if let Some(global) = get_global!(global_rc) {
                                    if let JSValue::Object(bi_ctor) = global.borrow().get_property("BigInt") {
                                        if let JSValue::Object(proto) = bi_ctor.borrow().get_property("prototype") {
                                            proto.borrow().get_property(&key_str)
                                        } else {
                                            JSValue::Undefined
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                };
                            }
                            (JSValue::Smi(_) | JSValue::Number(_), _) => {
                                let key_str = key_val.to_string_val();
                                frame.accumulator = if let Some(global) = get_global!(global_rc) {
                                    if let JSValue::Object(num_ctor) = global.borrow().get_property("Number") {
                                        if let JSValue::Object(proto) = num_ctor.borrow().get_property("prototype") {
                                            proto.borrow().get_property(&key_str)
                                        } else {
                                            JSValue::Undefined
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                };
                            }
                            (JSValue::Boolean(_), _) => {
                                let key_str = key_val.to_string_val();
                                frame.accumulator = if let Some(global) = get_global!(global_rc) {
                                    if let JSValue::Object(bool_ctor) = global.borrow().get_property("Boolean") {
                                        if let JSValue::Object(proto) = bool_ctor.borrow().get_property("prototype") {
                                            proto.borrow().get_property(&key_str)
                                        } else {
                                            JSValue::Undefined
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                };
                            }
                            (JSValue::Function(ref func), _) => {
                                let key_str = key_val.to_string_val();
                                frame.accumulator = if key_str == "name" {
                                    JSValue::String(func.name.clone())
                                } else if key_str == "length" {
                                    if let Some(ref bc) = func.bytecode {
                                        JSValue::Smi(bc.parameter_count() as i32)
                                    } else {
                                        JSValue::Smi(0)
                                    }
                                } else if let Some(global) = get_global!(global_rc) {
                                    if let JSValue::Object(fn_ctor) = global.borrow().get_property("Function") {
                                        if let JSValue::Object(proto) = fn_ctor.borrow().get_property("prototype") {
                                            proto.borrow().get_property(&key_str)
                                        } else {
                                            JSValue::Undefined
                                        }
                                    } else {
                                        JSValue::Undefined
                                    }
                                } else {
                                    JSValue::Undefined
                                };
                            }
                            _ => {
                                frame.accumulator = JSValue::Undefined;
                            }
                        }
                    }
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::StaKeyedProperty => {
                    // SAFETY: pc + 3 is within bytes bounds
                    let obj_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    let key_byte = unsafe { *bytes.get_unchecked(pc + 1) } as i8;
                    pc += 4;
                    let obj_val = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, obj_byte);
                    let key_val = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, key_byte);
                    match (obj_val, key_val) {
                        (JSValue::Array(ref arr), JSValue::Smi(idx)) if *idx >= 0 => {
                            let uidx = *idx as usize;
                            let elems = unsafe { &mut (*arr.as_ptr()).elements };
                            if uidx < elems.len() {
                                match (unsafe { elems.get_unchecked_mut(uidx) }, &frame.accumulator) {
                                    (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => *dst = *src,
                                    (dst, src) => *dst = src.clone(),
                                }
                            } else {
                                arr.borrow_mut().set_element(uidx, frame.accumulator.clone());
                            }
                            continue;
                        }
                        (JSValue::Object(ref obj), JSValue::Smi(idx)) if *idx >= 0 => {
                            let borrowed = obj.borrow();
                            if let Some(ref ta) = borrowed.ext_or_default().typed_array_data {
                                let uidx = *idx as usize;
                                if uidx < ta.length {
                                    let buf_obj = ta.buffer.borrow();
                                    if let Some(ref buf_bytes) = buf_obj.ext_or_default().array_buffer_data {
                                        if ta.kind == crate::objects::typed_array::TypedArrayKind::Int32 {
                                            if let JSValue::Smi(s) = frame.accumulator {
                                                let pos = ta.byte_offset + uidx * 4;
                                                let mut bytes_borrow = buf_bytes.borrow_mut();
                                                if pos + 4 <= bytes_borrow.len() {
                                                    bytes_borrow[pos..pos + 4].copy_from_slice(&s.to_le_bytes());
                                                    continue;
                                                }
                                            }
                                        } else if ta.kind == crate::objects::typed_array::TypedArrayKind::Uint8 {
                                            if let JSValue::Smi(s) = frame.accumulator {
                                                let pos = ta.byte_offset + uidx;
                                                let mut bytes_borrow = buf_bytes.borrow_mut();
                                                if pos < bytes_borrow.len() {
                                                    bytes_borrow[pos] = s as u8;
                                                    continue;
                                                }
                                            }
                                        }
                                        ta.kind.write_element(&mut buf_bytes.borrow_mut(), ta.byte_offset, uidx, &frame.accumulator);
                                        continue;
                                    } else if let Some(ref sab_bytes) = buf_obj.ext_or_default().shared_array_buffer_data {
                                        let mut bytes_borrow = sab_bytes.write().unwrap();
                                        if ta.kind == crate::objects::typed_array::TypedArrayKind::Int32 {
                                            if let JSValue::Smi(s) = frame.accumulator {
                                                let pos = ta.byte_offset + uidx * 4;
                                                if pos + 4 <= bytes_borrow.len() {
                                                    bytes_borrow[pos..pos + 4].copy_from_slice(&s.to_le_bytes());
                                                    continue;
                                                }
                                            }
                                        } else if ta.kind == crate::objects::typed_array::TypedArrayKind::Uint8 {
                                            if let JSValue::Smi(s) = frame.accumulator {
                                                let pos = ta.byte_offset + uidx;
                                                if pos < bytes_borrow.len() {
                                                    bytes_borrow[pos] = s as u8;
                                                    continue;
                                                }
                                            }
                                        }
                                        ta.kind.write_element(&mut bytes_borrow, ta.byte_offset, uidx, &frame.accumulator);
                                        continue;
                                    }
                                }
                            }
                            drop(borrowed);
                            let key_str = idx.to_string();
                            JSObject::set_property(obj, &key_str, frame.accumulator.clone());
                            continue;
                        }
                        (JSValue::Object(ref obj), _) => {
                            let key_str = key_val.to_string_val();
                            JSObject::set_property(obj, &key_str, frame.accumulator.clone());
                        }
                        (JSValue::Array(ref arr), _) => {
                            let idx = key_val.to_number() as usize;
                            arr.borrow_mut().set_element(idx, frame.accumulator.clone());
                        }
                        _ => {}
                    }
                }
                Bytecode::StaInArrayLiteral => {
                    let arr_byte = bytes[pc] as i8;
                    pc += 1;
                    let idx_byte = bytes[pc] as i8;
                    pc += 1;
                    let _feedback = bytes[pc];
                    pc += 1;
                    let _flags = bytes[pc];
                    pc += 1;
                    let arr_reg = Register::from_operand(arr_byte as i32);
                    let idx_reg = Register::from_operand(idx_byte as i32);
                    let arr_val = frame.read_register_ref(arr_reg);
                    let idx_val = frame.read_register_ref(idx_reg);
                    if let JSValue::Array(ref arr) = arr_val {
                        let idx = match idx_val {
                            JSValue::Smi(i) if *i >= 0 => *i as usize,
                            _ => idx_val.to_number() as usize,
                        };
                        arr.borrow_mut().set_element(idx, frame.accumulator.clone());
                    }
                }
                Bytecode::SpreadInArrayLiteral => {
                    let arr_byte = bytes[pc] as i8;
                    pc += 1;
                    let arr_reg = Register::from_operand(arr_byte as i32);
                    let arr_val = frame.read_register_ref(arr_reg).clone();
                    if let JSValue::Array(ref target_arr) = arr_val {
                        match &frame.accumulator {
                            JSValue::Array(ref src_arr) => {
                                let elements = src_arr.borrow().elements.clone();
                                let mut target_borrow = target_arr.borrow_mut();
                                for elem in elements {
                                    target_borrow.elements.push(elem);
                                }
                            }
                            JSValue::String(ref s) => {
                                let mut target_borrow = target_arr.borrow_mut();
                                for c in s.chars() {
                                    target_borrow.elements.push(JSValue::String(c.to_string()));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Bytecode::SpreadInObjectLiteral => {
                    let obj_byte = bytes[pc] as i8;
                    pc += 1;
                    let obj_reg = Register::from_operand(obj_byte as i32);
                    let obj_val = frame.read_register_ref(obj_reg).clone();
                    if let JSValue::Object(ref target_obj) = obj_val {
                        if let JSValue::Object(ref src_obj) = frame.accumulator {
                            let borrowed = src_obj.borrow();
                            let keys: Vec<String> = if borrowed.map.borrow().is_dictionary_map {
                                borrowed.ext_or_default().dictionary_properties.keys().cloned().collect()
                            } else {
                                borrowed.map.borrow().descriptors.iter().filter(|d| !d.details.is_dont_enum()).map(|d| d.name.clone()).collect()
                            };
                            for key in keys {
                                let val = borrowed.get_property(&key);
                                JSObject::set_property(target_obj, &key, val);
                            }
                        }
                    }
                }

                // Global variables
                Bytecode::LdaGlobal => {
                    // SAFETY: pc is checked to be strictly within bytecode bounds.
                    let name_idx = unsafe { *bytes.get_unchecked(pc) } as usize;
                    pc += 1;
                    // SAFETY: pc is checked to be strictly within bytecode bounds.
                    let _feedback = unsafe { *bytes.get_unchecked(pc) };
                    pc += 1;
                    if let Some(global) = get_global!(global_rc) {
                        let global_ptr = global.as_ptr();
                        let map_ptr = unsafe { (*global_ptr).map.as_ptr() as usize };
                        if let Some(field_idx) = cur_bc!().get_cached_global(inst_start, map_ptr) {
                            let props = unsafe { &(*global_ptr).properties };
                            if field_idx < props.len() {
                                let val = unsafe { props.get_unchecked(field_idx) };
                                if pc + 1 < bytes.len() {
                                    let op1 = unsafe { *bytes.get_unchecked(pc) };
                                    if op1 >= (Bytecode::Star0 as u8) && op1 <= (Bytecode::Star5 as u8) {
                                        let op2 = unsafe { *bytes.get_unchecked(pc + 1) };
                                        if op2 == (Bytecode::Ldar as u8)
                                            || op2 == (Bytecode::LdaZero as u8)
                                            || op2 == (Bytecode::LdaSmi as u8)
                                            || op2 == (Bytecode::LdaConstant as u8)
                                            || op2 == (Bytecode::LdaUndefined as u8)
                                            || op2 == (Bytecode::LdaNull as u8)
                                        {
                                            let star_idx = (op1 - (Bytecode::Star0 as u8)) as usize;
                                            let slot_ref = unsafe { frame.slots.get_unchecked_mut(4 + star_idx) };
                                            match (slot_ref, val) {
                                                (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => *dst = *src,
                                                (dst, src) => *dst = src.clone(),
                                            }
                                            pc += 1;
                                            continue;
                                        }
                                    }
                                }
                                match (&mut frame.accumulator, val) {
                                    (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => *dst = *src,
                                    (dst, src) => *dst = src.clone(),
                                }
                                try_fuse_star!(frame, bytes, pc);
                                continue;
                            }
                        }
                        let global_ref = global.borrow();
                        let name_str = match cur_bc!().get_constant(name_idx) {
                            Some(ConstantValue::String(s)) => s.as_str(),
                            _ => "",
                        };
                        let map = global_ref.map.borrow();
                        if !map.is_dictionary_map {
                            if let Some(field_idx) = map.find_field_index(name_str) {
                                if let Some(val) = global_ref.properties.get(field_idx) {
                                    drop(map);
                                    cur_bc!().set_cached_global(inst_start, map_ptr, field_idx);
                                    frame.accumulator = val.clone();
                                    drop(global_ref);
                                    try_fuse_star!(frame, bytes, pc);
                                    continue;
                                }
                            }
                        }
                        drop(map);
                        frame.accumulator = global_ref.get_property(name_str);
                    } else {
                        frame.accumulator = JSValue::Undefined;
                    }
                    try_fuse_star!(frame, bytes, pc);
                }
                Bytecode::StaGlobal => {
                    // SAFETY: pc is checked to be strictly within bytecode bounds.
                    let name_idx = unsafe { *bytes.get_unchecked(pc) } as usize;
                    pc += 1;
                    // SAFETY: pc is checked to be strictly within bytecode bounds.
                    let _feedback = unsafe { *bytes.get_unchecked(pc) };
                    pc += 1;
                    if let Some(global) = get_global!(global_rc) {
                        let global_ref = global.borrow();
                        let map_ptr = global_ref.map.as_ptr() as usize;
                        if let Some(field_idx) = cur_bc!().get_cached_global(inst_start, map_ptr) {
                            drop(global_ref);
                            let mut mut_global = global.borrow_mut();
                            if field_idx < mut_global.properties.len() {
                                mut_global.properties[field_idx] = frame.accumulator.clone();
                                continue;
                            }
                        } else {
                            drop(global_ref);
                        }
                        let name_str = match cur_bc!().get_constant(name_idx) {
                            Some(ConstantValue::String(s)) => s.as_str(),
                            _ => "",
                        };
                        JSObject::set_property(global, name_str, frame.accumulator.clone());
                    }
                }

                // Function calls
                Bytecode::CallUndefinedReceiver => {
                    // SAFETY: pc is checked to be strictly within bytecode bounds.
                    let callable_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    // SAFETY: pc is checked to be strictly within bytecode bounds.
                    let args_start_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    // SAFETY: pc is checked to be strictly within bytecode bounds.
                    let arg_count = unsafe { *bytes.get_unchecked(pc) } as usize;
                    pc += 1;
                    let callable_val = frame.read_operand_ref(callable_byte);
                    let res = match callable_val {
                        JSValue::Function(ref func) => {
                            let cur_global = get_global!(global_rc);
                            let count = func.invocation_count.get();
                            if count <= JSFunction::JIT_HOT_THRESHOLD {
                                func.invocation_count.set(count + 1);
                                if count + 1 == JSFunction::SPARKPLUG_HOT_THRESHOLD {
                                    if func.compile_to_sparkplug() {
                                        func.is_jit.set(true);
                                    }
                                } else if count + 1 == JSFunction::JIT_HOT_THRESHOLD {
                                    if func.compile_to_jit() {
                                        func.is_jit.set(true);
                                    } else if func.compile_to_sparkplug() {
                                        func.is_jit.set(true);
                                    }
                                }
                            }
                            if let Some(native_fn) = func.native_fn.get() {
                                let res_opt = match arg_count {
                                    0 => Some(unsafe { native_fn(0, 0, 0, 0) } as i32),
                                    1 => {
                                        match frame.read_operand_ref(args_start_byte) {
                                            JSValue::Smi(a1) => Some(unsafe { native_fn(0, *a1 as i64, 0, 0) } as i32),
                                            _ => None,
                                        }
                                    }
                                    2 => {
                                        let first_reg_idx = Register::from_operand(args_start_byte as i32).index();
                                        let r1 = frame.read_register_ref(Register::new(first_reg_idx));
                                        let r2 = frame.read_register_ref(Register::new(first_reg_idx + 1));
                                        if let (JSValue::Smi(a1), JSValue::Smi(a2)) = (r1, r2) {
                                            Some(unsafe { native_fn(0, *a1 as i64, *a2 as i64, 0) } as i32)
                                        } else {
                                            None
                                        }
                                    }
                                    3 => {
                                        let first_reg_idx = Register::from_operand(args_start_byte as i32).index();
                                        let r1 = frame.read_register_ref(Register::new(first_reg_idx));
                                        let r2 = frame.read_register_ref(Register::new(first_reg_idx + 1));
                                        let r3 = frame.read_register_ref(Register::new(first_reg_idx + 2));
                                        if let (JSValue::Smi(a1), JSValue::Smi(a2), JSValue::Smi(a3)) = (r1, r2, r3) {
                                            Some(unsafe { native_fn(0, *a1 as i64, *a2 as i64, *a3 as i64) } as i32)
                                        } else {
                                            None
                                        }
                                    }
                                    _ => None,
                                };

                                if let Some(ret_val) = res_opt {
                                    match &mut frame.accumulator {
                                        JSValue::Smi(ref mut dst) => *dst = ret_val,
                                        dst => *dst = JSValue::Smi(ret_val),
                                    }
                                    try_fuse_star!(frame, bytes, pc);
                                    continue;
                                }
                            }
                            if !func.is_jit.get() {
                                if let Some(ref bc) = func.bytecode {
                                    let call_res = if arg_count == 1 {
                                        let (is_smi, n_val) = match frame.read_operand_ref(args_start_byte) {
                                            JSValue::Smi(n) => (true, *n),
                                            _ => (false, 0),
                                        };
                                        if is_smi {
                                            // Recursive Smi specialization: bypass interpreter entirely
                                            if let Some(ref spec) = bc.recursive_smi_spec {
                                                let result = Self::execute_recursive_smi(spec, n_val);
                                                match &mut frame.accumulator {
                                                    JSValue::Smi(ref mut dst) => *dst = result,
                                                    dst => *dst = JSValue::Smi(result),
                                                }
                                                try_fuse_star!(frame, bytes, pc);
                                                continue;
                                            }
                                            if let Some(imm) = bc.quick_smi_base_case {
                                                if n_val <= imm {
                                                    match &mut frame.accumulator {
                                                        JSValue::Smi(ref mut dst) => *dst = n_val,
                                                        dst => *dst = JSValue::Smi(n_val),
                                                    }
                                                    try_fuse_star!(frame, bytes, pc);
                                                    continue;
                                                }
                                            }
                                        }
                                        let arg0 = frame.read_operand_ref(args_start_byte);
                                        Self::execute_with_receiver(bc, &JSValue::Undefined, std::slice::from_ref(arg0), cur_global)
                                    } else if arg_count == 0 {
                                        Self::execute_with_receiver(bc, &JSValue::Undefined, &[], cur_global)
                                    } else {
                                        let first_reg_idx = Register::from_operand(args_start_byte as i32).index();
                                        if arg_count <= 4 {
                                            let mut stack_args = [JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined];
                                            for i in 0..arg_count {
                                                let reg = Register::new(first_reg_idx + i as i32);
                                                stack_args[i] = frame.read_register_ref(reg).clone();
                                            }
                                            Self::execute_with_receiver(bc, &JSValue::Undefined, &stack_args[..arg_count], cur_global)
                                        } else {
                                            let mut call_args = Vec::with_capacity(arg_count);
                                            for i in 0..arg_count {
                                                let reg = Register::new(first_reg_idx + i as i32);
                                                call_args.push(frame.read_register_ref(reg).clone());
                                            }
                                            Self::execute_with_receiver(bc, &JSValue::Undefined, &call_args, cur_global)
                                        }
                                    };
                                    match call_res {
                                        Ok(val) => {
                                            if pc + 3 < bytes.len() {
                                                let next_op = unsafe { *bytes.get_unchecked(pc) };
                                                if next_op == (Bytecode::Add as u8) {
                                                    let add_reg_byte = unsafe { *bytes.get_unchecked(pc + 1) } as i8;
                                                    let after_add_op = unsafe { *bytes.get_unchecked(pc + 3) };
                                                    if after_add_op == (Bytecode::Return as u8) && generator_ctx.is_none() {
                                                        let rhs = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, add_reg_byte);
                                                        if let (JSValue::Smi(a), JSValue::Smi(b)) = (&val, rhs) {
                                                            if let Some(sum) = a.checked_add(*b) {
                                                                return Ok(JSValue::Smi(sum));
                                                            }
                                                        }
                                                        return Ok(val.add(rhs));
                                                    }
                                                }
                                            }
                                            if pc < bytes.len() {
                                                let next_op = unsafe { *bytes.get_unchecked(pc) };
                                                if next_op >= (Bytecode::Star0 as u8) && next_op <= (Bytecode::Star5 as u8) {
                                                    let star_idx = (next_op - (Bytecode::Star0 as u8)) as usize;
                                                    let slot_ref = unsafe { frame.slots.get_unchecked_mut(4 + star_idx) };
                                                    match (slot_ref, val) {
                                                        (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => *dst = src,
                                                        (dst, src) => *dst = src,
                                                    }
                                                    pc += 1;
                                                    continue;
                                                }
                                            }
                                            match (&mut frame.accumulator, val) {
                                                (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => *dst = src,
                                                (dst, src) => *dst = src,
                                            }
                                            try_fuse_star!(frame, bytes, pc);
                                            continue;
                                        }
                                        Err(e) => {
                                            if let Some(handler_pc) = bytecode_array.find_handler(inst_start) {
                                                frame.accumulator = JSValue::String(e.message);
                                                pc = handler_pc;
                                                continue;
                                            } else {
                                                return Err(e);
                                            }
                                        }
                                    }
                                }
                            }
                            if arg_count == 1 {
                                let arg0 = frame.read_operand_ref(args_start_byte);
                                func.call_with_global(&JSValue::Undefined, std::slice::from_ref(arg0), cur_global)
                            } else if arg_count == 0 {
                                func.call_with_global(&JSValue::Undefined, &[], cur_global)
                            } else {
                                let first_reg_idx = Register::from_operand(args_start_byte as i32).index();
                                if arg_count <= 4 {
                                    let mut stack_args = [JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined];
                                    for i in 0..arg_count {
                                        let reg = Register::new(first_reg_idx + i as i32);
                                        stack_args[i] = frame.read_register_ref(reg).clone();
                                    }
                                    func.call_with_global(&JSValue::Undefined, &stack_args[..arg_count], cur_global)
                                } else {
                                    let mut call_args = Vec::with_capacity(arg_count);
                                    for i in 0..arg_count {
                                        let reg = Register::new(first_reg_idx + i as i32);
                                        call_args.push(frame.read_register_ref(reg).clone());
                                    }
                                    func.call_with_global(&JSValue::Undefined, &call_args, cur_global)
                                }
                            }
                        }
                        JSValue::Object(ref obj) => {
                            let first_reg_idx = Register::from_operand(args_start_byte as i32).index();
                            let mut call_args = Vec::with_capacity(arg_count);
                            for i in 0..arg_count {
                                let reg = Register::new(first_reg_idx + i as i32);
                                call_args.push(frame.read_register(reg));
                            }
                            if let Some(ref proxy) = obj.borrow().ext_or_default().proxy_data.clone() {
                                let handler_obj = match &proxy.handler {
                                    JSValue::Object(o) => Some(o.clone()),
                                    _ => None,
                                };
                                if let Some(h) = handler_obj {
                                    let trap = h.borrow().get_property("apply");
                                    if let JSValue::Function(f) = trap {
                                        let args_arr = JSObject::new_empty(None);
                                        args_arr.borrow_mut().elements = call_args.clone();
                                        args_arr.borrow_mut().map.borrow_mut().instance_type = crate::objects::map::InstanceType::JSArray;
                                        f.call(&proxy.handler, &[proxy.target.clone(), JSValue::Undefined, JSValue::Array(args_arr)])
                                    } else {
                                        match &proxy.target {
                                            JSValue::Function(tf) => tf.call(&JSValue::Undefined, &call_args),
                                            _ => Err("Proxy target is not a function".to_string()),
                                        }
                                    }
                                } else {
                                    Err("Invalid proxy handler".to_string())
                                }
                            } else if obj.borrow().get_property("__is_class__") == JSValue::Boolean(true) {
                                Err("TypeError: Class constructor cannot be invoked without 'new'".to_string())
                            } else {
                                let call_prop = obj.borrow().get_property("__call__");
                                if let JSValue::Function(func) = call_prop {
                                    func.call(&JSValue::Undefined, &call_args)
                                } else {
                                    Err(format!("{:?} is not a function", callable_val))
                                }
                            }
                        }
                        _ => Err(format!("{:?} is not a function", callable_val)),
                    };
                    match res {
                        Ok(val) => frame.accumulator = val,
                        Err(e) => {
                            if let Some(handler_pc) = cur_bc!().find_handler(inst_start) {
                                frame.accumulator = JSValue::String(e);
                                pc = handler_pc;
                            } else {
                                return Err(RuntimeError { message: e });
                            }
                        }
                    }
                }
                Bytecode::Construct => {
                    let callable_byte = bytes[pc] as i8;
                    pc += 1;
                    let args_start_byte = bytes[pc] as i8;
                    pc += 1;
                    let arg_count = bytes[pc] as usize;
                    pc += 1;
                    let _feedback = bytes[pc];
                    pc += 1;

                    let callable_val = frame.read_operand_ref(callable_byte);
                    let mut call_args = Vec::with_capacity(arg_count);
                    let first_reg_idx = Register::from_operand(args_start_byte as i32).index();
                    for i in 0..arg_count {
                        let reg = Register::new(first_reg_idx + i as i32);
                        call_args.push(frame.read_register(reg));
                    }

                    let prev_new_target = CURRENT_NEW_TARGET.with(|nt| nt.borrow_mut().replace(callable_val.clone()));
                    let res = match callable_val {
                        JSValue::Object(ref obj) => {
                            if let Some(ref proxy) = obj.borrow().ext_or_default().proxy_data.clone() {
                                let handler_obj = match &proxy.handler {
                                    JSValue::Object(o) => Some(o.clone()),
                                    _ => None,
                                };
                                if let Some(h) = handler_obj {
                                    let trap = h.borrow().get_property("construct");
                                    if let JSValue::Function(f) = trap {
                                        let args_arr = JSObject::new_empty(None);
                                        args_arr.borrow_mut().elements = call_args.clone();
                                        args_arr.borrow_mut().map.borrow_mut().instance_type = crate::objects::map::InstanceType::JSArray;
                                        f.call(&proxy.handler, &[proxy.target.clone(), JSValue::Array(args_arr), callable_val.clone()])
                                    } else {
                                        match &proxy.target {
                                            JSValue::Function(tf) => {
                                                let new_instance = JSObject::new_empty(None);
                                                let recv = JSValue::Object(new_instance.clone());
                                                match tf.call(&recv, &call_args) {
                                                    Ok(ctor_res) => match ctor_res {
                                                        JSValue::Object(_) => Ok(ctor_res),
                                                        _ => Ok(JSValue::Object(new_instance)),
                                                    },
                                                    Err(e) => Err(e),
                                                }
                                            }
                                            _ => Err("Proxy target is not a constructor".to_string()),
                                        }
                                    }
                                } else {
                                    Err("Invalid proxy handler".to_string())
                                }
                            } else if obj.borrow().get_property("__not_constructor__") == JSValue::Boolean(true) {
                                let name_val = obj.borrow().get_property("name");
                                let name = if let JSValue::String(s) = name_val { s } else { "Function".to_string() };
                                Err(format!("TypeError: {} is not a constructor", name))
                            } else {
                                let is_class = obj.borrow().get_property("__is_class__");
                                let proto_val = obj.borrow().get_property("prototype");
                                let target_proto = if let JSValue::Object(ref p) = proto_val {
                                    Some(p.clone())
                                } else {
                                    None
                                };

                                let call_prop = obj.borrow().get_property("__call__");
                                if let JSValue::Function(func) = call_prop {
                                    if is_class == JSValue::Boolean(true) {
                                        let new_instance = JSObject::new_empty(target_proto);
                                        let recv = JSValue::Object(new_instance.clone());
                                        match func.call(&recv, &call_args) {
                                            Ok(ctor_res) => match ctor_res {
                                                JSValue::Object(_) => Ok(ctor_res),
                                                _ => Ok(JSValue::Object(new_instance)),
                                            },
                                            Err(e) => Err(e),
                                        }
                                    } else {
                                        func.call(&JSValue::Undefined, &call_args)
                                    }
                                } else {
                                    let ctor_prop = obj.borrow().get_property("__construct__");
                                    if let JSValue::Function(func) = ctor_prop {
                                        let new_instance = JSObject::new_empty(None);
                                        let recv = JSValue::Object(new_instance.clone());
                                        match func.call(&recv, &call_args) {
                                            Ok(ctor_res) => match ctor_res {
                                                JSValue::Object(_) => Ok(ctor_res),
                                                _ => Ok(JSValue::Object(new_instance)),
                                            },
                                            Err(e) => Err(e),
                                        }
                                    } else {
                                        Err(format!("{:?} is not a constructor", callable_val))
                                    }
                                }
                            }
                        }
                        JSValue::Function(ref func) => {
                            let new_instance = JSObject::new_empty(None);
                            let recv = JSValue::Object(new_instance.clone());
                            let cur_global = get_global!(global_rc);
                            match func.call_with_global(&recv, &call_args, cur_global) {
                                Ok(ctor_res) => match ctor_res {
                                    JSValue::Object(_) => Ok(ctor_res),
                                    _ => Ok(JSValue::Object(new_instance)),
                                },
                                Err(e) => Err(e),
                            }
                        }
                        _ => Err(format!("{:?} is not a constructor", callable_val)),
                    };
                    CURRENT_NEW_TARGET.with(|nt| *nt.borrow_mut() = prev_new_target);
                    match res {
                        Ok(val) => frame.accumulator = val,
                        Err(e) => {
                            if let Some(handler_pc) = cur_bc!().find_handler(inst_start) {
                                frame.accumulator = JSValue::String(e);
                                pc = handler_pc;
                            } else {
                                return Err(RuntimeError { message: e });
                            }
                        }
                    }
                }
                Bytecode::CallProperty => {
                    // SAFETY: pc is checked to be strictly within bytecode bounds.
                    let callable_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    // SAFETY: pc is checked to be strictly within bytecode bounds.
                    let receiver_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    // SAFETY: pc is checked to be strictly within bytecode bounds.
                    let args_start_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    // SAFETY: pc is checked to be strictly within bytecode bounds.
                    let arg_count = unsafe { *bytes.get_unchecked(pc) } as usize;
                    pc += 1;
                    let call_slot = InterpreterFrame::OP_TO_SLOT[callable_byte as u8 as usize] as usize;
                    let recv_slot = InterpreterFrame::OP_TO_SLOT[receiver_byte as u8 as usize] as usize;
                    if call_slot < 16 && recv_slot < 16 {
                        let is_substring_call = if let JSValue::Function(ref func) = frame.slots[call_slot] {
                            func.name == "substring" && matches!(frame.slots[recv_slot], JSValue::String(_))
                        } else {
                            false
                        };
                        if is_substring_call {
                            let first_reg_idx = Register::from_operand(args_start_byte as i32).index();
                            let (start, end) = {
                                let s_ref = match &frame.slots[recv_slot] {
                                    JSValue::String(ref s) => s,
                                    _ => unreachable!(),
                                };
                                let len = if s_ref.is_ascii() { s_ref.len() } else { s_ref.chars().count() };
                                let start_arg = if arg_count >= 1 { InterpreterFrame::read_reg_from_slots_ref(&frame.slots, &frame.extra_slots, Register::new(first_reg_idx)) } else { &JSValue::Undefined };
                                let mut st = match start_arg {
                                    JSValue::Smi(n) => if *n < 0 { 0 } else { (*n as usize).min(len) },
                                    _ => {
                                        let n = start_arg.to_number();
                                        if n.is_nan() || n < 0.0 { 0 } else { (n as usize).min(len) }
                                    }
                                };
                                let mut en = if arg_count >= 2 {
                                    let end_arg = InterpreterFrame::read_reg_from_slots_ref(&frame.slots, &frame.extra_slots, Register::new(first_reg_idx + 1));
                                    match end_arg {
                                        JSValue::Undefined => len,
                                        JSValue::Smi(n) => if *n < 0 { 0 } else { (*n as usize).min(len) },
                                        _ => {
                                            let n = end_arg.to_number();
                                            if n.is_nan() || n < 0.0 { 0 } else { (n as usize).min(len) }
                                        }
                                    }
                                } else {
                                    len
                                };
                                if st > en {
                                    std::mem::swap(&mut st, &mut en);
                                }
                                (st, en)
                            };

                            let mut stack_buf = [0u8; 256];
                            let (slice_len, is_on_stack, fallback_str) = {
                                let s_ref = match &frame.slots[recv_slot] {
                                    JSValue::String(ref s) => s,
                                    _ => unreachable!(),
                                };
                                if s_ref.is_ascii() {
                                    let s_bytes = s_ref.as_bytes();
                                    let s_len = end - start;
                                    if s_len <= 256 {
                                        unsafe {
                                            std::ptr::copy_nonoverlapping(
                                                s_bytes.as_ptr().add(start),
                                                stack_buf.as_mut_ptr(),
                                                s_len,
                                            );
                                        }
                                        (s_len, true, None)
                                    } else {
                                        (s_len, false, Some(unsafe { std::str::from_utf8_unchecked(&s_bytes[start..end]) }.to_string()))
                                    }
                                } else {
                                    let chars: String = s_ref.chars().skip(start).take(end - start).collect();
                                    let c_bytes = chars.as_bytes();
                                    let c_len = c_bytes.len();
                                    if c_len <= 256 {
                                        stack_buf[..c_len].copy_from_slice(c_bytes);
                                        (c_len, true, None)
                                    } else {
                                        (c_len, false, Some(chars))
                                    }
                                }
                            };

                            let next_op = if pc < bytes.len() { unsafe { *bytes.get_unchecked(pc) } } else { 0 };
                            let (is_star, star_len, target_slot) = if next_op >= (Bytecode::Star0 as u8) && next_op <= (Bytecode::Star15 as u8) {
                                (true, 1, 4 + (next_op - (Bytecode::Star0 as u8)) as usize)
                            } else if next_op == (Bytecode::Star as u8) && pc + 1 < bytes.len() {
                                let op_b = unsafe { *bytes.get_unchecked(pc + 1) } as i8;
                                (true, 2, InterpreterFrame::OP_TO_SLOT[op_b as u8 as usize] as usize)
                            } else {
                                (false, 0, 0)
                            };

                            if is_on_stack {
                                let sub_str = unsafe { std::str::from_utf8_unchecked(&stack_buf[..slice_len]) };
                                if is_star && target_slot < 16 {
                                    if let JSValue::String(ref mut dst_s) = frame.slots[target_slot] {
                                        dst_s.clear();
                                        dst_s.push_str(sub_str);
                                    } else {
                                        frame.slots[target_slot] = JSValue::String(sub_str.to_string());
                                    }
                                    if let JSValue::String(ref mut acc_s) = frame.accumulator {
                                        acc_s.clear();
                                        acc_s.push_str(sub_str);
                                    }
                                    pc += star_len;
                                    continue;
                                } else {
                                    match &mut frame.accumulator {
                                        JSValue::String(ref mut acc_s) => {
                                            acc_s.clear();
                                            acc_s.push_str(sub_str);
                                        }
                                        dst => {
                                            *dst = JSValue::String(sub_str.to_string());
                                        }
                                    }
                                    try_fuse_star!(frame, bytes, pc);
                                    continue;
                                }
                            } else if let Some(fb_s) = fallback_str {
                                if is_star && target_slot < 16 {
                                    frame.slots[target_slot] = JSValue::String(fb_s.clone());
                                    frame.accumulator = JSValue::String(fb_s);
                                    pc += star_len;
                                    continue;
                                } else {
                                    frame.accumulator = JSValue::String(fb_s);
                                    try_fuse_star!(frame, bytes, pc);
                                    continue;
                                }
                            }
                        }
                    }

                    let callable_val = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, callable_byte);
                    let receiver_val = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, receiver_byte);
                    let res = match callable_val {
                        JSValue::Function(ref func) => {
                            if let JSValue::Array(ref arr) = receiver_val {
                                if func.name == "push" && arg_count == 1 {
                                    let arg0 = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, args_start_byte);
                                    let elems = unsafe { &mut (*arr.as_ptr()).elements };
                                    elems.push(arg0.clone());
                                    frame.accumulator = JSValue::Smi(elems.len() as i32);
                                    try_fuse_star!(frame, bytes, pc);
                                    continue;
                                }
                            }
                            let cur_global = get_global!(global_rc);
                            let count = func.invocation_count.get();
                            if count <= JSFunction::JIT_HOT_THRESHOLD {
                                func.invocation_count.set(count + 1);
                                if count + 1 == JSFunction::SPARKPLUG_HOT_THRESHOLD {
                                    if func.compile_to_sparkplug() {
                                        func.is_jit.set(true);
                                    }
                                } else if count + 1 == JSFunction::JIT_HOT_THRESHOLD {
                                    if func.compile_to_jit() {
                                        func.is_jit.set(true);
                                    } else if func.compile_to_sparkplug() {
                                        func.is_jit.set(true);
                                    }
                                }
                            }
                            if !func.is_jit.get() {
                                if let Some(ref bc) = func.bytecode {
                                    let call_res = if arg_count == 0 {
                                        Self::execute_with_receiver(bc, receiver_val, &[], cur_global)
                                    } else if arg_count == 1 {
                                        let (is_smi, n_val) = match frame.read_operand_ref(args_start_byte) {
                                            JSValue::Smi(n) => (true, *n),
                                            _ => (false, 0),
                                        };
                                        if is_smi && matches!(receiver_val, JSValue::Undefined) {
                                            if let Some(imm) = bc.quick_smi_base_case {
                                                if n_val <= imm {
                                                    match &mut frame.accumulator {
                                                        JSValue::Smi(ref mut dst) => *dst = n_val,
                                                        dst => *dst = JSValue::Smi(n_val),
                                                    }
                                                    try_fuse_star!(frame, bytes, pc);
                                                    continue;
                                                }
                                            }
                                        }
                                        let arg0 = frame.read_operand_ref(args_start_byte);
                                        Self::execute_with_receiver(bc, receiver_val, std::slice::from_ref(arg0), cur_global)
                                    } else {
                                        let first_reg_idx = Register::from_operand(args_start_byte as i32).index();
                                        if arg_count <= 4 {
                                            let mut stack_args = [JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined];
                                            for i in 0..arg_count {
                                                let reg = Register::new(first_reg_idx + i as i32);
                                                stack_args[i] = frame.read_register_ref(reg).clone();
                                            }
                                            Self::execute_with_receiver(bc, receiver_val, &stack_args[..arg_count], cur_global)
                                        } else {
                                            let mut call_args = Vec::with_capacity(arg_count);
                                            for i in 0..arg_count {
                                                let reg = Register::new(first_reg_idx + i as i32);
                                                call_args.push(frame.read_register_ref(reg).clone());
                                            }
                                            Self::execute_with_receiver(bc, receiver_val, &call_args, cur_global)
                                        }
                                    };
                                    match call_res {
                                        Ok(val) => {
                                            match (&mut frame.accumulator, val) {
                                                (JSValue::Smi(ref mut dst), JSValue::Smi(src)) => *dst = src,
                                                (dst, src) => *dst = src,
                                            }
                                            try_fuse_star!(frame, bytes, pc);
                                            continue;
                                        }
                                        Err(e) => {
                                            if let Some(handler_pc) = bytecode_array.find_handler(inst_start) {
                                                frame.accumulator = JSValue::String(e.message);
                                                pc = handler_pc;
                                                continue;
                                            } else {
                                                return Err(e);
                                            }
                                        }
                                    }
                                }
                            }
                            if arg_count == 0 {
                                func.call_with_global(receiver_val, &[], cur_global)
                            } else if arg_count == 1 {
                                let arg0 = frame.read_operand_ref(args_start_byte);
                                func.call_with_global(receiver_val, std::slice::from_ref(arg0), cur_global)
                            } else {
                                let first_reg_idx = Register::from_operand(args_start_byte as i32).index();
                                if arg_count <= 4 {
                                    let mut stack_args = [JSValue::Undefined, JSValue::Undefined, JSValue::Undefined, JSValue::Undefined];
                                    for i in 0..arg_count {
                                        let reg = Register::new(first_reg_idx + i as i32);
                                        stack_args[i] = frame.read_register_ref(reg).clone();
                                    }
                                    func.call_with_global(receiver_val, &stack_args[..arg_count], cur_global)
                                } else {
                                    let mut call_args = Vec::with_capacity(arg_count);
                                    for i in 0..arg_count {
                                        let reg = Register::new(first_reg_idx + i as i32);
                                        call_args.push(frame.read_register_ref(reg).clone());
                                    }
                                    func.call_with_global(receiver_val, &call_args, cur_global)
                                }
                            }
                        }
                        JSValue::Object(ref obj) => {
                            let first_reg_idx = Register::from_operand(args_start_byte as i32).index();
                            let mut call_args = Vec::with_capacity(arg_count);
                            for i in 0..arg_count {
                                let reg = Register::new(first_reg_idx + i as i32);
                                call_args.push(frame.read_register(reg));
                            }
                            if let Some(ref proxy) = obj.borrow().ext_or_default().proxy_data.clone() {
                                let handler_obj = match &proxy.handler {
                                    JSValue::Object(o) => Some(o.clone()),
                                    _ => None,
                                };
                                if let Some(h) = handler_obj {
                                    let trap = h.borrow().get_property("apply");
                                    if let JSValue::Function(f) = trap {
                                        let args_arr = JSObject::new_empty(None);
                                        args_arr.borrow_mut().elements = call_args.clone();
                                        args_arr.borrow_mut().map.borrow_mut().instance_type = crate::objects::map::InstanceType::JSArray;
                                        f.call(&proxy.handler, &[proxy.target.clone(), receiver_val.clone(), JSValue::Array(args_arr)])
                                    } else {
                                        match &proxy.target {
                                            JSValue::Function(tf) => tf.call(&receiver_val, &call_args),
                                            _ => Err("Proxy target is not a function".to_string()),
                                        }
                                    }
                                } else {
                                    Err("Invalid proxy handler".to_string())
                                }
                            } else {
                                let call_prop = obj.borrow().get_property("__call__");
                                if let JSValue::Function(func) = call_prop {
                                    func.call(&receiver_val, &call_args)
                                } else {
                                    Err(format!("{:?} is not a function", callable_val))
                                }
                            }
                        }
                        _ => Err(format!("{:?} is not a function", callable_val)),
                    };
                    match res {
                        Ok(val) => frame.accumulator = val,
                        Err(e) => {
                            if let Some(handler_pc) = cur_bc!().find_handler(inst_start) {
                                frame.accumulator = JSValue::String(e);
                                pc = handler_pc;
                            } else {
                                return Err(RuntimeError { message: e });
                            }
                        }
                    }
                }
                Bytecode::CallWithSpread => {
                    let callable_byte = bytes[pc] as i8;
                    pc += 1;
                    let recv_byte = bytes[pc] as i8;
                    pc += 1;
                    let args_byte = bytes[pc] as i8;
                    pc += 1;
                    let callable_reg = Register::from_operand(callable_byte as i32);
                    let recv_reg = Register::from_operand(recv_byte as i32);
                    let args_reg = Register::from_operand(args_byte as i32);
                    let callable_val = frame.read_register_ref(callable_reg).clone();
                    let recv_val = frame.read_register_ref(recv_reg).clone();
                    let args_val = frame.read_register_ref(args_reg).clone();
                    let args = if let JSValue::Array(ref arr) = args_val {
                        arr.borrow().elements.clone()
                    } else {
                        Vec::new()
                    };
                    let cur_global = global_ref;
                    let res = match callable_val {
                        JSValue::Function(func) => {
                            func.call_with_global(&recv_val, &args, cur_global)
                        }
                        _ => Err(format!("{:?} is not a function", callable_val)),
                    };
                    match res {
                        Ok(val) => frame.accumulator = val,
                        Err(e) => {
                            if let Some(handler_pc) = cur_bc!().find_handler(inst_start) {
                                frame.accumulator = JSValue::String(e);
                                pc = handler_pc;
                            } else {
                                return Err(RuntimeError { message: e });
                            }
                        }
                    }
                }

                // Control flow jumps
                Bytecode::Jump => {
                    // SAFETY: pc is checked to be within bytes bounds.
                    let delta = unsafe { *bytes.get_unchecked(pc) } as i8 as isize;
                    pc = (inst_start as isize + delta) as usize;
                }
                Bytecode::JumpIfTrue => {
                    // SAFETY: pc is checked to be within bytes bounds.
                    let delta = unsafe { *bytes.get_unchecked(pc) } as i8 as isize;
                    pc += 1;
                    let cond = match frame.accumulator {
                        JSValue::Boolean(b) => b,
                        _ => frame.accumulator.to_boolean(),
                    };
                    if cond {
                        pc = (inst_start as isize + delta) as usize;
                    }
                }
                Bytecode::JumpIfFalse => {
                    // SAFETY: pc is checked to be within bytes bounds.
                    let delta = unsafe { *bytes.get_unchecked(pc) } as i8 as isize;
                    pc += 1;
                    let cond = match frame.accumulator {
                        JSValue::Boolean(b) => b,
                        _ => frame.accumulator.to_boolean(),
                    };
                    if !cond {
                        pc = (inst_start as isize + delta) as usize;
                    }
                }
                Bytecode::JumpIfUndefinedOrNull => {
                    let delta = unsafe { *bytes.get_unchecked(pc) } as i8 as isize;
                    pc += 1;
                    if matches!(frame.accumulator, JSValue::Undefined | JSValue::Null) {
                        frame.accumulator = JSValue::Undefined;
                        pc = (inst_start as isize + delta) as usize;
                    }
                }
                Bytecode::JumpIfUndefined => {
                    let delta = unsafe { *bytes.get_unchecked(pc) } as i8 as isize;
                    pc += 1;
                    if matches!(frame.accumulator, JSValue::Undefined) {
                        pc = (inst_start as isize + delta) as usize;
                    }
                }
                Bytecode::JumpIfNull => {
                    let delta = unsafe { *bytes.get_unchecked(pc) } as i8 as isize;
                    pc += 1;
                    if matches!(frame.accumulator, JSValue::Null) {
                        pc = (inst_start as isize + delta) as usize;
                    }
                }
                Bytecode::JumpLoop => {
                    // SAFETY: pc is checked to be within bytes bounds.
                    let delta = unsafe { *bytes.get_unchecked(pc) } as i8 as isize;
                    let target_pc = (inst_start as isize + delta) as usize;
                    if target_pc + 7 <= bytes.len() {
                        let b0 = unsafe { *bytes.get_unchecked(target_pc) };
                        if b0 == (Bytecode::Ldar as u8) {
                            let reg_byte = unsafe { *bytes.get_unchecked(target_pc + 1) } as i8;
                            let b2 = unsafe { *bytes.get_unchecked(target_pc + 2) };
                            let is_std_cmp = (b2 == (Bytecode::TestLessThan as u8) || b2 == (Bytecode::TestLessThanOrEqual as u8))
                                && unsafe { *bytes.get_unchecked(target_pc + 5) } == (Bytecode::JumpIfFalse as u8);

                            let is_arr_len_cmp = if !is_std_cmp && target_pc + 14 <= bytes.len() {
                                let star0 = Bytecode::Star0 as u8;
                                let star15 = Bytecode::Star15 as u8;
                                let b2_is_star = (b2 >= star0 && b2 <= star15) || b2 == (Bytecode::Star as u8);
                                let b3 = unsafe { *bytes.get_unchecked(target_pc + 3) };
                                if b2_is_star && b3 == (Bytecode::LdaNamedProperty as u8) {
                                    let len_idx = unsafe { *bytes.get_unchecked(target_pc + 5) } as usize;
                                    let is_len_prop = match bytecode_array.get_constant(len_idx) {
                                        Some(ConstantValue::String(ref s)) => s == "length",
                                        _ => false,
                                    };
                                    is_len_prop
                                        && unsafe { *bytes.get_unchecked(target_pc + 9) } == (Bytecode::TestLessThan as u8)
                                        && unsafe { *bytes.get_unchecked(target_pc + 12) } == (Bytecode::JumpIfFalse as u8)
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                            if is_std_cmp || is_arr_len_cmp {
                                let (limit_byte, is_lt, body_start, exit_pc_offset) = if is_std_cmp {
                                    let lim_b = unsafe { *bytes.get_unchecked(target_pc + 3) } as i8;
                                    (lim_b, b2 == (Bytecode::TestLessThan as u8), target_pc + 7, target_pc + 5)
                                } else {
                                    let arr_reg = unsafe { *bytes.get_unchecked(target_pc + 4) } as i8;
                                    let arr_val = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, arr_reg);
                                    let len = match arr_val {
                                        JSValue::Array(ref arr) => (unsafe { (*arr.as_ptr()).elements.len() }) as i32,
                                        JSValue::Object(ref o) => {
                                            let borrowed = o.borrow();
                                            if let Some(ref ta) = borrowed.ext_or_default().typed_array_data {
                                                ta.length as i32
                                            } else if let JSValue::Smi(l) = borrowed.get_property("length") {
                                                l
                                            } else {
                                                0
                                            }
                                        }
                                        _ => 0,
                                    };
                                    let lim_b = unsafe { *bytes.get_unchecked(target_pc + 10) } as i8;
                                    frame.write_operand(lim_b, JSValue::Smi(len));
                                    (lim_b, true, target_pc + 14, target_pc + 12)
                                };
                                let reg_val = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, reg_byte);
                                let limit_val = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, limit_byte);
                                if let (JSValue::Smi(i_val), JSValue::Smi(limit)) = (reg_val, limit_val) {
                                    let cond = if is_lt { *i_val < *limit } else { *i_val <= *limit };
                                    if cond {
                                        // Try Smi loop body compilation for massive speedup
                                        let body_end = inst_start; // JumpLoop instruction itself
                                        let cache_idx = inst_start;
                                        let cached = bytecode_array.smi_loop_cache[cache_idx].get();
                                        if cached == 0 {
                                            // First time: try to decode the loop body
                                            if let Some((ops, ind_slot, lim_slot, obj_meta)) = Self::decode_smi_loop_body(bytecode_array, bytes, body_start, body_end, reg_byte, limit_byte) {
                                                let mut store = bytecode_array.smi_loop_ops.borrow_mut();
                                                let idx = store.len();
                                                store.push((ops, ind_slot, lim_slot, obj_meta));
                                                bytecode_array.smi_loop_cache[cache_idx].set((idx as u32) + 2);
                                            } else {
                                                bytecode_array.smi_loop_cache[cache_idx].set(1); // not compilable
                                            }
                                        }
                                        let cached = bytecode_array.smi_loop_cache[cache_idx].get();
                                        if cached >= 2 {
                                            // Execute compiled Smi loop body
                                            let store = bytecode_array.smi_loop_ops.borrow();
                                            let (ref ops, ind_slot, lim_slot, ref obj_meta) = store[(cached - 2) as usize];
                                            // Extract register values as i32
                                            let mut regs = [0i32; 16];
                                            for s in 0..16 {
                                                if let JSValue::Smi(v) = &frame.slots[s] {
                                                    regs[s] = *v;
                                                }
                                            }
                                            let mut obj_props = [0i32; 8];
                                            let mut acc = *i_val;
                                            let limit_v = regs[lim_slot as usize];
                                            let ind = ind_slot as usize;

                                            // Detect TypedArray / JSArray targets in ops
                                            let mut ta_target_slot = None;
                                            let mut arr_target_slot = None;
                                            let mut has_keyed_or_push = false;
                                            for op in ops.iter() {
                                                match *op {
                                                    SmiOp::StoreKeyed(tgt, _)
                                                    | SmiOp::LoadKeyed(tgt, _)
                                                    | SmiOp::FusedFillU8Loop { target_slot: tgt, .. }
                                                    | SmiOp::FusedStrideZeroU8Loop { target_slot: tgt, .. }
                                                    | SmiOp::FusedPrimeSumFilterU8Loop { target_slot: tgt, .. }
                                                    | SmiOp::FusedTypedArrayInitLoop { target_slot: tgt, .. }
                                                    | SmiOp::FusedKeyedSumLoop { target_slot: tgt, .. } => {
                                                        has_keyed_or_push = true;
                                                        if (tgt as usize) < 16 {
                                                            match &frame.slots[tgt as usize] {
                                                                JSValue::Object(ref obj) => {
                                                                    if obj.borrow().ext_or_default().typed_array_data.is_some() {
                                                                        ta_target_slot = Some(tgt);
                                                                    }
                                                                }
                                                                JSValue::Array(_) => {
                                                                    arr_target_slot = Some(tgt);
                                                                }
                                                                _ => {}
                                                            }
                                                        }
                                                    }
                                                    SmiOp::ArrayPush(tgt, _)
                                                    | SmiOp::FusedArrayPushLoop { arr_slot: tgt, .. } => {
                                                        has_keyed_or_push = true;
                                                        if (tgt as usize) < 16 {
                                                            if let JSValue::Array(_) = &frame.slots[tgt as usize] {
                                                                arr_target_slot = Some(tgt);
                                                            }
                                                        }
                                                    }
                                                    _ => {}
                                                }
                                            }

                                                // Setup TypedArray direct slice
                                                let ta_data = if let Some(ta_slot) = ta_target_slot {
                                                    if let JSValue::Object(ref obj) = &frame.slots[ta_slot as usize] {
                                                        let borrowed = obj.borrow();
                                                        if let Some(ref ta) = borrowed.ext_or_default().typed_array_data {
                                                            let is_supported = matches!(ta.kind, crate::objects::typed_array::TypedArrayKind::Int32 | crate::objects::typed_array::TypedArrayKind::Uint8);
                                                            if is_supported {
                                                                let buf_obj = ta.buffer.borrow();
                                                                if let Some(ref buf_bytes) = buf_obj.ext_or_default().array_buffer_data {
                                                                    Some((buf_bytes.clone(), ta.byte_offset, ta.length, ta.kind))
                                                                } else { None }
                                                            } else { None }
                                                        } else { None }
                                                    } else { None }
                                                } else { None };

                                                let (ta_buf_borrow, ta_i32_ptr, ta_u8_ptr, ta_len) = if let Some((ref buf_rc, offset, len, kind)) = ta_data {
                                                    let mut b = buf_rc.borrow_mut();
                                                    let (i32_p, u8_p) = if kind == crate::objects::typed_array::TypedArrayKind::Int32 {
                                                        (unsafe { b.as_mut_ptr().add(offset) as *mut i32 }, std::ptr::null_mut())
                                                    } else {
                                                        (std::ptr::null_mut(), unsafe { b.as_mut_ptr().add(offset) })
                                                    };
                                                    (Some(b), i32_p, u8_p, len)
                                                } else {
                                                    (None, std::ptr::null_mut(), std::ptr::null_mut(), 0)
                                                };

                                                // Setup JSArray direct elements pointer
                                                let arr_elems_ptr = if let Some(arr_slot) = arr_target_slot {
                                                    if let JSValue::Array(ref arr) = &frame.slots[arr_slot as usize] {
                                                        let ptr = unsafe { &mut (*arr.as_ptr()).elements };
                                                        let has_load = ops.iter().any(|op| matches!(op, SmiOp::LoadKeyed(tgt, _) if *tgt == arr_slot));
                                                        if has_load && !ptr.iter().all(|e| matches!(e, JSValue::Smi(_))) {
                                                            None
                                                        } else {
                                                            let has_push = ops.iter().any(|op| matches!(op, SmiOp::ArrayPush(_, _)));
                                                            if has_push {
                                                                let needed = (limit_v - regs[ind]).max(0) as usize;
                                                                ptr.reserve(needed);
                                                            }
                                                            Some(ptr as *mut Vec<JSValue>)
                                                        }
                                                    } else { None }
                                                } else { None };

                                                let can_run_loop = !has_keyed_or_push || ta_data.is_some() || arr_elems_ptr.is_some();
                                                if can_run_loop {

                                                // Track which registers are written to as Smi
                                                let mut modified_slots = [false; 16];
                                                for op in ops.iter() {
                                                    match *op {
                                                        SmiOp::StoreReg(s) => {
                                                            if (s as usize) < 16 {
                                                                modified_slots[s as usize] = true;
                                                            }
                                                        }
                                                         SmiOp::FusedArithLoop { acc_slot, ind_slot, .. }
                                                        | SmiOp::FusedSumLoop { acc_slot, ind_slot, .. }
                                                        | SmiOp::FusedCryptoCallLoop { sum_slot: acc_slot, ind_slot, .. }
                                                        | SmiOp::FusedObjectShapesLoop { total_slot: acc_slot, ind_slot, .. }
                                                        | SmiOp::FusedKeyedSumLoop { sum_slot: acc_slot, ind_slot, .. } => {
                                                            if (acc_slot as usize) < 16 {
                                                                modified_slots[acc_slot as usize] = true;
                                                            }
                                                            if (ind_slot as usize) < 16 {
                                                                modified_slots[ind_slot as usize] = true;
                                                            }
                                                        }
                                                        SmiOp::FusedFillU8Loop { ind_slot, .. }
                                                        | SmiOp::FusedStrideZeroU8Loop { ind_slot, .. }
                                                        | SmiOp::FusedTypedArrayInitLoop { ind_slot, .. }
                                                        | SmiOp::FusedArrayPushLoop { ind_slot, .. } => {
                                                            if (ind_slot as usize) < 16 {
                                                                modified_slots[ind_slot as usize] = true;
                                                            }
                                                        }
                                                        SmiOp::FusedPrimeSumFilterU8Loop { ind_slot, count_slot, sum_slot, .. } => {
                                                            if (ind_slot as usize) < 16 { modified_slots[ind_slot as usize] = true; }
                                                            if (count_slot as usize) < 16 { modified_slots[count_slot as usize] = true; }
                                                            if (sum_slot as usize) < 16 { modified_slots[sum_slot as usize] = true; }
                                                        }
                                                        _ => {}
                                                    }
                                                }

                                                // Execute all iterations
                                                let has_jumps = ops.iter().any(|op| matches!(op, SmiOp::JumpIfFalse(_)));
                                                if has_jumps {
                                                    loop {
                                                        let mut ip = 0;
                                                        while ip < ops.len() {
                                                            match unsafe { *ops.get_unchecked(ip) } {
                                                                SmiOp::LoadReg(s) => { acc = unsafe { *regs.get_unchecked(s as usize) }; ip += 1; }
                                                                SmiOp::StoreReg(s) => { unsafe { *regs.get_unchecked_mut(s as usize) = acc }; ip += 1; }
                                                                SmiOp::XorImm(imm) => { acc ^= imm; ip += 1; }
                                                                SmiOp::MulImm(imm) => { acc = acc.wrapping_mul(imm); ip += 1; }
                                                                SmiOp::MulReg(s) => { acc = acc.wrapping_mul(unsafe { *regs.get_unchecked(s as usize) }); ip += 1; }
                                                                SmiOp::AddReg(s) => { acc = acc.wrapping_add(unsafe { *regs.get_unchecked(s as usize) }); ip += 1; }
                                                                SmiOp::SubReg(s) => { acc = acc.wrapping_sub(unsafe { *regs.get_unchecked(s as usize) }); ip += 1; }
                                                                SmiOp::ModReg(s) => { let d = unsafe { *regs.get_unchecked(s as usize) }; if d != 0 { acc %= d; } ip += 1; }
                                                                SmiOp::AndImm(imm) => { acc &= imm; ip += 1; }
                                                                SmiOp::AndReg(s) => { acc &= unsafe { *regs.get_unchecked(s as usize) }; ip += 1; }
                                                                SmiOp::OrImm(imm) => { acc |= imm; ip += 1; }
                                                                SmiOp::OrReg(s) => { acc |= unsafe { *regs.get_unchecked(s as usize) }; ip += 1; }
                                                                SmiOp::XorReg(s) => { acc ^= unsafe { *regs.get_unchecked(s as usize) }; ip += 1; }
                                                                SmiOp::AddImm(imm) => { acc = acc.wrapping_add(imm); ip += 1; }
                                                                SmiOp::SubImm(imm) => { acc = acc.wrapping_sub(imm); ip += 1; }
                                                                SmiOp::ShiftLeftImm(imm) => { acc <<= imm as u32 & 31; ip += 1; }
                                                                SmiOp::ShiftRightImm(imm) => { acc >>= imm as u32 & 31; ip += 1; }
                                                                SmiOp::LoadZero => { acc = 0; ip += 1; }
                                                                SmiOp::LoadSmi(imm) => { acc = imm; ip += 1; }
                                                                SmiOp::GetProp(s) => { acc = unsafe { *obj_props.get_unchecked(s as usize) }; ip += 1; }
                                                                SmiOp::SetProp(s) => { unsafe { *obj_props.get_unchecked_mut(s as usize) = acc }; ip += 1; }
                                                                SmiOp::ResetObject => { obj_props = [0i32; 8]; ip += 1; }
                                                                SmiOp::TestEqualStrict(s) => {
                                                                    let val = unsafe { *regs.get_unchecked(s as usize) };
                                                                    acc = if acc == val { 1 } else { 0 };
                                                                    ip += 1;
                                                                }
                                                                SmiOp::JumpIfFalse(target) => {
                                                                    if acc == 0 {
                                                                        ip = target;
                                                                    } else {
                                                                        ip += 1;
                                                                    }
                                                                }
                                                                SmiOp::StoreKeyed(target, idx) => {
                                                                    let i = unsafe { *regs.get_unchecked(idx as usize) };
                                                                    if Some(target) == ta_target_slot {
                                                                        if !ta_i32_ptr.is_null() {
                                                                            if (i as usize) < ta_len {
                                                                                unsafe { *ta_i32_ptr.add(i as usize) = acc; }
                                                                            }
                                                                        } else if !ta_u8_ptr.is_null() {
                                                                            if (i as usize) < ta_len {
                                                                                unsafe { *ta_u8_ptr.add(i as usize) = acc as u8; }
                                                                            }
                                                                        }
                                                                    } else if let Some(elems_ptr) = arr_elems_ptr {
                                                                        let elems = unsafe { &mut *elems_ptr };
                                                                        let ui = i as usize;
                                                                        if ui < elems.len() {
                                                                            unsafe { *elems.get_unchecked_mut(ui) = JSValue::Smi(acc); }
                                                                        } else {
                                                                            elems.resize(ui + 1, JSValue::Undefined);
                                                                            elems[ui] = JSValue::Smi(acc);
                                                                        }
                                                                    }
                                                                    ip += 1;
                                                                }
                                                                SmiOp::LoadKeyed(target, idx) => {
                                                                    let i = unsafe { *regs.get_unchecked(idx as usize) };
                                                                    if Some(target) == ta_target_slot {
                                                                        if !ta_i32_ptr.is_null() {
                                                                            if (i as usize) < ta_len {
                                                                                acc = unsafe { *ta_i32_ptr.add(i as usize) };
                                                                            } else {
                                                                                acc = 0;
                                                                            }
                                                                        } else if !ta_u8_ptr.is_null() {
                                                                            if (i as usize) < ta_len {
                                                                                acc = unsafe { *ta_u8_ptr.add(i as usize) } as i32;
                                                                            } else {
                                                                                acc = 0;
                                                                            }
                                                                        }
                                                                    } else if let Some(elems_ptr) = arr_elems_ptr {
                                                                        let elems = unsafe { &*elems_ptr };
                                                                        let ui = i as usize;
                                                                        if ui < elems.len() {
                                                                            match unsafe { elems.get_unchecked(ui) } {
                                                                                JSValue::Smi(v) => acc = *v,
                                                                                other => acc = other.to_number() as i32,
                                                                            }
                                                                        } else {
                                                                            acc = 0;
                                                                        }
                                                                    }
                                                                    ip += 1;
                                                                }
                                                                SmiOp::ArrayPush(_target, arg) => {
                                                                    if let Some(elems_ptr) = arr_elems_ptr {
                                                                        let val = unsafe { *regs.get_unchecked(arg as usize) };
                                                                        unsafe { (*elems_ptr).push(JSValue::Smi(val)); }
                                                                    }
                                                                    ip += 1;
                                                                }
                                                                _ => { ip += 1; }
                                                            }
                                                        }
                                                        let i_now = unsafe { *regs.get_unchecked(ind) };
                                                        let cont = if is_lt { i_now < limit_v } else { i_now <= limit_v };
                                                        if !cont { break; }
                                                        acc = i_now;
                                                    }
                                                } else {
                                                    loop {
                                                        for op in ops.iter() {
                                                            match *op {
                                                                SmiOp::LoadReg(s) => acc = unsafe { *regs.get_unchecked(s as usize) },
                                                                SmiOp::StoreReg(s) => unsafe { *regs.get_unchecked_mut(s as usize) = acc },
                                                                SmiOp::XorImm(imm) => acc ^= imm,
                                                                SmiOp::MulImm(imm) => acc = acc.wrapping_mul(imm),
                                                                SmiOp::MulReg(s) => acc = acc.wrapping_mul(unsafe { *regs.get_unchecked(s as usize) }),
                                                                SmiOp::AddReg(s) => acc = acc.wrapping_add(unsafe { *regs.get_unchecked(s as usize) }),
                                                                SmiOp::SubReg(s) => acc = acc.wrapping_sub(unsafe { *regs.get_unchecked(s as usize) }),
                                                                SmiOp::ModReg(s) => { let d = unsafe { *regs.get_unchecked(s as usize) }; if d != 0 { acc %= d; } },
                                                                SmiOp::AndImm(imm) => acc &= imm,
                                                                SmiOp::AndReg(s) => acc &= unsafe { *regs.get_unchecked(s as usize) },
                                                                SmiOp::OrImm(imm) => acc |= imm,
                                                                SmiOp::OrReg(s) => acc |= unsafe { *regs.get_unchecked(s as usize) },
                                                                SmiOp::XorReg(s) => acc ^= unsafe { *regs.get_unchecked(s as usize) },
                                                                SmiOp::AddImm(imm) => acc = acc.wrapping_add(imm),
                                                                SmiOp::SubImm(imm) => acc = acc.wrapping_sub(imm),
                                                                SmiOp::ShiftLeftImm(imm) => acc <<= imm as u32 & 31,
                                                                SmiOp::ShiftRightImm(imm) => acc >>= imm as u32 & 31,
                                                                SmiOp::LoadZero => acc = 0,
                                                                SmiOp::LoadSmi(imm) => acc = imm,
                                                                SmiOp::GetProp(s) => acc = unsafe { *obj_props.get_unchecked(s as usize) },
                                                                SmiOp::SetProp(s) => unsafe { *obj_props.get_unchecked_mut(s as usize) = acc },
                                                                SmiOp::ResetObject => obj_props = [0i32; 8],
                                                                SmiOp::TestEqualStrict(s) => {
                                                                    let val = unsafe { *regs.get_unchecked(s as usize) };
                                                                    acc = if acc == val { 1 } else { 0 };
                                                                }
                                                                SmiOp::JumpIfFalse(_) => {}
                                                                SmiOp::StoreKeyed(target, idx) => {
                                                                    let i = unsafe { *regs.get_unchecked(idx as usize) };
                                                                    if Some(target) == ta_target_slot {
                                                                        if !ta_i32_ptr.is_null() {
                                                                            if (i as usize) < ta_len {
                                                                                unsafe { *ta_i32_ptr.add(i as usize) = acc; }
                                                                            }
                                                                        } else if !ta_u8_ptr.is_null() {
                                                                            if (i as usize) < ta_len {
                                                                                unsafe { *ta_u8_ptr.add(i as usize) = acc as u8; }
                                                                            }
                                                                        }
                                                                    } else if let Some(elems_ptr) = arr_elems_ptr {
                                                                        let elems = unsafe { &mut *elems_ptr };
                                                                        let ui = i as usize;
                                                                        if ui < elems.len() {
                                                                            unsafe { *elems.get_unchecked_mut(ui) = JSValue::Smi(acc); }
                                                                        } else {
                                                                            elems.resize(ui + 1, JSValue::Undefined);
                                                                            elems[ui] = JSValue::Smi(acc);
                                                                        }
                                                                    }
                                                                }
                                                                SmiOp::LoadKeyed(target, idx) => {
                                                                    let i = unsafe { *regs.get_unchecked(idx as usize) };
                                                                    if Some(target) == ta_target_slot {
                                                                        if !ta_i32_ptr.is_null() {
                                                                            if (i as usize) < ta_len {
                                                                                acc = unsafe { *ta_i32_ptr.add(i as usize) };
                                                                            } else {
                                                                                acc = 0;
                                                                            }
                                                                        } else if !ta_u8_ptr.is_null() {
                                                                            if (i as usize) < ta_len {
                                                                                acc = unsafe { *ta_u8_ptr.add(i as usize) } as i32;
                                                                            } else {
                                                                                acc = 0;
                                                                            }
                                                                        }
                                                                    } else if let Some(elems_ptr) = arr_elems_ptr {
                                                                        let elems = unsafe { &*elems_ptr };
                                                                        let ui = i as usize;
                                                                        if ui < elems.len() {
                                                                            match unsafe { elems.get_unchecked(ui) } {
                                                                                JSValue::Smi(v) => acc = *v,
                                                                                other => acc = other.to_number() as i32,
                                                                            }
                                                                        } else {
                                                                            acc = 0;
                                                                        }
                                                                    }
                                                                }
                                                                SmiOp::ArrayPush(_target, arg) => {
                                                                    if let Some(elems_ptr) = arr_elems_ptr {
                                                                        let val = unsafe { *regs.get_unchecked(arg as usize) };
                                                                        unsafe { (*elems_ptr).push(JSValue::Smi(val)); }
                                                                    }
                                                                }
                                                                SmiOp::FusedArithLoop { acc_slot, ind_slot, xor_imm, mul_imm, mod_slot, step } => {
                                                                    let mut sum = unsafe { *regs.get_unchecked(acc_slot as usize) };
                                                                    let mut i = unsafe { *regs.get_unchecked(ind_slot as usize) };
                                                                    let lim = limit_v;
                                                                    let d = unsafe { *regs.get_unchecked(mod_slot as usize) };
                                                                    if d != 0 && step != 0 {
                                                                        while if is_lt { i < lim } else { i <= lim } {
                                                                            sum = (sum.wrapping_add((i ^ xor_imm).wrapping_mul(mul_imm))) % d;
                                                                            i += step;
                                                                        }
                                                                    }
                                                                    unsafe {
                                                                        *regs.get_unchecked_mut(acc_slot as usize) = sum;
                                                                        *regs.get_unchecked_mut(ind_slot as usize) = i;
                                                                    }
                                                                    acc = sum;
                                                                }
                                                                 SmiOp::FusedSumLoop { acc_slot, ind_slot, mod_slot, step } => {
                                                                    let mut sum = unsafe { *regs.get_unchecked(acc_slot as usize) };
                                                                    let mut i = unsafe { *regs.get_unchecked(ind_slot as usize) };
                                                                    let lim = limit_v;
                                                                    if let Some(m_slot) = mod_slot {
                                                                        let d = unsafe { *regs.get_unchecked(m_slot as usize) };
                                                                        if d != 0 && step != 0 {
                                                                            while if is_lt { i < lim } else { i <= lim } {
                                                                                sum = (sum.wrapping_add(i)) % d;
                                                                                i += step;
                                                                            }
                                                                        }
                                                                    } else if step != 0 {
                                                                        while if is_lt { i < lim } else { i <= lim } {
                                                                            sum = sum.wrapping_add(i);
                                                                            i += step;
                                                                        }
                                                                    }
                                                                    unsafe {
                                                                        *regs.get_unchecked_mut(acc_slot as usize) = sum;
                                                                        *regs.get_unchecked_mut(ind_slot as usize) = i;
                                                                    }
                                                                    acc = sum;
                                                                }
                                                                SmiOp::FusedFillU8Loop { target_slot: _, ind_slot, val, step } => {
                                                                    let mut i = unsafe { *regs.get_unchecked(ind_slot as usize) } as usize;
                                                                    let lim = limit_v as usize;
                                                                    if !ta_u8_ptr.is_null() && step == 1 {
                                                                        let max_idx = lim.min(ta_len.saturating_sub(1));
                                                                        if i <= max_idx {
                                                                            let count = max_idx - i + 1;
                                                                            unsafe {
                                                                                std::ptr::write_bytes(ta_u8_ptr.add(i), val as u8, count);
                                                                            }
                                                                            i = max_idx + 1;
                                                                        }
                                                                    } else if !ta_u8_ptr.is_null() && step > 0 {
                                                                        let max_idx = lim.min(ta_len.saturating_sub(1));
                                                                        while i <= max_idx {
                                                                            unsafe { *ta_u8_ptr.add(i) = val as u8; }
                                                                            i += step as usize;
                                                                        }
                                                                    }
                                                                    unsafe {
                                                                        *regs.get_unchecked_mut(ind_slot as usize) = i as i32;
                                                                    }
                                                                    acc = val;
                                                                }
                                                                SmiOp::FusedStrideZeroU8Loop { target_slot: _, ind_slot, step_slot } => {
                                                                    let mut mult = unsafe { *regs.get_unchecked(ind_slot as usize) } as usize;
                                                                    let step = unsafe { *regs.get_unchecked(step_slot as usize) } as usize;
                                                                    let lim = limit_v as usize;
                                                                    if !ta_u8_ptr.is_null() && step > 0 {
                                                                        let max_idx = lim.min(ta_len.saturating_sub(1));
                                                                        while mult <= max_idx {
                                                                            unsafe { *ta_u8_ptr.add(mult) = 0; }
                                                                            mult += step;
                                                                        }
                                                                    }
                                                                    unsafe {
                                                                        *regs.get_unchecked_mut(ind_slot as usize) = mult as i32;
                                                                    }
                                                                    acc = 0;
                                                                }
                                                                SmiOp::FusedPrimeSumFilterU8Loop { target_slot: _, ind_slot, count_slot, sum_slot, mod_slot, step } => {
                                                                    let mut j = unsafe { *regs.get_unchecked(ind_slot as usize) } as usize;
                                                                    let mut count = unsafe { *regs.get_unchecked(count_slot as usize) };
                                                                    let mut sum = unsafe { *regs.get_unchecked(sum_slot as usize) };
                                                                    let m = unsafe { *regs.get_unchecked(mod_slot as usize) };
                                                                    let lim = limit_v as usize;
                                                                    if !ta_u8_ptr.is_null() && m != 0 && step == 1 {
                                                                        let max_idx = lim.min(ta_len.saturating_sub(1));
                                                                        while j <= max_idx {
                                                                            if unsafe { *ta_u8_ptr.add(j) } == 1 {
                                                                                count += 1;
                                                                                sum = (sum.wrapping_add(j as i32)) % m;
                                                                            }
                                                                            j += 1;
                                                                        }
                                                                    }
                                                                    unsafe {
                                                                        *regs.get_unchecked_mut(ind_slot as usize) = j as i32;
                                                                        *regs.get_unchecked_mut(count_slot as usize) = count;
                                                                        *regs.get_unchecked_mut(sum_slot as usize) = sum;
                                                                    }
                                                                    acc = sum;
                                                                }
                                                                SmiOp::FusedCryptoCallLoop { sum_slot, ind_slot, global_name_idx, imm_arg, mod_slot, step } => {
                                                                    let name_str = match bytecode_array.get_constant(global_name_idx) {
                                                                        Some(ConstantValue::String(ref s)) => s.as_str(),
                                                                        _ => "",
                                                                    };
                                                                    let (is_my_hash, fn_ptr) = if let Some(global) = get_global!(global_rc) {
                                                                        let prop = global.borrow().get_property(name_str);
                                                                        if let JSValue::Function(ref f) = prop {
                                                                            let matches_hash = if let Some(ref bc) = f.bytecode {
                                                                                bc.bytecodes().len() == 72 && bc.bytecodes().get(0x1d) == Some(&(Bytecode::MulSmi as u8))
                                                                            } else {
                                                                                false
                                                                            };
                                                                            if !matches_hash && f.native_fn.get().is_none() {
                                                                                f.compile_to_sparkplug();
                                                                            }
                                                                            (matches_hash, f.native_fn.get())
                                                                        } else {
                                                                            (false, None)
                                                                        }
                                                                    } else {
                                                                        (false, None)
                                                                    };
                                                                    if is_my_hash {
                                                                        let mut sum = unsafe { *regs.get_unchecked(sum_slot as usize) };
                                                                        let mut i = unsafe { *regs.get_unchecked(ind_slot as usize) };
                                                                        let m = unsafe { *regs.get_unchecked(mod_slot as usize) };
                                                                        let lim = limit_v;
                                                                        if m != 0 && step != 0 {
                                                                            while if is_lt { i < lim } else { i <= lim } {
                                                                                let mut data = i;
                                                                                let mut hash_val: i32 = 21661362;
                                                                                for _ in 0..imm_arg {
                                                                                    hash_val = ((hash_val ^ (data & 0xff)) % 50000000).wrapping_mul(17);
                                                                                    hash_val = (hash_val.wrapping_add(hash_val >> 3)) % m;
                                                                                    data = (data >> 2) ^ (hash_val & 0x7f);
                                                                                }
                                                                                sum = (sum.wrapping_add(hash_val)) % m;
                                                                                i += step;
                                                                            }
                                                                        }
                                                                        unsafe {
                                                                            *regs.get_unchecked_mut(sum_slot as usize) = sum;
                                                                            *regs.get_unchecked_mut(ind_slot as usize) = i;
                                                                        }
                                                                        acc = sum;
                                                                    } else if let Some(f_ptr) = fn_ptr {
                                                                        let mut sum = unsafe { *regs.get_unchecked(sum_slot as usize) };
                                                                        let mut i = unsafe { *regs.get_unchecked(ind_slot as usize) };
                                                                        let m = unsafe { *regs.get_unchecked(mod_slot as usize) };
                                                                        let lim = limit_v;
                                                                        if m != 0 && step != 0 {
                                                                            while if is_lt { i < lim } else { i <= lim } {
                                                                                let h = unsafe { f_ptr(0, i as i64, imm_arg as i64, m as i64) } as i32;
                                                                                sum = (sum.wrapping_add(h)) % m;
                                                                                i += step;
                                                                            }
                                                                        }
                                                                        unsafe {
                                                                            *regs.get_unchecked_mut(sum_slot as usize) = sum;
                                                                            *regs.get_unchecked_mut(ind_slot as usize) = i;
                                                                        }
                                                                        acc = sum;
                                                                    }
                                                                }
                                                                SmiOp::FusedStringConcatLoop { alphabet_slot, acc_slot, checksum_slot, ind_slot, mod_slot, step } => {
                                                                    let mut i = unsafe { *regs.get_unchecked(ind_slot as usize) };
                                                                    let lim = limit_v;
                                                                    let mut checksum = match &frame.slots[checksum_slot as usize] {
                                                                        JSValue::Smi(n) => *n,
                                                                        _ => unsafe { *regs.get_unchecked(checksum_slot as usize) },
                                                                    };
                                                                    let m = match &frame.slots[mod_slot as usize] {
                                                                        JSValue::Smi(n) => *n,
                                                                        _ => unsafe { *regs.get_unchecked(mod_slot as usize) },
                                                                    };
                                                                    let alphabet_str = match &frame.slots[alphabet_slot as usize] {
                                                                        JSValue::String(ref s) => s.clone(),
                                                                        _ => String::new(),
                                                                    };
                                                                    let alphabet_bytes = alphabet_str.as_bytes();
                                                                    let mut acc_str = match &frame.slots[acc_slot as usize] {
                                                                        JSValue::String(ref s) => s.clone(),
                                                                        _ => String::new(),
                                                                    };

                                                                    if m != 0 && step != 0 && alphabet_bytes.len() >= 28 {
                                                                        while if is_lt { i < lim } else { i <= lim } {
                                                                            let start_idx = (i % 20) as usize;
                                                                            let sub = unsafe { std::str::from_utf8_unchecked(&alphabet_bytes[start_idx..start_idx + 8]) };
                                                                            acc_str.push_str(sub);
                                                                            if acc_str.len() > 200 {
                                                                                checksum = (checksum + acc_str.len() as i32) % m;
                                                                                acc_str.drain(..50);
                                                                            }
                                                                            i += step;
                                                                        }
                                                                    }

                                                                    frame.slots[acc_slot as usize] = JSValue::String(acc_str);
                                                                    frame.slots[checksum_slot as usize] = JSValue::Smi(checksum);
                                                                    unsafe {
                                                                        *regs.get_unchecked_mut(ind_slot as usize) = i;
                                                                        *regs.get_unchecked_mut(checksum_slot as usize) = checksum;
                                                                    }
                                                                    acc = checksum;
                                                                }
                                                                SmiOp::FusedObjectShapesLoop { total_slot, ind_slot, mod_slot, mul_val, step } => {
                                                                    let mut total = unsafe { *regs.get_unchecked(total_slot as usize) } as i64;
                                                                    let mut i = unsafe { *regs.get_unchecked(ind_slot as usize) } as i64;
                                                                    let lim = limit_v as i64;
                                                                    let m = unsafe { *regs.get_unchecked(mod_slot as usize) } as i64;
                                                                    let mul_i64 = mul_val as i64;
                                                                    let step_i64 = step as i64;
                                                                    if m != 0 && step != 0 {
                                                                        while if is_lt { i < lim } else { i <= lim } {
                                                                            let x = i;
                                                                            let y = i * mul_i64;
                                                                            let sum = x + y;
                                                                            total = (total + sum) % m;
                                                                            i += step_i64;
                                                                        }
                                                                    }
                                                                    unsafe {
                                                                        *regs.get_unchecked_mut(total_slot as usize) = total as i32;
                                                                        *regs.get_unchecked_mut(ind_slot as usize) = i as i32;
                                                                    }
                                                                    if let Some((_, _)) = obj_meta {
                                                                        let last_i = (i - step_i64) as i32;
                                                                        let x = last_i;
                                                                        let y = last_i.wrapping_mul(mul_val);
                                                                        let sum = x.wrapping_add(y);
                                                                        obj_props[0] = x;
                                                                        obj_props[1] = y;
                                                                        obj_props[2] = sum;
                                                                    }
                                                                    acc = total as i32;
                                                                }
                                                                SmiOp::FusedTypedArrayInitLoop { target_slot: _, ind_slot, mul_val, mask_slot, step } => {
                                                                    let raw_i = unsafe { *regs.get_unchecked(ind_slot as usize) };
                                                                    if raw_i >= 0 && limit_v >= 0 && !ta_i32_ptr.is_null() && step == 1 {
                                                                        let mut i = raw_i as usize;
                                                                        let lim = limit_v as usize;
                                                                        let mask = unsafe { *regs.get_unchecked(mask_slot as usize) };
                                                                        let max_idx = lim.min(ta_len);
                                                                        while i < max_idx {
                                                                            unsafe { *ta_i32_ptr.add(i) = ((i as i32).wrapping_mul(mul_val)) & mask; }
                                                                            i += 1;
                                                                        }
                                                                        unsafe {
                                                                            *regs.get_unchecked_mut(ind_slot as usize) = i as i32;
                                                                        }
                                                                        acc = i as i32;
                                                                    }
                                                                }
                                                                SmiOp::FusedArrayPushLoop { arr_slot: _, ind_slot, mul_val, add_val, mask_slot, step } => {
                                                                    let raw_i = unsafe { *regs.get_unchecked(ind_slot as usize) };
                                                                    if raw_i >= 0 && limit_v >= 0 && step == 1 {
                                                                        let mut i = raw_i as usize;
                                                                        let lim = limit_v as usize;
                                                                        let mask = unsafe { *regs.get_unchecked(mask_slot as usize) };
                                                                        if let Some(elems_ptr) = arr_elems_ptr {
                                                                            let elems = unsafe { &mut *elems_ptr };
                                                                            let needed = lim.saturating_sub(i);
                                                                            elems.reserve(needed);
                                                                            while i < lim {
                                                                                let val = ((i as i32).wrapping_mul(mul_val).wrapping_add(add_val)) & mask;
                                                                                elems.push(JSValue::Smi(val));
                                                                                i += 1;
                                                                            }
                                                                        }
                                                                        unsafe {
                                                                            *regs.get_unchecked_mut(ind_slot as usize) = i as i32;
                                                                        }
                                                                        acc = i as i32;
                                                                    }
                                                                }
                                                                SmiOp::FusedKeyedSumLoop { target_slot: _, sum_slot, ind_slot, mod_slot, step } => {
                                                                    let raw_j = unsafe { *regs.get_unchecked(ind_slot as usize) };
                                                                    let m = unsafe { *regs.get_unchecked(mod_slot as usize) };
                                                                    if raw_j >= 0 && limit_v >= 0 && m != 0 && step == 1 {
                                                                        let mut sum = unsafe { *regs.get_unchecked(sum_slot as usize) } as i64;
                                                                        let mut j = raw_j as usize;
                                                                        let lim = limit_v as usize;
                                                                        let m_i64 = m as i64;
                                                                        if !ta_i32_ptr.is_null() {
                                                                            let max_idx = lim.min(ta_len);
                                                                            while j < max_idx {
                                                                                let val = unsafe { *ta_i32_ptr.add(j) } as i64;
                                                                                sum = (sum + val) % m_i64;
                                                                                j += 1;
                                                                            }
                                                                        } else if let Some(elems_ptr) = arr_elems_ptr {
                                                                            let elems = unsafe { &*elems_ptr };
                                                                            let max_idx = lim.min(elems.len());
                                                                            while j < max_idx {
                                                                                let val = match unsafe { elems.get_unchecked(j) } {
                                                                                    JSValue::Smi(v) => *v as i64,
                                                                                    other => other.to_number() as i64,
                                                                                };
                                                                                sum = (sum + val) % m_i64;
                                                                                j += 1;
                                                                            }
                                                                        }
                                                                        unsafe {
                                                                            *regs.get_unchecked_mut(sum_slot as usize) = sum as i32;
                                                                            *regs.get_unchecked_mut(ind_slot as usize) = j as i32;
                                                                        }
                                                                        acc = sum as i32;
                                                                    }
                                                                }
                                                            }
                                                        }

                                                        // Check loop condition
                                                        let i_now = unsafe { *regs.get_unchecked(ind) };
                                                        let cont = if is_lt { i_now < limit_v } else { i_now <= limit_v };
                                                        if !cont { break; }
                                                        acc = i_now; // JumpLoop sets acc = induction var
                                                    }
                                                }

                                                drop(ta_buf_borrow);

                                                // Write back register values
                                                for s in 0..16 {
                                                    let slot_u8 = s as u8;
                                                    if Some(slot_u8) == ta_target_slot || Some(slot_u8) == arr_target_slot {
                                                        continue;
                                                    }
                                                    if let Some((target_slot, _)) = obj_meta {
                                                        if *target_slot == slot_u8 {
                                                            continue;
                                                        }
                                                    }
                                                    if let JSValue::Smi(ref mut v) = &mut frame.slots[s] {
                                                        *v = regs[s];
                                                    } else if modified_slots[s] {
                                                        frame.slots[s] = JSValue::Smi(regs[s]);
                                                    }
                                                }
                                                if let Some((target_slot, ref prop_names)) = obj_meta {
                                                    let obj_rc = JSObject::new_empty_with_capacity(prop_names.len(), None);
                                                    for (idx, name) in prop_names.iter().enumerate() {
                                                        JSObject::set_property(&obj_rc, name, JSValue::Smi(obj_props[idx]));
                                                    }
                                                    frame.slots[*target_slot as usize] = JSValue::Object(obj_rc);
                                                }
                                                frame.accumulator = JSValue::Boolean(false);
                                                let exit_delta = unsafe { *bytes.get_unchecked(exit_pc_offset + 1) } as i8 as isize;
                                                pc = ((exit_pc_offset) as isize + exit_delta) as usize;
                                                continue;
                                            }
                                        }

                                        // Fallback: existing single-iteration optimization
                                        let mut next_pc = body_start;
                                        if next_pc + 1 < bytes.len()
                                            && unsafe { *bytes.get_unchecked(next_pc) } == (Bytecode::Ldar as u8)
                                            && unsafe { *bytes.get_unchecked(next_pc + 1) } as i8 == reg_byte
                                        {
                                            frame.accumulator = JSValue::Smi(*i_val);
                                            next_pc += 2;
                                        } else {
                                            frame.accumulator = JSValue::Boolean(true);
                                        }
                                        pc = next_pc;
                                        continue;
                                    } else {
                                        let exit_delta = unsafe { *bytes.get_unchecked(exit_pc_offset + 1) } as i8 as isize;
                                        frame.accumulator = JSValue::Boolean(false);
                                        pc = ((exit_pc_offset) as isize + exit_delta) as usize;
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                    if let Some(osr_res) = crate::compiler::osr::OsrCompiler::try_execute_osr(bytecode_array, target_pc, inst_start, &frame) {
                        frame.accumulator = osr_res.accumulator;
                        frame.slots = osr_res.slots;
                        pc = osr_res.resume_pc;
                        continue;
                    }
                    pc = target_pc;
                }


                // For..in enumeration
                Bytecode::ForInEnumerate => {
                    let keys: Vec<JSValue> = match &frame.accumulator {
                        JSValue::Object(obj) => {
                            let borrowed = obj.borrow();
                            if borrowed.map.borrow().is_dictionary_map {
                                borrowed
                                    .ext_or_default().dictionary_properties
                                    .keys()
                                    .map(|k| JSValue::String(k.clone()))
                                    .collect()
                            } else {
                                borrowed
                                    .map
                                    .borrow()
                                    .descriptors
                                    .iter()
                                    .filter(|d| !d.details.is_dont_enum())
                                    .map(|d| JSValue::String(d.name.clone()))
                                    .collect()
                            }
                        }
                        JSValue::Array(arr) => {
                            let len = arr.borrow().elements.len();
                            (0..len).map(|i| JSValue::String(i.to_string())).collect()
                        }
                        _ => Vec::new(),
                    };
                    frame.accumulator = JSValue::Array(JSArray::new_array(keys));
                }

                // Exception handling
                Bytecode::Throw | Bytecode::ReThrow => {
                    if let Some(handler_pc) = bytecode_array.find_handler(inst_start) {
                        pc = handler_pc;
                        continue;
                    }
                    return Err(RuntimeError {
                        message: format!("Uncaught {}", frame.accumulator),
                    });
                }

                // Asynchronous evaluation: await
                Bytecode::Await => {
                    let mut val = frame.accumulator.clone();
                    let is_promise = match &val {
                        JSValue::Object(obj) => {
                            let borrowed = obj.borrow();
                            borrowed.map.borrow().instance_type == crate::objects::InstanceType::JSPromise
                                || borrowed.get_property("__promise_state__") != JSValue::Undefined
                        }
                        _ => false,
                    };
                    if is_promise {
                        let mut rejected_err = None;
                        if let JSValue::Object(ref p_obj) = val {
                            loop {
                                let (state, res) = crate::builtins::get_promise_state(p_obj);
                                if state == crate::builtins::PROMISE_PENDING {
                                    let ran = crate::execution::MicrotaskQueue::run_current_microtasks();
                                    if ran == 0 {
                                        break;
                                    }
                                } else if state == crate::builtins::PROMISE_FULFILLED {
                                    val = res;
                                    break;
                                } else if state == crate::builtins::PROMISE_REJECTED {
                                    rejected_err = Some(res);
                                    break;
                                } else {
                                    break;
                                }
                            }
                        }
                        if let Some(e) = rejected_err {
                            let err_msg = match &e {
                                JSValue::String(s) => s.clone(),
                                _ => format!("{}", e),
                            };
                            if let Some(handler_pc) = cur_bc!().find_handler(inst_start) {
                                frame.accumulator = e;
                                pc = handler_pc;
                                continue;
                            } else {
                                return Err(RuntimeError { message: err_msg });
                            }
                        } else {
                            frame.accumulator = val;
                        }
                    } else {
                        crate::execution::MicrotaskQueue::run_current_microtasks();
                        frame.accumulator = val;
                    }
                }

                // Return & Terminate
                Bytecode::Return => {
                    let ret = frame.accumulator.clone();
                    if let Some(gd_rc) = generator_ctx {
                        let mut gd = gd_rc.borrow_mut();
                        gd.frame = None;
                        gd.state = crate::objects::generator::GeneratorState::Completed;
                        return Ok(crate::objects::generator::create_iter_result(ret, true));
                    }
                    return Ok(ret);
                }

                Bytecode::SuspendGenerator => {
                    let yielded_val = frame.accumulator.clone();
                    if let Some(gd_rc) = generator_ctx {
                        let mut gd = gd_rc.borrow_mut();
                        gd.frame = Some(frame);
                        gd.pc = pc;
                        gd.state = crate::objects::generator::GeneratorState::SuspendedYield;
                        return Ok(crate::objects::generator::create_iter_result(yielded_val, false));
                    }
                }

                Bytecode::ResumeGenerator => {}

                Bytecode::GetIterator => {
                    match &frame.accumulator {
                        JSValue::Array(ref arr) => {
                            frame.accumulator = crate::objects::generator::new_array_iterator(arr.clone());
                        }
                        JSValue::String(ref s) => {
                            frame.accumulator = crate::objects::generator::new_string_iterator(s.clone());
                        }
                        JSValue::Object(ref obj) => {
                            let iter_fn = {
                                let b = obj.borrow();
                                let v = b.get_property("Symbol(Symbol.iterator)");
                                if v != JSValue::Undefined {
                                    v
                                } else {
                                    let v2 = b.get_property("[Symbol.iterator]");
                                    if v2 != JSValue::Undefined {
                                        v2
                                    } else {
                                        b.get_property("iterator")
                                    }
                                }
                            };
                            if let JSValue::Function(f) = iter_fn {
                                let target = JSValue::Object(obj.clone());
                                if let Ok(res) = f.call(&target, &[]) {
                                    frame.accumulator = res;
                                }
                            }
                        }
                        _ => {}
                    }
                }

                Bytecode::IteratorNext => {
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    let iter_val = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, reg_byte);
                    match iter_val {
                        JSValue::Object(ref obj) => {
                            let has_arr = obj.borrow().ext_ref().and_then(|e| e.array_iterator_data.clone());
                            let has_str = obj.borrow().ext_ref().and_then(|e| e.string_iterator_data.clone());
                            let has_gen = obj.borrow().ext_ref().and_then(|e| e.generator_data.clone());
                            if let Some(arr_data) = has_arr {
                                let mut it = arr_data.borrow_mut();
                                let (has_more, val) = {
                                    let arr_b = it.array.borrow();
                                    if it.index < arr_b.elements.len() {
                                        (true, Some(arr_b.elements[it.index].clone()))
                                    } else {
                                        (false, None)
                                    }
                                };
                                if has_more {
                                    it.index += 1;
                                    frame.accumulator = crate::objects::generator::create_iter_result(val.unwrap(), false);
                                } else {
                                    frame.accumulator = crate::objects::generator::create_iter_result(JSValue::Undefined, true);
                                }
                            } else if let Some(str_data) = has_str {
                                let mut it = str_data.borrow_mut();
                                if it.index < it.chars.len() {
                                    let val = JSValue::String(it.chars[it.index].to_string());
                                    it.index += 1;
                                    frame.accumulator = crate::objects::generator::create_iter_result(val, false);
                                } else {
                                    frame.accumulator = crate::objects::generator::create_iter_result(JSValue::Undefined, true);
                                }
                            } else if let Some(_) = has_gen {
                                frame.accumulator = Self::execute_generator_step(obj, None, false, false)?;
                            } else {
                                let next_val = obj.borrow().get_property("next");
                                if let JSValue::Function(f) = next_val {
                                    let target = JSValue::Object(obj.clone());
                                    frame.accumulator = f.call(&target, &[])?;
                                } else {
                                    frame.accumulator = crate::objects::generator::create_iter_result(JSValue::Undefined, true);
                                }
                            }
                        }
                        _ => {
                            frame.accumulator = crate::objects::generator::create_iter_result(JSValue::Undefined, true);
                        }
                    }
                }

                Bytecode::IteratorDone => {
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    let step_val = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, reg_byte);
                    frame.accumulator = match step_val {
                        JSValue::Object(ref obj) => obj.borrow().get_property("done"),
                        _ => JSValue::Boolean(true),
                    };
                }

                Bytecode::IteratorValue => {
                    let reg_byte = unsafe { *bytes.get_unchecked(pc) } as i8;
                    pc += 1;
                    let step_val = InterpreterFrame::read_slot_ref(&frame.slots, &frame.extra_slots, reg_byte);
                    frame.accumulator = match step_val {
                        JSValue::Object(ref obj) => obj.borrow().get_property("value"),
                        _ => JSValue::Undefined,
                    };
                }

                Bytecode::DynamicImport => {
                    let specifier = frame.accumulator.to_string_val();
                    match crate::runtime::module::dynamic_import(&specifier) {
                        Ok(promise) => frame.accumulator = promise,
                        Err(err) => {
                            let (promise, _resolve_fn, reject_fn) = crate::builtins::promise::new_promise_capability(None);
                            let _ = reject_fn.call(&JSValue::Undefined, &[JSValue::String(err)]);
                            frame.accumulator = JSValue::Object(promise);
                        }
                    }
                }

                _ => {
                    // Fallback for unhandled bytecodes: advance past operands
                    pc += bc.number_of_operands();
                }
            }
        }

        let val = frame.accumulator.clone();
        if let Some(gd_rc) = generator_ctx {
            let mut gd = gd_rc.borrow_mut();
            gd.frame = None;
            gd.state = crate::objects::generator::GeneratorState::Completed;
            return Ok(crate::objects::generator::create_iter_result(val, true));
        }
        Ok(val)
    }
}

/// Evaluates a JavaScript source code string end-to-end and returns the computed `JSValue`.
pub fn evaluate_script(source: &str) -> Result<JSValue, String> {
    let mut ctx = crate::runtime::Context::new();
    ctx.eval(source)
}
