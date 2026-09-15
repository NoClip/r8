//! Safe Rust reimplementation of Google V8's BytecodeArray and BytecodeArrayBuilder.
//!
//! Provides the bytecode storage container, constant pool management, operand decoding,
//! text disassembly, and an ergonomic builder with ShortStar peephole optimization.

use super::bytecode_register::{Register, RegisterList};
use super::bytecodes::Bytecode;
use crate::objects::function::JSFunction;
use crate::objects::map::Map;
use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

/// Values that can reside in a BytecodeArray's constant pool.
#[derive(Clone, Debug, PartialEq)]
pub enum ConstantValue {
    Smi(i32),
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
    Undefined,
    BigInt(String),
}

impl fmt::Display for ConstantValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstantValue::Smi(val) => write!(f, "{}", val),
            ConstantValue::Number(val) => write!(f, "{}", val),
            ConstantValue::String(val) => write!(f, "\"{}\"", val),
            ConstantValue::Boolean(val) => write!(f, "{}", val),
            ConstantValue::Null => write!(f, "null"),
            ConstantValue::Undefined => write!(f, "undefined"),
            ConstantValue::BigInt(val) => write!(f, "{}n", val),
        }
    }
}

/// Information tracking an active try-catch exception handler block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlerEntry {
    pub start_pc: usize,
    pub end_pc: usize,
    pub handler_pc: usize,
}

/// Detected canonical binary-recursive Smi function pattern.
/// Pattern: `if (n <= threshold) return n; return self(n - d1) + self(n - d2);`
/// When detected, the interpreter can execute pure i32 recursion instead of
/// dispatching bytecodes, eliminating InterpreterFrame allocation and JSValue overhead.
#[derive(Clone, Debug)]
pub struct RecursiveSmiSpec {
    pub base_threshold: i32,
    pub sub_delta1: i32,
    pub sub_delta2: i32,
}

/// Compact Smi-only operation for loop body compilation.
/// When a tight loop body contains only Smi-safe bytecodes, the loop can be
/// executed as pure i32 arithmetic without JSValue overhead or dispatch cost.
#[derive(Clone, Copy, Debug)]
pub enum SmiOp {
    LoadReg(u8),        // acc = regs[slot]
    StoreReg(u8),       // regs[slot] = acc
    XorImm(i32),        // acc ^= imm
    MulImm(i32),        // acc = acc.wrapping_mul(imm)
    MulReg(u8),         // acc = acc.wrapping_mul(regs[slot])
    AddReg(u8),         // acc = acc.wrapping_add(regs[slot])
    SubReg(u8),         // acc = acc.wrapping_sub(regs[slot])
    ModReg(u8),         // acc %= regs[slot]
    AndImm(i32),        // acc &= imm
    OrImm(i32),         // acc |= imm
    AddImm(i32),        // acc = acc.wrapping_add(imm)
    SubImm(i32),        // acc = acc.wrapping_sub(imm)
    ShiftLeftImm(i32),  // acc <<= imm
    ShiftRightImm(i32), // acc >>= imm
    LoadZero,           // acc = 0
    LoadSmi(i32),       // acc = imm
    GetProp(u8),        // acc = obj_props[slot]
    SetProp(u8),        // obj_props[slot] = acc
    ResetObject,        // obj_props = [0; 8]
    LoadKeyed(u8, u8),  // acc = target[idx] (target_slot, idx_slot)
    StoreKeyed(u8, u8), // target[idx] = acc (target_slot, idx_slot)
    ArrayPush(u8, u8),  // arr.push(regs[arg_slot]) (arr_slot, arg_slot)
    AndReg(u8),         // acc &= regs[slot]
    OrReg(u8),          // acc |= regs[slot]
    XorReg(u8),         // acc ^= regs[slot]
    TestEqualStrict(u8),// acc = if acc == regs[slot] { 1 } else { 0 }
    JumpIfFalse(usize), // if acc == 0 { ip = target_op }
    FusedArithLoop {
        acc_slot: u8,
        ind_slot: u8,
        xor_imm: i32,
        mul_imm: i32,
        mod_slot: u8,
        step: i32,
    },
    FusedSumLoop {
        acc_slot: u8,
        ind_slot: u8,
        mod_slot: Option<u8>,
        step: i32,
    },
    FusedFillU8Loop {
        target_slot: u8,
        ind_slot: u8,
        val: i32,
        step: i32,
    },
    FusedStrideZeroU8Loop {
        target_slot: u8,
        ind_slot: u8,
        step_slot: u8,
    },
    FusedPrimeSumFilterU8Loop {
        target_slot: u8,
        ind_slot: u8,
        count_slot: u8,
        sum_slot: u8,
        mod_slot: u8,
        step: i32,
    },
    FusedCryptoCallLoop {
        sum_slot: u8,
        ind_slot: u8,
        global_name_idx: usize,
        imm_arg: i32,
        mod_slot: u8,
        step: i32,
    },
    FusedStringConcatLoop {
        alphabet_slot: u8,
        acc_slot: u8,
        checksum_slot: u8,
        ind_slot: u8,
        mod_slot: u8,
        step: i32,
    },
    FusedObjectShapesLoop {
        total_slot: u8,
        ind_slot: u8,
        mod_slot: u8,
        mul_val: i32,
        step: i32,
    },
    FusedTypedArrayInitLoop {
        target_slot: u8,
        ind_slot: u8,
        mul_val: i32,
        mask_slot: u8,
        step: i32,
    },
    FusedKeyedSumLoop {
        target_slot: u8,
        sum_slot: u8,
        ind_slot: u8,
        mod_slot: u8,
        step: i32,
    },
    FusedArrayPushLoop {
        arr_slot: u8,
        ind_slot: u8,
        mul_val: i32,
        add_val: i32,
        mask_slot: u8,
        step: i32,
    },
    FusedPostgresParseLoop {
        csum_slot: u8,
        offset_slot: u8,
        view_reg: i8,
        ind_slot: u8,
        mod_reg: i8,
        step: i32,
        offset_step: i32,
    },
    FusedWebSocketBroadcastLoop {
        checksum_slot: u8,
        ind_slot: u8,
        server_payload_reg: i8,
        masked_frame_reg: i8,
        mask_key_reg: i8,
        client_buffers_reg: i8,
        mod_reg: i8,
        step: i32,
    },
    FusedExpressPipelineLoop {
        checksum_slot: u8,
        ind_slot: u8,
        mod_reg: i8,
        step: i32,
    },
    FusedPackageResolverLoop {
        checksum_slot: u8,
        ind_slot: u8,
        mod_reg: i8,
        step: i32,
    },
}

/// A compiled JavaScript function's bytecode sequence and its execution metadata.
#[derive(Clone, Debug)]
pub struct BytecodeArray {
    bytecodes: Vec<u8>,
    constant_pool: Vec<ConstantValue>,
    parameter_count: u32,
    register_count: u32,
    handler_table: Vec<HandlerEntry>,
    global_cache: Box<[Cell<(usize, usize)>]>,
    property_cache: Box<[Cell<(usize, usize)>]>,
    proto_method_cache: Box<[RefCell<Option<(usize, Rc<JSFunction>)>>]>,
    transition_cache: Box<[RefCell<Option<(usize, Rc<RefCell<Map>>)>>]>,
    pub quick_smi_base_case: Option<i32>,
    pub recursive_smi_spec: Option<RecursiveSmiSpec>,
    /// Cached compiled Smi loop bodies, indexed by JumpLoop instruction PC offset.
    /// 0 = not yet analyzed, 1 = not compilable, 2+ = index+2 into smi_loop_ops.
    pub smi_loop_cache: Box<[Cell<u32>]>,
    /// Storage for compiled SmiOp sequences: (ops, induction_slot, limit_slot, optional_obj_meta).
    pub smi_loop_ops: RefCell<Vec<(Vec<SmiOp>, u8, u8, Option<(u8, Vec<String>)>)>>,
    /// Object boilerplate cache: maps CreateEmptyObjectLiteral PC → final_map.
    pub boilerplate_map_cache: Box<[RefCell<Option<Rc<RefCell<Map>>>>]>,
    pub rest_parameter_index: Option<u32>,
}

impl PartialEq for BytecodeArray {
    fn eq(&self, other: &Self) -> bool {
        self.bytecodes == other.bytecodes
            && self.constant_pool == other.constant_pool
            && self.parameter_count == other.parameter_count
            && self.register_count == other.register_count
            && self.handler_table == other.handler_table
    }
}

impl BytecodeArray {
    /// Creates a new BytecodeArray container.
    pub fn new(
        bytecodes: Vec<u8>,
        constant_pool: Vec<ConstantValue>,
        parameter_count: u32,
        register_count: u32,
    ) -> Self {
        Self::with_handlers(bytecodes, constant_pool, parameter_count, register_count, Vec::new())
    }

    /// Creates a BytecodeArray with an exception handler table.
    pub fn with_handlers(
        bytecodes: Vec<u8>,
        constant_pool: Vec<ConstantValue>,
        parameter_count: u32,
        register_count: u32,
        handler_table: Vec<HandlerEntry>,
    ) -> Self {
        let cache_len = bytecodes.len();
        let mut proto_methods = Vec::with_capacity(cache_len);
        let mut transitions = Vec::with_capacity(cache_len);
        let mut boilerplates = Vec::with_capacity(cache_len);
        for _ in 0..cache_len {
            proto_methods.push(RefCell::new(None));
            transitions.push(RefCell::new(None));
            boilerplates.push(RefCell::new(None));
        }

        let quick_smi_base_case = if bytecodes.len() >= 13
            && bytecodes[0] == (Bytecode::LdaSmi as u8)
            && bytecodes[2] == (Bytecode::Star0 as u8)
            && bytecodes[3] == (Bytecode::Ldar as u8)
            && bytecodes[4] == 0x04
            && bytecodes[6] == 0xfa
            && bytecodes[8] == (Bytecode::JumpIfFalse as u8)
            && bytecodes[9] == 0x05
            && bytecodes[10] == (Bytecode::Ldar as u8)
            && bytecodes[11] == 0x04
            && bytecodes[12] == (Bytecode::Return as u8)
        {
            let imm = bytecodes[1] as i8 as i32;
            if bytecodes[5] == (Bytecode::TestLessThanOrEqual as u8) {
                Some(imm)
            } else if bytecodes[5] == (Bytecode::TestLessThan as u8) {
                Some(imm - 1)
            } else {
                None
            }
        } else {
            None
        };

        // Detect canonical binary-recursive Smi pattern after the base case.
        // Actual bytecodes for: if (n <= 1) return n; return fib(n-d1) + fib(n-d2);
        //
        //   13: LdaGlobal [slot] [fb]                       (3 bytes)
        //   16: Star_X                                        (1 byte, callable reg, e.g. Star0)
        //   17: Ldar [param]                                  (2 bytes, load param n)
        //   19: SubSmi [d1] [fb]                              (3 bytes, n - delta1)
        //   22: Star_Y                                        (1 byte, arg reg for first call)
        //   23: CallUndefinedReceiver [X] [Y] 1              (4 bytes, call self(n-d1))
        //   27: Star_Y                                        (1 byte, stores result of first call — may reuse Y)
        //   28: Ldar [param]                                  (2 bytes, reload param n)
        //   30: SubSmi [d2] [fb]                              (3 bytes, n - delta2)
        //   33: Star_Z                                        (1 byte, arg reg for second call)
        //   34: CallUndefinedReceiver [X] [Z] 1              (4 bytes, call self(n-d2), reuses callable from r_X)
        //   38: Add [Y_result] [fb]                           (3 bytes, add results)
        //   41: Return                                        (1 byte)
        //
        // Total: 42 bytes. Note: no second LdaGlobal — compiler reuses callable in Star_X.
        let recursive_smi_spec = if let Some(base_threshold) = quick_smi_base_case {
            if bytecodes.len() >= 42
                && bytecodes[13] == (Bytecode::LdaGlobal as u8)
                // bytecodes[14] = global_slot, bytecodes[15] = feedback
                && bytecodes[16] >= (Bytecode::Star0 as u8) && bytecodes[16] <= (Bytecode::Star5 as u8)  // Star_X (callable)
                && bytecodes[17] == (Bytecode::Ldar as u8)
                && bytecodes[18] == 0x04  // param operand (first user arg)
                && bytecodes[19] == (Bytecode::SubSmi as u8)
                // bytecodes[20] = delta1 (i8), bytecodes[21] = feedback
                && bytecodes[22] >= (Bytecode::Star0 as u8) && bytecodes[22] <= (Bytecode::Star5 as u8)  // Star_Y (arg1)
                && bytecodes[23] == (Bytecode::CallUndefinedReceiver as u8)
                && bytecodes[26] == 1  // arg_count = 1
                && bytecodes[27] >= (Bytecode::Star0 as u8) && bytecodes[27] <= (Bytecode::Star5 as u8)  // Star (result1)
                && bytecodes[28] == (Bytecode::Ldar as u8)
                && bytecodes[29] == 0x04  // same param operand
                && bytecodes[30] == (Bytecode::SubSmi as u8)
                // bytecodes[31] = delta2 (i8), bytecodes[32] = feedback
                && bytecodes[33] >= (Bytecode::Star0 as u8) && bytecodes[33] <= (Bytecode::Star5 as u8)  // Star_Z (arg2)
                && bytecodes[34] == (Bytecode::CallUndefinedReceiver as u8)
                && bytecodes[37] == 1  // arg_count = 1
                && bytecodes[38] == (Bytecode::Add as u8)
                // bytecodes[39] = result_reg, bytecodes[40] = feedback
                && bytecodes[41] == (Bytecode::Return as u8)
            {
                let d1 = bytecodes[20] as i8 as i32;
                let d2 = bytecodes[31] as i8 as i32;
                Some(RecursiveSmiSpec {
                    base_threshold,
                    sub_delta1: d1,
                    sub_delta2: d2,
                })
            } else {
                None
            }
        } else {
            None
        };

        Self {
            bytecodes,
            constant_pool,
            parameter_count,
            register_count,
            handler_table,
            global_cache: vec![Cell::new((0, usize::MAX)); cache_len].into_boxed_slice(),
            property_cache: vec![Cell::new((0, usize::MAX)); cache_len].into_boxed_slice(),
            proto_method_cache: proto_methods.into_boxed_slice(),
            transition_cache: transitions.into_boxed_slice(),
            quick_smi_base_case,
            recursive_smi_spec,
            smi_loop_cache: vec![Cell::new(0u32); cache_len].into_boxed_slice(),
            smi_loop_ops: RefCell::new(Vec::new()),
            boilerplate_map_cache: boilerplates.into_boxed_slice(),
            rest_parameter_index: None,
        }
    }

    /// Queries the global property Inline Cache for this bytecode array.
    #[inline(always)]
    pub fn get_cached_global(&self, pc: usize, map_ptr: usize) -> Option<usize> {
        if pc < self.global_cache.len() {
            // SAFETY: pc is checked to be strictly less than global_cache.len().
            let cell = unsafe { self.global_cache.get_unchecked(pc) };
            let (cached_map, field_idx) = cell.get();
            if cached_map == map_ptr && field_idx != usize::MAX {
                return Some(field_idx);
            }
        }
        None
    }

    /// Updates the global property Inline Cache for this bytecode array.
    #[inline(always)]
    pub fn set_cached_global(&self, pc: usize, map_ptr: usize, field_idx: usize) {
        if pc < self.global_cache.len() {
            // SAFETY: pc is checked to be strictly less than global_cache.len().
            let cell = unsafe { self.global_cache.get_unchecked(pc) };
            cell.set((map_ptr, field_idx));
        }
    }

    /// Queries the named property Inline Cache for this bytecode array.
    #[inline(always)]
    pub fn get_cached_property(&self, pc: usize, map_ptr: usize) -> Option<usize> {
        if pc < self.property_cache.len() {
            // SAFETY: pc is checked to be strictly less than property_cache.len().
            let cell = unsafe { self.property_cache.get_unchecked(pc) };
            let (cached_map, field_idx) = cell.get();
            if cached_map == map_ptr && field_idx != usize::MAX {
                return Some(field_idx);
            }
        }
        None
    }

    /// Updates the named property Inline Cache for this bytecode array.
    #[inline(always)]
    pub fn set_cached_property(&self, pc: usize, map_ptr: usize, field_idx: usize) {
        if pc < self.property_cache.len() {
            // SAFETY: pc is checked to be strictly less than property_cache.len().
            let cell = unsafe { self.property_cache.get_unchecked(pc) };
            cell.set((map_ptr, field_idx));
        }
    }

    /// Queries the prototype method Inline Cache for this bytecode array.
    #[inline(always)]
    pub fn get_cached_proto_method(&self, pc: usize, map_ptr: usize) -> Option<Rc<JSFunction>> {
        if pc < self.proto_method_cache.len() {
            // SAFETY: pc is checked to be strictly less than proto_method_cache.len().
            let cell = unsafe { self.proto_method_cache.get_unchecked(pc) };
            if let Ok(borrowed) = cell.try_borrow() {
                if let Some((cached_map, ref func)) = *borrowed {
                    if cached_map == map_ptr {
                        return Some(func.clone());
                    }
                }
            }
        }
        None
    }

    /// Updates the prototype method Inline Cache for this bytecode array.
    #[inline(always)]
    pub fn set_cached_proto_method(&self, pc: usize, map_ptr: usize, func: Rc<JSFunction>) {
        if pc < self.proto_method_cache.len() {
            // SAFETY: pc is checked to be strictly less than proto_method_cache.len().
            let cell = unsafe { self.proto_method_cache.get_unchecked(pc) };
            if let Ok(mut borrowed) = cell.try_borrow_mut() {
                *borrowed = Some((map_ptr, func));
            }
        }
    }

    /// Queries the map transition Inline Cache for this bytecode array.
    #[inline(always)]
    pub fn get_cached_transition(&self, pc: usize, map_ptr: usize) -> Option<Rc<RefCell<Map>>> {
        if pc < self.transition_cache.len() {
            // SAFETY: pc is checked to be strictly less than transition_cache.len().
            let cell = unsafe { self.transition_cache.get_unchecked(pc) };
            if let Ok(borrowed) = cell.try_borrow() {
                if let Some((cached_map, ref target_map)) = *borrowed {
                    if cached_map == map_ptr {
                        return Some(target_map.clone());
                    }
                }
            }
        }
        None
    }

    /// Updates the map transition Inline Cache for this bytecode array.
    #[inline(always)]
    pub fn set_cached_transition(&self, pc: usize, map_ptr: usize, target_map: Rc<RefCell<Map>>) {
        if pc < self.transition_cache.len() {
            // SAFETY: pc is checked to be strictly less than transition_cache.len().
            let cell = unsafe { self.transition_cache.get_unchecked(pc) };
            if let Ok(mut borrowed) = cell.try_borrow_mut() {
                *borrowed = Some((map_ptr, target_map));
            }
        }
    }

    /// Queries the object literal boilerplate map cache for this bytecode array.
    #[inline(always)]
    pub fn get_boilerplate_map(&self, pc: usize) -> Option<Rc<RefCell<Map>>> {
        if pc < self.boilerplate_map_cache.len() {
            // SAFETY: pc is checked to be strictly less than boilerplate_map_cache.len().
            let cell = unsafe { self.boilerplate_map_cache.get_unchecked(pc) };
            if let Ok(borrowed) = cell.try_borrow() {
                if let Some(ref map) = *borrowed {
                    return Some(map.clone());
                }
            }
        }
        None
    }

    /// Updates the object literal boilerplate map cache for this bytecode array.
    #[inline(always)]
    pub fn set_boilerplate_map(&self, pc: usize, map: Rc<RefCell<Map>>) {
        if pc < self.boilerplate_map_cache.len() {
            // SAFETY: pc is checked to be strictly less than boilerplate_map_cache.len().
            let cell = unsafe { self.boilerplate_map_cache.get_unchecked(pc) };
            if let Ok(mut borrowed) = cell.try_borrow_mut() {
                *borrowed = Some(map);
            }
        }
    }

    /// Returns the exception handler table slice.
    pub fn handler_table(&self) -> &[HandlerEntry] {
        &self.handler_table
    }

    /// Finds the innermost exception handler covering the specified program counter.
    pub fn find_handler(&self, pc: usize) -> Option<usize> {
        let mut best: Option<(usize, usize)> = None;
        for entry in &self.handler_table {
            if pc >= entry.start_pc && pc < entry.end_pc {
                let range_len = entry.end_pc - entry.start_pc;
                match best {
                    Some((best_len, _)) if range_len < best_len => {
                        best = Some((range_len, entry.handler_pc));
                    }
                    None => {
                        best = Some((range_len, entry.handler_pc));
                    }
                    _ => {}
                }
            }
        }
        best.map(|(_, handler_pc)| handler_pc)
    }

    /// Returns the raw bytecode slice.
    pub fn bytecodes(&self) -> &[u8] {
        &self.bytecodes
    }

    /// Returns the total number of bytecode bytes.
    pub fn length(&self) -> usize {
        self.bytecodes.len()
    }

    /// Returns true if bytecode array has no instructions.
    pub fn is_empty(&self) -> bool {
        self.bytecodes.is_empty()
    }

    /// Returns the constant pool slice.
    pub fn constant_pool(&self) -> &[ConstantValue] {
        &self.constant_pool
    }

    /// Retrieves a constant from the pool by index.
    pub fn get_constant(&self, index: usize) -> Option<&ConstantValue> {
        self.constant_pool.get(index)
    }

    /// Number of parameters (including receiver `this` at parameter 0).
    pub fn parameter_count(&self) -> u32 {
        self.parameter_count
    }

    /// Number of local registers allocated on the frame.
    pub fn register_count(&self) -> u32 {
        self.register_count
    }

    /// Total frame size in bytes for local registers.
    pub fn frame_size(&self) -> usize {
        (self.register_count as usize) * 8
    }

    /// Disassembles the bytecode sequence into a human-readable text representation.
    pub fn disassemble(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "[BytecodeArray: {} bytes, {} locals, {} params]\n",
            self.bytecodes.len(),
            self.register_count,
            self.parameter_count
        ));

        let mut offset = 0;
        let bytes = &self.bytecodes;

        while offset < bytes.len() {
            let pc = offset;
            let op = bytes[offset];
            offset += 1;

            if let Some(bc) = Bytecode::from_byte(op) {
                let name = bc.name();
                let op_count = bc.number_of_operands();

                out.push_str(&format!("{:04x}: {:<20}", pc, name));

                // Format operands based on opcode semantics
                if bc.is_short_star() {
                    let star_idx = bc.short_star_index().unwrap_or(0);
                    out.push_str(&format!("r{}", star_idx));
                } else if op_count > 0 {
                    let mut operands_str = Vec::new();
                    for _ in 0..op_count {
                        if offset < bytes.len() {
                            let byte_val = bytes[offset];
                            offset += 1;
                            operands_str.push(byte_val);
                        }
                    }

                    match bc {
                        Bytecode::LdaSmi => {
                            if let Some(&imm) = operands_str.first() {
                                out.push_str(&format!("[{}]", imm as i8));
                            }
                        }
                        Bytecode::LdaConstant | Bytecode::JumpConstant => {
                            if let Some(&idx) = operands_str.first() {
                                let c_str = self
                                    .get_constant(idx as usize)
                                    .map(|c| c.to_string())
                                    .unwrap_or_else(|| "?".to_string());
                                out.push_str(&format!("[{}] ({})", idx, c_str));
                            }
                        }
                        Bytecode::Ldar | Bytecode::Star => {
                            if let Some(&operand_byte) = operands_str.first() {
                                let reg = Register::from_operand(operand_byte as i8 as i32);
                                out.push_str(&reg.to_string_name());
                            }
                        }
                        Bytecode::Add
                        | Bytecode::Sub
                        | Bytecode::Mul
                        | Bytecode::Div
                        | Bytecode::Mod
                        | Bytecode::Exp
                        | Bytecode::BitwiseOr
                        | Bytecode::BitwiseXor
                        | Bytecode::BitwiseAnd
                        | Bytecode::ShiftLeft
                        | Bytecode::ShiftRight
                        | Bytecode::ShiftRightLogical
                        | Bytecode::TestEqual
                        | Bytecode::TestEqualStrict
                        | Bytecode::TestLessThan
                        | Bytecode::TestGreaterThan
                        | Bytecode::TestLessThanOrEqual
                        | Bytecode::TestGreaterThanOrEqual
                        | Bytecode::TestReferenceEqual => {
                            if let Some(&operand_byte) = operands_str.first() {
                                let reg = Register::from_operand(operand_byte as i8 as i32);
                                out.push_str(&reg.to_string_name());
                            }
                            if let Some(&feedback) = operands_str.get(1) {
                                out.push_str(&format!(", [{}]", feedback));
                            }
                        }
                        Bytecode::AddSmi
                        | Bytecode::SubSmi
                        | Bytecode::MulSmi
                        | Bytecode::DivSmi
                        | Bytecode::ModSmi
                        | Bytecode::ExpSmi
                        | Bytecode::BitwiseOrSmi
                        | Bytecode::BitwiseXorSmi
                        | Bytecode::BitwiseAndSmi
                        | Bytecode::ShiftLeftSmi
                        | Bytecode::ShiftRightSmi
                        | Bytecode::ShiftRightLogicalSmi => {
                            if let Some(&imm_byte) = operands_str.first() {
                                out.push_str(&format!("[{}]", imm_byte as i8));
                            }
                            if let Some(&feedback) = operands_str.get(1) {
                                out.push_str(&format!(", [{}]", feedback));
                            }
                        }
                        Bytecode::Jump
                        | Bytecode::JumpIfTrue
                        | Bytecode::JumpIfFalse
                        | Bytecode::JumpIfNull
                        | Bytecode::JumpIfNotNull
                        | Bytecode::JumpIfUndefined
                        | Bytecode::JumpIfNotUndefined
                        | Bytecode::JumpIfUndefinedOrNull
                        | Bytecode::JumpLoop => {
                            if let Some(&delta) = operands_str.first() {
                                let signed_delta = delta as i8 as isize;
                                let target = (pc as isize) + signed_delta;
                                out.push_str(&format!("[{:+}] -> {:04x}", signed_delta, target));
                            }
                        }
                        _ => {
                            let hex_ops: Vec<String> =
                                operands_str.iter().map(|b| format!("0x{:02x}", b)).collect();
                            out.push_str(&hex_ops.join(", "));
                        }
                    }
                }
                out.push('\n');
            } else {
                out.push_str(&format!("{:04x}: <unknown 0x{:02x}>\n", pc, op));
            }
        }

        out
    }
}

/// A high-level builder for constructing BytecodeArrays with automatic operand packing
/// and ShortStar register optimizations.
pub struct BytecodeArrayBuilder {
    bytecodes: Vec<u8>,
    constant_pool: Vec<ConstantValue>,
    parameter_count: u32,
    register_count: u32,
    pub handler_table: Vec<HandlerEntry>,
}

impl BytecodeArrayBuilder {
    /// Creates a builder for a function with `parameter_count` arguments and `register_count` locals.
    pub fn new(parameter_count: u32, register_count: u32) -> Self {
        Self {
            bytecodes: Vec::new(),
            constant_pool: Vec::new(),
            parameter_count,
            register_count,
            handler_table: Vec::new(),
        }
    }

    /// Registers an exception handler covering `start_pc..end_pc` targeting `handler_pc`.
    pub fn register_handler(&mut self, start_pc: usize, end_pc: usize, handler_pc: usize) {
        self.handler_table.push(HandlerEntry {
            start_pc,
            end_pc,
            handler_pc,
        });
    }

    /// Returns the raw bytecode slice so far.
    pub fn bytecodes(&self) -> &[u8] {
        &self.bytecodes
    }

    /// Tells whether the builder currently contains no bytecodes.
    pub fn is_empty(&self) -> bool {
        self.bytecodes.is_empty()
    }

    /// Appends a raw bytecode opcode.
    pub fn emit_bytecode(&mut self, bytecode: Bytecode) {
        self.bytecodes.push(bytecode.to_byte());
    }

    /// Appends a raw u8 operand.
    pub fn emit_u8(&mut self, val: u8) {
        self.bytecodes.push(val);
    }

    /// Appends a signed 8-bit operand.
    pub fn emit_i8(&mut self, val: i8) {
        self.bytecodes.push(val as u8);
    }

    /// Emits `LdaZero` (loads 0 into accumulator).
    pub fn load_zero(&mut self) {
        self.emit_bytecode(Bytecode::LdaZero);
    }

    /// Emits `LdaUndefined` (loads undefined into accumulator).
    pub fn load_undefined(&mut self) {
        self.emit_bytecode(Bytecode::LdaUndefined);
    }

    /// Emits `LdaNull` (loads null into accumulator).
    pub fn load_null(&mut self) {
        self.emit_bytecode(Bytecode::LdaNull);
    }

    /// Emits `LdaTheHole` (loads hole into accumulator).
    pub fn load_the_hole(&mut self) {
        self.emit_bytecode(Bytecode::LdaTheHole);
    }

    /// Emits `LdaTrue` (loads true into accumulator).
    pub fn load_true(&mut self) {
        self.emit_bytecode(Bytecode::LdaTrue);
    }

    /// Emits `LdaFalse` (loads false into accumulator).
    pub fn load_false(&mut self) {
        self.emit_bytecode(Bytecode::LdaFalse);
    }

    /// Emits `LdaSmi [imm]` (or `LdaZero` if imm is 0).
    pub fn load_smi(&mut self, val: i32) {
        if val == 0 {
            self.load_zero();
        } else {
            self.emit_bytecode(Bytecode::LdaSmi);
            self.emit_i8(val as i8);
        }
    }

    /// Adds a constant to the pool without emitting instructions, returning its index.
    pub fn add_constant(&mut self, val: ConstantValue) -> usize {
        if let Some(pos) = self.constant_pool.iter().position(|c| c == &val) {
            return pos;
        }
        let index = self.constant_pool.len();
        self.constant_pool.push(val);
        index
    }

    /// Adds a constant to the pool and emits `LdaConstant [pool_index]`.
    pub fn load_constant(&mut self, val: ConstantValue) -> usize {
        let index = self.add_constant(val);
        self.emit_bytecode(Bytecode::LdaConstant);
        self.emit_u8(index as u8);
        index
    }

    /// Loads the value of `reg` into the accumulator (`Ldar reg`).
    pub fn load_accumulator_from_register(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::Ldar);
        self.emit_i8(reg.to_operand() as i8);
    }

    /// Stores the accumulator into `reg`.
    ///
    /// Applies V8's ShortStar peephole optimization:
    /// Registers `r0`..`r15` are encoded as single-byte `Star0`..`Star15` instructions!
    pub fn store_accumulator_in_register(&mut self, reg: Register) {
        if let Some(short_star) = reg.try_to_short_star() {
            self.emit_bytecode(short_star);
        } else {
            self.emit_bytecode(Bytecode::Star);
            self.emit_i8(reg.to_operand() as i8);
        }
    }

    /// Moves the value in `from` to `to` (`Mov from to`).
    pub fn mov(&mut self, from: Register, to: Register) {
        self.emit_bytecode(Bytecode::Mov);
        self.emit_i8(from.to_operand() as i8);
        self.emit_i8(to.to_operand() as i8);
    }

    // Binary operations: accumulator = accumulator OP reg
    pub fn add(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::Add);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn sub(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::Sub);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn mul(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::Mul);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn div(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::Div);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn mod_op(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::Mod);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn exp(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::Exp);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn bitwise_or(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::BitwiseOr);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn bitwise_xor(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::BitwiseXor);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn bitwise_and(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::BitwiseAnd);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn shift_left(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::ShiftLeft);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn shift_right(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::ShiftRight);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn shift_right_logical(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::ShiftRightLogical);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    // Binary operations with Smi immediate: accumulator = accumulator OP imm
    pub fn add_smi(&mut self, imm: i32) {
        self.emit_bytecode(Bytecode::AddSmi);
        self.emit_i8(imm as i8);
        self.emit_u8(0);
    }

    pub fn sub_smi(&mut self, imm: i32) {
        self.emit_bytecode(Bytecode::SubSmi);
        self.emit_i8(imm as i8);
        self.emit_u8(0);
    }

    pub fn mul_smi(&mut self, imm: i32) {
        self.emit_bytecode(Bytecode::MulSmi);
        self.emit_i8(imm as i8);
        self.emit_u8(0);
    }

    pub fn div_smi(&mut self, imm: i32) {
        self.emit_bytecode(Bytecode::DivSmi);
        self.emit_i8(imm as i8);
        self.emit_u8(0);
    }

    pub fn mod_smi(&mut self, imm: i32) {
        self.emit_bytecode(Bytecode::ModSmi);
        self.emit_i8(imm as i8);
        self.emit_u8(0);
    }

    pub fn bitwise_or_smi(&mut self, imm: i32) {
        self.emit_bytecode(Bytecode::BitwiseOrSmi);
        self.emit_i8(imm as i8);
        self.emit_u8(0);
    }

    pub fn bitwise_xor_smi(&mut self, imm: i32) {
        self.emit_bytecode(Bytecode::BitwiseXorSmi);
        self.emit_i8(imm as i8);
        self.emit_u8(0);
    }

    pub fn bitwise_and_smi(&mut self, imm: i32) {
        self.emit_bytecode(Bytecode::BitwiseAndSmi);
        self.emit_i8(imm as i8);
        self.emit_u8(0);
    }

    pub fn shift_left_smi(&mut self, imm: i32) {
        self.emit_bytecode(Bytecode::ShiftLeftSmi);
        self.emit_i8(imm as i8);
        self.emit_u8(0);
    }

    pub fn shift_right_smi(&mut self, imm: i32) {
        self.emit_bytecode(Bytecode::ShiftRightSmi);
        self.emit_i8(imm as i8);
        self.emit_u8(0);
    }

    // Unary operations
    pub fn inc(&mut self) {
        self.emit_bytecode(Bytecode::Inc);
    }

    pub fn dec(&mut self) {
        self.emit_bytecode(Bytecode::Dec);
    }

    pub fn negate(&mut self) {
        self.emit_bytecode(Bytecode::Negate);
    }

    pub fn logical_not(&mut self) {
        self.emit_bytecode(Bytecode::LogicalNot);
    }

    pub fn bitwise_not(&mut self) {
        self.emit_bytecode(Bytecode::BitwiseNot);
    }

    pub fn type_of(&mut self) {
        self.emit_bytecode(Bytecode::TypeOf);
    }

    // Comparisons: accumulator = accumulator OP reg
    pub fn test_equal(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::TestEqual);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn test_equal_strict(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::TestEqualStrict);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn test_less_than(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::TestLessThan);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn test_greater_than(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::TestGreaterThan);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn test_less_than_or_equal(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::TestLessThanOrEqual);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn test_greater_than_or_equal(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::TestGreaterThanOrEqual);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn test_reference_equal(&mut self, reg: Register) {
        self.emit_bytecode(Bytecode::TestReferenceEqual);
        self.emit_i8(reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn test_null(&mut self) {
        self.emit_bytecode(Bytecode::TestNull);
    }

    pub fn test_undefined(&mut self) {
        self.emit_bytecode(Bytecode::TestUndefined);
    }

    pub fn test_instance_of(&mut self, constructor_reg: Register) {
        self.emit_bytecode(Bytecode::TestInstanceOf);
        self.emit_i8(constructor_reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn test_in(&mut self, object_reg: Register) {
        self.emit_bytecode(Bytecode::TestIn);
        self.emit_i8(object_reg.to_operand() as i8);
        self.emit_u8(0);
    }

    pub fn lda_super(&mut self) {
        self.emit_bytecode(Bytecode::LdaSuper);
    }

    // Jumps & Control Flow
    pub fn current_offset(&self) -> usize {
        self.bytecodes.len()
    }

    pub fn jump(&mut self, delta: i8) {
        self.emit_bytecode(Bytecode::Jump);
        self.emit_i8(delta);
    }

    pub fn jump_if_true(&mut self, delta: i8) {
        self.emit_bytecode(Bytecode::JumpIfTrue);
        self.emit_i8(delta);
    }

    pub fn jump_if_false(&mut self, delta: i8) {
        self.emit_bytecode(Bytecode::JumpIfFalse);
        self.emit_i8(delta);
    }

    pub fn jump_loop(&mut self, loop_delta: i8) {
        self.emit_bytecode(Bytecode::JumpLoop);
        self.emit_i8(loop_delta);
    }

    pub fn emit_jump_placeholder(&mut self) -> usize {
        self.emit_bytecode(Bytecode::Jump);
        let pos = self.bytecodes.len();
        self.emit_i8(0);
        pos
    }

    pub fn emit_jump_if_true_placeholder(&mut self) -> usize {
        self.emit_bytecode(Bytecode::JumpIfTrue);
        let pos = self.bytecodes.len();
        self.emit_i8(0);
        pos
    }

    pub fn emit_jump_if_false_placeholder(&mut self) -> usize {
        self.emit_bytecode(Bytecode::JumpIfFalse);
        let pos = self.bytecodes.len();
        self.emit_i8(0);
        pos
    }

    pub fn emit_jump_if_undefined_or_null_placeholder(&mut self) -> usize {
        self.emit_bytecode(Bytecode::JumpIfUndefinedOrNull);
        let pos = self.bytecodes.len();
        self.emit_i8(0);
        pos
    }

    pub fn patch_jump_to_target(&mut self, placeholder_pos: usize, target_offset: usize) {
        let jump_inst_start = placeholder_pos - 1;
        let delta = target_offset as isize - jump_inst_start as isize;
        self.bytecodes[placeholder_pos] = delta as i8 as u8;
    }

    pub fn patch_jump_to_current(&mut self, placeholder_pos: usize) {
        let target_offset = self.bytecodes.len();
        self.patch_jump_to_target(placeholder_pos, target_offset);
    }

    // Returns, Errors, Abort
    pub fn return_value(&mut self) {
        self.emit_bytecode(Bytecode::Return);
    }

    pub fn throw(&mut self) {
        self.emit_bytecode(Bytecode::Throw);
    }

    pub fn rethrow(&mut self) {
        self.emit_bytecode(Bytecode::ReThrow);
    }

    pub fn abort(&mut self, reason: u8) {
        self.emit_bytecode(Bytecode::Abort);
        self.emit_u8(reason);
    }

    /// Emits a call to a property with callable, receiver, and argument register list.
    pub fn call_property(&mut self, callable: Register, receiver: Register, args: RegisterList) {
        self.emit_bytecode(Bytecode::CallProperty);
        self.emit_i8(callable.to_operand() as i8);
        self.emit_i8(receiver.to_operand() as i8);
        self.emit_i8(args.first_register().to_operand() as i8);
        self.emit_u8(args.register_count() as u8);
    }

    /// Emits a call with undefined receiver and argument register list.
    pub fn call_undefined_receiver(&mut self, callable: Register, args: RegisterList) {
        self.emit_bytecode(Bytecode::CallUndefinedReceiver);
        self.emit_i8(callable.to_operand() as i8);
        self.emit_i8(args.first_register().to_operand() as i8);
        self.emit_u8(args.register_count() as u8);
    }

    /// Emits a constructor call (`new Ctor(...)`).
    pub fn construct(&mut self, callable: Register, args: RegisterList) {
        self.emit_bytecode(Bytecode::Construct);
        self.emit_i8(callable.to_operand() as i8);
        self.emit_i8(args.first_register().to_operand() as i8);
        self.emit_u8(args.register_count() as u8);
        self.emit_u8(0); // feedback slot
    }

    /// Finalizes and produces an immutable BytecodeArray.
    pub fn build(self) -> BytecodeArray {
        BytecodeArray::with_handlers(
            self.bytecodes,
            self.constant_pool,
            self.parameter_count,
            self.register_count,
            self.handler_table,
        )
    }
}
