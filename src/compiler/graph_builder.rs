//! Safe Rust reimplementation of Google V8's BytecodeGraphBuilder.
//!
//! Translates Ignition BytecodeArrays into Sea-of-Nodes Intermediate Representation.

use super::graph::Graph;
use super::node::{NodeId, NodeOp};
use crate::interpreter::bytecode_array::{BytecodeArray, ConstantValue};
use crate::interpreter::bytecode_register::Register;
use crate::interpreter::bytecodes::Bytecode;
use crate::objects::value::JSValue;
use std::collections::HashMap;

pub struct BytecodeGraphBuilder<'a> {
    bytecode_array: &'a BytecodeArray,
    graph: Graph,
    accumulator: Option<NodeId>,
    registers: HashMap<i32, NodeId>,
    parameters: HashMap<usize, NodeId>,
    current_control: NodeId,
}

impl<'a> BytecodeGraphBuilder<'a> {
    pub fn new(bytecode_array: &'a BytecodeArray) -> Self {
        let graph = Graph::new();
        let start = graph.start;
        Self {
            bytecode_array,
            graph,
            accumulator: None,
            registers: HashMap::new(),
            parameters: HashMap::new(),
            current_control: start,
        }
    }

    /// Checks whether a BytecodeArray contains only straight-line operations supported by the Sea-of-Nodes JIT.
    pub fn can_compile(bytecode_array: &BytecodeArray) -> bool {
        let bytes = bytecode_array.bytecodes();
        let mut pc: usize = 0;
        let mut has_return = false;

        while pc < bytes.len() {
            let opcode_byte = bytes[pc];
            pc += 1;

            let bc = match Bytecode::from_byte(opcode_byte) {
                Some(b) => b,
                None => return false,
            };

            if bc.is_short_star() {
                continue;
            }

            match bc {
                Bytecode::LdaZero
                | Bytecode::LdaUndefined
                | Bytecode::LdaNull
                | Bytecode::LdaTrue
                | Bytecode::LdaFalse => {}

                Bytecode::LdaSmi | Bytecode::LdaConstant | Bytecode::Ldar | Bytecode::Star => {
                    if pc >= bytes.len() { return false; }
                    pc += 1;
                }

                Bytecode::Mov => {
                    if pc + 1 >= bytes.len() { return false; }
                    pc += 2;
                }

                Bytecode::Add
                | Bytecode::Sub
                | Bytecode::Mul
                | Bytecode::Div
                | Bytecode::Mod
                | Bytecode::BitwiseAnd
                | Bytecode::BitwiseOr
                | Bytecode::BitwiseXor
                | Bytecode::ShiftLeft
                | Bytecode::ShiftRight
                | Bytecode::AddSmi
                | Bytecode::SubSmi
                | Bytecode::MulSmi
                | Bytecode::DivSmi
                | Bytecode::ModSmi
                | Bytecode::BitwiseAndSmi
                | Bytecode::BitwiseOrSmi
                | Bytecode::BitwiseXorSmi
                | Bytecode::ShiftLeftSmi
                | Bytecode::ShiftRightSmi
                | Bytecode::TestEqual
                | Bytecode::TestEqualStrict
                | Bytecode::TestLessThan
                | Bytecode::TestGreaterThan
                | Bytecode::TestLessThanOrEqual
                | Bytecode::TestGreaterThanOrEqual => {
                    if pc + 1 >= bytes.len() { return false; }
                    pc += 2; // reg + feedback slot
                }

                Bytecode::Negate => {
                    if pc >= bytes.len() { return false; }
                    pc += 1; // feedback slot
                }

                Bytecode::Return => {
                    has_return = true;
                    // If there are instructions following Return, it's an early return / branch
                    if pc < bytes.len() {
                        return false;
                    }
                }

                // Any branch, loop, call, or complex bytecode is not supported for straight-line JIT
                _ => return false,
            }
        }

        has_return
    }

    /// Translates the bytecode array into a Sea-of-Nodes graph.
    pub fn build(mut self) -> Graph {
        let bytes = self.bytecode_array.bytecodes();
        let mut pc: usize = 0;

        while pc < bytes.len() {
            let opcode_byte = bytes[pc];
            pc += 1;

            let bc = match Bytecode::from_byte(opcode_byte) {
                Some(b) => b,
                None => continue,
            };

            // ShortStar optimization handlers: Star0..Star15
            if bc.is_short_star() {
                let star_idx = bc.short_star_index().unwrap_or(0);
                if let Some(acc) = self.accumulator {
                    self.registers.insert(star_idx as i32, acc);
                }
                continue;
            }

            match bc {
                Bytecode::LdaZero => {
                    let node = self.graph.add_node(
                        NodeOp::Constant(JSValue::Smi(0)),
                        Vec::new(),
                        None,
                    );
                    self.accumulator = Some(node);
                }
                Bytecode::LdaUndefined => {
                    let node = self.graph.add_node(
                        NodeOp::Constant(JSValue::Undefined),
                        Vec::new(),
                        None,
                    );
                    self.accumulator = Some(node);
                }
                Bytecode::LdaNull => {
                    let node = self.graph.add_node(
                        NodeOp::Constant(JSValue::Null),
                        Vec::new(),
                        None,
                    );
                    self.accumulator = Some(node);
                }
                Bytecode::LdaTrue => {
                    let node = self.graph.add_node(
                        NodeOp::Constant(JSValue::Boolean(true)),
                        Vec::new(),
                        None,
                    );
                    self.accumulator = Some(node);
                }
                Bytecode::LdaFalse => {
                    let node = self.graph.add_node(
                        NodeOp::Constant(JSValue::Boolean(false)),
                        Vec::new(),
                        None,
                    );
                    self.accumulator = Some(node);
                }
                Bytecode::LdaSmi => {
                    let imm = bytes[pc] as i8;
                    pc += 1;
                    let node = self.graph.add_node(
                        NodeOp::Constant(JSValue::Smi(imm as i32)),
                        Vec::new(),
                        None,
                    );
                    self.accumulator = Some(node);
                }
                Bytecode::LdaConstant => {
                    let idx = bytes[pc] as usize;
                    pc += 1;
                    let constant = self
                        .bytecode_array
                        .get_constant(idx)
                        .cloned()
                        .unwrap_or(ConstantValue::Undefined);
                    let val = match constant {
                        ConstantValue::Smi(n) => JSValue::Smi(n),
                        ConstantValue::Number(f) => JSValue::Number(f),
                        ConstantValue::String(s) => JSValue::String(s),
                        ConstantValue::Boolean(b) => JSValue::Boolean(b),
                        ConstantValue::Null => JSValue::Null,
                        ConstantValue::Undefined => JSValue::Undefined,
                        ConstantValue::BigInt(s) => {
                            let bi = crate::objects::bigint::BigIntData::from_str(&s).unwrap_or_else(|_| crate::objects::bigint::BigIntData::zero());
                            JSValue::BigInt(std::rc::Rc::new(bi))
                        }
                    };
                    let node = self.graph.add_node(NodeOp::Constant(val), Vec::new(), None);
                    self.accumulator = Some(node);
                }
                Bytecode::Ldar => {
                    let operand_byte = bytes[pc] as i8;
                    pc += 1;
                    let reg = Register::from_operand(operand_byte as i32);
                    self.accumulator = Some(self.get_register_node(reg));
                }
                Bytecode::Star => {
                    let operand_byte = bytes[pc] as i8;
                    pc += 1;
                    let reg = Register::from_operand(operand_byte as i32);
                    if let Some(acc) = self.accumulator {
                        self.set_register_node(reg, acc);
                    }
                }
                Bytecode::Mov => {
                    let src_byte = bytes[pc] as i8;
                    pc += 1;
                    let dst_byte = bytes[pc] as i8;
                    pc += 1;
                    let src_reg = Register::from_operand(src_byte as i32);
                    let dst_reg = Register::from_operand(dst_byte as i32);
                    let val_node = self.get_register_node(src_reg);
                    self.set_register_node(dst_reg, val_node);
                }

                // Arithmetic
                Bytecode::Add => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::Add, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::Sub => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::Sub, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::Mul => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::Mul, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::Div => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::Div, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::Mod => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::Mod, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::BitwiseAnd => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::BitAnd, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::BitwiseOr => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::BitOr, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::BitwiseXor => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::BitXor, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::ShiftLeft => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::ShiftLeft, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::ShiftRight => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::ShiftRight, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }

                // Smi immediate operations
                Bytecode::AddSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let rhs = self.graph.add_node(NodeOp::Constant(JSValue::Smi(imm)), Vec::new(), None);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::Add, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::SubSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let rhs = self.graph.add_node(NodeOp::Constant(JSValue::Smi(imm)), Vec::new(), None);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::Sub, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::MulSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let rhs = self.graph.add_node(NodeOp::Constant(JSValue::Smi(imm)), Vec::new(), None);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::Mul, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::DivSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let rhs = self.graph.add_node(NodeOp::Constant(JSValue::Smi(imm)), Vec::new(), None);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::Div, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::ModSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let rhs = self.graph.add_node(NodeOp::Constant(JSValue::Smi(imm)), Vec::new(), None);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::Mod, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::BitwiseAndSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let rhs = self.graph.add_node(NodeOp::Constant(JSValue::Smi(imm)), Vec::new(), None);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::BitAnd, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::BitwiseOrSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let rhs = self.graph.add_node(NodeOp::Constant(JSValue::Smi(imm)), Vec::new(), None);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::BitOr, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::BitwiseXorSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let rhs = self.graph.add_node(NodeOp::Constant(JSValue::Smi(imm)), Vec::new(), None);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::BitXor, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::ShiftLeftSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let rhs = self.graph.add_node(NodeOp::Constant(JSValue::Smi(imm)), Vec::new(), None);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::ShiftLeft, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::ShiftRightSmi => {
                    let imm = bytes[pc] as i8 as i32;
                    pc += 1;
                    pc += 1; // skip feedback slot
                    let rhs = self.graph.add_node(NodeOp::Constant(JSValue::Smi(imm)), Vec::new(), None);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::ShiftRight, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::Negate => {
                    pc += 1; // feedback
                    if let Some(val) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::Negate, vec![val], None);
                        self.accumulator = Some(node);
                    }
                }

                // Relational comparisons
                Bytecode::TestEqual => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::CompareEqual, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::TestEqualStrict => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::CompareStrictEqual, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::TestLessThan => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::CompareLessThan, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::TestGreaterThan => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::CompareGreaterThan, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::TestLessThanOrEqual => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::CompareLessThanOrEqual, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }
                Bytecode::TestGreaterThanOrEqual => {
                    let reg_byte = bytes[pc] as i8;
                    pc += 1;
                    pc += 1;
                    let reg = Register::from_operand(reg_byte as i32);
                    let rhs = self.get_register_node(reg);
                    if let Some(lhs) = self.accumulator {
                        let node = self.graph.add_node(NodeOp::CompareGreaterThanOrEqual, vec![lhs, rhs], None);
                        self.accumulator = Some(node);
                    }
                }

                // Return
                Bytecode::Return => {
                    let return_val = self.accumulator.unwrap_or_else(|| {
                        self.graph.add_node(NodeOp::Constant(JSValue::Undefined), Vec::new(), None)
                    });
                    self.graph.add_node(
                        NodeOp::Return,
                        vec![return_val],
                        Some(self.current_control),
                    );
                }

                // Remaining bytecodes advance pc according to operand sizes
                _ => {
                    // Skip remaining operands if any
                }
            }
        }

        self.graph
    }

    fn get_register_node(&mut self, reg: Register) -> NodeId {
        if reg.is_parameter() {
            let param_idx = reg.to_parameter_index() as usize;
            if let Some(&node_id) = self.parameters.get(&param_idx) {
                node_id
            } else {
                let node_id = self.graph.add_node(
                    NodeOp::Parameter(param_idx),
                    Vec::new(),
                    Some(self.graph.start),
                );
                self.parameters.insert(param_idx, node_id);
                node_id
            }
        } else {
            let idx = reg.index();
            if let Some(&node_id) = self.registers.get(&idx) {
                node_id
            } else {
                let node_id = self.graph.add_node(
                    NodeOp::Constant(JSValue::Undefined),
                    Vec::new(),
                    None,
                );
                self.registers.insert(idx, node_id);
                node_id
            }
        }
    }

    fn set_register_node(&mut self, reg: Register, node: NodeId) {
        if reg.is_parameter() {
            let param_idx = reg.to_parameter_index() as usize;
            self.parameters.insert(param_idx, node);
        } else {
            self.registers.insert(reg.index(), node);
        }
    }
}
