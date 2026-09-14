//! Safe Rust reimplementation of Google V8's heap object headers and representations.
//!
//! Models object headers, generational ages, forwarding pointers for scavenge evacuation,
//! tri-color marking states, and outgoing reference graphs.

use crate::objects::feedback_vector::FeedbackVector;
use crate::objects::function::JSFunction;
use crate::objects::js_object::JSObject;
use crate::objects::map::Map;
use crate::objects::value::JSValue;

/// Memory allocation space identifying object generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AllocationSpace {
    /// Young generation nursery space (managed by Scavenger).
    New,
    /// Old generation tenured space (managed by Mark-Sweep).
    Old,
}

/// Unique identifier for an object in the managed heap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HeapId {
    pub space: AllocationSpace,
    pub index: u32,
    pub generation: u32,
}

impl HeapId {
    pub fn new(space: AllocationSpace, index: u32, generation: u32) -> Self {
        Self {
            space,
            index,
            generation,
        }
    }
}

/// Tri-color marking state for the Mark-Sweep collector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkColor {
    /// Object is unvisited (potential garbage).
    White,
    /// Object is reachable, but its outgoing references have not yet been scanned.
    Grey,
    /// Object is reachable and all its outgoing references have been scanned.
    Black,
}

/// Header containing GC metadata for a heap object.
#[derive(Clone, Debug)]
pub struct HeapHeader {
    /// Allocation space (New vs Old).
    pub space: AllocationSpace,
    /// Generational age: number of scavenge cycles survived.
    pub age: u8,
    /// Forwarding address used during Scavenger evacuation.
    pub forwarding_address: Option<HeapId>,
    /// Tri-color mark state.
    pub color: MarkColor,
    /// Approximate memory footprint in bytes.
    pub size_bytes: usize,
}

impl HeapHeader {
    pub fn new(space: AllocationSpace, size_bytes: usize) -> Self {
        Self {
            space,
            age: 0,
            forwarding_address: None,
            color: MarkColor::White,
            size_bytes,
        }
    }
}

/// Concrete payload stored inside a managed heap object.
#[derive(Clone, Debug)]
pub enum HeapPayload {
    Object(JSObject),
    Array(JSObject),
    Map(Map),
    Function(JSFunction),
    String(String),
    FeedbackVector(FeedbackVector),
    Raw(Vec<JSValue>),
}

/// A managed object allocated on the V8 heap.
#[derive(Clone, Debug)]
pub struct HeapObject {
    pub id: HeapId,
    pub header: HeapHeader,
    pub payload: HeapPayload,
    pub outgoing_references: Vec<HeapId>,
}

impl HeapObject {
    pub fn new(id: HeapId, payload: HeapPayload, size_bytes: usize) -> Self {
        let header = HeapHeader::new(id.space, size_bytes);
        Self {
            id,
            header,
            payload,
            outgoing_references: Vec::new(),
        }
    }

    /// Records an outgoing pointer from this object to another heap object.
    pub fn add_reference(&mut self, target: HeapId) {
        if !self.outgoing_references.contains(&target) {
            self.outgoing_references.push(target);
        }
    }

    /// Removes an outgoing reference if present.
    pub fn remove_reference(&mut self, target: HeapId) {
        self.outgoing_references.retain(|&id| id != target);
    }

    /// Clears all outgoing references.
    pub fn clear_references(&mut self) {
        self.outgoing_references.clear();
    }

    /// Returns a list of all outgoing references for GC tracing.
    pub fn get_references(&self) -> &[HeapId] {
        &self.outgoing_references
    }
}
