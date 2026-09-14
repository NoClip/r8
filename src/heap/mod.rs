//! Safe Rust reimplementation of Google V8's memory management and garbage collection subsystem (`src/heap/`).
//!
//! Submodules:
//! - [`heap`]: Central `Heap` coordinator and statistics.
//! - [`spaces`]: Generational memory spaces (`NewSpace`, `SemiSpace`, `OldSpace`).
//! - [`heap_object`]: Object headers, ages, tri-color mark states, and payload variants.
//! - [`scavenger`]: Fast young-generation copying collector with Cheney evacuation and generational tenuring.
//! - [`mark_sweep`]: Full-heap mark-sweep collector with tri-color marking and cycle collection.
//! - [`handle`]: Indirect rooted `Handle<T>` and `HandleScope` lifetime management.
//! - [`store_buffer`]: Remembered set and generational write barrier.
//! - [`factory`]: High-level allocation factory for objects, arrays, maps, strings, and functions.

pub mod factory;
pub mod handle;
pub mod heap;
pub mod heap_object;
pub mod mark_sweep;
pub mod scavenger;
pub mod pointer_compression;
pub mod spaces;
pub mod store_buffer;

pub use factory::Factory;
pub use handle::{Handle, HandleScopeManager};
pub use heap::{GarbageCollectionType, Heap, HeapStats};
pub use heap_object::{AllocationSpace, HeapHeader, HeapId, HeapObject, HeapPayload, MarkColor};
pub use mark_sweep::MarkSweepCollector;
pub use pointer_compression::{CompressedHeapPage, CompressedPointer, CompressedValue, IsolateRoot};
pub use scavenger::{Scavenger, TENURING_THRESHOLD};
pub use spaces::{NewSpace, OldSpace, SemiSpace};
pub use store_buffer::{StoreBuffer, StoreBufferEntry};
