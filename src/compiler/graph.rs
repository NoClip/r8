//! Safe Rust reimplementation of Google V8's Sea-of-Nodes Graph container.
//!
//! Stores the DAG of operations, control structures, and optimizations.

use super::node::{Node, NodeId, NodeOp};

/// The Sea-of-Nodes Intermediate Representation (IR) Graph.
#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub start: NodeId,
    pub returns: Vec<NodeId>,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    pub fn new() -> Self {
        let start_id = NodeId(0);
        let start_node = Node::new(start_id, NodeOp::Start, Vec::new(), None);
        Self {
            nodes: vec![start_node],
            start: start_id,
            returns: Vec::new(),
        }
    }

    /// Adds a new node to the graph and returns its unique NodeId.
    pub fn add_node(
        &mut self,
        op: NodeOp,
        inputs: Vec<NodeId>,
        control: Option<NodeId>,
    ) -> NodeId {
        let id = NodeId(self.nodes.len());
        let node = Node::new(id, op, inputs, control);
        if node.op == NodeOp::Return {
            self.returns.push(id);
        }
        self.nodes.push(node);
        id
    }

    /// Replaces an existing node's operation and inputs (used in optimization passes).
    pub fn replace_node(&mut self, id: NodeId, op: NodeOp, inputs: Vec<NodeId>) {
        if let Some(node) = self.nodes.get_mut(id.0) {
            node.op = op;
            node.inputs = inputs;
        }
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0)
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.0)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
