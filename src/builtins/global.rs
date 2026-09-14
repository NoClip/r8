//! Safe Rust reimplementation of ECMAScript global utility functions.
//!
//! Provides `isNaN`, `isFinite`, `parseInt`, and `parseFloat`.

use crate::objects::{JSFunction, JSValue};
use std::rc::Rc;

pub fn is_nan_function() -> Rc<JSFunction> {
    JSFunction::new_native("isNaN", |_this, args| {
        let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
        Ok(JSValue::Boolean(n.is_nan()))
    })
}

pub fn is_finite_function() -> Rc<JSFunction> {
    JSFunction::new_native("isFinite", |_this, args| {
        let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
        Ok(JSValue::Boolean(n.is_finite()))
    })
}

pub fn parse_int_function() -> Rc<JSFunction> {
    JSFunction::new_native("parseInt", |_this, args| {
        let s = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let trimmed = s.trim();
        // Extract leading sign and digits
        let mut end = 0;
        let bytes = trimmed.as_bytes();
        if !bytes.is_empty() && (bytes[0] == b'+' || bytes[0] == b'-') {
            end += 1;
        }
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == 0 || (end == 1 && (bytes[0] == b'+' || bytes[0] == b'-')) {
            return Ok(JSValue::Number(f64::NAN));
        }
        let parsed = trimmed[..end].parse::<i64>().unwrap_or(0);
        if parsed >= i32::MIN as i64 && parsed <= i32::MAX as i64 {
            Ok(JSValue::Smi(parsed as i32))
        } else {
            Ok(JSValue::Number(parsed as f64))
        }
    })
}

pub fn parse_float_function() -> Rc<JSFunction> {
    JSFunction::new_native("parseFloat", |_this, args| {
        let s = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let trimmed = s.trim();
        let mut end = 0;
        let bytes = trimmed.as_bytes();
        let mut seen_dot = false;
        if !bytes.is_empty() && (bytes[0] == b'+' || bytes[0] == b'-') {
            end += 1;
        }
        while end < bytes.len() {
            if bytes[end].is_ascii_digit() {
                end += 1;
            } else if bytes[end] == b'.' && !seen_dot {
                seen_dot = true;
                end += 1;
            } else {
                break;
            }
        }
        if end == 0 || (end == 1 && (bytes[0] == b'+' || bytes[0] == b'-')) {
            return Ok(JSValue::Number(f64::NAN));
        }
        let parsed = trimmed[..end].parse::<f64>().unwrap_or(f64::NAN);
        Ok(JSValue::Number(parsed))
    })
}

/// Annex B.2.1.1: `escape(string)`
pub fn escape_function() -> Rc<JSFunction> {
    JSFunction::new_native("escape", |_this, args| {
        let s = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let mut result = String::with_capacity(s.len());
        for c in s.chars() {
            let cp = c as u32;
            if matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '@' | '*' | '_' | '+' | '-' | '.' | '/') {
                result.push(c);
            } else if cp < 256 {
                result.push_str(&format!("%{:02X}", cp));
            } else {
                result.push_str(&format!("%u{:04X}", cp));
            }
        }
        Ok(JSValue::String(result))
    })
}

/// Annex B.2.1.2: `unescape(string)`
pub fn unescape_function() -> Rc<JSFunction> {
    JSFunction::new_native("unescape", |_this, args| {
        let s = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let mut result = String::with_capacity(s.len());
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '%' {
                if i + 5 < chars.len() && chars[i + 1] == 'u' {
                    let hex_str: String = chars[i + 2..=i + 5].iter().collect();
                    if let Ok(cp) = u32::from_str_radix(&hex_str, 16) {
                        if let Some(ch) = char::from_u32(cp) {
                            result.push(ch);
                            i += 6;
                            continue;
                        }
                    }
                } else if i + 2 < chars.len() {
                    let hex_str: String = chars[i + 1..=i + 2].iter().collect();
                    if let Ok(cp) = u8::from_str_radix(&hex_str, 16) {
                        result.push(cp as char);
                        i += 3;
                        continue;
                    }
                }
            }
            result.push(chars[i]);
            i += 1;
        }
        Ok(JSValue::String(result))
    })
}
