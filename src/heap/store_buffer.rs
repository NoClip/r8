//! Safe Rust reimplementation of Google V8's `StoreBuffer` (Remembered Set).
//!
//! Tracks Old-to-Young references across generations to enable scoped
//! young generation garbage collection (Scavenger) without scanning the Old Generation.

use super::heap_object::{AllocationSpace, HeapId};

/// An entry in the remembered set recording an Old-to-Young reference pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StoreBufferEntry {
    pub source: HeapId,
    pub target: HeapId,
}

/// The V8 Store Buffer managing the remembered set for the generational write barrier.
#[derive(Clone, Debug, Default)]
pub struct StoreBuffer {
    entries: Vec<StoreBufferEntry>,
}

impl StoreBuffer {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Generational write barrier: records an edge if `source` is in OldSpace
    /// and `target` is in NewSpace.
    pub fn record_write(&mut self, source: HeapId, target: HeapId) -> bool {
        if source.space == AllocationSpace::Old && target.space == AllocationSpace::New {
            let entry = StoreBufferEntry { source, target };
            if !self.entries.contains(&entry) {
                self.entries.push(entry);
                return true;
            }
        }
        false
    }

    /// Returns a slice of all recorded entries.
    pub fn entries(&self) -> &[StoreBufferEntry] {
        &self.entries
    }

    /// Clears the store buffer after a scavenge cycle.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Retains only entries satisfying a predicate (e.g. updating addresses during evacuation).
    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&StoreBufferEntry) -> bool,
    {
        self.entries.retain(f);
    }

    /// Number of remembered entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
