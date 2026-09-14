//! Safe Rust reimplementation of Google V8's Young Generation Copying Collector (`Scavenger`).
//!
//! Employs Cheney's copying algorithm with generational tenuring:
//! - Reachable objects in FromSpace are evacuated to ToSpace.
//! - Objects surviving >= TENURING_THRESHOLD cycles are promoted to OldSpace.
//! - Semi-spaces are flipped upon completion, instantly reclaiming all unreachable garbage.

use super::handle::HandleScopeManager;
use super::heap_object::{AllocationSpace, HeapId, HeapObject};
use super::spaces::{NewSpace, OldSpace};
use super::store_buffer::{StoreBuffer, StoreBufferEntry};
use std::collections::HashMap;

/// Survival threshold before an object is promoted to the Old Generation.
pub const TENURING_THRESHOLD: u8 = 2;

pub struct Scavenger<'a> {
    pub new_space: &'a mut NewSpace,
    pub old_space: &'a mut OldSpace,
    pub store_buffer: &'a mut StoreBuffer,
    pub handle_scope: &'a mut HandleScopeManager,
    pub global_roots: &'a mut Vec<HeapId>,
}

impl<'a> Scavenger<'a> {
    pub fn new(
        new_space: &'a mut NewSpace,
        old_space: &'a mut OldSpace,
        store_buffer: &'a mut StoreBuffer,
        handle_scope: &'a mut HandleScopeManager,
        global_roots: &'a mut Vec<HeapId>,
    ) -> Self {
        Self {
            new_space,
            old_space,
            store_buffer,
            handle_scope,
            global_roots,
        }
    }

    /// Executes a complete Scavenge garbage collection cycle.
    pub fn collect(&mut self) -> usize {
        let mut forwarding_map: HashMap<HeapId, HeapId> = HashMap::new();
        let mut worklist: Vec<HeapId> = Vec::new();

        // 1. Gather all roots: HandleScope roots, global roots, and StoreBuffer targets.
        let mut root_ids: Vec<HeapId> = Vec::new();
        root_ids.extend_from_slice(self.handle_scope.roots());
        root_ids.extend_from_slice(self.global_roots);
        for entry in self.store_buffer.entries() {
            root_ids.push(entry.target);
        }

        // 2. Evacuate roots that reside in FromSpace
        for root_id in root_ids {
            if self.new_space.from_space.contains(root_id) {
                self.evacuate_object(root_id, &mut forwarding_map, &mut worklist);
            }
        }

        // 3. Cheney scan: process transitively reachable children
        while let Some(current_id) = worklist.pop() {
            // Obtain references from the evacuated object
            let references = self.get_object_references(current_id);
            let mut updated_references = Vec::new();

            for ref_id in references {
                if self.new_space.from_space.contains(ref_id) {
                    let new_ref_id = self.evacuate_object(ref_id, &mut forwarding_map, &mut worklist);
                    updated_references.push(new_ref_id);
                } else if let Some(&forwarded) = forwarding_map.get(&ref_id) {
                    updated_references.push(forwarded);
                } else {
                    updated_references.push(ref_id);
                }
            }

            self.update_object_references(current_id, updated_references);
        }

        // 4. Update handles in HandleScopeManager
        for (from, to) in &forwarding_map {
            self.handle_scope.update_forwarded(*from, *to);
        }

        // 5. Update global roots
        for root in self.global_roots.iter_mut() {
            if let Some(&new_id) = forwarding_map.get(root) {
                *root = new_id;
            }
        }

        // 6. Update references in OldSpace objects that pointed into FromSpace
        for entry in self.store_buffer.entries().to_vec() {
            if let Some(old_obj) = self.old_space.get_mut(entry.source.index) {
                if let Some(&new_target) = forwarding_map.get(&entry.target) {
                    old_obj.remove_reference(entry.target);
                    old_obj.add_reference(new_target);
                }
            }
        }

        // 7. Update StoreBuffer: retain only Old-to-New references (drop Old-to-Old where target was tenured)
        let mut new_store_entries = Vec::new();
        for entry in self.store_buffer.entries() {
            let target = forwarding_map.get(&entry.target).copied().unwrap_or(entry.target);
            if target.space == AllocationSpace::New {
                new_store_entries.push(StoreBufferEntry {
                    source: entry.source,
                    target,
                });
            }
        }
        self.store_buffer.clear();
        for entry in new_store_entries {
            self.store_buffer.record_write(entry.source, entry.target);
        }

        let evacuated_count = forwarding_map.len();

        // 8. Flip semi-spaces (FromSpace <=> ToSpace). Old FromSpace is completely reset!
        self.new_space.flip();

        evacuated_count
    }

    /// Evacuates an object from FromSpace either to ToSpace or promotes to OldSpace.
    fn evacuate_object(
        &mut self,
        id: HeapId,
        forwarding_map: &mut HashMap<HeapId, HeapId>,
        worklist: &mut Vec<HeapId>,
    ) -> HeapId {
        if let Some(&forwarded) = forwarding_map.get(&id) {
            return forwarded;
        }

        let slot = self.new_space.from_space.get(id.index).cloned();
        if let Some(mut obj) = slot {
            let new_age = obj.header.age + 1;
            if new_age >= TENURING_THRESHOLD {
                // Promote to OldSpace
                obj.header.age = new_age;
                let new_id = self.old_space.place(obj);
                forwarding_map.insert(id, new_id);
                worklist.push(new_id);
                new_id
            } else {
                // Evacuate to ToSpace
                obj.header.age = new_age;
                let new_id = self.new_space.to_space.place(obj).expect("ToSpace capacity exceeded during scavenge");
                forwarding_map.insert(id, new_id);
                worklist.push(new_id);
                new_id
            }
        } else {
            id
        }
    }

    fn get_object_references(&self, id: HeapId) -> Vec<HeapId> {
        match id.space {
            AllocationSpace::New => self
                .new_space
                .to_space
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

    fn update_object_references(&mut self, id: HeapId, references: Vec<HeapId>) {
        let obj: Option<&mut HeapObject> = match id.space {
            AllocationSpace::New => self.new_space.to_space.get_mut(id.index),
            AllocationSpace::Old => self.old_space.get_mut(id.index),
        };
        if let Some(obj) = obj {
            obj.outgoing_references = references;
        }
    }
}
