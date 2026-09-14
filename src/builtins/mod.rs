//! Safe Rust reimplementation of Google V8's `src/builtins/` subsystem.
//!
//! Provides standard ECMAScript built-in objects: `Math`, `Object`, `Array`,
//! `String`, `Date`, `JSON`, `Promise`, `console`, and top-level utility functions.

pub mod array;
pub mod atomics;
mod bigint;
pub mod base64;
pub mod boolean;
pub mod collections;
pub mod console;
pub mod crypto;
pub mod date;
pub mod disposable_stack;
pub mod encoding;
pub mod error;
pub mod events;
pub mod function_builtin;
mod generator;
pub mod global;
pub mod intl;
pub mod iterator_helpers;
pub mod json;
pub mod math;
pub mod number;
pub mod object;
pub mod performance;
pub mod promise;
pub mod proxy_builtin;
pub mod reflect;
pub mod regexp;
pub mod shared_array_buffer;
pub mod streams;
pub mod string;
pub mod structured_clone;
pub mod symbol;
pub mod typed_arrays;
pub mod url;
pub mod weak_ref;

pub use array::{create_array_constructor, create_array_prototype};
pub use atomics::create_atomics_object;
pub use base64::{create_atob_function, create_btoa_function};
pub use bigint::{create_bigint_constructor, create_bigint_prototype};
pub use boolean::{create_boolean_constructor, create_boolean_prototype};
pub use collections::{
    create_map_constructor, create_map_prototype, create_set_constructor, create_set_prototype,
    create_weak_map_constructor, create_weak_map_prototype, create_weak_set_constructor,
    create_weak_set_prototype, new_map_instance, new_set_instance, new_weak_map_instance,
    new_weak_set_instance, same_value_zero,
};
pub use console::{clear_logs, create_console_object, get_last_log};
pub use crypto::create_crypto_object;
pub use date::{create_date_constructor, create_date_prototype};
pub use disposable_stack::{
    create_async_disposable_stack_constructor, create_async_disposable_stack_prototype,
    create_disposable_stack_constructor, create_disposable_stack_prototype,
};
pub use encoding::{create_text_decoder_constructor, create_text_encoder_constructor};
pub use error::{
    create_aggregate_error_constructor, create_base_error_constructor, create_error_constructor,
    create_error_prototype, create_suppressed_error_constructor, new_error_instance,
};
pub use function_builtin::{create_function_constructor, create_function_prototype};
pub use generator::{
    create_generator_function_constructor, create_generator_function_prototype,
    create_generator_prototype,
};
pub use global::{escape_function, is_finite_function, is_nan_function, parse_float_function, parse_int_function, unescape_function};
pub use intl::{create_intl_object, DateTimeFormatOptions, LocaleInfo, NumberFormatOptions};
pub use iterator_helpers::{create_iterator_constructor, create_iterator_prototype};
pub use json::create_json_object;
pub use math::create_math_object;
pub use number::{create_number_constructor, create_number_prototype};
pub use object::create_object_constructor;
pub use performance::create_performance_object;
pub use promise::{
    create_promise_constructor, create_promise_prototype, create_queue_microtask_function,
    get_promise_state, new_promise_capability, new_promise_instance, PROMISE_FULFILLED,
    PROMISE_PENDING, PROMISE_REJECTED,
};
pub use proxy_builtin::create_proxy_constructor;
pub use reflect::create_reflect_object;
pub use regexp::{create_regexp_constructor, create_regexp_prototype, new_regexp_instance};
pub use shared_array_buffer::{create_shared_array_buffer_constructor, create_shared_array_buffer_prototype};
pub use string::{create_string_constructor, create_string_prototype};
pub use structured_clone::create_structured_clone_function;
pub use symbol::{create_symbol_constructor, create_symbol_prototype, new_symbol};
pub use typed_arrays::{
    create_array_buffer_constructor, create_array_buffer_prototype,
    create_data_view_constructor, create_data_view_prototype,
    create_typed_array_constructor, create_typed_array_prototype,
};
pub use url::{
    create_url_constructor, create_url_search_params_constructor, decode_uri_component_function,
    decode_uri_function, encode_uri_component_function, encode_uri_function,
};
pub use events::{
    create_custom_event_constructor, create_custom_event_prototype, create_event_constructor,
    create_event_prototype, create_event_target_constructor, create_event_target_prototype,
};
pub use streams::{
    create_readable_stream_constructor, create_readable_stream_prototype,
    create_transform_stream_constructor, create_transform_stream_prototype,
    create_writable_stream_constructor, create_writable_stream_prototype,
};
pub use weak_ref::{create_finalization_registry_constructor, create_weak_ref_constructor};


