//! Safe Rust reimplementation of Google V8's Sea-of-Nodes Intermediate Representation (IR).
//!
//! Models computation nodes, operands, constants, control structures (Branches, Merges, Phis),
//! and data-flow dependencies.

use crate::objects::value::JSValue;

/// Unique identifier for a node within an IR graph.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub usize);

/// The operational kind of a Sea-of-Nodes IR node.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeOp {
    /// Graph entry point.
    Start,
    /// Function parameter input.
    Parameter(usize),
    /// Compile-time constant value.
    Constant(JSValue),

    // Arithmetic & Bitwise
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    Negate,
    Not,

    // Relational & Comparisons
    CompareEqual,
    CompareStrictEqual,
    CompareLessThan,
    CompareGreaterThan,
    CompareLessThanOrEqual,
    CompareGreaterThanOrEqual,

    // Control flow
    Branch,
    IfTrue,
    IfFalse,
    Merge,
    Phi,
    Return,

    // Marker for eliminated nodes
    Dead,
}

/// A node in the Sea-of-Nodes graph combining data-flow inputs and control edges.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub op: NodeOp,
    pub inputs: Vec<NodeId>,
    pub control: Option<NodeId>,
}

impl Node {
    pub fn new(id: NodeId, op: NodeOp, inputs: Vec<NodeId>, control: Option<NodeId>) -> Self {
        Self {
            id,
            op,
            inputs,
            control,
        }
    }

    pub fn is_constant(&self) -> bool {
        matches!(self.op, NodeOp::Constant(_))
    }

    pub fn as_constant(&self) -> Option<&JSValue> {
        if let NodeOp::Constant(ref val) = self.op {
            Some(val)
        } else {
            None
        }
    }

    pub fn is_dead(&self) -> bool {
        self.op == NodeOp::Dead
    }
}
