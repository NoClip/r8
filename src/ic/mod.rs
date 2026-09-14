//! Safe Rust reimplementation of Google V8's Inline Cache subsystem (`src/ic/`).

pub mod ic;

pub use ic::{KeyedIC, LoadIC, StoreIC};
