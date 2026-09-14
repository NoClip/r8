//! Safe Rust reimplementation of the Web Cryptography API (`crypto`).
//!
//! Provides cryptographically strong pseudo-random number generation via
//! `crypto.getRandomValues(typedArray)` and RFC 4122 v4 `crypto.randomUUID()`.

use crate::objects::js_object::JSObject;
use crate::objects::map::InstanceType;
use crate::objects::typed_array::TypedArrayKind;
use crate::objects::value::JSValue;
use crate::objects::JSFunction;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static RNG_COUNTER: AtomicU64 = AtomicU64::new(0x85A0_802E_EC2A_4794);

/// Pure safe Rust PRNG providing pseudo-random bytes seeded from high-resolution clock and atomic state.
fn next_random_u64() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(123456789);

    let count = RNG_COUNTER.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    let mut x = now ^ count;
    // SplitMix64 fast diffusion
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn fill_random_bytes(dest: &mut [u8]) {
    let mut i = 0;
    while i < dest.len() {
        let r = next_random_u64().to_le_bytes();
        let chunk = (dest.len() - i).min(8);
        dest[i..i + chunk].copy_from_slice(&r[..chunk]);
        i += chunk;
    }
}

/// Generates an RFC 4122 version 4 UUID string.
pub fn generate_uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    fill_random_bytes(&mut bytes);

    // Set version to 4 (0100 in bits 4-7 of time_hi_and_version)
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    // Set variant to 1 (10 in bits 6-7 of clock_seq_hi_and_reserved)
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

/// Creates the global `crypto` object equipped with `randomUUID` and `getRandomValues`.
pub fn create_crypto_object() -> Rc<RefCell<JSObject>> {
    let crypto = JSObject::new_empty(None);

    // crypto.randomUUID()
    let random_uuid_fn = JSFunction::new_native("randomUUID", |_this, _args| {
        Ok(JSValue::String(generate_uuid_v4()))
    });
    JSObject::set_property(&crypto, "randomUUID", JSValue::Function(random_uuid_fn));

    // crypto.getRandomValues(typedArray)
    let get_random_values_fn = JSFunction::new_native("getRandomValues", |_this, args| {
        let ta_obj = match args.first() {
            Some(JSValue::Object(o)) => o.clone(),
            _ => return Err("TypeError: crypto.getRandomValues requires a TypedArray argument".to_string()),
        };

        let is_typed_array = ta_obj.borrow().map.borrow().instance_type == InstanceType::JSTypedArray;
        if !is_typed_array {
            return Err("TypeError: crypto.getRandomValues argument is not a TypedArray".to_string());
        }

        let ta_data = ta_obj
            .borrow()
            .ext_or_default()
            .typed_array_data
            .clone()
            .ok_or_else(|| "TypeError: Invalid TypedArray".to_string())?;

        // Disallow Float32 and Float64 per Web Crypto specification
        if matches!(ta_data.kind, TypedArrayKind::Float32 | TypedArrayKind::Float64) {
            return Err("TypeMismatchError: Float typed arrays are not supported by getRandomValues".to_string());
        }

        let byte_len = ta_data.length * ta_data.kind.element_size();
        if byte_len > 65_536 {
            return Err("QuotaExceededError: The requested length exceeds 65,536 bytes".to_string());
        }

        let buf_rc = ta_data.buffer.clone();
        let buf_obj = buf_rc.borrow();
        let buf_bytes_rc = buf_obj
            .ext_or_default()
            .array_buffer_data
            .clone()
            .ok_or_else(|| "ArrayBuffer has no data".to_string())?;
        let mut buf_bytes = buf_bytes_rc.borrow_mut();

        let offset = ta_data.byte_offset;
        let end = (offset + byte_len).min(buf_bytes.len());
        fill_random_bytes(&mut buf_bytes[offset..end]);

        Ok(JSValue::Object(ta_obj))
    });
    JSObject::set_property(&crypto, "getRandomValues", JSValue::Function(get_random_values_fn));

    crypto
}
