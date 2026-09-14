//! Safe Rust reimplementation of Google V8's central `Heap` coordinator.
//!
//! Manages generational spaces (NewSpace and OldSpace), triggers Scavenger and
//! Mark-Sweep GC cycles, tracks roots, maintains HandleScopes, and records
//! generational write barriers via the StoreBuffer.

use super::handle::{Handle, HandleScopeManager};
use super::heap_object::{AllocationSpace, HeapId, HeapObject, HeapPayload};
use super::mark_sweep::MarkSweepCollector;
use super::scavenger::Scavenger;
use super::spaces::{NewSpace, OldSpace};
use super::store_buffer::StoreBuffer;

/// Type of Garbage Collection cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GarbageCollectionType {
    /// Young generation copying collector.
    Scavenge,
    /// Full heap mark-sweep collector.
    MarkSweep,
    /// Concurrent OldSpace background sweeping collector.
    ConcurrentMarkSweep,
}

/// Statistics reporting heap memory usage and collection counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeapStats {
    pub used_heap_size: usize,
    pub total_heap_capacity: usize,
    pub scavenge_count: usize,
    pub mark_sweep_count: usize,
    pub total_allocated_bytes: usize,
}

/// The V8 Heap manager coordinating memory allocation and garbage collection.
pub struct Heap {
    pub new_space: NewSpace,
    pub old_space: OldSpace,
    pub store_buffer: StoreBuffer,
    pub handle_scope: HandleScopeManager,
    pub global_roots: Vec<HeapId>,

    pub scavenge_count: usize,
    pub mark_sweep_count: usize,
    pub total_allocated_bytes: usize,
}

impl Heap {
    pub fn new(new_space_capacity: usize, old_space_capacity: usize) -> Self {
        Self {
            new_space: NewSpace::new(new_space_capacity),
            old_space: OldSpace::new(old_space_capacity),
            store_buffer: StoreBuffer::new(),
            handle_scope: HandleScopeManager::new(),
            global_roots: Vec::new(),
            scavenge_count: 0,
            mark_sweep_count: 0,
            total_allocated_bytes: 0,
        }
    }

    /// Allocates an object into NewSpace (nursery). If the semi-space is full,
    /// automatically triggers a Scavenge cycle.
    pub fn allocate(&mut self, payload: HeapPayload, size_bytes: usize) -> HeapId {
        if self.new_space.is_full() {
            self.collect_garbage(GarbageCollectionType::Scavenge);
        }

        self.total_allocated_bytes += size_bytes;

        // Try allocating in nursery
        if let Some(id) = self.new_space.allocate_raw(payload.clone(), size_bytes) {
            id
        } else {
            // If nursery remains full after scavenge, allocate in OldSpace directly
            self.old_space.allocate(payload, size_bytes)
        }
    }

    /// Allocates an object directly into OldSpace (tenured space).
    pub fn allocate_old(&mut self, payload: HeapPayload, size_bytes: usize) -> HeapId {
        self.total_allocated_bytes += size_bytes;
        self.old_space.allocate(payload, size_bytes)
    }

    /// Retrieves an immutable reference to a heap object.
    pub fn get(&self, id: HeapId) -> Option<&HeapObject> {
        match id.space {
            AllocationSpace::New => self.new_space.get(id),
            AllocationSpace::Old => self.old_space.get(id.index),
        }
    }

    /// Retrieves a mutable reference to a heap object.
    pub fn get_mut(&mut self, id: HeapId) -> Option<&mut HeapObject> {
        match id.space {
            AllocationSpace::New => self.new_space.get_mut(id),
            AllocationSpace::Old => self.old_space.get_mut(id.index),
        }
    }

    /// Registers a pointer edge from `source` to `target`, checking the
    /// generational write barrier if `source` is Old and `target` is New.
    pub fn write_barrier(&mut self, source: HeapId, target: HeapId) {
        if let Some(source_obj) = self.get_mut(source) {
            source_obj.add_reference(target);
        }
        self.store_buffer.record_write(source, target);
    }

    /// Adds an object to the global roots set.
    pub fn add_global_root(&mut self, id: HeapId) {
        if !self.global_roots.contains(&id) {
            self.global_roots.push(id);
        }
    }

    /// Removes an object from the global roots set.
    pub fn remove_global_root(&mut self, id: HeapId) {
        self.global_roots.retain(|&r| r != id);
    }

    /// Opens a new HandleScope.
    pub fn open_handle_scope(&mut self) -> usize {
        self.handle_scope.open_scope()
    }

    /// Closes the current HandleScope, unrooting its handles.
    pub fn close_handle_scope(&mut self) {
        self.handle_scope.close_scope();
    }

    /// Creates and roots a new typed Handle in the active HandleScope.
    pub fn create_handle<T>(&mut self, id: HeapId) -> Handle<T> {
        self.handle_scope.register(id);
        Handle::new(id)
    }

    /// Triggers garbage collection of the requested type.
    pub fn collect_garbage(&mut self, gc_type: GarbageCollectionType) -> usize {
        match gc_type {
            GarbageCollectionType::Scavenge => {
                self.scavenge_count += 1;
                let mut scavenger = Scavenger::new(
                    &mut self.new_space,
                    &mut self.old_space,
                    &mut self.store_buffer,
                    &mut self.handle_scope,
                    &mut self.global_roots,
                );
                scavenger.collect()
            }
            GarbageCollectionType::MarkSweep => {
                self.mark_sweep_count += 1;
                let mut collector = MarkSweepCollector::new(
                    &mut self.new_space,
                    &mut self.old_space,
                    &mut self.store_buffer,
                    &mut self.handle_scope,
                    &self.global_roots,
                );
                collector.collect()
            }
            GarbageCollectionType::ConcurrentMarkSweep => {
                self.mark_sweep_count += 1;
                let mut collector = MarkSweepCollector::new(
                    &mut self.new_space,
                    &mut self.old_space,
                    &mut self.store_buffer,
                    &mut self.handle_scope,
                    &self.global_roots,
                );
                collector.collect_concurrent()
            }
        }
    }

    /// Returns current heap statistics.
    pub fn stats(&self) -> HeapStats {
        let used = self.new_space.used_bytes() + self.old_space.used_bytes();
        let capacity = self.new_space.capacity() + (self.old_space.slots.len() * 64);
        HeapStats {
            used_heap_size: used,
            total_heap_capacity: capacity,
            scavenge_count: self.scavenge_count,
            mark_sweep_count: self.mark_sweep_count,
            total_allocated_bytes: self.total_allocated_bytes,
        }
    }
}
