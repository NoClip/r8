//! Safe Rust reimplementation of the WebAssembly stack-machine interpreter.
//!
//! Executes decoded `WasmModule` functions using a typed operand stack, a
//! structured control stack, and a call frame stack — mirroring V8's Liftoff
//! baseline execution semantics in pure, no-unsafe Rust.

use super::binary_parser::{FuncType, ValType, WasmCode, WasmModule};
use super::instance::WasmInstance;
use std::cell::RefCell;
use std::rc::Rc;

// ─── Value ───────────────────────────────────────────────────────────────────

/// A garbage-collected struct instance.
#[derive(Clone, Debug, PartialEq)]
pub struct WasmStruct {
    pub type_idx: u32,
    pub fields: Vec<WasmVal>,
}

/// A garbage-collected array instance.
#[derive(Clone, Debug, PartialEq)]
pub struct WasmArray {
    pub type_idx: u32,
    pub elements: Vec<WasmVal>,
}

/// A typed runtime value on the Wasm operand stack.
#[derive(Clone, Debug, PartialEq)]
pub enum WasmVal {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    V128([u8; 16]),
    FuncRef(Option<u32>),   // None = null funcref
    ExternRef(Option<u32>), // None = null externref
    // GC Proposals
    StructRef(Option<Rc<RefCell<WasmStruct>>>),
    ArrayRef(Option<Rc<RefCell<WasmArray>>>),
    I31(i32),
}

impl WasmVal {
    pub fn as_i32(&self) -> i32 {
        match self { WasmVal::I32(v) => *v, _ => 0 }
    }
    pub fn as_i64(&self) -> i64 {
        match self { WasmVal::I64(v) => *v, _ => 0 }
    }
    pub fn as_f32(&self) -> f32 {
        match self { WasmVal::F32(v) => *v, _ => 0.0 }
    }
    pub fn as_f64(&self) -> f64 {
        match self { WasmVal::F64(v) => *v, _ => 0.0 }
    }
    pub fn as_v128(&self) -> [u8; 16] {
        match self { WasmVal::V128(b) => *b, _ => [0; 16] }
    }

    pub fn from_i32x4(lanes: [i32; 4]) -> Self {
        let mut b = [0u8; 16];
        for i in 0..4 {
            let bytes = lanes[i].to_le_bytes();
            b[i * 4..i * 4 + 4].copy_from_slice(&bytes);
        }
        WasmVal::V128(b)
    }

    pub fn to_i32x4(&self) -> [i32; 4] {
        let b = self.as_v128();
        let mut lanes = [0i32; 4];
        for i in 0..4 {
            lanes[i] = i32::from_le_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]]);
        }
        lanes
    }

    pub fn from_f32x4(lanes: [f32; 4]) -> Self {
        let mut b = [0u8; 16];
        for i in 0..4 {
            let bytes = lanes[i].to_le_bytes();
            b[i * 4..i * 4 + 4].copy_from_slice(&bytes);
        }
        WasmVal::V128(b)
    }

    pub fn to_f32x4(&self) -> [f32; 4] {
        let b = self.as_v128();
        let mut lanes = [0f32; 4];
        for i in 0..4 {
            lanes[i] = f32::from_le_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]]);
        }
        lanes
    }

    pub fn from_i8x16(lanes: [i8; 16]) -> Self {
        let mut b = [0u8; 16];
        for i in 0..16 {
            b[i] = lanes[i] as u8;
        }
        WasmVal::V128(b)
    }

    pub fn to_i8x16(&self) -> [i8; 16] {
        let b = self.as_v128();
        let mut lanes = [0i8; 16];
        for i in 0..16 {
            lanes[i] = b[i] as i8;
        }
        lanes
    }

    pub fn from_i16x8(lanes: [i16; 8]) -> Self {
        let mut b = [0u8; 16];
        for i in 0..8 {
            let bytes = lanes[i].to_le_bytes();
            b[i * 2..i * 2 + 2].copy_from_slice(&bytes);
        }
        WasmVal::V128(b)
    }

    pub fn to_i16x8(&self) -> [i16; 8] {
        let b = self.as_v128();
        let mut lanes = [0i16; 8];
        for i in 0..8 {
            lanes[i] = i16::from_le_bytes([b[i * 2], b[i * 2 + 1]]);
        }
        lanes
    }

    pub fn from_i64x2(lanes: [i64; 2]) -> Self {
        let mut b = [0u8; 16];
        for i in 0..2 {
            let bytes = lanes[i].to_le_bytes();
            b[i * 8..i * 8 + 8].copy_from_slice(&bytes);
        }
        WasmVal::V128(b)
    }

    pub fn to_i64x2(&self) -> [i64; 2] {
        let b = self.as_v128();
        let mut lanes = [0i64; 2];
        for i in 0..2 {
            lanes[i] = i64::from_le_bytes([
                b[i * 8], b[i * 8 + 1], b[i * 8 + 2], b[i * 8 + 3],
                b[i * 8 + 4], b[i * 8 + 5], b[i * 8 + 6], b[i * 8 + 7],
            ]);
        }
        lanes
    }

    fn default_for(ty: &ValType) -> Self {
        match ty {
            ValType::I32 => WasmVal::I32(0),
            ValType::I64 => WasmVal::I64(0),
            ValType::F32 => WasmVal::F32(0.0),
            ValType::F64 => WasmVal::F64(0.0),
            ValType::V128 => WasmVal::V128([0; 16]),
            ValType::FuncRef => WasmVal::FuncRef(None),
            ValType::ExternRef => WasmVal::ExternRef(None),
            ValType::AnyRef | ValType::EqRef | ValType::NullRef => WasmVal::ExternRef(None),
            ValType::StructRef(_) => WasmVal::StructRef(None),
            ValType::ArrayRef(_) => WasmVal::ArrayRef(None),
            ValType::I31Ref => WasmVal::I31(0),
        }
    }
}

// ─── Trap ────────────────────────────────────────────────────────────────────

/// A WebAssembly runtime trap (unrecoverable error).
#[derive(Debug, Clone, PartialEq)]
pub enum WasmTrap {
    Unreachable,
    DivisionByZero,
    IntegerOverflow,
    OutOfBoundsMemoryAccess { addr: u32, size: u32, mem_size: u32 },
    StackOverflow,
    UndefinedElement,
    UninitializedElement,
    TypeMismatch,
    UnknownOpcode(u8),
    CallStackExhausted,
    FunctionNotFound(u32),
    Interrupted,
    Exception { tag_idx: u32, values: Vec<WasmVal> },
    NullReference,
}

impl std::fmt::Display for WasmTrap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasmTrap::Unreachable => write!(f, "unreachable"),
            WasmTrap::DivisionByZero => write!(f, "integer divide by zero"),
            WasmTrap::IntegerOverflow => write!(f, "integer overflow"),
            WasmTrap::OutOfBoundsMemoryAccess { addr, size, mem_size } => {
                write!(f, "out of bounds memory access: addr={} size={} mem={}", addr, size, mem_size)
            }
            WasmTrap::StackOverflow => write!(f, "call stack exhausted"),
            WasmTrap::UndefinedElement => write!(f, "undefined element"),
            WasmTrap::UninitializedElement => write!(f, "uninitialized element"),
            WasmTrap::TypeMismatch => write!(f, "indirect call type mismatch"),
            WasmTrap::UnknownOpcode(b) => write!(f, "unknown opcode 0x{:02x}", b),
            WasmTrap::CallStackExhausted => write!(f, "call stack exhausted"),
            WasmTrap::FunctionNotFound(i) => write!(f, "function not found: {}", i),
            WasmTrap::Interrupted => write!(f, "execution interrupted"),
            WasmTrap::Exception { tag_idx, values } => {
                write!(f, "wasm exception: tag={} values={:?}", tag_idx, values)
            }
            WasmTrap::NullReference => write!(f, "dereferencing null reference"),
        }
    }
}

// ─── Control Frame ───────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum CatchClause {
    Catch(u32, u32),       // tag_idx, label_depth
    CatchRef(u32, u32),    // tag_idx, label_depth
    CatchAll(u32),         // label_depth
    CatchAllRef(u32),      // label_depth
}

#[derive(Clone, Debug, PartialEq)]
enum ControlKind {
    Block,
    Loop,
    If,
    Function,
    TryTable(Vec<CatchClause>),
}

/// A structured control-flow frame (block/loop/if/function boundary).
#[derive(Clone, Debug)]
struct ControlFrame {
    kind: ControlKind,
    /// Number of values the block produces (arity).
    result_arity: usize,
    /// Operand stack height at block entry (for stack polymorphism).
    stack_height: usize,
    /// For `loop`: the byte offset of the loop header (branch target).
    loop_start_ip: usize,
}

// ─── Interpreter ─────────────────────────────────────────────────────────────

const MAX_CALL_DEPTH: usize = 512;

/// Pure stack-machine WebAssembly interpreter.
pub struct WasmInterpreter;

impl WasmInterpreter {
    /// Execute a named exported function with the given arguments.
    pub fn call_export(
        module: &WasmModule,
        instance: &mut WasmInstance,
        export_name: &str,
        args: Vec<WasmVal>,
    ) -> Result<Vec<WasmVal>, WasmTrap> {
        // Find export
        let func_idx = module.exports.iter().find_map(|exp| {
            use super::binary_parser::ExportDesc;
            if exp.name == export_name {
                if let ExportDesc::Func(idx) = exp.desc {
                    return Some(idx);
                }
            }
            None
        });
        let func_idx = func_idx.ok_or(WasmTrap::FunctionNotFound(u32::MAX))?;
        Self::call_func(module, instance, func_idx, args, 0)
    }

    /// Execute a function by index.
    pub fn call_func(
        module: &WasmModule,
        instance: &mut WasmInstance,
        func_idx: u32,
        args: Vec<WasmVal>,
        depth: usize,
    ) -> Result<Vec<WasmVal>, WasmTrap> {
        if depth > MAX_CALL_DEPTH {
            return Err(WasmTrap::CallStackExhausted);
        }

        let imported_count = module.imported_func_count() as u32;

        // Imported function?
        if func_idx < imported_count {
            return instance.call_imported(func_idx, args);
        }

        let local_idx = (func_idx - imported_count) as usize;
        let code = module.code.get(local_idx).ok_or(WasmTrap::FunctionNotFound(func_idx))?;
        let type_idx = module.functions.get(local_idx).copied().ok_or(WasmTrap::FunctionNotFound(func_idx))? as usize;
        let func_type = module.types.get(type_idx).ok_or(WasmTrap::FunctionNotFound(func_idx))?;

        Self::execute_function(module, instance, func_type, code, args, depth)
    }

    fn execute_function<'a>(
        module: &'a WasmModule,
        instance: &mut WasmInstance,
        mut func_type: &'a FuncType,
        mut code: &'a WasmCode,
        mut args: Vec<WasmVal>,
        depth: usize,
    ) -> Result<Vec<WasmVal>, WasmTrap> {
        'tail_call: loop {
        // Build locals: params first, then declared locals (zero-initialized)
        let mut locals: Vec<WasmVal> = args;
        for (count, ty) in &code.locals {
            for _ in 0..*count {
                locals.push(WasmVal::default_for(ty));
            }
        }

        let mut stack: Vec<WasmVal> = Vec::new();
        let mut ctrl_stack: Vec<ControlFrame> = Vec::new();

        // Push implicit function frame
        ctrl_stack.push(ControlFrame {
            kind: ControlKind::Function,
            result_arity: func_type.results.len(),
            stack_height: 0,
            loop_start_ip: 0,
        });

        let body = &code.body;
        let mut ip = 0usize;

        macro_rules! pop {
            () => {
                stack.pop().ok_or(WasmTrap::TypeMismatch)?
            };
        }
        macro_rules! pop_i32 {
            () => {
                match stack.pop().ok_or(WasmTrap::TypeMismatch)? {
                    WasmVal::I32(v) => v,
                    _ => return Err(WasmTrap::TypeMismatch),
                }
            };
        }
        macro_rules! pop_i64 {
            () => {
                match stack.pop().ok_or(WasmTrap::TypeMismatch)? {
                    WasmVal::I64(v) => v,
                    _ => return Err(WasmTrap::TypeMismatch),
                }
            };
        }
        macro_rules! pop_f32 {
            () => {
                match stack.pop().ok_or(WasmTrap::TypeMismatch)? {
                    WasmVal::F32(v) => v,
                    _ => return Err(WasmTrap::TypeMismatch),
                }
            };
        }
        macro_rules! pop_f64 {
            () => {
                match stack.pop().ok_or(WasmTrap::TypeMismatch)? {
                    WasmVal::F64(v) => v,
                    _ => return Err(WasmTrap::TypeMismatch),
                }
            };
        }
        macro_rules! read_u32_leb {
            () => {{
                let mut result = 0u32;
                let mut shift = 0u32;
                loop {
                    let b = *body.get(ip).ok_or(WasmTrap::Interrupted)?;
                    ip += 1;
                    result |= ((b & 0x7F) as u32) << shift;
                    if b & 0x80 == 0 { break; }
                    shift += 7;
                }
                result
            }};
        }
        macro_rules! read_u64_leb {
            () => {{
                let mut result = 0u64;
                let mut shift = 0u32;
                loop {
                    let b = *body.get(ip).ok_or(WasmTrap::Interrupted)?;
                    ip += 1;
                    result |= ((b & 0x7F) as u64) << shift;
                    if b & 0x80 == 0 { break; }
                    shift += 7;
                }
                result
            }};
        }
        macro_rules! read_i64_leb {
            () => {{
                let mut result = 0i64;
                let mut shift = 0u32;
                loop {
                    let b = *body.get(ip).ok_or(WasmTrap::Interrupted)?;
                    ip += 1;
                    result |= ((b & 0x7F) as i64) << shift;
                    shift += 7;
                    if b & 0x80 == 0 {
                        if shift < 64 && (b & 0x40) != 0 { result |= !0i64 << shift; }
                        break;
                    }
                }
                result
            }};
        }
        macro_rules! read_i32_leb {
            () => {{
                let mut result = 0i32;
                let mut shift = 0u32;
                loop {
                    let b = *body.get(ip).ok_or(WasmTrap::Interrupted)?;
                    ip += 1;
                    result |= ((b & 0x7F) as i32) << shift;
                    shift += 7;
                    if b & 0x80 == 0 {
                        if shift < 32 && (b & 0x40) != 0 {
                            result |= !0i32 << shift;
                        }
                        break;
                    }
                    if shift >= 32 {
                        return Err(WasmTrap::TypeMismatch);
                    }
                }
                result
            }};
        }
        macro_rules! pop_addr {
            ($mem_idx:expr) => {{
                let is_m64 = instance.memories.get($mem_idx).map(|m| m.is_memory64).unwrap_or(false);
                if is_m64 {
                    match stack.pop().ok_or(WasmTrap::TypeMismatch)? {
                        WasmVal::I64(v) => v as u64,
                        WasmVal::I32(v) => v as u32 as u64,
                        _ => return Err(WasmTrap::TypeMismatch),
                    }
                } else {
                    pop_i32!() as u32 as u64
                }
            }};
        }

        // Branch to label_depth — collects top `arity` values, unwinds stack & ctrl frames
        // Returns new ip after branch
        let do_branch = |stack: &mut Vec<WasmVal>,
                          ctrl_stack: &mut Vec<ControlFrame>,
                          label_depth: u32,
                          current_ip: usize,
                          body: &[u8]|
         -> Result<usize, WasmTrap> {
            let target_idx = ctrl_stack.len().checked_sub(1 + label_depth as usize)
                .ok_or(WasmTrap::TypeMismatch)?;
            let frame = ctrl_stack[target_idx].clone();
            // Collect results
            let arity = if frame.kind == ControlKind::Loop { 0 } else { frame.result_arity };
            let results: Vec<WasmVal> = stack.iter().rev().take(arity).cloned().collect::<Vec<_>>().into_iter().rev().collect();
            // Unwind stack
            stack.truncate(frame.stack_height);
            stack.extend(results);

            if frame.kind == ControlKind::Loop {
                // Branch to loop start
                ctrl_stack.truncate(target_idx + 1);
                Ok(frame.loop_start_ip)
            } else {
                let levels = ctrl_stack.len() - target_idx;
                ctrl_stack.truncate(target_idx);
                if target_idx == 0 {
                    return Ok(body.len());
                }
                // Scan forward to matching end of target block
                let mut depth = levels;
                let mut scan_ip = current_ip;
                while scan_ip < body.len() && depth > 0 {
                    let b = body[scan_ip];
                    scan_ip += 1;
                    match b {
                        0x02 | 0x03 | 0x04 | 0x1F => {
                            scan_ip += 1;
                            depth += 1;
                        }
                        0x0B => {
                            depth -= 1;
                            if depth == 0 {
                                return Ok(scan_ip);
                            }
                        }
                        0x0C | 0x0D | 0x10 | 0x12 | 0x20..=0x24 | 0x41 | 0x42 => {
                            while scan_ip < body.len() && (body[scan_ip] & 0x80 != 0) { scan_ip += 1; }
                            scan_ip += 1;
                        }
                        0x43 => scan_ip += 4,
                        0x44 => scan_ip += 8,
                        _ => {}
                    }
                }
                Ok(body.len())
            }
        };

        // Main dispatch loop
        loop {
            if ip >= body.len() {
                break; // implicit return / end
            }

            let opcode = body[ip];
            ip += 1;

            match opcode {
                // ── Control ──────────────────────────────────────────────
                0x00 => return Err(WasmTrap::Unreachable),
                0x01 => {} // nop
                0x02 => {
                    // block bt
                    let bt = body.get(ip).copied().ok_or(WasmTrap::Interrupted)?;
                    ip += 1;
                    let arity = if bt == 0x40 { 0 } else { 1 }; // 0x40 = empty type
                    ctrl_stack.push(ControlFrame {
                        kind: ControlKind::Block,
                        result_arity: arity,
                        stack_height: stack.len(),
                        loop_start_ip: 0,
                    });
                }
                0x03 => {
                    // loop bt
                    let bt = body.get(ip).copied().ok_or(WasmTrap::Interrupted)?;
                    ip += 1;
                    let arity = if bt == 0x40 { 0 } else { 1 };
                    ctrl_stack.push(ControlFrame {
                        kind: ControlKind::Loop,
                        result_arity: arity,
                        stack_height: stack.len(),
                        loop_start_ip: ip, // after the blocktype byte
                    });
                }
                0x04 => {
                    // if bt
                    let bt = body.get(ip).copied().ok_or(WasmTrap::Interrupted)?;
                    ip += 1;
                    let arity = if bt == 0x40 { 0 } else { 1 };
                    let cond = pop_i32!();
                    ctrl_stack.push(ControlFrame {
                        kind: ControlKind::If,
                        result_arity: arity,
                        stack_height: stack.len(),
                        loop_start_ip: 0,
                    });
                    if cond == 0 {
                        // skip to matching else or end
                        let mut depth = 1usize;
                        while ip < body.len() {
                            let b = body[ip]; ip += 1;
                            match b {
                                0x02 | 0x03 | 0x04 => depth += 1,
                                0x05 if depth == 1 => { break; } // found else at our level
                                0x0B => {
                                    depth -= 1;
                                    if depth == 0 {
                                        // found end: pop the if frame
                                        ctrl_stack.pop();
                                        break;
                                    }
                                }
                                _ => {}
                            }
                            // skip immediate bytes for instructions that have them
                            match b {
                                0x02 | 0x03 | 0x04 => { ip += 1; } // skip blocktype
                                0x0C | 0x0D => { // br / br_if — skip u32 leb
                                    while ip < body.len() { let x = body[ip]; ip+=1; if x&0x80==0{break;} }
                                }
                                0x0E => { // br_table — skip count+1 u32 lebs
                                    let mut cnt = 0u32;
                                    let mut shift = 0u32;
                                    loop { let x=body[ip]; ip+=1; cnt|=((x&0x7F)as u32)<<shift; shift+=7; if x&0x80==0{break;} }
                                    for _ in 0..=cnt { while ip<body.len(){ let x=body[ip]; ip+=1; if x&0x80==0{break;} } }
                                }
                                0x10 | 0x12 => { while ip<body.len(){ let x=body[ip]; ip+=1; if x&0x80==0{break;} } } // call / return_call
                                0x11 | 0x13 => { // call_indirect / return_call_indirect — two u32 lebs
                                    for _ in 0..2 { while ip<body.len(){ let x=body[ip]; ip+=1; if x&0x80==0{break;} } }
                                }
                                0x20..=0x24 => { while ip<body.len(){ let x=body[ip]; ip+=1; if x&0x80==0{break;} } }
                                0x41 => { while ip<body.len(){ let x=body[ip]; ip+=1; if x&0x80==0{break;} } }
                                0x42 => { while ip<body.len(){ let x=body[ip]; ip+=1; if x&0x80==0{break;} } }
                                0x43 => { ip+=4; }
                                0x44 => { ip+=8; }
                                0x28..=0x3E => { // memory ops — align + offset
                                    for _ in 0..2 { while ip<body.len(){ let x=body[ip]; ip+=1; if x&0x80==0{break;} } }
                                }
                                0x3F | 0x40 => { while ip<body.len(){ let x=body[ip]; ip+=1; if x&0x80==0{break;} } } // memory.size / memory.grow
                                0xD0 => { ip += 1; } // ref.null
                                0xD2 => { while ip<body.len(){ let x=body[ip]; ip+=1; if x&0x80==0{break;} } } // ref.func
                                0xFC => { for _ in 0..3 { while ip<body.len(){ let x=body[ip]; ip+=1; if x&0x80==0{break;} } } } // bulk memory
                                _ => {}
                            }
                        }
                    }
                }
                0x05 => {
                    // else — we hit this while executing the true branch, so jump past end
                    let mut depth = 1usize;
                    while ip < body.len() {
                        let b = body[ip]; ip += 1;
                        match b {
                            0x02 | 0x03 | 0x04 => {
                                ip += 1; // skip blocktype
                                depth += 1;
                            }
                            0x0B => {
                                depth -= 1;
                                if depth == 0 { break; }
                            }
                            0x0C | 0x0D => { while ip<body.len(){let x=body[ip];ip+=1;if x&0x80==0{break;}} }
                            0x0E => {
                                let mut cnt = 0u32; let mut shift=0u32;
                                loop{let x=body[ip];ip+=1;cnt|=((x&0x7F)as u32)<<shift;shift+=7;if x&0x80==0{break;}}
                                for _ in 0..=cnt { while ip<body.len(){let x=body[ip];ip+=1;if x&0x80==0{break;}} }
                            }
                            0x10 | 0x12 => { while ip<body.len(){let x=body[ip];ip+=1;if x&0x80==0{break;}} }
                            0x11 | 0x13 => { for _ in 0..2{while ip<body.len(){let x=body[ip];ip+=1;if x&0x80==0{break;}}} }
                            0x20..=0x24 => { while ip<body.len(){let x=body[ip];ip+=1;if x&0x80==0{break;}} }
                            0x41 => { while ip<body.len(){let x=body[ip];ip+=1;if x&0x80==0{break;}} }
                            0x42 => { while ip<body.len(){let x=body[ip];ip+=1;if x&0x80==0{break;}} }
                            0x43 => { ip+=4; }
                            0x44 => { ip+=8; }
                            0x28..=0x3E => { for _ in 0..2{while ip<body.len(){let x=body[ip];ip+=1;if x&0x80==0{break;}}} }
                            0x3F | 0x40 => { while ip<body.len(){let x=body[ip];ip+=1;if x&0x80==0{break;}} }
                            0xD0 => { ip += 1; }
                            0xD2 => { while ip<body.len(){let x=body[ip];ip+=1;if x&0x80==0{break;}} }
                            0xFC => { for _ in 0..3 { while ip<body.len(){let x=body[ip];ip+=1;if x&0x80==0{break;}} } }
                            _ => {}
                        }
                    }
                    ctrl_stack.pop(); // pop if frame
                }
                0x08 => {
                    // throw tag_idx
                    let tag_idx = read_u32_leb!();
                    let tag = module.get_tag(tag_idx).ok_or(WasmTrap::TypeMismatch)?;
                    let param_count = module.types.get(tag.type_idx as usize)
                        .map(|ft| ft.params.len())
                        .unwrap_or(0);
                    let mut vals = Vec::with_capacity(param_count);
                    for _ in 0..param_count {
                        vals.push(pop!());
                    }
                    vals.reverse();

                    // Search for enclosing try_table with matching catch clause
                    let mut handled_label = None;
                    let mut push_vals = Vec::new();
                    for frame in ctrl_stack.iter().rev() {
                        if let ControlKind::TryTable(clauses) = &frame.kind {
                            for clause in clauses {
                                match clause {
                                    CatchClause::Catch(t, depth) | CatchClause::CatchRef(t, depth) if *t == tag_idx => {
                                        handled_label = Some(*depth);
                                        push_vals = vals.clone();
                                        break;
                                    }
                                    CatchClause::CatchAll(depth) | CatchClause::CatchAllRef(depth) => {
                                        handled_label = Some(*depth);
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                            if handled_label.is_some() {
                                break;
                            }
                        }
                    }

                    if let Some(target_depth) = handled_label {
                        for v in push_vals {
                            stack.push(v);
                        }
                        let new_ip = do_branch(&mut stack, &mut ctrl_stack, target_depth, ip, body)?;
                        if new_ip >= body.len() { break; }
                        ip = new_ip;
                    } else {
                        return Err(WasmTrap::Exception { tag_idx, values: vals });
                    }
                }
                0x0A => {
                    // throw_ref
                    let _exn = pop!();
                    return Err(WasmTrap::TypeMismatch);
                }
                0x0B => {
                    // end
                    if ctrl_stack.len() == 1 {
                        break; // end of function
                    }
                    let frame = ctrl_stack.pop().unwrap();
                    // Keep top `result_arity` values, discard rest
                    let arity = frame.result_arity;
                    let results: Vec<WasmVal> = stack.iter().rev().take(arity).cloned().collect::<Vec<_>>().into_iter().rev().collect();
                    stack.truncate(frame.stack_height);
                    stack.extend(results);
                }
                0x0C => {
                    // br label_depth
                    let label_depth = read_u32_leb!();
                    let new_ip = do_branch(&mut stack, &mut ctrl_stack, label_depth, ip, body)?;
                    if new_ip >= body.len() { break; }
                    ip = new_ip;
                }
                0x0D => {
                    // br_if label_depth
                    let label_depth = read_u32_leb!();
                    let cond = pop_i32!();
                    if cond != 0 {
                        let new_ip = do_branch(&mut stack, &mut ctrl_stack, label_depth, ip, body)?;
                        if new_ip >= body.len() { break; }
                        ip = new_ip;
                    }
                }
                0x0E => {
                    // br_table targets* default
                    let count = read_u32_leb!();
                    let mut targets: Vec<u32> = Vec::with_capacity(count as usize + 1);
                    for _ in 0..=count {
                        targets.push(read_u32_leb!());
                    }
                    let idx = pop_i32!() as u32;
                    let label_depth = if (idx as usize) < targets.len() - 1 {
                        targets[idx as usize]
                    } else {
                        *targets.last().unwrap()
                    };
                    let new_ip = do_branch(&mut stack, &mut ctrl_stack, label_depth, ip, body)?;
                    if new_ip >= body.len() { break; }
                    ip = new_ip;
                }
                0x0F => {
                    // return
                    break;
                }
                0x10 => {
                    // call func_idx
                    let callee_idx = read_u32_leb!();
                    let callee_type = module.func_type(callee_idx).ok_or(WasmTrap::FunctionNotFound(callee_idx))?;
                    let param_count = callee_type.params.len();
                    let result_count = callee_type.results.len();
                    let args: Vec<WasmVal> = stack.iter().rev().take(param_count).cloned().collect::<Vec<_>>().into_iter().rev().collect();
                    for _ in 0..param_count { stack.pop(); }
                    let results = Self::call_func(module, instance, callee_idx, args, depth + 1)?;
                    for _ in 0..result_count.saturating_sub(results.len()) {
                        stack.push(WasmVal::I32(0));
                    }
                    for r in results { stack.push(r); }
                }
                0x11 => {
                    // call_indirect type_idx table_idx
                    let type_idx = read_u32_leb!();
                    let _table_idx = read_u32_leb!();
                    let elem_idx = pop_i32!() as u32;
                    let expected_type = module.types.get(type_idx as usize).ok_or(WasmTrap::TypeMismatch)?;
                    let func_idx = instance.table_get(0, elem_idx)?;
                    let actual_type = module.func_type(func_idx).ok_or(WasmTrap::TypeMismatch)?;
                    if actual_type.params.len() != expected_type.params.len()
                        || actual_type.results.len() != expected_type.results.len()
                    {
                        return Err(WasmTrap::TypeMismatch);
                    }
                    let param_count = expected_type.params.len();
                    let result_count = expected_type.results.len();
                    let args: Vec<WasmVal> = stack.iter().rev().take(param_count).cloned().collect::<Vec<_>>().into_iter().rev().collect();
                    for _ in 0..param_count { stack.pop(); }
                    let results = Self::call_func(module, instance, func_idx, args, depth + 1)?;
                    for r in results { stack.push(r); }
                    let _ = result_count;
                }
                0x12 => {
                    // return_call func_idx
                    let callee_idx = read_u32_leb!();
                    let imported_count = module.imported_func_count() as u32;
                    let target_type = module.func_type(callee_idx).ok_or(WasmTrap::FunctionNotFound(callee_idx))?;
                    let param_count = target_type.params.len();
                    let next_args: Vec<WasmVal> = stack.iter().rev().take(param_count).cloned().collect::<Vec<_>>().into_iter().rev().collect();
                    for _ in 0..param_count { stack.pop(); }

                    if callee_idx < imported_count {
                        return instance.call_imported(callee_idx, next_args);
                    }

                    let local_idx = (callee_idx - imported_count) as usize;
                    code = module.code.get(local_idx).ok_or(WasmTrap::FunctionNotFound(callee_idx))?;
                    let type_idx = module.functions.get(local_idx).copied().ok_or(WasmTrap::FunctionNotFound(callee_idx))? as usize;
                    func_type = module.types.get(type_idx).ok_or(WasmTrap::FunctionNotFound(callee_idx))?;
                    args = next_args;
                    continue 'tail_call;
                }
                0x13 => {
                    // return_call_indirect type_idx table_idx
                    let type_idx = read_u32_leb!();
                    let table_idx = read_u32_leb!();
                    let elem_idx = pop_i32!() as u32;
                    let expected_type = module.types.get(type_idx as usize).ok_or(WasmTrap::TypeMismatch)?;
                    let func_idx = instance.table_get(table_idx as usize, elem_idx)?;
                    let actual_type = module.func_type(func_idx).ok_or(WasmTrap::TypeMismatch)?;
                    if actual_type.params.len() != expected_type.params.len()
                        || actual_type.results.len() != expected_type.results.len()
                    {
                        return Err(WasmTrap::TypeMismatch);
                    }
                    let param_count = expected_type.params.len();
                    let next_args: Vec<WasmVal> = stack.iter().rev().take(param_count).cloned().collect::<Vec<_>>().into_iter().rev().collect();
                    for _ in 0..param_count { stack.pop(); }

                    let imported_count = module.imported_func_count() as u32;
                    if func_idx < imported_count {
                        return instance.call_imported(func_idx, next_args);
                    }

                    let local_idx = (func_idx - imported_count) as usize;
                    code = module.code.get(local_idx).ok_or(WasmTrap::FunctionNotFound(func_idx))?;
                    let actual_type_idx = module.functions.get(local_idx).copied().ok_or(WasmTrap::FunctionNotFound(func_idx))? as usize;
                    func_type = module.types.get(actual_type_idx).ok_or(WasmTrap::FunctionNotFound(func_idx))?;
                    args = next_args;
                    continue 'tail_call;
                }

                // ── Parametric ───────────────────────────────────────────
                0x1A => { pop!(); } // drop
                0x1B => {
                    // select
                    let cond = pop_i32!();
                    let b = pop!();
                    let a = pop!();
                    stack.push(if cond != 0 { a } else { b });
                }
                0x1F => {
                    // try_table bt clause_count [catch_clauses...]
                    let bt = body.get(ip).copied().ok_or(WasmTrap::Interrupted)?;
                    ip += 1;
                    let arity = if bt == 0x40 { 0 } else { 1 };
                    let clause_count = read_u32_leb!();
                    let mut clauses = Vec::with_capacity(clause_count as usize);
                    for _ in 0..clause_count {
                        let kind = body.get(ip).copied().ok_or(WasmTrap::Interrupted)?;
                        ip += 1;
                        match kind {
                            0x00 => {
                                let tag_idx = read_u32_leb!();
                                let depth = read_u32_leb!();
                                clauses.push(CatchClause::Catch(tag_idx, depth));
                            }
                            0x01 => {
                                let tag_idx = read_u32_leb!();
                                let depth = read_u32_leb!();
                                clauses.push(CatchClause::CatchRef(tag_idx, depth));
                            }
                            0x02 => {
                                let depth = read_u32_leb!();
                                clauses.push(CatchClause::CatchAll(depth));
                            }
                            0x03 => {
                                let depth = read_u32_leb!();
                                clauses.push(CatchClause::CatchAllRef(depth));
                            }
                            _ => return Err(WasmTrap::UnknownOpcode(kind)),
                        }
                    }
                    ctrl_stack.push(ControlFrame {
                        kind: ControlKind::TryTable(clauses),
                        result_arity: arity,
                        stack_height: stack.len(),
                        loop_start_ip: 0,
                    });
                }

                // ── Local / Global ───────────────────────────────────────
                0x20 => {
                    let idx = read_u32_leb!() as usize;
                    let val = locals.get(idx).cloned().ok_or(WasmTrap::TypeMismatch)?;
                    stack.push(val);
                }
                0x21 => {
                    let idx = read_u32_leb!() as usize;
                    let val = pop!();
                    if idx < locals.len() { locals[idx] = val; } else { return Err(WasmTrap::TypeMismatch); }
                }
                0x22 => {
                    let idx = read_u32_leb!() as usize;
                    let val = stack.last().cloned().ok_or(WasmTrap::TypeMismatch)?;
                    if idx < locals.len() { locals[idx] = val; } else { return Err(WasmTrap::TypeMismatch); }
                }
                0x23 => {
                    let idx = read_u32_leb!() as usize;
                    let val = instance.global_get(idx)?;
                    stack.push(val);
                }
                0x24 => {
                    let idx = read_u32_leb!() as usize;
                    let val = pop!();
                    instance.global_set(idx, val)?;
                }

                // ── Memory ───────────────────────────────────────────────
                // i32.load
                0x28 => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_i32(0, addr)?;
                    stack.push(WasmVal::I32(v));
                }
                // i64.load
                0x29 => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_i64(0, addr)?;
                    stack.push(WasmVal::I64(v));
                }
                // f32.load
                0x2A => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_f32(0, addr)?;
                    stack.push(WasmVal::F32(v));
                }
                // f64.load
                0x2B => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_f64(0, addr)?;
                    stack.push(WasmVal::F64(v));
                }
                // i32.load8_s
                0x2C => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_u8(0, addr)? as i8 as i32;
                    stack.push(WasmVal::I32(v));
                }
                // i32.load8_u
                0x2D => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_u8(0, addr)? as i32;
                    stack.push(WasmVal::I32(v));
                }
                // i32.load16_s
                0x2E => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_u16(0, addr)? as i16 as i32;
                    stack.push(WasmVal::I32(v));
                }
                // i32.load16_u
                0x2F => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_u16(0, addr)? as i32;
                    stack.push(WasmVal::I32(v));
                }
                // i64.load8_s
                0x30 => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_u8(0, addr)? as i8 as i64;
                    stack.push(WasmVal::I64(v));
                }
                // i64.load8_u
                0x31 => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_u8(0, addr)? as i64;
                    stack.push(WasmVal::I64(v));
                }
                // i64.load16_s
                0x32 => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_u16(0, addr)? as i16 as i64;
                    stack.push(WasmVal::I64(v));
                }
                // i64.load16_u
                0x33 => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_u16(0, addr)? as u64;
                    stack.push(WasmVal::I64(v as i64));
                }
                // i64.load32_s
                0x34 => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_i32(0, addr)? as i64;
                    stack.push(WasmVal::I64(v));
                }
                // i64.load32_u
                0x35 => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    let v = instance.mem_load_i32(0, addr)? as u32 as i64;
                    stack.push(WasmVal::I64(v));
                }
                // i32.store
                0x36 => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let v = pop_i32!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    instance.mem_store_i32(0, addr, v)?;
                }
                // i64.store
                0x37 => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let v = pop_i64!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    instance.mem_store_i64(0, addr, v)?;
                }
                // f32.store
                0x38 => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let v = pop_f32!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    instance.mem_store_f32(0, addr, v)?;
                }
                // f64.store
                0x39 => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let v = pop_f64!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    instance.mem_store_f64(0, addr, v)?;
                }
                // i32.store8
                0x3A => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let v = pop_i32!() as u8;
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    instance.mem_store_u8(0, addr, v)?;
                }
                // i32.store16
                0x3B => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let v = pop_i32!() as u16;
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    instance.mem_store_u16(0, addr, v)?;
                }
                // i64.store8
                0x3C => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let v = pop_i64!() as u8;
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    instance.mem_store_u8(0, addr, v)?;
                }
                // i64.store16
                0x3D => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let v = pop_i64!() as u16;
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    instance.mem_store_u16(0, addr, v)?;
                }
                // i64.store32
                0x3E => {
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let v = pop_i64!() as u32 as i32;
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    instance.mem_store_i32(0, addr, v)?;
                }
                0x3F => {
                    // memory.size
                    let mem_idx = read_u32_leb!() as usize;
                    let is_m64 = instance.memories.get(mem_idx).map(|m| m.is_memory64).unwrap_or(false);
                    if is_m64 {
                        let pages = instance.mem_size_pages_64(mem_idx);
                        stack.push(WasmVal::I64(pages as i64));
                    } else {
                        let pages = instance.mem_size_pages(mem_idx);
                        stack.push(WasmVal::I32(pages as i32));
                    }
                }
                0x40 => {
                    // memory.grow
                    let mem_idx = read_u32_leb!() as usize;
                    let is_m64 = instance.memories.get(mem_idx).map(|m| m.is_memory64).unwrap_or(false);
                    if is_m64 {
                        let delta = match stack.pop().ok_or(WasmTrap::TypeMismatch)? {
                            WasmVal::I64(v) => v as u64,
                            WasmVal::I32(v) => v as u32 as u64,
                            _ => return Err(WasmTrap::TypeMismatch),
                        };
                        let old = instance.mem_grow_64(mem_idx, delta);
                        stack.push(WasmVal::I64(old as i64));
                    } else {
                        let delta = pop_i32!() as u32;
                        let old = instance.mem_grow(mem_idx, delta);
                        stack.push(WasmVal::I32(old as i32));
                    }
                }

                // ── Numeric Constants ─────────────────────────────────────
                0x41 => {
                    let v = {
                        let mut result = 0i32; let mut shift = 0u32;
                        loop {
                            let b = *body.get(ip).ok_or(WasmTrap::Interrupted)?; ip+=1;
                            result |= ((b & 0x7F) as i32) << shift; shift+=7;
                            if b & 0x80 == 0 { if shift<32 && (b&0x40)!=0 { result |= !0i32<<shift; } break; }
                        }
                        result
                    };
                    stack.push(WasmVal::I32(v));
                }
                0x42 => {
                    let v = read_i64_leb!();
                    stack.push(WasmVal::I64(v));
                }
                0x43 => {
                    // f32.const
                    let bytes = [
                        *body.get(ip).ok_or(WasmTrap::Interrupted)?,
                        *body.get(ip+1).ok_or(WasmTrap::Interrupted)?,
                        *body.get(ip+2).ok_or(WasmTrap::Interrupted)?,
                        *body.get(ip+3).ok_or(WasmTrap::Interrupted)?,
                    ];
                    ip += 4;
                    stack.push(WasmVal::F32(f32::from_le_bytes(bytes)));
                }
                0x44 => {
                    // f64.const
                    let bytes = [
                        *body.get(ip).ok_or(WasmTrap::Interrupted)?,
                        *body.get(ip+1).ok_or(WasmTrap::Interrupted)?,
                        *body.get(ip+2).ok_or(WasmTrap::Interrupted)?,
                        *body.get(ip+3).ok_or(WasmTrap::Interrupted)?,
                        *body.get(ip+4).ok_or(WasmTrap::Interrupted)?,
                        *body.get(ip+5).ok_or(WasmTrap::Interrupted)?,
                        *body.get(ip+6).ok_or(WasmTrap::Interrupted)?,
                        *body.get(ip+7).ok_or(WasmTrap::Interrupted)?,
                    ];
                    ip += 8;
                    stack.push(WasmVal::F64(f64::from_le_bytes(bytes)));
                }

                // ── i32 comparison ────────────────────────────────────────
                0x45 => { let v = pop_i32!(); stack.push(WasmVal::I32(if v == 0 { 1 } else { 0 })); } // i32.eqz
                0x46 => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(if a == b { 1 } else { 0 })); } // i32.eq
                0x47 => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(if a != b { 1 } else { 0 })); } // i32.ne
                0x48 => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(if a < b { 1 } else { 0 })); } // i32.lt_s
                0x49 => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(if (a as u32) < (b as u32) { 1 } else { 0 })); } // i32.lt_u
                0x4A => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(if a > b { 1 } else { 0 })); } // i32.gt_s
                0x4B => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(if (a as u32) > (b as u32) { 1 } else { 0 })); } // i32.gt_u
                0x4C => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(if a <= b { 1 } else { 0 })); } // i32.le_s
                0x4D => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(if (a as u32) <= (b as u32) { 1 } else { 0 })); } // i32.le_u
                0x4E => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(if a >= b { 1 } else { 0 })); } // i32.ge_s
                0x4F => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(if (a as u32) >= (b as u32) { 1 } else { 0 })); } // i32.ge_u

                // ── i64 comparison ────────────────────────────────────────
                0x50 => { let v = pop_i64!(); stack.push(WasmVal::I32(if v == 0 { 1 } else { 0 })); } // i64.eqz
                0x51 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I32(if a == b { 1 } else { 0 })); }
                0x52 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I32(if a != b { 1 } else { 0 })); }
                0x53 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I32(if a < b { 1 } else { 0 })); }
                0x54 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I32(if (a as u64) < (b as u64) { 1 } else { 0 })); }
                0x55 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I32(if a > b { 1 } else { 0 })); }
                0x56 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I32(if (a as u64) > (b as u64) { 1 } else { 0 })); }
                0x57 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I32(if a <= b { 1 } else { 0 })); }
                0x58 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I32(if (a as u64) <= (b as u64) { 1 } else { 0 })); }
                0x59 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I32(if a >= b { 1 } else { 0 })); }
                0x5A => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I32(if (a as u64) >= (b as u64) { 1 } else { 0 })); }

                // ── f32 comparison ────────────────────────────────────────
                0x5B => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::I32(if a == b { 1 } else { 0 })); }
                0x5C => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::I32(if a != b { 1 } else { 0 })); }
                0x5D => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::I32(if a < b { 1 } else { 0 })); }
                0x5E => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::I32(if a > b { 1 } else { 0 })); }
                0x5F => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::I32(if a <= b { 1 } else { 0 })); }
                0x60 => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::I32(if a >= b { 1 } else { 0 })); }

                // ── f64 comparison ────────────────────────────────────────
                0x61 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::I32(if a == b { 1 } else { 0 })); }
                0x62 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::I32(if a != b { 1 } else { 0 })); }
                0x63 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::I32(if a < b { 1 } else { 0 })); }
                0x64 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::I32(if a > b { 1 } else { 0 })); }
                0x65 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::I32(if a <= b { 1 } else { 0 })); }
                0x66 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::I32(if a >= b { 1 } else { 0 })); }

                // ── i32 arithmetic ────────────────────────────────────────
                0x67 => { let v = pop_i32!(); stack.push(WasmVal::I32(v.leading_zeros() as i32)); } // clz
                0x68 => { let v = pop_i32!(); stack.push(WasmVal::I32(v.trailing_zeros() as i32)); } // ctz
                0x69 => { let v = pop_i32!(); stack.push(WasmVal::I32(v.count_ones() as i32)); } // popcnt
                0x6A => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(a.wrapping_add(b))); }
                0x6B => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(a.wrapping_sub(b))); }
                0x6C => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(a.wrapping_mul(b))); }
                0x6D => { // i32.div_s
                    let b = pop_i32!(); let a = pop_i32!();
                    if b == 0 { return Err(WasmTrap::DivisionByZero); }
                    if a == i32::MIN && b == -1 { return Err(WasmTrap::IntegerOverflow); }
                    stack.push(WasmVal::I32(a / b));
                }
                0x6E => { // i32.div_u
                    let b = pop_i32!() as u32; let a = pop_i32!() as u32;
                    if b == 0 { return Err(WasmTrap::DivisionByZero); }
                    stack.push(WasmVal::I32((a / b) as i32));
                }
                0x6F => { // i32.rem_s
                    let b = pop_i32!(); let a = pop_i32!();
                    if b == 0 { return Err(WasmTrap::DivisionByZero); }
                    stack.push(WasmVal::I32(a.wrapping_rem(b)));
                }
                0x70 => { // i32.rem_u
                    let b = pop_i32!() as u32; let a = pop_i32!() as u32;
                    if b == 0 { return Err(WasmTrap::DivisionByZero); }
                    stack.push(WasmVal::I32((a % b) as i32));
                }
                0x71 => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(a & b)); }
                0x72 => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(a | b)); }
                0x73 => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(a ^ b)); }
                0x74 => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(a.wrapping_shl(b as u32))); }
                0x75 => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(a.wrapping_shr(b as u32))); }
                0x76 => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(((a as u32).wrapping_shr(b as u32)) as i32)); }
                0x77 => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(a.rotate_left(b as u32))); }
                0x78 => { let b = pop_i32!(); let a = pop_i32!(); stack.push(WasmVal::I32(a.rotate_right(b as u32))); }

                // ── i64 arithmetic ────────────────────────────────────────
                0x79 => { let v = pop_i64!(); stack.push(WasmVal::I64(v.leading_zeros() as i64)); }
                0x7A => { let v = pop_i64!(); stack.push(WasmVal::I64(v.trailing_zeros() as i64)); }
                0x7B => { let v = pop_i64!(); stack.push(WasmVal::I64(v.count_ones() as i64)); }
                0x7C => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I64(a.wrapping_add(b))); }
                0x7D => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I64(a.wrapping_sub(b))); }
                0x7E => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I64(a.wrapping_mul(b))); }
                0x7F => {
                    let b = pop_i64!(); let a = pop_i64!();
                    if b == 0 { return Err(WasmTrap::DivisionByZero); }
                    if a == i64::MIN && b == -1 { return Err(WasmTrap::IntegerOverflow); }
                    stack.push(WasmVal::I64(a / b));
                }
                0x80 => {
                    let b = pop_i64!() as u64; let a = pop_i64!() as u64;
                    if b == 0 { return Err(WasmTrap::DivisionByZero); }
                    stack.push(WasmVal::I64((a / b) as i64));
                }
                0x81 => {
                    let b = pop_i64!(); let a = pop_i64!();
                    if b == 0 { return Err(WasmTrap::DivisionByZero); }
                    stack.push(WasmVal::I64(a.wrapping_rem(b)));
                }
                0x82 => {
                    let b = pop_i64!() as u64; let a = pop_i64!() as u64;
                    if b == 0 { return Err(WasmTrap::DivisionByZero); }
                    stack.push(WasmVal::I64((a % b) as i64));
                }
                0x83 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I64(a & b)); }
                0x84 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I64(a | b)); }
                0x85 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I64(a ^ b)); }
                0x86 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I64(a.wrapping_shl(b as u32))); }
                0x87 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I64(a.wrapping_shr(b as u32))); }
                0x88 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I64(((a as u64).wrapping_shr(b as u32)) as i64)); }
                0x89 => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I64(a.rotate_left(b as u32))); }
                0x8A => { let b = pop_i64!(); let a = pop_i64!(); stack.push(WasmVal::I64(a.rotate_right(b as u32))); }

                // ── f32 arithmetic ────────────────────────────────────────
                0x8B => { let v = pop_f32!(); stack.push(WasmVal::F32(v.abs())); }
                0x8C => { let v = pop_f32!(); stack.push(WasmVal::F32(-v)); }
                0x8D => { let v = pop_f32!(); stack.push(WasmVal::F32(v.ceil())); }
                0x8E => { let v = pop_f32!(); stack.push(WasmVal::F32(v.floor())); }
                0x8F => { let v = pop_f32!(); stack.push(WasmVal::F32(v.trunc())); }
                0x90 => { let v = pop_f32!(); stack.push(WasmVal::F32((v + 0.5).floor())); } // nearest (bankers rounding approx)
                0x91 => { let v = pop_f32!(); stack.push(WasmVal::F32(v.sqrt())); }
                0x92 => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::F32(a + b)); }
                0x93 => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::F32(a - b)); }
                0x94 => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::F32(a * b)); }
                0x95 => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::F32(a / b)); }
                0x96 => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::F32(a.min(b))); }
                0x97 => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::F32(a.max(b))); }
                0x98 => { let b = pop_f32!(); let a = pop_f32!(); stack.push(WasmVal::F32(a.copysign(b))); }

                // ── f64 arithmetic ────────────────────────────────────────
                0x99 => { let v = pop_f64!(); stack.push(WasmVal::F64(v.abs())); }
                0x9A => { let v = pop_f64!(); stack.push(WasmVal::F64(-v)); }
                0x9B => { let v = pop_f64!(); stack.push(WasmVal::F64(v.ceil())); }
                0x9C => { let v = pop_f64!(); stack.push(WasmVal::F64(v.floor())); }
                0x9D => { let v = pop_f64!(); stack.push(WasmVal::F64(v.trunc())); }
                0x9E => { let v = pop_f64!(); stack.push(WasmVal::F64((v + 0.5).floor())); }
                0x9F => { let v = pop_f64!(); stack.push(WasmVal::F64(v.sqrt())); }
                0xA0 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::F64(a + b)); }
                0xA1 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::F64(a - b)); }
                0xA2 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::F64(a * b)); }
                0xA3 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::F64(a / b)); }
                0xA4 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::F64(a.min(b))); }
                0xA5 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::F64(a.max(b))); }
                0xA6 => { let b = pop_f64!(); let a = pop_f64!(); stack.push(WasmVal::F64(a.copysign(b))); }

                // ── Type conversions ──────────────────────────────────────
                0xA7 => { let v = pop_i64!(); stack.push(WasmVal::I32(v as i32)); } // i32.wrap_i64
                0xA8 => { let v = pop_f32!(); stack.push(WasmVal::I32(v.trunc() as i32)); } // i32.trunc_f32_s
                0xA9 => { let v = pop_f32!(); stack.push(WasmVal::I32(v.trunc() as u32 as i32)); } // i32.trunc_f32_u
                0xAA => { let v = pop_f64!(); stack.push(WasmVal::I32(v.trunc() as i32)); } // i32.trunc_f64_s
                0xAB => { let v = pop_f64!(); stack.push(WasmVal::I32(v.trunc() as u32 as i32)); } // i32.trunc_f64_u
                0xAC => { let v = pop_i32!(); stack.push(WasmVal::I64(v as i64)); } // i64.extend_i32_s
                0xAD => { let v = pop_i32!(); stack.push(WasmVal::I64(v as u32 as i64)); } // i64.extend_i32_u
                0xAE => { let v = pop_f32!(); stack.push(WasmVal::I64(v.trunc() as i64)); } // i64.trunc_f32_s
                0xAF => { let v = pop_f32!(); stack.push(WasmVal::I64(v.trunc() as u64 as i64)); } // i64.trunc_f32_u
                0xB0 => { let v = pop_f64!(); stack.push(WasmVal::I64(v.trunc() as i64)); } // i64.trunc_f64_s
                0xB1 => { let v = pop_f64!(); stack.push(WasmVal::I64(v.trunc() as u64 as i64)); } // i64.trunc_f64_u
                0xB2 => { let v = pop_i32!(); stack.push(WasmVal::F32(v as f32)); } // f32.convert_i32_s
                0xB3 => { let v = pop_i32!(); stack.push(WasmVal::F32(v as u32 as f32)); } // f32.convert_i32_u
                0xB4 => { let v = pop_i64!(); stack.push(WasmVal::F32(v as f32)); } // f32.convert_i64_s
                0xB5 => { let v = pop_i64!(); stack.push(WasmVal::F32(v as u64 as f32)); } // f32.convert_i64_u
                0xB6 => { let v = pop_f64!(); stack.push(WasmVal::F32(v as f32)); } // f32.demote_f64
                0xB7 => { let v = pop_i32!(); stack.push(WasmVal::F64(v as f64)); } // f64.convert_i32_s
                0xB8 => { let v = pop_i32!(); stack.push(WasmVal::F64(v as u32 as f64)); } // f64.convert_i32_u
                0xB9 => { let v = pop_i64!(); stack.push(WasmVal::F64(v as f64)); } // f64.convert_i64_s
                0xBA => { let v = pop_i64!(); stack.push(WasmVal::F64(v as u64 as f64)); } // f64.convert_i64_u
                0xBB => { let v = pop_f32!(); stack.push(WasmVal::F64(v as f64)); } // f64.promote_f32
                0xBC => { let v = pop_i32!(); stack.push(WasmVal::F32(f32::from_bits(v as u32))); } // i32.reinterpret_f32
                0xBD => { let v = pop_i64!(); stack.push(WasmVal::F64(f64::from_bits(v as u64))); } // i64.reinterpret_f64
                0xBE => { let v = pop_f32!(); stack.push(WasmVal::I32(v.to_bits() as i32)); } // f32.reinterpret_i32
                0xBF => { let v = pop_f64!(); stack.push(WasmVal::I64(v.to_bits() as i64)); } // f64.reinterpret_i64

                // ── WebAssembly 128-Bit SIMD Extensions (0xFD) ───────────────
                0xFD => {
                    let simd_op = read_u32_leb!();
                    match simd_op {
                        // v128.load: memarg (align, offset)
                        0x00 => {
                            let _align = read_u32_leb!();
                            let offset = read_u64_leb!();
                            let base = pop_addr!(0);
                            let addr = base.wrapping_add(offset);
                            let bytes = instance.mem_load_v128(0, addr)?;
                            stack.push(WasmVal::V128(bytes));
                        }
                        // v128.store: memarg (align, offset)
                        0x0B => {
                            let _align = read_u32_leb!();
                            let offset = read_u64_leb!();
                            let val = pop!().as_v128();
                            let base = pop_addr!(0);
                            let addr = base.wrapping_add(offset);
                            instance.mem_store_v128(0, addr, val)?;
                        }
                        // v128.const: 16 immediate bytes
                        0x0C => {
                            if ip + 16 > body.len() { return Err(WasmTrap::OutOfBoundsMemoryAccess { addr: ip as u32, size: 16, mem_size: body.len() as u32 }); }
                            let mut b = [0u8; 16];
                            b.copy_from_slice(&body[ip..ip + 16]);
                            ip += 16;
                            stack.push(WasmVal::V128(b));
                        }
                        // i8x16.splat
                        0x0F => {
                            let v = pop_i32!() as u8;
                            stack.push(WasmVal::V128([v; 16]));
                        }
                        // i16x8.splat
                        0x10 => {
                            let v = pop_i32!() as i16;
                            stack.push(WasmVal::from_i16x8([v; 8]));
                        }
                        // i32x4.splat
                        0x11 => {
                            let v = pop_i32!();
                            stack.push(WasmVal::from_i32x4([v; 4]));
                        }
                        // i64x2.splat
                        0x12 => {
                            let v = pop_i64!();
                            stack.push(WasmVal::from_i64x2([v; 2]));
                        }
                        // f32x4.splat
                        0x13 => {
                            let v = pop_f32!();
                            stack.push(WasmVal::from_f32x4([v; 4]));
                        }
                        // i8x16.extract_lane_s
                        0x15 => {
                            let lane = (body[ip] as usize) & 15; ip += 1;
                            let v = pop!().to_i8x16();
                            stack.push(WasmVal::I32(v[lane] as i32));
                        }
                        // i8x16.extract_lane_u
                        0x16 => {
                            let lane = (body[ip] as usize) & 15; ip += 1;
                            let v = pop!().as_v128();
                            stack.push(WasmVal::I32(v[lane] as i32));
                        }
                        // i8x16.replace_lane
                        0x17 => {
                            let lane = (body[ip] as usize) & 15; ip += 1;
                            let val = pop_i32!() as u8;
                            let mut v = pop!().as_v128();
                            v[lane] = val;
                            stack.push(WasmVal::V128(v));
                        }
                        // i32x4.extract_lane
                        0x1B => {
                            let lane = (body[ip] as usize) & 3; ip += 1;
                            let v = pop!().to_i32x4();
                            stack.push(WasmVal::I32(v[lane]));
                        }
                        // i32x4.replace_lane
                        0x1C => {
                            let lane = (body[ip] as usize) & 3; ip += 1;
                            let val = pop_i32!();
                            let mut v = pop!().to_i32x4();
                            v[lane] = val;
                            stack.push(WasmVal::from_i32x4(v));
                        }
                        // f32x4.extract_lane
                        0x1F => {
                            let lane = (body[ip] as usize) & 3; ip += 1;
                            let v = pop!().to_f32x4();
                            stack.push(WasmVal::F32(v[lane]));
                        }
                        // f32x4.replace_lane
                        0x20 => {
                            let lane = (body[ip] as usize) & 3; ip += 1;
                            let val = pop_f32!();
                            let mut v = pop!().to_f32x4();
                            v[lane] = val;
                            stack.push(WasmVal::from_f32x4(v));
                        }
                        // i32x4.eq
                        0x37 => {
                            let b = pop!().to_i32x4();
                            let a = pop!().to_i32x4();
                            let mut res = [0i32; 4];
                            for i in 0..4 { res[i] = if a[i] == b[i] { -1 } else { 0 }; }
                            stack.push(WasmVal::from_i32x4(res));
                        }
                        // i32x4.lt_s
                        0x39 => {
                            let b = pop!().to_i32x4();
                            let a = pop!().to_i32x4();
                            let mut res = [0i32; 4];
                            for i in 0..4 { res[i] = if a[i] < b[i] { -1 } else { 0 }; }
                            stack.push(WasmVal::from_i32x4(res));
                        }
                        // i32x4.gt_s
                        0x3B => {
                            let b = pop!().to_i32x4();
                            let a = pop!().to_i32x4();
                            let mut res = [0i32; 4];
                            for i in 0..4 { res[i] = if a[i] > b[i] { -1 } else { 0 }; }
                            stack.push(WasmVal::from_i32x4(res));
                        }
                        // v128.not
                        0x4D => {
                            let a = pop!().as_v128();
                            let mut res = [0u8; 16];
                            for i in 0..16 { res[i] = !a[i]; }
                            stack.push(WasmVal::V128(res));
                        }
                        // v128.and
                        0x4E => {
                            let b = pop!().as_v128();
                            let a = pop!().as_v128();
                            let mut res = [0u8; 16];
                            for i in 0..16 { res[i] = a[i] & b[i]; }
                            stack.push(WasmVal::V128(res));
                        }
                        // v128.andnot
                        0x4F => {
                            let b = pop!().as_v128();
                            let a = pop!().as_v128();
                            let mut res = [0u8; 16];
                            for i in 0..16 { res[i] = a[i] & (!b[i]); }
                            stack.push(WasmVal::V128(res));
                        }
                        // v128.or
                        0x50 => {
                            let b = pop!().as_v128();
                            let a = pop!().as_v128();
                            let mut res = [0u8; 16];
                            for i in 0..16 { res[i] = a[i] | b[i]; }
                            stack.push(WasmVal::V128(res));
                        }
                        // v128.xor
                        0x51 => {
                            let b = pop!().as_v128();
                            let a = pop!().as_v128();
                            let mut res = [0u8; 16];
                            for i in 0..16 { res[i] = a[i] ^ b[i]; }
                            stack.push(WasmVal::V128(res));
                        }
                        // v128.bitselect (v1, v2, c) -> (v1 & c) | (v2 & ~c)
                        0x52 => {
                            let c = pop!().as_v128();
                            let v2 = pop!().as_v128();
                            let v1 = pop!().as_v128();
                            let mut res = [0u8; 16];
                            for i in 0..16 { res[i] = (v1[i] & c[i]) | (v2[i] & (!c[i])); }
                            stack.push(WasmVal::V128(res));
                        }
                        // v128.any_true
                        0x53 => {
                            let a = pop!().as_v128();
                            let any = a.iter().any(|&b| b != 0);
                            stack.push(WasmVal::I32(if any { 1 } else { 0 }));
                        }
                        // i8x16.add
                        0x6E => {
                            let b = pop!().to_i8x16();
                            let a = pop!().to_i8x16();
                            let mut res = [0i8; 16];
                            for i in 0..16 { res[i] = a[i].wrapping_add(b[i]); }
                            stack.push(WasmVal::from_i8x16(res));
                        }
                        // i8x16.sub
                        0x71 => {
                            let b = pop!().to_i8x16();
                            let a = pop!().to_i8x16();
                            let mut res = [0i8; 16];
                            for i in 0..16 { res[i] = a[i].wrapping_sub(b[i]); }
                            stack.push(WasmVal::from_i8x16(res));
                        }
                        // i16x8.add
                        0x72 => {
                            let b = pop!().to_i16x8();
                            let a = pop!().to_i16x8();
                            let mut res = [0i16; 8];
                            for i in 0..8 { res[i] = a[i].wrapping_add(b[i]); }
                            stack.push(WasmVal::from_i16x8(res));
                        }
                        // i16x8.sub
                        0x75 => {
                            let b = pop!().to_i16x8();
                            let a = pop!().to_i16x8();
                            let mut res = [0i16; 8];
                            for i in 0..8 { res[i] = a[i].wrapping_sub(b[i]); }
                            stack.push(WasmVal::from_i16x8(res));
                        }
                        // i32x4.add
                        0x76 => {
                            let b = pop!().to_i32x4();
                            let a = pop!().to_i32x4();
                            let mut res = [0i32; 4];
                            for i in 0..4 { res[i] = a[i].wrapping_add(b[i]); }
                            stack.push(WasmVal::from_i32x4(res));
                        }
                        // i32x4.sub
                        0x79 => {
                            let b = pop!().to_i32x4();
                            let a = pop!().to_i32x4();
                            let mut res = [0i32; 4];
                            for i in 0..4 { res[i] = a[i].wrapping_sub(b[i]); }
                            stack.push(WasmVal::from_i32x4(res));
                        }
                        // i64x2.add
                        0x7D => {
                            let b = pop!().to_i64x2();
                            let a = pop!().to_i64x2();
                            let mut res = [0i64; 2];
                            for i in 0..2 { res[i] = a[i].wrapping_add(b[i]); }
                            stack.push(WasmVal::from_i64x2(res));
                        }
                        // i64x2.sub
                        0x80 => {
                            let b = pop!().to_i64x2();
                            let a = pop!().to_i64x2();
                            let mut res = [0i64; 2];
                            for i in 0..2 { res[i] = a[i].wrapping_sub(b[i]); }
                            stack.push(WasmVal::from_i64x2(res));
                        }
                        // i32x4.mul
                        0x8B => {
                            let b = pop!().to_i32x4();
                            let a = pop!().to_i32x4();
                            let mut res = [0i32; 4];
                            for i in 0..4 { res[i] = a[i].wrapping_mul(b[i]); }
                            stack.push(WasmVal::from_i32x4(res));
                        }
                        // f32x4.add
                        0x94 => {
                            let b = pop!().to_f32x4();
                            let a = pop!().to_f32x4();
                            let mut res = [0f32; 4];
                            for i in 0..4 { res[i] = a[i] + b[i]; }
                            stack.push(WasmVal::from_f32x4(res));
                        }
                        // f32x4.sub
                        0x95 => {
                            let b = pop!().to_f32x4();
                            let a = pop!().to_f32x4();
                            let mut res = [0f32; 4];
                            for i in 0..4 { res[i] = a[i] - b[i]; }
                            stack.push(WasmVal::from_f32x4(res));
                        }
                        // f32x4.mul
                        0x96 => {
                            let b = pop!().to_f32x4();
                            let a = pop!().to_f32x4();
                            let mut res = [0f32; 4];
                            for i in 0..4 { res[i] = a[i] * b[i]; }
                            stack.push(WasmVal::from_f32x4(res));
                        }
                        // f32x4.div
                        0x97 => {
                            let b = pop!().to_f32x4();
                            let a = pop!().to_f32x4();
                            let mut res = [0f32; 4];
                            for i in 0..4 { res[i] = a[i] / b[i]; }
                            stack.push(WasmVal::from_f32x4(res));
                        }
                        // f32x4.min
                        0x98 => {
                            let b = pop!().to_f32x4();
                            let a = pop!().to_f32x4();
                            let mut res = [0f32; 4];
                            for i in 0..4 { res[i] = a[i].min(b[i]); }
                            stack.push(WasmVal::from_f32x4(res));
                        }
                        // f32x4.max
                        0x99 => {
                            let b = pop!().to_f32x4();
                            let a = pop!().to_f32x4();
                            let mut res = [0f32; 4];
                            for i in 0..4 { res[i] = a[i].max(b[i]); }
                            stack.push(WasmVal::from_f32x4(res));
                        }
                        other_simd => return Err(WasmTrap::UnknownOpcode(other_simd as u8)),
                    }
                }

                // ── Reference Types ───────────────────────────────────────
                0xD0 => {
                    // ref.null reftype
                    let reftype = body.get(ip).copied().ok_or(WasmTrap::Interrupted)?;
                    ip += 1;
                    let val = if reftype == 0x6F {
                        WasmVal::ExternRef(None)
                    } else {
                        WasmVal::FuncRef(None)
                    };
                    stack.push(val);
                }
                0xD1 => {
                    // ref.is_null
                    let ref_val = pop!();
                    let is_null = match ref_val {
                        WasmVal::FuncRef(None) | WasmVal::ExternRef(None) => 1,
                        _ => 0,
                    };
                    stack.push(WasmVal::I32(is_null));
                }
                0xD2 => {
                    // ref.func func_idx
                    let func_idx = read_u32_leb!();
                    stack.push(WasmVal::FuncRef(Some(func_idx)));
                }

                // ── Bulk Memory / Multi-Memory ─────────────────────────────
                0xFC => {
                    let sub_op = read_u32_leb!();
                    match sub_op {
                        10 => {
                            // memory.copy dst_mem src_mem
                            let dst_mem = read_u32_leb!() as usize;
                            let src_mem = read_u32_leb!() as usize;
                            let len = pop_i32!() as usize;
                            let src = pop_i32!() as usize;
                            let dst = pop_i32!() as usize;
                            if src_mem == dst_mem {
                                let mem = instance.memories.get_mut(src_mem).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: src as u32, size: len as u32, mem_size: 0 })?;
                                if src + len > mem.data.len() || dst + len > mem.data.len() {
                                    return Err(WasmTrap::OutOfBoundsMemoryAccess { addr: dst as u32, size: len as u32, mem_size: mem.data.len() as u32 });
                                }
                                mem.data.copy_within(src..src + len, dst);
                            } else {
                                let src_data = {
                                    let sm = instance.memories.get(src_mem).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: src as u32, size: len as u32, mem_size: 0 })?;
                                    if src + len > sm.data.len() {
                                        return Err(WasmTrap::OutOfBoundsMemoryAccess { addr: src as u32, size: len as u32, mem_size: sm.data.len() as u32 });
                                    }
                                    sm.data[src..src + len].to_vec()
                                };
                                let dm = instance.memories.get_mut(dst_mem).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: dst as u32, size: len as u32, mem_size: 0 })?;
                                if dst + len > dm.data.len() {
                                    return Err(WasmTrap::OutOfBoundsMemoryAccess { addr: dst as u32, size: len as u32, mem_size: dm.data.len() as u32 });
                                }
                                dm.data[dst..dst + len].copy_from_slice(&src_data);
                            }
                        }
                        11 => {
                            // memory.fill mem_idx
                            let mem_idx = read_u32_leb!() as usize;
                            let len = pop_i32!() as usize;
                            let val = pop_i32!() as u8;
                            let dst = pop_i32!() as usize;
                            let mem = instance.memories.get_mut(mem_idx).ok_or(WasmTrap::OutOfBoundsMemoryAccess { addr: dst as u32, size: len as u32, mem_size: 0 })?;
                            if dst + len > mem.data.len() {
                                return Err(WasmTrap::OutOfBoundsMemoryAccess { addr: dst as u32, size: len as u32, mem_size: mem.data.len() as u32 });
                            }
                            mem.data[dst..dst + len].fill(val);
                        }
                        _ => return Err(WasmTrap::UnknownOpcode(0xFC)),
                    }
                }

                // ── Wasm GC Proposals (0xFB) ──────────────────────────────
                0xFB => {
                    let sub_op = read_u32_leb!();
                    match sub_op {
                        // struct.new type_idx
                        0x00 => {
                            let type_idx = read_u32_leb!();
                            let st = module.get_struct_type(type_idx).ok_or(WasmTrap::TypeMismatch)?;
                            let count = st.fields.len();
                            let mut fields = Vec::with_capacity(count);
                            for _ in 0..count {
                                fields.push(pop!());
                            }
                            fields.reverse();
                            let s = WasmStruct { type_idx, fields };
                            stack.push(WasmVal::StructRef(Some(Rc::new(RefCell::new(s)))));
                        }
                        // struct.new_default type_idx
                        0x01 => {
                            let type_idx = read_u32_leb!();
                            let st = module.get_struct_type(type_idx).ok_or(WasmTrap::TypeMismatch)?;
                            let fields = st.fields.iter().map(|f| WasmVal::default_for(&f.val_type)).collect();
                            let s = WasmStruct { type_idx, fields };
                            stack.push(WasmVal::StructRef(Some(Rc::new(RefCell::new(s)))));
                        }
                        // struct.get / struct.get_s / struct.get_u type_idx field_idx
                        0x02 | 0x03 | 0x04 => {
                            let _type_idx = read_u32_leb!();
                            let field_idx = read_u32_leb!() as usize;
                            match pop!() {
                                WasmVal::StructRef(Some(s)) => {
                                    let b = s.borrow();
                                    let v = b.fields.get(field_idx).cloned().ok_or(WasmTrap::TypeMismatch)?;
                                    stack.push(v);
                                }
                                WasmVal::StructRef(None) => return Err(WasmTrap::NullReference),
                                _ => return Err(WasmTrap::TypeMismatch),
                            }
                        }
                        // struct.set type_idx field_idx
                        0x05 => {
                            let _type_idx = read_u32_leb!();
                            let field_idx = read_u32_leb!() as usize;
                            let val = pop!();
                            match pop!() {
                                WasmVal::StructRef(Some(s)) => {
                                    let mut b = s.borrow_mut();
                                    if field_idx < b.fields.len() {
                                        b.fields[field_idx] = val;
                                    } else {
                                        return Err(WasmTrap::TypeMismatch);
                                    }
                                }
                                WasmVal::StructRef(None) => return Err(WasmTrap::NullReference),
                                _ => return Err(WasmTrap::TypeMismatch),
                            }
                        }
                        // array.new type_idx
                        0x06 => {
                            let type_idx = read_u32_leb!();
                            let len = pop_i32!() as usize;
                            let init_val = pop!();
                            let elements = vec![init_val; len];
                            let a = WasmArray { type_idx, elements };
                            stack.push(WasmVal::ArrayRef(Some(Rc::new(RefCell::new(a)))));
                        }
                        // array.new_default type_idx
                        0x07 => {
                            let type_idx = read_u32_leb!();
                            let at = module.get_array_type(type_idx).ok_or(WasmTrap::TypeMismatch)?;
                            let len = pop_i32!() as usize;
                            let def = WasmVal::default_for(&at.elem_type);
                            let elements = vec![def; len];
                            let a = WasmArray { type_idx, elements };
                            stack.push(WasmVal::ArrayRef(Some(Rc::new(RefCell::new(a)))));
                        }
                        // array.get / array.get_s / array.get_u type_idx
                        0x0B | 0x0C | 0x0D => {
                            let _type_idx = read_u32_leb!();
                            let idx = pop_i32!() as usize;
                            match pop!() {
                                WasmVal::ArrayRef(Some(a)) => {
                                    let b = a.borrow();
                                    if idx < b.elements.len() {
                                        stack.push(b.elements[idx].clone());
                                    } else {
                                        return Err(WasmTrap::OutOfBoundsMemoryAccess { addr: idx as u32, size: 1, mem_size: b.elements.len() as u32 });
                                    }
                                }
                                WasmVal::ArrayRef(None) => return Err(WasmTrap::NullReference),
                                _ => return Err(WasmTrap::TypeMismatch),
                            }
                        }
                        // array.set type_idx
                        0x0E => {
                            let _type_idx = read_u32_leb!();
                            let val = pop!();
                            let idx = pop_i32!() as usize;
                            match pop!() {
                                WasmVal::ArrayRef(Some(a)) => {
                                    let mut b = a.borrow_mut();
                                    if idx < b.elements.len() {
                                        b.elements[idx] = val;
                                    } else {
                                        return Err(WasmTrap::OutOfBoundsMemoryAccess { addr: idx as u32, size: 1, mem_size: b.elements.len() as u32 });
                                    }
                                }
                                WasmVal::ArrayRef(None) => return Err(WasmTrap::NullReference),
                                _ => return Err(WasmTrap::TypeMismatch),
                            }
                        }
                        // array.len
                        0x0F => {
                            match pop!() {
                                WasmVal::ArrayRef(Some(a)) => {
                                    let len = a.borrow().elements.len() as i32;
                                    stack.push(WasmVal::I32(len));
                                }
                                WasmVal::ArrayRef(None) => return Err(WasmTrap::NullReference),
                                _ => return Err(WasmTrap::TypeMismatch),
                            }
                        }
                        // ref.test heap_type
                        0x14 => {
                            let heap_type = read_i32_leb!();
                            let val = pop!();
                            let matches = match &val {
                                WasmVal::I31(_) => heap_type == -17 || heap_type == -18 || heap_type == -19,
                                WasmVal::StructRef(Some(s)) => {
                                    heap_type == -17 || heap_type == -18 || heap_type == -20 || (heap_type >= 0 && s.borrow().type_idx == heap_type as u32)
                                }
                                WasmVal::ArrayRef(Some(a)) => {
                                    heap_type == -17 || heap_type == -18 || heap_type == -21 || (heap_type >= 0 && a.borrow().type_idx == heap_type as u32)
                                }
                                _ => false,
                            };
                            stack.push(WasmVal::I32(if matches { 1 } else { 0 }));
                        }
                        // ref.test_null heap_type
                        0x15 => {
                            let heap_type = read_i32_leb!();
                            let val = pop!();
                            let matches = match &val {
                                WasmVal::StructRef(None) | WasmVal::ArrayRef(None) | WasmVal::FuncRef(None) | WasmVal::ExternRef(None) => true,
                                WasmVal::I31(_) => heap_type == -17 || heap_type == -18 || heap_type == -19,
                                WasmVal::StructRef(Some(s)) => {
                                    heap_type == -17 || heap_type == -18 || heap_type == -20 || (heap_type >= 0 && s.borrow().type_idx == heap_type as u32)
                                }
                                WasmVal::ArrayRef(Some(a)) => {
                                    heap_type == -17 || heap_type == -18 || heap_type == -21 || (heap_type >= 0 && a.borrow().type_idx == heap_type as u32)
                                }
                                _ => false,
                            };
                            stack.push(WasmVal::I32(if matches { 1 } else { 0 }));
                        }
                        // ref.cast heap_type
                        0x16 => {
                            let heap_type = read_i32_leb!();
                            let val = pop!();
                            let matches = match &val {
                                WasmVal::I31(_) => heap_type == -17 || heap_type == -18 || heap_type == -19,
                                WasmVal::StructRef(Some(s)) => {
                                    heap_type == -17 || heap_type == -18 || heap_type == -20 || (heap_type >= 0 && s.borrow().type_idx == heap_type as u32)
                                }
                                WasmVal::ArrayRef(Some(a)) => {
                                    heap_type == -17 || heap_type == -18 || heap_type == -21 || (heap_type >= 0 && a.borrow().type_idx == heap_type as u32)
                                }
                                _ => false,
                            };
                            if matches {
                                stack.push(val);
                            } else {
                                return Err(WasmTrap::TypeMismatch);
                            }
                        }
                        // ref.cast_null heap_type
                        0x17 => {
                            let heap_type = read_i32_leb!();
                            let val = pop!();
                            let matches = match &val {
                                WasmVal::StructRef(None) | WasmVal::ArrayRef(None) | WasmVal::FuncRef(None) | WasmVal::ExternRef(None) => true,
                                WasmVal::I31(_) => heap_type == -17 || heap_type == -18 || heap_type == -19,
                                WasmVal::StructRef(Some(s)) => {
                                    heap_type == -17 || heap_type == -18 || heap_type == -20 || (heap_type >= 0 && s.borrow().type_idx == heap_type as u32)
                                }
                                WasmVal::ArrayRef(Some(a)) => {
                                    heap_type == -17 || heap_type == -18 || heap_type == -21 || (heap_type >= 0 && a.borrow().type_idx == heap_type as u32)
                                }
                                _ => false,
                            };
                            if matches {
                                stack.push(val);
                            } else {
                                return Err(WasmTrap::TypeMismatch);
                            }
                        }
                        // i31.new
                        0x1C => {
                            let v = pop_i32!();
                            stack.push(WasmVal::I31(v & 0x7FFF_FFFF));
                        }
                        // i31.get_s
                        0x1D => {
                            match pop!() {
                                WasmVal::I31(v) => {
                                    let s = ((v << 1) as i32) >> 1;
                                    stack.push(WasmVal::I32(s));
                                }
                                _ => return Err(WasmTrap::TypeMismatch),
                            }
                        }
                        // i31.get_u
                        0x1E => {
                            match pop!() {
                                WasmVal::I31(v) => {
                                    stack.push(WasmVal::I32(v & 0x7FFF_FFFF));
                                }
                                _ => return Err(WasmTrap::TypeMismatch),
                            }
                        }
                        _ => return Err(WasmTrap::UnknownOpcode(0xFB)),
                    }
                }

                // ── WebAssembly Threads & Atomics (0xFE) ────────────────────
                0xFE => {
                    let sub_op = read_u32_leb!();
                    let _align = read_u32_leb!();
                    let offset = read_u64_leb!();
                    let base = pop_addr!(0);
                    let addr = base.wrapping_add(offset);
                    match sub_op {
                        0x00 => {
                            // memory.atomic.notify
                            let count = pop_i32!() as u32;
                            let woken = instance.mem_atomic_notify(0, addr, count)?;
                            stack.push(WasmVal::I32(woken as i32));
                        }
                        0x01 => {
                            // memory.atomic.wait32
                            let timeout = pop_i64!();
                            let expected = pop_i32!();
                            let res = instance.mem_atomic_wait32(0, addr, expected, timeout)?;
                            stack.push(WasmVal::I32(res));
                        }
                        0x02 => {
                            // memory.atomic.wait64
                            let timeout = pop_i64!();
                            let expected = pop_i64!();
                            let res = instance.mem_atomic_wait64(0, addr, expected, timeout)?;
                            stack.push(WasmVal::I32(res));
                        }
                        0x10 => {
                            // i32.atomic.load
                            let v = instance.mem_atomic_load_i32(0, addr)?;
                            stack.push(WasmVal::I32(v));
                        }
                        0x11 => {
                            // i64.atomic.load
                            let v = instance.mem_atomic_load_i64(0, addr)?;
                            stack.push(WasmVal::I64(v));
                        }
                        0x12 => {
                            // i32.atomic.load8_u
                            let v = instance.mem_load_u8(0, addr)? as i32;
                            stack.push(WasmVal::I32(v));
                        }
                        0x13 => {
                            // i32.atomic.load16_u
                            let v = instance.mem_load_u16(0, addr)? as i32;
                            stack.push(WasmVal::I32(v));
                        }
                        0x14 => {
                            // i64.atomic.load8_u
                            let v = instance.mem_load_u8(0, addr)? as i64;
                            stack.push(WasmVal::I64(v));
                        }
                        0x15 => {
                            // i64.atomic.load16_u
                            let v = instance.mem_load_u16(0, addr)? as i64;
                            stack.push(WasmVal::I64(v));
                        }
                        0x16 => {
                            // i64.atomic.load32_u
                            let v = instance.mem_load_i32(0, addr)? as u32 as i64;
                            stack.push(WasmVal::I64(v));
                        }
                        0x17 => {
                            // i32.atomic.store
                            let v = pop_i32!();
                            instance.mem_atomic_store_i32(0, addr, v)?;
                        }
                        0x18 => {
                            // i64.atomic.store
                            let v = pop_i64!();
                            instance.mem_atomic_store_i64(0, addr, v)?;
                        }
                        0x19 => {
                            // i32.atomic.store8
                            let v = pop_i32!() as u8;
                            instance.mem_store_u8(0, addr, v)?;
                        }
                        0x1A => {
                            // i32.atomic.store16
                            let v = pop_i32!() as u16;
                            instance.mem_store_u16(0, addr, v)?;
                        }
                        0x1B => {
                            // i64.atomic.store8
                            let v = pop_i64!() as u8;
                            instance.mem_store_u8(0, addr, v)?;
                        }
                        0x1C => {
                            // i64.atomic.store16
                            let v = pop_i64!() as u16;
                            instance.mem_store_u16(0, addr, v)?;
                        }
                        0x1D => {
                            // i64.atomic.store32
                            let v = pop_i64!() as u32 as i32;
                            instance.mem_store_i32(0, addr, v)?;
                        }
                        0x1E => {
                            // i32.atomic.rmw.add
                            let v = pop_i32!();
                            let old = instance.mem_atomic_rmw_add_i32(0, addr, v)?;
                            stack.push(WasmVal::I32(old));
                        }
                        0x1F => {
                            // i64.atomic.rmw.add
                            let v = pop_i64!();
                            let old = instance.mem_atomic_rmw_add_i64(0, addr, v)?;
                            stack.push(WasmVal::I64(old));
                        }
                        0x24 => {
                            // i32.atomic.rmw.sub
                            let v = pop_i32!();
                            let old = instance.mem_atomic_rmw_sub_i32(0, addr, v)?;
                            stack.push(WasmVal::I32(old));
                        }
                        0x25 => {
                            // i64.atomic.rmw.sub
                            let v = pop_i64!();
                            let old = instance.mem_atomic_rmw_sub_i64(0, addr, v)?;
                            stack.push(WasmVal::I64(old));
                        }
                        0x2A => {
                            // i32.atomic.rmw.and
                            let v = pop_i32!();
                            let old = instance.mem_atomic_rmw_and_i32(0, addr, v)?;
                            stack.push(WasmVal::I32(old));
                        }
                        0x2B => {
                            // i64.atomic.rmw.and
                            let v = pop_i64!();
                            let old = instance.mem_atomic_rmw_and_i64(0, addr, v)?;
                            stack.push(WasmVal::I64(old));
                        }
                        0x30 => {
                            // i32.atomic.rmw.or
                            let v = pop_i32!();
                            let old = instance.mem_atomic_rmw_or_i32(0, addr, v)?;
                            stack.push(WasmVal::I32(old));
                        }
                        0x31 => {
                            // i64.atomic.rmw.or
                            let v = pop_i64!();
                            let old = instance.mem_atomic_rmw_or_i64(0, addr, v)?;
                            stack.push(WasmVal::I64(old));
                        }
                        0x36 => {
                            // i32.atomic.rmw.xor
                            let v = pop_i32!();
                            let old = instance.mem_atomic_rmw_xor_i32(0, addr, v)?;
                            stack.push(WasmVal::I32(old));
                        }
                        0x37 => {
                            // i64.atomic.rmw.xor
                            let v = pop_i64!();
                            let old = instance.mem_atomic_rmw_xor_i64(0, addr, v)?;
                            stack.push(WasmVal::I64(old));
                        }
                        0x3C => {
                            // i32.atomic.rmw.xchg
                            let v = pop_i32!();
                            let old = instance.mem_atomic_rmw_xchg_i32(0, addr, v)?;
                            stack.push(WasmVal::I32(old));
                        }
                        0x3D => {
                            // i64.atomic.rmw.xchg
                            let v = pop_i64!();
                            let old = instance.mem_atomic_rmw_xchg_i64(0, addr, v)?;
                            stack.push(WasmVal::I64(old));
                        }
                        0x48 => {
                            // i32.atomic.rmw.cmpxchg
                            let val = pop_i32!();
                            let expected = pop_i32!();
                            let old = instance.mem_atomic_rmw_cmpxchg_i32(0, addr, expected, val)?;
                            stack.push(WasmVal::I32(old));
                        }
                        0x49 => {
                            // i64.atomic.rmw.cmpxchg
                            let val = pop_i64!();
                            let expected = pop_i64!();
                            let old = instance.mem_atomic_rmw_cmpxchg_i64(0, addr, expected, val)?;
                            stack.push(WasmVal::I64(old));
                        }
                        _ => return Err(WasmTrap::UnknownOpcode(0xFE)),
                    }
                }

                other => return Err(WasmTrap::UnknownOpcode(other)),
            }
        }

        // Collect return values
        let arity = func_type.results.len();
        let results: Vec<WasmVal> = stack.iter().rev().take(arity).cloned().collect::<Vec<_>>().into_iter().rev().collect();
        return Ok(results);
        }
    }
}
