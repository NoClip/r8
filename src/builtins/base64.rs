//! Safe Rust reimplementation of the WHATWG Base64 `btoa` and `atob` APIs.
//!
//! Provides standard RFC 4648 base64 encoding and decoding using only
//! the Rust standard library (zero external crates).

use crate::objects::{JSFunction, JSValue};
use std::rc::Rc;

const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes a binary/Latin-1 string into Base64 (ECMAScript `btoa`).
pub fn btoa_encode(input: &str) -> Result<String, String> {
    let mut bytes = Vec::with_capacity(input.len());
    for ch in input.chars() {
        let cp = ch as u32;
        if cp > 0xff {
            return Err("InvalidCharacterError: The string contains characters outside the Latin1 range (0-255)".to_string());
        }
        bytes.push(cp as u8);
    }
    Ok(base64_encode(&bytes))
}

/// Encodes raw bytes into standard RFC 4648 Base64 string.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut chunks = bytes.chunks_exact(3);
    for chunk in chunks.by_ref() {
        let b0 = chunk[0] as usize;
        let b1 = chunk[1] as usize;
        let b2 = chunk[2] as usize;

        result.push(B64_CHARS[b0 >> 2] as char);
        result.push(B64_CHARS[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        result.push(B64_CHARS[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        result.push(B64_CHARS[b2 & 0x3f] as char);
    }

    let rem = chunks.remainder();
    if rem.len() == 1 {
        let b0 = rem[0] as usize;
        result.push(B64_CHARS[b0 >> 2] as char);
        result.push(B64_CHARS[(b0 & 0x03) << 4] as char);
        result.push('=');
        result.push('=');
    } else if rem.len() == 2 {
        let b0 = rem[0] as usize;
        let b1 = rem[1] as usize;
        result.push(B64_CHARS[b0 >> 2] as char);
        result.push(B64_CHARS[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        result.push(B64_CHARS[(b1 & 0x0f) << 2] as char);
        result.push('=');
    }

    result
}

fn b64_char_to_val(ch: u8) -> Result<u8, String> {
    match ch {
        b'A'..=b'Z' => Ok(ch - b'A'),
        b'a'..=b'z' => Ok(ch - b'a' + 26),
        b'0'..=b'9' => Ok(ch - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err("InvalidCharacterError: Invalid character in base64 string".to_string()),
    }
}

/// Decodes a Base64 string into binary/Latin-1 string (ECMAScript `atob`).
pub fn atob_decode(input: &str) -> Result<String, String> {
    // Strip ASCII whitespace
    let filtered: Vec<u8> = input
        .bytes()
        .filter(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c'))
        .collect();

    if filtered.len() % 4 != 0 {
        return Err("InvalidCharacterError: The string to be decoded is not correctly encoded".to_string());
    }

    let mut decoded = Vec::with_capacity(filtered.len() / 4 * 3);
    for chunk in filtered.chunks_exact(4) {
        let c0 = chunk[0];
        let c1 = chunk[1];
        let c2 = chunk[2];
        let c3 = chunk[3];

        if c0 == b'=' || c1 == b'=' {
            return Err("InvalidCharacterError: Invalid padding in base64 string".to_string());
        }

        let v0 = b64_char_to_val(c0)?;
        let v1 = b64_char_to_val(c1)?;

        let b0 = (v0 << 2) | (v1 >> 4);
        decoded.push(b0);

        if c2 != b'=' {
            let v2 = b64_char_to_val(c2)?;
            let b1 = ((v1 & 0x0f) << 4) | (v2 >> 2);
            decoded.push(b1);

            if c3 != b'=' {
                let v3 = b64_char_to_val(c3)?;
                let b2 = ((v2 & 0x03) << 6) | v3;
                decoded.push(b2);
            }
        } else if c3 != b'=' {
            return Err("InvalidCharacterError: Invalid padding in base64 string".to_string());
        }
    }

    let res: String = decoded.into_iter().map(|b| b as char).collect();
    Ok(res)
}

/// Creates the global `btoa` function.
pub fn create_btoa_function() -> Rc<JSFunction> {
    JSFunction::new_native("btoa", |_this, args| {
        let input = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let encoded = btoa_encode(&input)?;
        Ok(JSValue::String(encoded))
    })
}

/// Creates the global `atob` function.
pub fn create_atob_function() -> Rc<JSFunction> {
    JSFunction::new_native("atob", |_this, args| {
        let input = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let decoded = atob_decode(&input)?;
        Ok(JSValue::String(decoded))
    })
}
