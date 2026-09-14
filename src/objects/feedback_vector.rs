//! Safe Rust reimplementation of Google V8's `src/objects/feedback-vector.h`.
//!
//! Provides Inline Cache (IC) metadata structures, tracking Monomorphic,
//! Polymorphic, and Megamorphic states for accelerated property lookups.

use std::cell::RefCell;

/// Identifies a feedback slot in a function's FeedbackVector.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FeedbackSlot(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InlineCacheState {
    Uninitialized,
    Monomorphic,
    Polymorphic,
    Megamorphic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CacheEntry {
    pub map_ptr: usize,
    pub field_index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FeedbackSlotData {
    pub state: InlineCacheState,
    pub entries: Vec<CacheEntry>,
}

impl Default for FeedbackSlotData {
    fn default() -> Self {
        Self {
            state: InlineCacheState::Uninitialized,
            entries: Vec::new(),
        }
    }
}

/// FeedbackVector attached to compiled functions to record inline cache transitions.
#[derive(Clone, Debug)]
pub struct FeedbackVector {
    pub slots: Vec<RefCell<FeedbackSlotData>>,
}

impl FeedbackVector {
    pub fn new(num_slots: usize) -> Self {
        let mut slots = Vec::with_capacity(num_slots);
        for _ in 0..num_slots {
            slots.push(RefCell::new(FeedbackSlotData::default()));
        }
        Self { slots }
    }

    /// Fast-path query: returns the cached field index if the map matches the cache.
    pub fn get_cached_field_index(&self, slot: FeedbackSlot, map_ptr: usize) -> Option<usize> {
        let slot_idx = slot.0 as usize;
        if slot_idx >= self.slots.len() {
            return None;
        }

        let data = self.slots[slot_idx].borrow();
        match data.state {
            InlineCacheState::Monomorphic | InlineCacheState::Polymorphic => {
                data.entries.iter().find(|e| e.map_ptr == map_ptr).map(|e| e.field_index)
            }
            _ => None,
        }
    }

    /// Records a property lookup result to update the Inline Cache state.
    pub fn record_lookup(&self, slot: FeedbackSlot, map_ptr: usize, field_index: usize) {
        let slot_idx = slot.0 as usize;
        if slot_idx >= self.slots.len() {
            return;
        }

        let mut data = self.slots[slot_idx].borrow_mut();
        match data.state {
            InlineCacheState::Uninitialized => {
                data.state = InlineCacheState::Monomorphic;
                data.entries.push(CacheEntry { map_ptr, field_index });
            }
            InlineCacheState::Monomorphic => {
                if data.entries.iter().any(|e| e.map_ptr == map_ptr) {
                    return; // Already cached
                }
                data.state = InlineCacheState::Polymorphic;
                data.entries.push(CacheEntry { map_ptr, field_index });
            }
            InlineCacheState::Polymorphic => {
                if data.entries.iter().any(|e| e.map_ptr == map_ptr) {
                    return;
                }
                if data.entries.len() >= 4 {
                    data.state = InlineCacheState::Megamorphic;
                    data.entries.clear();
                } else {
                    data.entries.push(CacheEntry { map_ptr, field_index });
                }
            }
            InlineCacheState::Megamorphic => {
                // Megamorphic: do not cache individual maps
            }
        }
    }
}
