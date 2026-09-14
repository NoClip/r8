//! Safe Rust reimplementation of Google V8's `Handle<T>` and `HandleScope`.
//!
//! Handles provide indirect rooted references to managed heap objects.
//! When a garbage collection cycle moves objects (e.g., Scavenger evacuation),
//! registered handles are automatically updated with the forwarding addresses.

use super::heap_object::HeapId;
use std::marker::PhantomData;

/// A strongly-typed rooted handle to a heap-allocated object.
#[derive(Debug)]
pub struct Handle<T> {
    pub id: HeapId,
    _marker: PhantomData<T>,
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            _marker: PhantomData,
        }
    }
}

impl<T> Copy for Handle<T> {}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Handle<T> {}

impl<T> Handle<T> {
    pub fn new(id: HeapId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    pub fn id(&self) -> HeapId {
        self.id
    }

    pub fn update_id(&mut self, new_id: HeapId) {
        self.id = new_id;
    }
}

/// Handle storage stack tracking active handles across nested scopes.
#[derive(Clone, Debug, Default)]
pub struct HandleScopeManager {
    handles: Vec<HeapId>,
    scope_marks: Vec<usize>,
}

impl HandleScopeManager {
    pub fn new() -> Self {
        Self {
            handles: Vec::new(),
            scope_marks: Vec::new(),
        }
    }

    /// Opens a new HandleScope frame.
    pub fn open_scope(&mut self) -> usize {
        let mark = self.handles.len();
        self.scope_marks.push(mark);
        mark
    }

    /// Closes the current HandleScope frame, unrooting all handles created within it.
    pub fn close_scope(&mut self) {
        if let Some(mark) = self.scope_marks.pop() {
            self.handles.truncate(mark);
        }
    }

    /// Registers a new handle in the current scope.
    pub fn register(&mut self, id: HeapId) {
        self.handles.push(id);
    }

    /// Updates all handles pointing to `from` with `to` (used after object evacuation).
    pub fn update_forwarded(&mut self, from: HeapId, to: HeapId) {
        for handle_id in &mut self.handles {
            if *handle_id == from {
                *handle_id = to;
            }
        }
    }

    /// Returns a slice of all active handle roots.
    pub fn roots(&self) -> &[HeapId] {
        &self.handles
    }

    pub fn depth(&self) -> usize {
        self.scope_marks.len()
    }
}
