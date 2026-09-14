//! Instruction Selector lowering Sea-of-Nodes IR to machine code.
//!
//! Traverses the optimized IR graph, assigns hardware registers, and emits
//! target machine code through the MacroAssembler.

use super::code_allocator::NativeExecutable;
use super::register_allocator::{RegisterAllocator, StorageLocation};
use super::registers::{CallingConvention, X64Register};
use super::x64::assembler::X64Assembler;
use super::{Architecture, Condition};
use crate::compiler::graph::Graph;
use crate::compiler::node::{NodeId, NodeOp};
use crate::objects::value::JSValue;

pub struct InstructionSelector;

impl InstructionSelector {
    /// Compiles a Sea-of-Nodes Graph to native x86_64 machine code.
    pub fn compile_x64(graph: &Graph, calling_convention: CallingConvention) -> NativeExecutable {
        let mut masm = X64Assembler::new();
        let mut allocator = RegisterAllocator::new();

        // 1. Assign parameter registers according to the calling convention
        let param_regs = calling_convention.x64_parameter_registers();
        for node in &graph.nodes {
            if let NodeOp::Parameter(idx) = node.op {
                if idx < param_regs.len() {
                    allocator.assign_register(node.id, param_regs[idx]);
                }
            }
        }

        // 2. Pre-allocate storage for non-dead nodes
        for node in &graph.nodes {
            if !node.is_dead() && node.op != NodeOp::Start && node.op != NodeOp::Return {
                allocator.allocate(node.id);
            }
        }

        // 3. Emit function prologue
        let stack_size = allocator.stack_size_bytes();
        masm.emit_prologue(stack_size);

        // 4. Lower instructions in topological/sequential node order
        for node in &graph.nodes {
            if node.is_dead() {
                continue;
            }

            match &node.op {
                NodeOp::Start => {}
                NodeOp::Parameter(_) => {
                    // Already in assigned parameter register
                }
                NodeOp::Constant(val) => {
                    let imm = match val {
                        JSValue::Smi(n) => *n as i64,
                        JSValue::Number(f) => *f as i64,
                        JSValue::Boolean(b) => if *b { 1 } else { 0 },
                        _ => 0,
                    };
                    let loc = allocator.allocate(node.id);
                    match loc {
                        StorageLocation::Register(r) => {
                            masm.mov_reg_imm64(r, imm);
                        }
                        StorageLocation::StackSlot(disp) => {
                            masm.mov_reg_imm64(X64Register::R11, imm);
                            masm.mov_mem_reg(X64Register::Rbp, disp, X64Register::R11);
                        }
                    }
                }
                NodeOp::Add
                | NodeOp::Sub
                | NodeOp::Mul
                | NodeOp::BitAnd
                | NodeOp::BitOr
                | NodeOp::BitXor => {
                    if node.inputs.len() >= 2 {
                        let lhs_id = node.inputs[0];
                        let rhs_id = node.inputs[1];
                        let dst_loc = allocator.allocate(node.id);

                        let dst_reg = match dst_loc {
                            StorageLocation::Register(r) => r,
                            StorageLocation::StackSlot(_) => X64Register::R10,
                        };

                        // Move LHS to dst_reg
                        Self::load_value_to_reg(&mut masm, &allocator, lhs_id, dst_reg);

                        // Load RHS into temporary R11
                        let rhs_reg = X64Register::R11;
                        Self::load_value_to_reg(&mut masm, &allocator, rhs_id, rhs_reg);

                        // Perform ALU operation
                        match node.op {
                            NodeOp::Add => masm.add_reg_reg(dst_reg, rhs_reg),
                            NodeOp::Sub => masm.sub_reg_reg(dst_reg, rhs_reg),
                            NodeOp::Mul => masm.imul_reg_reg(dst_reg, rhs_reg),
                            NodeOp::BitAnd => masm.and_reg_reg(dst_reg, rhs_reg),
                            NodeOp::BitOr => masm.or_reg_reg(dst_reg, rhs_reg),
                            NodeOp::BitXor => masm.xor_reg_reg(dst_reg, rhs_reg),
                            _ => {}
                        }

                        // Store back to stack if needed
                        if let StorageLocation::StackSlot(disp) = dst_loc {
                            masm.mov_mem_reg(X64Register::Rbp, disp, dst_reg);
                        }
                    }
                }
                NodeOp::CompareEqual
                | NodeOp::CompareStrictEqual
                | NodeOp::CompareLessThan
                | NodeOp::CompareGreaterThan
                | NodeOp::CompareLessThanOrEqual
                | NodeOp::CompareGreaterThanOrEqual => {
                    if node.inputs.len() >= 2 {
                        let lhs_id = node.inputs[0];
                        let rhs_id = node.inputs[1];
                        let dst_loc = allocator.allocate(node.id);

                        let lhs_reg = X64Register::R10;
                        let rhs_reg = X64Register::R11;
                        Self::load_value_to_reg(&mut masm, &allocator, lhs_id, lhs_reg);
                        Self::load_value_to_reg(&mut masm, &allocator, rhs_id, rhs_reg);

                        masm.cmp_reg_reg(lhs_reg, rhs_reg);

                        let cond = match node.op {
                            NodeOp::CompareEqual | NodeOp::CompareStrictEqual => Condition::Equal,
                            NodeOp::CompareLessThan => Condition::LessThan,
                            NodeOp::CompareGreaterThan => Condition::GreaterThan,
                            NodeOp::CompareLessThanOrEqual => Condition::LessThanOrEqual,
                            NodeOp::CompareGreaterThanOrEqual => Condition::GreaterThanOrEqual,
                            _ => Condition::Equal,
                        };

                        let dst_reg = match dst_loc {
                            StorageLocation::Register(r) => r,
                            StorageLocation::StackSlot(_) => X64Register::R10,
                        };

                        masm.setcc(cond, dst_reg);

                        if let StorageLocation::StackSlot(disp) = dst_loc {
                            masm.mov_mem_reg(X64Register::Rbp, disp, dst_reg);
                        }
                    }
                }
                NodeOp::Return => {
                    if let Some(&val_id) = node.inputs.first() {
                        Self::load_value_to_reg(&mut masm, &allocator, val_id, X64Register::Rax);
                    } else {
                        masm.mov_reg_imm64(X64Register::Rax, 0);
                    }
                    masm.emit_epilogue();
                    break;
                }
                _ => {}
            }
        }

        // If no explicit return was encountered, return 0
        if graph.returns.is_empty() {
            masm.mov_reg_imm64(X64Register::Rax, 0);
            masm.emit_epilogue();
        }

        let code_bytes = masm.finalize();
        NativeExecutable::new(Architecture::X64, code_bytes)
    }

    fn load_value_to_reg(
        masm: &mut X64Assembler,
        allocator: &RegisterAllocator,
        id: NodeId,
        target_reg: X64Register,
    ) {
        if let Some(loc) = allocator.get(id) {
            match loc {
                StorageLocation::Register(r) => {
                    if r != target_reg {
                        masm.mov_reg_reg(target_reg, r);
                    }
                }
                StorageLocation::StackSlot(disp) => {
                    masm.mov_reg_mem(target_reg, X64Register::Rbp, disp);
                }
            }
        }
    }
}
