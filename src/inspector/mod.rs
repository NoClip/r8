//! Safe Rust reimplementation of Google V8's Chrome DevTools Protocol (CDP) & Inspector subsystem.
//!
//! Provides JSON-RPC based debugging, evaluation, and profiling capabilities
//! compatible with Chrome DevTools Protocol.

pub mod cpu_profile;
pub mod heap_snapshot;
pub mod protocol;
pub mod server;
pub mod session;

pub use cpu_profile::{export_cpu_profile, CpuProfileBuilder, CpuProfileNode};
pub use heap_snapshot::{export_heap_snapshot, HeapSnapshotBuilder};
pub use protocol::{CdpRequest, CdpResponse};
pub use server::{
    compute_websocket_accept, decode_websocket_frame, encode_websocket_frame, handle_http_discovery,
    sha1, InspectorServer,
};
pub use session::{Breakpoint, InspectorSession};
