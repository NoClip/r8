//! Safe Rust reimplementation of Google V8's `src/runtime/` subsystem.
//!
//! Provides `Context` and `Isolate` execution environments.

pub mod context;
pub mod module;
pub mod process;
pub mod realm;

pub use context::{
    current_global, current_super, pop_current_super, push_current_super, set_current_global,
    Context, Isolate,
};
pub use module::{
    clear_module_registry, dynamic_import, get_module, link_and_evaluate_module, register_module,
    ModuleRecord, ModuleStatus,
};
pub use process::create_process_object;
pub use realm::{create_realm_object, RealmManager};


