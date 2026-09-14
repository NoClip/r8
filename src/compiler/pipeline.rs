//! Safe Rust reimplementation of Google V8's Compiler Pipeline.
//!
//! Translates bytecode into an optimized Sea-of-Nodes IR graph, executes optimization
//! passes, and runs accelerated pipeline execution for tiered-up hot functions.

use super::graph::Graph;
use super::graph_builder::BytecodeGraphBuilder;
use super::node::{NodeId, NodeOp};
use super::optimizer::{OptimizationStats, Optimizer};
use crate::interpreter::bytecode_array::BytecodeArray;
use crate::objects::value::JSValue;
use std::collections::HashMap;

pub struct CompilerPipeline;

impl CompilerPipeline {
    /// Compiles a BytecodeArray into an optimized Sea-of-Nodes Graph.
    pub fn compile(bytecode_array: &BytecodeArray) -> (Graph, OptimizationStats) {
        let builder = BytecodeGraphBuilder::new(bytecode_array);
        let mut graph = builder.build();
        let stats = Optimizer::optimize(&mut graph);
        (graph, stats)
    }

    /// Compiles a BytecodeArray through the Turbofan optimizer directly into native machine code.
    pub fn compile_to_native(
        bytecode_array: &BytecodeArray,
        calling_convention: super::backend::registers::CallingConvention,
    ) -> (super::backend::code_allocator::NativeExecutable, OptimizationStats) {
        let (graph, stats) = Self::compile(bytecode_array);
        let executable = super::backend::instruction_selector::InstructionSelector::compile_x64(
            &graph,
            calling_convention,
        );
        (executable, stats)
    }

    /// Evaluates an optimized Sea-of-Nodes graph directly on the provided arguments.
    pub fn execute(graph: &Graph, arguments: &[JSValue]) -> Result<JSValue, String> {
        let mut memo: HashMap<NodeId, JSValue> = HashMap::new();

        if let Some(&return_id) = graph.returns.last() {
            let ret_node = graph.get(return_id).ok_or("Invalid return node")?;
            if let Some(&val_id) = ret_node.inputs.first() {
                return Self::evaluate_node(graph, val_id, arguments, &mut memo);
            }
            Ok(JSValue::Undefined)
        } else {
            Ok(JSValue::Undefined)
        }
    }

    fn evaluate_node(
        graph: &Graph,
        id: NodeId,
        arguments: &[JSValue],
        memo: &mut HashMap<NodeId, JSValue>,
    ) -> Result<JSValue, String> {
        if let Some(val) = memo.get(&id) {
            return Ok(val.clone());
        }

        let node = graph.get(id).ok_or_else(|| format!("Node {:?} not found", id))?;
        let res = match &node.op {
            NodeOp::Start => JSValue::Undefined,
            NodeOp::Constant(c) => c.clone(),
            NodeOp::Parameter(idx) => arguments
                .get(*idx)
                .cloned()
                .unwrap_or(JSValue::Undefined),
            NodeOp::Add => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                lhs.add(&rhs)
            }
            NodeOp::Sub => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                lhs.sub(&rhs)
            }
            NodeOp::Mul => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                lhs.mul(&rhs)
            }
            NodeOp::Div => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                lhs.div(&rhs)
            }
            NodeOp::Mod => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                lhs.modulo(&rhs)
            }
            NodeOp::BitAnd => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                lhs.bitwise_and(&rhs)
            }
            NodeOp::BitOr => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                lhs.bitwise_or(&rhs)
            }
            NodeOp::BitXor => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                lhs.bitwise_xor(&rhs)
            }
            NodeOp::ShiftLeft => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                lhs.shift_left(&rhs)
            }
            NodeOp::ShiftRight => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                lhs.shift_right(&rhs)
            }
            NodeOp::Negate => {
                let in_val = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                JSValue::Number(-in_val.to_number())
            }
            NodeOp::Not => {
                let in_val = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                JSValue::Boolean(!in_val.to_boolean())
            }
            NodeOp::CompareEqual => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                JSValue::Boolean(lhs == rhs)
            }
            NodeOp::CompareStrictEqual => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                JSValue::Boolean(lhs == rhs)
            }
            NodeOp::CompareLessThan => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                JSValue::Boolean(lhs.to_number() < rhs.to_number())
            }
            NodeOp::CompareGreaterThan => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                JSValue::Boolean(lhs.to_number() > rhs.to_number())
            }
            NodeOp::CompareLessThanOrEqual => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                JSValue::Boolean(lhs.to_number() <= rhs.to_number())
            }
            NodeOp::CompareGreaterThanOrEqual => {
                let lhs = Self::evaluate_node(graph, node.inputs[0], arguments, memo)?;
                let rhs = Self::evaluate_node(graph, node.inputs[1], arguments, memo)?;
                JSValue::Boolean(lhs.to_number() >= rhs.to_number())
            }
            NodeOp::Return => {
                if let Some(&in_id) = node.inputs.first() {
                    Self::evaluate_node(graph, in_id, arguments, memo)?
                } else {
                    JSValue::Undefined
                }
            }
            _ => JSValue::Undefined,
        };

        memo.insert(id, res.clone());
        Ok(res)
    }
}
