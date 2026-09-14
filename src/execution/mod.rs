//! Safe Rust reimplementation of Google V8's `src/execution/` subsystem.
//!
//! Provides the execution environment, microtask queue, and asynchronous task coordination.

pub mod microtask_queue;
pub mod timer_queue;
pub mod worker;

pub use microtask_queue::{Microtask, MicrotaskQueue};
pub use timer_queue::{Timer, TimerQueue};
pub use worker::{create_worker_constructor, WorkerHandle};

