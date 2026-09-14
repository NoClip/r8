//! Safe Rust reimplementation of Google V8's `src/objects/` subsystem.
//!
//! Exposes Hidden Classes (`Map`), JavaScript objects (`JSObject`), arrays (`JSArray`),
//! property details, inline caches (`FeedbackVector`), and runtime values (`JSValue`).

pub mod bigint;
pub mod feedback_vector;
pub mod function;
pub mod generator;
pub mod js_array;
pub mod js_object;
pub mod map;
pub mod property_details;
pub mod proxy;
pub mod typed_array;
pub mod value;

pub use bigint::BigIntData;
pub use feedback_vector::{FeedbackSlot, FeedbackVector, InlineCacheState};
pub use function::{FunctionKind, JSFunction, NativeCallback};
pub use generator::*;
pub use js_array::JSArray;
pub use js_object::{JSObject, RegExpData};
pub use map::{Descriptor, InstanceType, Map};
pub use property_details::{attributes, PropertyAttributes, PropertyConstness, PropertyDetails, PropertyKind, PropertyLocation, Representation};
pub use proxy::ProxyData;
pub use typed_array::{DataViewData, TypedArrayData, TypedArrayKind};
pub use value::{JSValue, SymbolData};
