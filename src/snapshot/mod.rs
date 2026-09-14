//! Safe Rust reimplementation of Google V8's `src/snapshot/` subsystem (`mksnapshot`).
//!
//! Provides binary heap snapshot serialization and rapid deserialization for sub-millisecond
//! cold starts without re-executing initial bootstrap scripts.

pub mod serializer;
pub mod deserializer;

pub use serializer::SnapshotSerializer;
pub use deserializer::SnapshotDeserializer;
