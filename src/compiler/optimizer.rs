//! Safe Rust reimplementation of Google V8's Compiler Optimization passes.
//!
//! Implements Constant Folding, Algebraic Simplification, and Dead Code Elimination
//! over the Sea-of-Nodes intermediate representation graph.

use super::graph::Graph;
use super::node::{NodeId, NodeOp};
use crate::objects::value::JSValue;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OptimizationStats {
    pub constants_folded: usize,
    pub algebraic_reductions: usize,
    pub dead_nodes_eliminated: usize,
}

pub struct Optimizer;

impl Optimizer {
    /// Runs all optimization passes until reaching a fixed point.
    pub fn optimize(graph: &mut Graph) -> OptimizationStats {
        let mut stats = OptimizationStats::default();

        loop {
            let folded = Self::constant_folding(graph);
            let reduced = Self::algebraic_reductions(graph);
            stats.constants_folded += folded;
            stats.algebraic_reductions += reduced;

            if folded == 0 && reduced == 0 {
                break;
            }
        }

        stats.dead_nodes_eliminated = Self::dead_code_elimination(graph);
        stats
    }

    /// Evaluates operations with constant inputs at compile-time.
    pub fn constant_folding(graph: &mut Graph) -> usize {
        let mut folded = 0;
        let len = graph.len();

        for i in 0..len {
            let id = NodeId(i);
            let node = match graph.get(id) {
                Some(n) if !n.is_dead() && !n.is_constant() => n.clone(),
                _ => continue,
            };

            // Unary constant folding
            if node.inputs.len() == 1 {
                let in_id = node.inputs[0];
                if let Some(in_node) = graph.get(in_id) {
                    if let Some(val) = in_node.as_constant() {
                        let res = match node.op {
                            NodeOp::Negate => Some(JSValue::Number(-val.to_number())),
                            NodeOp::Not => Some(JSValue::Boolean(!val.to_boolean())),
                            _ => None,
                        };
                        if let Some(c) = res {
                            graph.replace_node(id, NodeOp::Constant(c), Vec::new());
                            folded += 1;
                            continue;
                        }
                    }
                }
            }

            // Binary constant folding
            if node.inputs.len() == 2 {
                let left_id = node.inputs[0];
                let right_id = node.inputs[1];

                let left_const = graph.get(left_id).and_then(|n| n.as_constant()).cloned();
                let right_const = graph.get(right_id).and_then(|n| n.as_constant()).cloned();

                if let (Some(l), Some(r)) = (left_const, right_const) {
                    let result = match node.op {
                        NodeOp::Add => Some(l.add(&r)),
                        NodeOp::Sub => Some(l.sub(&r)),
                        NodeOp::Mul => Some(l.mul(&r)),
                        NodeOp::Div => Some(l.div(&r)),
                        NodeOp::Mod => Some(l.modulo(&r)),
                        NodeOp::BitAnd => Some(l.bitwise_and(&r)),
                        NodeOp::BitOr => Some(l.bitwise_or(&r)),
                        NodeOp::BitXor => Some(l.bitwise_xor(&r)),
                        NodeOp::ShiftLeft => Some(l.shift_left(&r)),
                        NodeOp::ShiftRight => Some(l.shift_right(&r)),
                        NodeOp::CompareEqual => Some(JSValue::Boolean(l == r)),
                        NodeOp::CompareStrictEqual => Some(JSValue::Boolean(l == r)),
                        NodeOp::CompareLessThan => Some(JSValue::Boolean(l.to_number() < r.to_number())),
                        NodeOp::CompareGreaterThan => Some(JSValue::Boolean(l.to_number() > r.to_number())),
                        NodeOp::CompareLessThanOrEqual => Some(JSValue::Boolean(l.to_number() <= r.to_number())),
                        NodeOp::CompareGreaterThanOrEqual => Some(JSValue::Boolean(l.to_number() >= r.to_number())),
                        _ => None,
                    };

                    if let Some(folded_val) = result {
                        graph.replace_node(id, NodeOp::Constant(folded_val), Vec::new());
                        folded += 1;
                    }
                }
            }
        }

        folded
    }

    /// Algebraic reductions such as x + 0, x * 1, x * 0.
    pub fn algebraic_reductions(graph: &mut Graph) -> usize {
        let mut reductions = 0;
        let len = graph.len();

        for i in 0..len {
            let id = NodeId(i);
            let node = match graph.get(id) {
                Some(n) if !n.is_dead() && !n.is_constant() => n.clone(),
                _ => continue,
            };

            if node.inputs.len() == 2 {
                let left_id = node.inputs[0];
                let right_id = node.inputs[1];

                let left_is_zero = graph.get(left_id).and_then(|n| n.as_constant()).map(|c| c.to_number() == 0.0).unwrap_or(false);
                let right_is_zero = graph.get(right_id).and_then(|n| n.as_constant()).map(|c| c.to_number() == 0.0).unwrap_or(false);
                let left_is_one = graph.get(left_id).and_then(|n| n.as_constant()).map(|c| c.to_number() == 1.0).unwrap_or(false);
                let right_is_one = graph.get(right_id).and_then(|n| n.as_constant()).map(|c| c.to_number() == 1.0).unwrap_or(false);

                match node.op {
                    NodeOp::Add => {
                        if right_is_zero {
                            // x + 0 => x
                            if let Some(lhs) = graph.get(left_id).cloned() {
                                graph.replace_node(id, lhs.op, lhs.inputs);
                                reductions += 1;
                            }
                        } else if left_is_zero {
                            // 0 + x => x
                            if let Some(rhs) = graph.get(right_id).cloned() {
                                graph.replace_node(id, rhs.op, rhs.inputs);
                                reductions += 1;
                            }
                        }
                    }
                    NodeOp::Sub => {
                        if right_is_zero {
                            // x - 0 => x
                            if let Some(lhs) = graph.get(left_id).cloned() {
                                graph.replace_node(id, lhs.op, lhs.inputs);
                                reductions += 1;
                            }
                        } else if left_id == right_id {
                            // x - x => 0
                            graph.replace_node(id, NodeOp::Constant(JSValue::Smi(0)), Vec::new());
                            reductions += 1;
                        }
                    }
                    NodeOp::Mul => {
                        if right_is_one {
                            // x * 1 => x
                            if let Some(lhs) = graph.get(left_id).cloned() {
                                graph.replace_node(id, lhs.op, lhs.inputs);
                                reductions += 1;
                            }
                        } else if left_is_one {
                            // 1 * x => x
                            if let Some(rhs) = graph.get(right_id).cloned() {
                                graph.replace_node(id, rhs.op, rhs.inputs);
                                reductions += 1;
                            }
                        } else if left_is_zero || right_is_zero {
                            // x * 0 => 0
                            graph.replace_node(id, NodeOp::Constant(JSValue::Smi(0)), Vec::new());
                            reductions += 1;
                        }
                    }
                    _ => {}
                }
            }
        }

        reductions
    }

    /// Eliminates nodes unreachable from graph return points.
    pub fn dead_code_elimination(graph: &mut Graph) -> usize {
        let mut live_set: HashSet<NodeId> = HashSet::new();
        let mut worklist: Vec<NodeId> = Vec::new();

        // Seed with all Return nodes and the Start node
        live_set.insert(graph.start);
        for &ret_id in &graph.returns {
            live_set.insert(ret_id);
            worklist.push(ret_id);
        }

        while let Some(curr_id) = worklist.pop() {
            if let Some(node) = graph.get(curr_id) {
                for &input_id in &node.inputs {
                    if live_set.insert(input_id) {
                        worklist.push(input_id);
                    }
                }
                if let Some(ctrl_id) = node.control {
                    if live_set.insert(ctrl_id) {
                        worklist.push(ctrl_id);
                    }
                }
            }
        }

        let mut eliminated = 0;
        let len = graph.len();
        for i in 0..len {
            let id = NodeId(i);
            if !live_set.contains(&id) {
                if let Some(node) = graph.get_mut(id) {
                    if !node.is_dead() {
                        node.op = NodeOp::Dead;
                        node.inputs.clear();
                        node.control = None;
                        eliminated += 1;
                    }
                }
            }
        }

        eliminated
    }
}
