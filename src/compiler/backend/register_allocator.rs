//! Register allocation and stack slot assignment.
//!
//! Assigns physical registers (or stack slots) to Sea-of-Nodes values.

use super::registers::X64Register;
use crate::compiler::node::NodeId;
use std::collections::HashMap;

/// Storage location for an intermediate value in the JIT compiler.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StorageLocation {
    Register(X64Register),
    StackSlot(i32), // Byte displacement from Rbp, e.g. -8, -16, etc.
}

/// Simple register allocator assigning physical registers and stack spill slots.
#[derive(Clone, Debug)]
pub struct RegisterAllocator {
    allocations: HashMap<NodeId, StorageLocation>,
    available_registers: Vec<X64Register>,
    next_stack_slot: i32,
}

impl Default for RegisterAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl RegisterAllocator {
    pub fn new() -> Self {
        // Scratch / allocatable general-purpose registers (excluding RSP and RBP)
        let available_registers = vec![
            X64Register::R10,
            X64Register::R11,
            X64Register::Rbx,
            X64Register::Rsi,
            X64Register::Rdi,
            X64Register::R8,
            X64Register::R9,
            X64Register::Rdx,
            X64Register::Rcx,
            X64Register::Rax,
        ];

        Self {
            allocations: HashMap::new(),
            available_registers,
            next_stack_slot: 1, // Slot 1 = [rbp - 8]
        }
    }

    /// Allocates storage for a node.
    pub fn allocate(&mut self, id: NodeId) -> StorageLocation {
        if let Some(&loc) = self.allocations.get(&id) {
            return loc;
        }

        let loc = if let Some(reg) = self.available_registers.pop() {
            StorageLocation::Register(reg)
        } else {
            let offset = self.next_stack_slot * 8;
            self.next_stack_slot += 1;
            StorageLocation::StackSlot(-offset)
        };

        self.allocations.insert(id, loc);
        loc
    }

    /// Associates a specific register with a node (e.g. parameter registers).
    pub fn assign_register(&mut self, id: NodeId, reg: X64Register) {
        self.allocations.insert(id, StorageLocation::Register(reg));
        self.available_registers.retain(|&r| r != reg);
    }

    /// Retrieves the allocated storage location for a node.
    pub fn get(&self, id: NodeId) -> Option<StorageLocation> {
        self.allocations.get(&id).copied()
    }

    /// Returns the total stack size in bytes required for spilled values.
    pub fn stack_size_bytes(&self) -> u32 {
        (self.next_stack_slot as u32) * 8
    }
}
