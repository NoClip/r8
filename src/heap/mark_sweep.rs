//! Safe Rust reimplementation of Google V8's Full Mark-Sweep Collector.
//!
//! Performs tri-color marking across all generations (OldSpace and NewSpace)
//! and sweeps unreachable objects, cleanly collecting cyclic data structures.

use super::handle::HandleScopeManager;
use super::heap_object::{AllocationSpace, HeapId, MarkColor};
use super::spaces::{NewSpace, OldSpace};
use super::store_buffer::StoreBuffer;

pub struct MarkSweepCollector<'a> {
    pub new_space: &'a mut NewSpace,
    pub old_space: &'a mut OldSpace,
    pub store_buffer: &'a mut StoreBuffer,
    pub handle_scope: &'a mut HandleScopeManager,
    pub global_roots: &'a [HeapId],
}

impl<'a> MarkSweepCollector<'a> {
    pub fn new(
        new_space: &'a mut NewSpace,
        old_space: &'a mut OldSpace,
        store_buffer: &'a mut StoreBuffer,
        handle_scope: &'a mut HandleScopeManager,
        global_roots: &'a [HeapId],
    ) -> Self {
        Self {
            new_space,
            old_space,
            store_buffer,
            handle_scope,
            global_roots,
        }
    }

    /// Executes full Mark-Sweep garbage collection, returning the number of reclaimed objects.
    pub fn collect(&mut self) -> usize {
        // 1. Initialize all live objects to White
        self.reset_all_colors();

        // 2. Tri-color marking from roots
        let mut grey_worklist: Vec<HeapId> = Vec::new();

        // Seed worklist with global roots and HandleScope roots
        for &root_id in self.global_roots {
            self.paint_grey(root_id, &mut grey_worklist);
        }
        let handle_roots: Vec<HeapId> = self.handle_scope.roots().to_vec();
        for &root_id in &handle_roots {
            self.paint_grey(root_id, &mut grey_worklist);
        }

        // Process grey worklist
        while let Some(current_id) = grey_worklist.pop() {
            let references = self.get_object_references(current_id);
            for child_id in references {
                self.paint_grey(child_id, &mut grey_worklist);
            }
            // Mark current object Black
            self.set_color(current_id, MarkColor::Black);
        }

        // 3. Sweeping phase: reclaim all White objects
        let mut reclaimed = 0;

        // Sweep OldSpace
        let old_len = self.old_space.slots.len();
        for index in 0..old_len {
            if let Some(ref obj) = self.old_space.slots[index] {
                if obj.header.color == MarkColor::White {
                    self.old_space.free(index as u32);
                    reclaimed += 1;
                } else {
                    // Reset to White for the next GC
                    if let Some(ref mut obj_mut) = self.old_space.slots[index] {
                        obj_mut.header.color = MarkColor::White;
                    }
                }
            }
        }

        // Sweep NewSpace (FromSpace)
        for slot in &mut self.new_space.from_space.slots {
            if let Some(ref obj) = slot {
                if obj.header.color == MarkColor::White {
                    let size = obj.header.size_bytes;
                    self.new_space.from_space.used_bytes = self
                        .new_space
                        .from_space
                        .used_bytes
                        .saturating_sub(size);
                    *slot = None;
                    reclaimed += 1;
                } else {
                    // Reset to White for the next GC
                    if let Some(ref mut obj_mut) = slot {
                        obj_mut.header.color = MarkColor::White;
                    }
                }
            }
        }

        // 4. Retain only StoreBuffer entries where both source and target are still alive
        let old_space = &self.old_space;
        let new_space = &self.new_space;
        self.store_buffer.retain(|entry| {
            let source_alive = old_space.contains(entry.source);
            let target_alive = new_space.from_space.contains(entry.target)
                || old_space.contains(entry.target);
            source_alive && target_alive
        });

        reclaimed
    }

    /// Executes full Mark-Sweep garbage collection using concurrent background sweeping for OldSpace.
    pub fn collect_concurrent(&mut self) -> usize {
        // 1. Initialize all live objects to White
        self.reset_all_colors();

        // 2. Tri-color marking from roots
        let mut grey_worklist: Vec<HeapId> = Vec::new();

        // Seed worklist with global roots and HandleScope roots
        for &root_id in self.global_roots {
            self.paint_grey(root_id, &mut grey_worklist);
        }
        let handle_roots: Vec<HeapId> = self.handle_scope.roots().to_vec();
        for &root_id in &handle_roots {
            self.paint_grey(root_id, &mut grey_worklist);
        }

        // Process grey worklist
        while let Some(current_id) = grey_worklist.pop() {
            let references = self.get_object_references(current_id);
            for child_id in references {
                self.paint_grey(child_id, &mut grey_worklist);
            }
            // Mark current object Black
            self.set_color(current_id, MarkColor::Black);
        }

        // 3. Sweeping phase:
        // Concurrently sweep OldSpace in background worker thread
        let old_reclaimed = self.old_space.sweep_concurrent();

        // Sweep NewSpace (FromSpace) on main thread
        let mut new_reclaimed = 0;
        for slot in &mut self.new_space.from_space.slots {
            if let Some(ref obj) = slot {
                if obj.header.color == MarkColor::White {
                    let size = obj.header.size_bytes;
                    self.new_space.from_space.used_bytes = self
                        .new_space
                        .from_space
                        .used_bytes
                        .saturating_sub(size);
                    *slot = None;
                    new_reclaimed += 1;
                } else {
                    if let Some(ref mut obj_mut) = slot {
                        obj_mut.header.color = MarkColor::White;
                    }
                }
            }
        }

        // 4. Retain only StoreBuffer entries where both source and target are still alive
        let old_space = &self.old_space;
        let new_space = &self.new_space;
        self.store_buffer.retain(|entry| {
            let source_alive = old_space.contains(entry.source);
            let target_alive = new_space.from_space.contains(entry.target)
                || old_space.contains(entry.target);
            source_alive && target_alive
        });

        old_reclaimed + new_reclaimed
    }

    fn reset_all_colors(&mut self) {
        for slot in &mut self.new_space.from_space.slots {
            if let Some(ref mut obj) = slot {
                obj.header.color = MarkColor::White;
            }
        }
        for slot in &mut self.old_space.slots {
            if let Some(ref mut obj) = slot {
                obj.header.color = MarkColor::White;
            }
        }
    }

    fn paint_grey(&mut self, id: HeapId, worklist: &mut Vec<HeapId>) {
        let color = self.get_color(id);
        if color == Some(MarkColor::White) {
            self.set_color(id, MarkColor::Grey);
            worklist.push(id);
        }
    }

    fn get_color(&self, id: HeapId) -> Option<MarkColor> {
        match id.space {
            AllocationSpace::New => self
                .new_space
                .from_space
                .get(id.index)
                .map(|o| o.header.color),
            AllocationSpace::Old => self.old_space.get(id.index).map(|o| o.header.color),
        }
    }

    fn set_color(&mut self, id: HeapId, color: MarkColor) {
        match id.space {
            AllocationSpace::New => {
                if let Some(obj) = self.new_space.from_space.get_mut(id.index) {
                    obj.header.color = color;
                }
            }
            AllocationSpace::Old => {
                if let Some(obj) = self.old_space.get_mut(id.index) {
                    obj.header.color = color;
                }
            }
        }
    }

    fn get_object_references(&self, id: HeapId) -> Vec<HeapId> {
        match id.space {
            AllocationSpace::New => self
                .new_space
                .from_space
                .get(id.index)
                .map(|o| o.get_references().to_vec())
                .unwrap_or_default(),
            AllocationSpace::Old => self
                .old_space
                .get(id.index)
                .map(|o| o.get_references().to_vec())
                .unwrap_or_default(),
        }
    }
}
