//! Safe Rust reimplementation of Google V8's memory spaces (`SemiSpace`, `NewSpace`, `OldSpace`).
//!
//! Provides bump-pointer allocation for the nursery (NewSpace semi-spaces) and
//! slot-recycled free-list allocation for tenured objects (OldSpace).

use super::heap_object::{AllocationSpace, HeapId, HeapObject, HeapPayload, MarkColor};

/// A single semi-space buffer within the young generation nursery.
#[derive(Clone, Debug)]
pub struct SemiSpace {
    pub slots: Vec<Option<HeapObject>>,
    pub capacity: usize,
    pub used_bytes: usize,
    pub generation_id: u32,
}

impl SemiSpace {
    pub fn new(capacity: usize, generation_id: u32) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            capacity,
            used_bytes: 0,
            generation_id,
        }
    }

    /// Bump-allocates a new heap object in this semi-space.
    pub fn allocate(&mut self, payload: HeapPayload, size_bytes: usize) -> Option<HeapId> {
        if self.slots.len() >= self.capacity {
            return None;
        }
        let index = self.slots.len() as u32;
        let id = HeapId::new(AllocationSpace::New, index, self.generation_id);
        let obj = HeapObject::new(id, payload, size_bytes);
        self.used_bytes += size_bytes;
        self.slots.push(Some(obj));
        Some(id)
    }

    /// Directly places an already constructed HeapObject (used during Scavenger evacuation).
    pub fn place(&mut self, mut obj: HeapObject) -> Option<HeapId> {
        if self.slots.len() >= self.capacity {
            return None;
        }
        let index = self.slots.len() as u32;
        let id = HeapId::new(AllocationSpace::New, index, self.generation_id);
        obj.id = id;
        obj.header.space = AllocationSpace::New;
        self.used_bytes += obj.header.size_bytes;
        self.slots.push(Some(obj));
        Some(id)
    }

    pub fn get(&self, index: u32) -> Option<&HeapObject> {
        self.slots.get(index as usize).and_then(|slot| slot.as_ref())
    }

    pub fn get_mut(&mut self, index: u32) -> Option<&mut HeapObject> {
        self.slots.get_mut(index as usize).and_then(|slot| slot.as_mut())
    }

    pub fn reset(&mut self) {
        self.slots.clear();
        self.used_bytes = 0;
        self.generation_id = self.generation_id.wrapping_add(1);
    }

    pub fn contains(&self, id: HeapId) -> bool {
        id.space == AllocationSpace::New && id.generation == self.generation_id && (id.index as usize) < self.slots.len()
    }
}

/// The Young Generation nursery composed of two equal semi-spaces: FromSpace and ToSpace.
#[derive(Clone, Debug)]
pub struct NewSpace {
    pub from_space: SemiSpace,
    pub to_space: SemiSpace,
}

impl NewSpace {
    pub fn new(capacity: usize) -> Self {
        Self {
            from_space: SemiSpace::new(capacity, 0),
            to_space: SemiSpace::new(capacity, 1),
        }
    }

    /// Fast bump-pointer allocation in the active FromSpace.
    pub fn allocate_raw(&mut self, payload: HeapPayload, size_bytes: usize) -> Option<HeapId> {
        self.from_space.allocate(payload, size_bytes)
    }

    /// Flips FromSpace and ToSpace after a Scavenge cycle.
    pub fn flip(&mut self) {
        // The old FromSpace becomes the new ToSpace and is reset.
        self.from_space.reset();
        std::mem::swap(&mut self.from_space, &mut self.to_space);
    }

    pub fn get(&self, id: HeapId) -> Option<&HeapObject> {
        if self.from_space.contains(id) {
            self.from_space.get(id.index)
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, id: HeapId) -> Option<&mut HeapObject> {
        if self.from_space.contains(id) {
            self.from_space.get_mut(id.index)
        } else {
            None
        }
    }

    pub fn used_bytes(&self) -> usize {
        self.from_space.used_bytes
    }

    pub fn capacity(&self) -> usize {
        self.from_space.capacity
    }

    pub fn is_full(&self) -> bool {
        self.from_space.slots.len() >= self.from_space.capacity
    }
}

/// The Old Generation space for tenured objects that survived multiple scavenge cycles.
#[derive(Clone, Debug)]
pub struct OldSpace {
    pub slots: Vec<Option<HeapObject>>,
    pub free_list: Vec<u32>,
    pub used_bytes: usize,
    pub generation_id: u32,
}

impl OldSpace {
    pub fn new(initial_capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(initial_capacity),
            free_list: Vec::new(),
            used_bytes: 0,
            generation_id: 100,
        }
    }

    /// Allocates an object into OldSpace, reusing free list slots if available.
    pub fn allocate(&mut self, payload: HeapPayload, size_bytes: usize) -> HeapId {
        if let Some(index) = self.free_list.pop() {
            let id = HeapId::new(AllocationSpace::Old, index, self.generation_id);
            let obj = HeapObject::new(id, payload, size_bytes);
            self.used_bytes += size_bytes;
            self.slots[index as usize] = Some(obj);
            id
        } else {
            let index = self.slots.len() as u32;
            let id = HeapId::new(AllocationSpace::Old, index, self.generation_id);
            let obj = HeapObject::new(id, payload, size_bytes);
            self.used_bytes += size_bytes;
            self.slots.push(Some(obj));
            id
        }
    }

    /// Places an existing HeapObject into OldSpace during tenuring promotion.
    pub fn place(&mut self, mut obj: HeapObject) -> HeapId {
        let size_bytes = obj.header.size_bytes;
        self.used_bytes += size_bytes;
        obj.header.space = AllocationSpace::Old;

        if let Some(index) = self.free_list.pop() {
            let id = HeapId::new(AllocationSpace::Old, index, self.generation_id);
            obj.id = id;
            self.slots[index as usize] = Some(obj);
            id
        } else {
            let index = self.slots.len() as u32;
            let id = HeapId::new(AllocationSpace::Old, index, self.generation_id);
            obj.id = id;
            self.slots.push(Some(obj));
            id
        }
    }

    /// Frees an object slot in OldSpace and adds it to the free list.
    pub fn free(&mut self, index: u32) {
        if let Some(slot) = self.slots.get_mut(index as usize) {
            if let Some(obj) = slot.take() {
                self.used_bytes = self.used_bytes.saturating_sub(obj.header.size_bytes);
                self.free_list.push(index);
            }
        }
    }

    pub fn get(&self, index: u32) -> Option<&HeapObject> {
        self.slots.get(index as usize).and_then(|slot| slot.as_ref())
    }

    pub fn get_mut(&mut self, index: u32) -> Option<&mut HeapObject> {
        self.slots.get_mut(index as usize).and_then(|slot| slot.as_mut())
    }

    pub fn contains(&self, id: HeapId) -> bool {
        id.space == AllocationSpace::Old && (id.index as usize) < self.slots.len() && self.slots[id.index as usize].is_some()
    }

    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    /// Concurrently sweeps unreachable (White) objects in OldSpace using a background worker thread.
    /// Returns the count of reclaimed objects and adds their slot indices back to the free list.
    pub fn sweep_concurrent(&mut self) -> usize {
        if self.slots.is_empty() {
            return 0;
        }

        // Extract metadata needed by the background sweeping worker thread
        let slot_info: Vec<(u32, bool, usize)> = self
            .slots
            .iter()
            .enumerate()
            .map(|(idx, slot)| match slot {
                Some(obj) => (
                    idx as u32,
                    obj.header.color == MarkColor::White,
                    obj.header.size_bytes,
                ),
                None => (idx as u32, false, 0),
            })
            .collect();

        // Spawn concurrent background sweeping thread (std::thread + mpsc channel)
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let mut dead_indices = Vec::new();
            let mut freed_bytes = 0usize;
            for (idx, is_white, size) in slot_info {
                if is_white {
                    dead_indices.push(idx);
                    freed_bytes += size;
                }
            }
            let _ = tx.send((dead_indices, freed_bytes));
        });

        // Join concurrent sweeper thread and process results
        let _ = handle.join();
        if let Ok((dead_indices, freed_bytes)) = rx.recv() {
            let reclaimed = dead_indices.len();
            for &idx in &dead_indices {
                if let Some(slot) = self.slots.get_mut(idx as usize) {
                    *slot = None;
                    self.free_list.push(idx);
                }
            }
            self.used_bytes = self.used_bytes.saturating_sub(freed_bytes);

            // Reset surviving live objects to White for the next GC cycle
            for slot in &mut self.slots {
                if let Some(ref mut obj) = slot {
                    obj.header.color = MarkColor::White;
                }
            }

            reclaimed
        } else {
            0
        }
    }
}
