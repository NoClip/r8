//! Safe Rust reimplementation of Google V8's ECMAScript `String` built-in constructor and prototype.
//!
//! Implements `toUpperCase`, `toLowerCase`, `trim`, `trimStart`, `trimEnd`,
//! `slice`, `substring`, `indexOf`, `lastIndexOf`, `includes`, `startsWith`,
//! `endsWith`, `repeat`, `charAt`, `charCodeAt`, `split`, `replace`, `replaceAll`,
//! `padStart`, `padEnd`, `concat`, and static `String.fromCharCode`.

use crate::builtins::regexp::new_regexp_instance;
use crate::objects::{JSArray, JSFunction, JSObject, JSValue};
use crate::regexp::RegExpEngine;
use std::cell::RefCell;
use std::rc::Rc;

/// Helper function to perform ECMAScript `$1`, `$&`, `$'`, etc. replacement string expansions.
fn expand_replacement_string(
    template: &str,
    subject: &str,
    match_start: usize,
    match_end: usize,
    captures: &[Option<String>],
) -> String {
    let mut out = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() {
            match chars[i + 1] {
                '$' => {
                    out.push('$');
                    i += 2;
                }
                '&' => {
                    if let Some(Some(ref full)) = captures.get(0) {
                        out.push_str(full);
                    } else if match_start <= match_end && match_end <= subject.len() {
                        out.push_str(&subject[match_start..match_end]);
                    }
                    i += 2;
                }
                '`' => {
                    let sub_chars: Vec<char> = subject.chars().collect();
                    let prefix: String = sub_chars[..match_start.min(sub_chars.len())].iter().collect();
                    out.push_str(&prefix);
                    i += 2;
                }
                '\'' => {
                    let sub_chars: Vec<char> = subject.chars().collect();
                    let suffix: String = sub_chars[match_end.min(sub_chars.len())..].iter().collect();
                    out.push_str(&suffix);
                    i += 2;
                }
                '0'..='9' => {
                    let d1 = (chars[i + 1] as u32 - '0' as u32) as usize;
                    if i + 2 < chars.len() && chars[i + 2].is_ascii_digit() {
                        let d2 = (chars[i + 2] as u32 - '0' as u32) as usize;
                        let two_digits = d1 * 10 + d2;
                        if two_digits < captures.len() {
                            if let Some(Some(ref cap)) = captures.get(two_digits) {
                                out.push_str(cap);
                            }
                            i += 3;
                            continue;
                        }
                    }
                    if d1 < captures.len() {
                        if let Some(Some(ref cap)) = captures.get(d1) {
                            out.push_str(cap);
                        }
                        i += 2;
                    } else {
                        out.push('$');
                        out.push(chars[i + 1]);
                        i += 2;
                    }
                }
                _ => {
                    out.push('$');
                    out.push(chars[i + 1]);
                    i += 2;
                }
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Creates the `String.prototype` object equipped with ECMAScript string manipulation methods.
pub fn create_string_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // String.prototype.toUpperCase()
    JSObject::set_property(
        &proto,
        "toUpperCase",
        JSValue::Function(JSFunction::new_native("toUpperCase", |this, _args| {
            let s = this.to_string_val();
            Ok(JSValue::String(s.to_uppercase()))
        })),
    );

    // String.prototype.toLowerCase()
    JSObject::set_property(
        &proto,
        "toLowerCase",
        JSValue::Function(JSFunction::new_native("toLowerCase", |this, _args| {
            let s = this.to_string_val();
            Ok(JSValue::String(s.to_lowercase()))
        })),
    );

    // String.prototype.trim()
    JSObject::set_property(
        &proto,
        "trim",
        JSValue::Function(JSFunction::new_native("trim", |this, _args| {
            let s = this.to_string_val();
            Ok(JSValue::String(s.trim().to_string()))
        })),
    );

    // String.prototype.trimStart()
    JSObject::set_property(
        &proto,
        "trimStart",
        JSValue::Function(JSFunction::new_native("trimStart", |this, _args| {
            let s = this.to_string_val();
            Ok(JSValue::String(s.trim_start().to_string()))
        })),
    );

    // String.prototype.trimEnd()
    JSObject::set_property(
        &proto,
        "trimEnd",
        JSValue::Function(JSFunction::new_native("trimEnd", |this, _args| {
            let s = this.to_string_val();
            Ok(JSValue::String(s.trim_end().to_string()))
        })),
    );

    // String.prototype.slice(start, end)
    JSObject::set_property(
        &proto,
        "slice",
        JSValue::Function(JSFunction::new_native("slice", |this, args| {
            let s = this.to_string_val();
            if s.is_ascii() {
                let bytes = s.as_bytes();
                let len = bytes.len() as isize;
                let start = args.get(0).map(|v| v.to_number() as isize).unwrap_or(0);
                let end = args.get(1).map(|v| v.to_number() as isize).unwrap_or(len);
                let norm_start = if start < 0 {
                    (len + start).max(0) as usize
                } else {
                    start.min(len) as usize
                };
                let norm_end = if end < 0 {
                    (len + end).max(0) as usize
                } else {
                    end.min(len) as usize
                };
                return if norm_start < norm_end {
                    Ok(JSValue::String(unsafe { std::str::from_utf8_unchecked(&bytes[norm_start..norm_end]) }.to_string()))
                } else {
                    Ok(JSValue::String(String::new()))
                };
            }
            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as isize;

            let start = args.get(0).map(|v| v.to_number() as isize).unwrap_or(0);
            let end = args.get(1).map(|v| v.to_number() as isize).unwrap_or(len);

            let norm_start = if start < 0 {
                (len + start).max(0) as usize
            } else {
                start.min(len) as usize
            };

            let norm_end = if end < 0 {
                (len + end).max(0) as usize
            } else {
                end.min(len) as usize
            };

            if norm_start < norm_end {
                let sliced: String = chars[norm_start..norm_end].iter().collect();
                Ok(JSValue::String(sliced))
            } else {
                Ok(JSValue::String(String::new()))
            }
        })),
    );

    // String.prototype.substring(start, end)
    JSObject::set_property(
        &proto,
        "substring",
        JSValue::Function(JSFunction::new_native("substring", |this, args| {
            let s = this.to_string_val();
            if s.is_ascii() {
                let bytes = s.as_bytes();
                let len = bytes.len();

                let mut start = args.get(0).map(|v| {
                    let n = v.to_number();
                    if n.is_nan() || n < 0.0 { 0 } else { (n as usize).min(len) }
                }).unwrap_or(0);

                let mut end = args.get(1).map(|v| {
                    let n = v.to_number();
                    if n.is_nan() || n < 0.0 { 0 } else { (n as usize).min(len) }
                }).unwrap_or(len);

                if start > end {
                    std::mem::swap(&mut start, &mut end);
                }

                return Ok(JSValue::String(unsafe { std::str::from_utf8_unchecked(&bytes[start..end]) }.to_string()));
            }

            let chars: Vec<char> = s.chars().collect();
            let len = chars.len();

            let mut start = args.get(0).map(|v| {
                let n = v.to_number();
                if n.is_nan() || n < 0.0 { 0 } else { (n as usize).min(len) }
            }).unwrap_or(0);

            let mut end = args.get(1).map(|v| {
                let n = v.to_number();
                if n.is_nan() || n < 0.0 { 0 } else { (n as usize).min(len) }
            }).unwrap_or(len);

            if start > end {
                std::mem::swap(&mut start, &mut end);
            }

            let res: String = chars[start..end].iter().collect();
            Ok(JSValue::String(res))
        })),
    );


    // String.prototype.indexOf(searchValue, fromIndex)
    JSObject::set_property(
        &proto,
        "indexOf",
        JSValue::Function(JSFunction::new_native("indexOf", |this, args| {
            let s = this.to_string_val();
            let search = args.get(0).map(|v| v.to_string_val()).unwrap_or_else(|| "undefined".to_string());
            let from_pos = args.get(1).map(|v| {
                let n = v.to_number();
                if n < 0.0 { 0 } else { n as usize }
            }).unwrap_or(0);

            let chars: Vec<char> = s.chars().collect();
            let search_chars: Vec<char> = search.chars().collect();

            if search_chars.is_empty() {
                let res = from_pos.min(chars.len());
                return Ok(JSValue::Smi(res as i32));
            }

            if from_pos >= chars.len() {
                return Ok(JSValue::Smi(-1));
            }

            for i in from_pos..=chars.len().saturating_sub(search_chars.len()) {
                if chars[i..i + search_chars.len()] == search_chars[..] {
                    return Ok(JSValue::Smi(i as i32));
                }
            }

            Ok(JSValue::Smi(-1))
        })),
    );

    // String.prototype.lastIndexOf(searchValue, fromIndex)
    JSObject::set_property(
        &proto,
        "lastIndexOf",
        JSValue::Function(JSFunction::new_native("lastIndexOf", |this, args| {
            let s = this.to_string_val();
            let search = args.get(0).map(|v| v.to_string_val()).unwrap_or_else(|| "undefined".to_string());
            let chars: Vec<char> = s.chars().collect();
            let search_chars: Vec<char> = search.chars().collect();

            let from_pos = args.get(1).map(|v| {
                let n = v.to_number();
                if n.is_nan() { chars.len() } else if n < 0.0 { 0 } else { (n as usize).min(chars.len()) }
            }).unwrap_or(chars.len());

            if search_chars.is_empty() {
                return Ok(JSValue::Smi(from_pos.min(chars.len()) as i32));
            }

            let max_start = from_pos.min(chars.len().saturating_sub(search_chars.len()));
            for i in (0..=max_start).rev() {
                if chars[i..i + search_chars.len()] == search_chars[..] {
                    return Ok(JSValue::Smi(i as i32));
                }
            }

            Ok(JSValue::Smi(-1))
        })),
    );

    // String.prototype.includes(searchString, position)
    JSObject::set_property(
        &proto,
        "includes",
        JSValue::Function(JSFunction::new_native("includes", |this, args| {
            let s = this.to_string_val();
            let search = args.get(0).map(|v| v.to_string_val()).unwrap_or_else(|| "undefined".to_string());
            let pos = args.get(1).map(|v| {
                let n = v.to_number();
                if n < 0.0 { 0 } else { n as usize }
            }).unwrap_or(0);

            let chars: Vec<char> = s.chars().collect();
            let search_chars: Vec<char> = search.chars().collect();

            if search_chars.is_empty() {
                return Ok(JSValue::Boolean(true));
            }
            if pos >= chars.len() {
                return Ok(JSValue::Boolean(false));
            }

            for i in pos..=chars.len().saturating_sub(search_chars.len()) {
                if chars[i..i + search_chars.len()] == search_chars[..] {
                    return Ok(JSValue::Boolean(true));
                }
            }

            Ok(JSValue::Boolean(false))
        })),
    );

    // String.prototype.startsWith(searchString, position)
    JSObject::set_property(
        &proto,
        "startsWith",
        JSValue::Function(JSFunction::new_native("startsWith", |this, args| {
            let s = this.to_string_val();
            let search = args.get(0).map(|v| v.to_string_val()).unwrap_or_else(|| "undefined".to_string());
            let pos = args.get(1).map(|v| {
                let n = v.to_number();
                if n < 0.0 { 0 } else { n as usize }
            }).unwrap_or(0);

            let chars: Vec<char> = s.chars().collect();
            let search_chars: Vec<char> = search.chars().collect();

            if pos + search_chars.len() > chars.len() {
                return Ok(JSValue::Boolean(false));
            }

            let matches = chars[pos..pos + search_chars.len()] == search_chars[..];
            Ok(JSValue::Boolean(matches))
        })),
    );

    // String.prototype.endsWith(searchString, endPosition)
    JSObject::set_property(
        &proto,
        "endsWith",
        JSValue::Function(JSFunction::new_native("endsWith", |this, args| {
            let s = this.to_string_val();
            let search = args.get(0).map(|v| v.to_string_val()).unwrap_or_else(|| "undefined".to_string());
            let chars: Vec<char> = s.chars().collect();
            let search_chars: Vec<char> = search.chars().collect();

            let end_pos = args.get(1).map(|v| {
                let n = v.to_number();
                if n.is_nan() { chars.len() } else if n < 0.0 { 0 } else { (n as usize).min(chars.len()) }
            }).unwrap_or(chars.len());

            if search_chars.len() > end_pos {
                return Ok(JSValue::Boolean(false));
            }

            let start_pos = end_pos - search_chars.len();
            let matches = chars[start_pos..end_pos] == search_chars[..];
            Ok(JSValue::Boolean(matches))
        })),
    );

    // String.prototype.repeat(count)
    JSObject::set_property(
        &proto,
        "repeat",
        JSValue::Function(JSFunction::new_native("repeat", |this, args| {
            let s = this.to_string_val();
            let count_num = args.get(0).map(|v| v.to_number()).unwrap_or(0.0);
            if count_num < 0.0 || count_num.is_infinite() {
                return Err("RangeError: Invalid count value".to_string());
            }
            let count = count_num as usize;
            Ok(JSValue::String(s.repeat(count)))
        })),
    );

    // String.prototype.charAt(index)
    JSObject::set_property(
        &proto,
        "charAt",
        JSValue::Function(JSFunction::new_native("charAt", |this, args| {
            let s = this.to_string_val();
            let idx = args.get(0).map(|v| v.to_number() as isize).unwrap_or(0);
            if idx < 0 {
                return Ok(JSValue::String(String::new()));
            }
            let ch = s.chars().nth(idx as usize).map(|c| c.to_string()).unwrap_or_default();
            Ok(JSValue::String(ch))
        })),
    );

    // String.prototype.charCodeAt(index)
    JSObject::set_property(
        &proto,
        "charCodeAt",
        JSValue::Function(JSFunction::new_native("charCodeAt", |this, args| {
            let s = this.to_string_val();
            let idx = args.get(0).map(|v| v.to_number() as isize).unwrap_or(0);
            if idx < 0 {
                return Ok(JSValue::Number(f64::NAN));
            }
            let code = s.encode_utf16().nth(idx as usize);
            match code {
                Some(c) => Ok(JSValue::Smi(c as i32)),
                None => Ok(JSValue::Number(f64::NAN)),
            }
        })),
    );

    // String.prototype.split(separator, limit)
    JSObject::set_property(
        &proto,
        "split",
        JSValue::Function(JSFunction::new_native("split", |this, args| {
            let s = this.to_string_val();
            let limit = args.get(1).map(|v| v.to_number() as usize).unwrap_or(usize::MAX);

            if limit == 0 {
                return Ok(JSValue::Array(JSArray::new_array(Vec::new())));
            }

            let sep_val = args.first();
            let re_data_opt = if let Some(JSValue::Object(ref obj)) = sep_val {
                obj.borrow().ext_or_default().regexp_data.clone()
            } else {
                None
            };

            if let Some(re_data) = re_data_opt {
                let s_chars: Vec<char> = s.chars().collect();
                let mut parts = Vec::new();
                let mut last_end = 0;
                let mut cursor = 0;

                while let Some(m) = RegExpEngine::exec(&re_data.bytecode, &s, cursor) {
                    if m.start == s_chars.len() && m.start == last_end {
                        break;
                    }
                    let chunk: String = s_chars[last_end..m.start].iter().collect();
                    parts.push(JSValue::String(chunk));
                    if parts.len() >= limit {
                        return Ok(JSValue::Array(JSArray::new_array(parts)));
                    }

                    // Push capture groups into parts per ECMAScript spec
                    for cap in m.captures.iter().skip(1) {
                        match cap {
                            Some(c) => parts.push(JSValue::String(c.clone())),
                            None => parts.push(JSValue::Undefined),
                        }
                        if parts.len() >= limit {
                            return Ok(JSValue::Array(JSArray::new_array(parts)));
                        }
                    }

                    if m.end == cursor {
                        cursor += 1;
                    } else {
                        cursor = m.end;
                    }
                    last_end = m.end;

                    if cursor > s_chars.len() {
                        break;
                    }
                }

                let tail: String = s_chars[last_end..].iter().collect();
                parts.push(JSValue::String(tail));
                if parts.len() > limit {
                    parts.truncate(limit);
                }

                Ok(JSValue::Array(JSArray::new_array(parts)))
            } else {
                let parts: Vec<JSValue> = match sep_val {
                    None => vec![JSValue::String(s)],
                    Some(sep) => {
                        let sep_str = sep.to_string_val();
                        if sep_str.is_empty() {
                            s.chars().take(limit).map(|c| JSValue::String(c.to_string())).collect()
                        } else {
                            s.split(&sep_str).take(limit).map(|p| JSValue::String(p.to_string())).collect()
                        }
                    }
                };

                Ok(JSValue::Array(JSArray::new_array(parts)))
            }
        })),
    );

    // String.prototype.replace(search, replacement)
    JSObject::set_property(
        &proto,
        "replace",
        JSValue::Function(JSFunction::new_native("replace", |this, args| {
            let s = this.to_string_val();
            let search_val = args.first().cloned().unwrap_or(JSValue::Undefined);
            let repl_val = args.get(1).cloned().unwrap_or(JSValue::Undefined);

            let re_data_opt = if let JSValue::Object(ref obj) = search_val {
                obj.borrow().ext_or_default().regexp_data.clone()
            } else {
                None
            };

            if let Some(re_data) = re_data_opt {
                let is_global = re_data.global;
                let mut matches = Vec::new();
                let mut cursor = 0;
                let s_chars: Vec<char> = s.chars().collect();

                while let Some(m) = RegExpEngine::exec(&re_data.bytecode, &s, cursor) {
                    matches.push(m.clone());
                    if !is_global {
                        break;
                    }
                    if m.end == cursor {
                        cursor += 1;
                    } else {
                        cursor = m.end;
                    }
                    if cursor > s_chars.len() {
                        break;
                    }
                }

                if matches.is_empty() {
                    return Ok(JSValue::String(s));
                }

                let mut result = String::new();
                let mut last_end = 0;

                for m in matches {
                    let before_str: String = s_chars[last_end..m.start].iter().collect();
                    result.push_str(&before_str);

                    let replacement = match repl_val {
                        JSValue::Function(ref f) => {
                            let mut cb_args = Vec::new();
                            for cap in &m.captures {
                                match cap {
                                    Some(c) => cb_args.push(JSValue::String(c.clone())),
                                    None => cb_args.push(JSValue::Undefined),
                                }
                            }
                            cb_args.push(JSValue::Smi(m.start as i32));
                            cb_args.push(JSValue::String(s.clone()));
                            f.call(&JSValue::Undefined, &cb_args)?.to_string_val()
                        }
                        _ => {
                            let template = repl_val.to_string_val();
                            expand_replacement_string(&template, &s, m.start, m.end, &m.captures)
                        }
                    };

                    result.push_str(&replacement);
                    last_end = m.end;
                }

                let remaining_str: String = s_chars[last_end..].iter().collect();
                result.push_str(&remaining_str);

                Ok(JSValue::String(result))
            } else {
                let search_str = search_val.to_string_val();
                if let Some(idx) = s.find(&search_str) {
                    let mut result = String::new();
                    result.push_str(&s[..idx]);

                    let match_end = idx + search_str.len();
                    let replacement = match repl_val {
                        JSValue::Function(ref f) => {
                            let cb_args = vec![
                                JSValue::String(search_str.clone()),
                                JSValue::Smi(idx as i32),
                                JSValue::String(s.clone()),
                            ];
                            f.call(&JSValue::Undefined, &cb_args)?.to_string_val()
                        }
                        _ => {
                            let template = repl_val.to_string_val();
                            expand_replacement_string(&template, &s, idx, match_end, &[Some(search_str)])
                        }
                    };

                    result.push_str(&replacement);
                    result.push_str(&s[match_end..]);
                    Ok(JSValue::String(result))
                } else {
                    Ok(JSValue::String(s))
                }
            }
        })),
    );

    // String.prototype.replaceAll(search, replacement)
    JSObject::set_property(
        &proto,
        "replaceAll",
        JSValue::Function(JSFunction::new_native("replaceAll", |this, args| {
            let s = this.to_string_val();
            let search_val = args.first().cloned().unwrap_or(JSValue::Undefined);
            let repl_val = args.get(1).cloned().unwrap_or(JSValue::Undefined);

            if let JSValue::Object(ref obj) = search_val {
                if let Some(ref re_data) = obj.borrow().ext_or_default().regexp_data {
                    if !re_data.global {
                        return Err("TypeError: String.prototype.replaceAll called with a non-global RegExp".to_string());
                    }

                    let mut matches = Vec::new();
                    let mut cursor = 0;
                    let s_chars: Vec<char> = s.chars().collect();

                    while let Some(m) = RegExpEngine::exec(&re_data.bytecode, &s, cursor) {
                        matches.push(m.clone());
                        if m.end == cursor {
                            cursor += 1;
                        } else {
                            cursor = m.end;
                        }
                        if cursor > s_chars.len() {
                            break;
                        }
                    }

                    if matches.is_empty() {
                        return Ok(JSValue::String(s));
                    }

                    let mut result = String::new();
                    let mut last_end = 0;

                    for m in matches {
                        let before_str: String = s_chars[last_end..m.start].iter().collect();
                        result.push_str(&before_str);

                        let replacement = match repl_val {
                            JSValue::Function(ref f) => {
                                let mut cb_args = Vec::new();
                                for cap in &m.captures {
                                    match cap {
                                        Some(c) => cb_args.push(JSValue::String(c.clone())),
                                        None => cb_args.push(JSValue::Undefined),
                                    }
                                }
                                cb_args.push(JSValue::Smi(m.start as i32));
                                cb_args.push(JSValue::String(s.clone()));
                                f.call(&JSValue::Undefined, &cb_args)?.to_string_val()
                            }
                            _ => {
                                let template = repl_val.to_string_val();
                                expand_replacement_string(&template, &s, m.start, m.end, &m.captures)
                            }
                        };

                        result.push_str(&replacement);
                        last_end = m.end;
                    }

                    let remaining_str: String = s_chars[last_end..].iter().collect();
                    result.push_str(&remaining_str);

                    return Ok(JSValue::String(result));
                }
            }

            let search_str = search_val.to_string_val();
            let repl_str = repl_val.to_string_val();
            let res = s.replace(&search_str, &repl_str);
            Ok(JSValue::String(res))
        })),
    );

    // String.prototype.match(regexp)
    JSObject::set_property(
        &proto,
        "match",
        JSValue::Function(JSFunction::new_native("match", |this, args| {
            let s = this.to_string_val();
            let arg = args.first().cloned().unwrap_or(JSValue::Undefined);

            let re_obj = match arg {
                JSValue::Object(ref obj) if obj.borrow().ext_or_default().regexp_data.is_some() => obj.clone(),
                JSValue::Undefined => new_regexp_instance("", "", None)?,
                _ => {
                    let pattern = arg.to_string_val();
                    new_regexp_instance(&pattern, "", None)?
                }
            };

            let is_global = re_obj.borrow().ext_or_default().regexp_data.as_ref().unwrap().global;
            if !is_global {
                let exec_fn = re_obj.borrow().get_property("exec");
                if let JSValue::Function(f) = exec_fn {
                    return f.call(&JSValue::Object(re_obj), &[this.clone()]);
                }
                return Ok(JSValue::Null);
            }

            re_obj.borrow_mut().ext_mut().regexp_data.as_mut().unwrap().last_index = 0;
            let bytecode = re_obj.borrow().ext_or_default().regexp_data.as_ref().unwrap().bytecode.clone();
            let s_chars: Vec<char> = s.chars().collect();
            let mut matches = Vec::new();
            let mut cursor = 0;

            while let Some(m) = RegExpEngine::exec(&bytecode, &s, cursor) {
                if let Some(Some(ref full)) = m.captures.get(0) {
                    matches.push(JSValue::String(full.clone()));
                }
                if m.end == cursor {
                    cursor += 1;
                } else {
                    cursor = m.end;
                }
                if cursor > s_chars.len() {
                    break;
                }
            }

            if matches.is_empty() {
                Ok(JSValue::Null)
            } else {
                Ok(JSValue::Array(JSArray::new_array(matches)))
            }
        })),
    );

    // String.prototype.matchAll(regexp)
    JSObject::set_property(
        &proto,
        "matchAll",
        JSValue::Function(JSFunction::new_native("matchAll", |this, args| {
            let s = this.to_string_val();
            let arg = args.first().cloned().unwrap_or(JSValue::Undefined);

            let re_obj = match arg {
                JSValue::Object(ref obj) if obj.borrow().ext_or_default().regexp_data.is_some() => {
                    if !obj.borrow().ext_or_default().regexp_data.as_ref().unwrap().global {
                        return Err("TypeError: String.prototype.matchAll called with a non-global RegExp".to_string());
                    }
                    obj.clone()
                }
                _ => {
                    let pattern = arg.to_string_val();
                    new_regexp_instance(&pattern, "g", None)?
                }
            };

            let bytecode = re_obj.borrow().ext_or_default().regexp_data.as_ref().unwrap().bytecode.clone();
            let s_chars: Vec<char> = s.chars().collect();
            let mut match_arrays = Vec::new();
            let mut cursor = 0;

            while let Some(m) = RegExpEngine::exec(&bytecode, &s, cursor) {
                let elements: Vec<JSValue> = m
                    .captures
                    .iter()
                    .map(|c| match c {
                        Some(s) => JSValue::String(s.clone()),
                        None => JSValue::Undefined,
                    })
                    .collect();

                let arr = JSArray::new_array(elements);
                JSObject::set_property(&arr, "index", JSValue::Smi(m.start as i32));
                JSObject::set_property(&arr, "input", JSValue::String(s.clone()));

                if !m.named_groups.is_empty() {
                    let groups_obj = JSObject::new_empty(None);
                    for (name, val) in &m.named_groups {
                        JSObject::set_property(&groups_obj, name, JSValue::String(val.clone()));
                    }
                    JSObject::set_property(&arr, "groups", JSValue::Object(groups_obj));
                } else {
                    JSObject::set_property(&arr, "groups", JSValue::Undefined);
                }

                match_arrays.push(JSValue::Array(arr));

                if m.end == cursor {
                    cursor += 1;
                } else {
                    cursor = m.end;
                }
                if cursor > s_chars.len() {
                    break;
                }
            }

            Ok(JSValue::Array(JSArray::new_array(match_arrays)))
        })),
    );

    // String.prototype.search(regexp)
    JSObject::set_property(
        &proto,
        "search",
        JSValue::Function(JSFunction::new_native("search", |this, args| {
            let s = this.to_string_val();
            let arg = args.first().cloned().unwrap_or(JSValue::Undefined);

            let re_obj = match arg {
                JSValue::Object(ref obj) if obj.borrow().ext_or_default().regexp_data.is_some() => obj.clone(),
                _ => {
                    let pattern = arg.to_string_val();
                    new_regexp_instance(&pattern, "", None)?
                }
            };

            let bytecode = re_obj.borrow().ext_or_default().regexp_data.as_ref().unwrap().bytecode.clone();
            match RegExpEngine::exec(&bytecode, &s, 0) {
                Some(m) => Ok(JSValue::Smi(m.start as i32)),
                None => Ok(JSValue::Smi(-1)),
            }
        })),
    );

    // String.prototype.padStart(targetLength, padString)
    JSObject::set_property(
        &proto,
        "padStart",
        JSValue::Function(JSFunction::new_native("padStart", |this, args| {
            let s = this.to_string_val();
            let target_len = args.get(0).map(|v| v.to_number() as usize).unwrap_or(0);
            let pad = args.get(1).map(|v| v.to_string_val()).unwrap_or_else(|| " ".to_string());

            let chars: Vec<char> = s.chars().collect();
            if chars.len() >= target_len || pad.is_empty() {
                return Ok(JSValue::String(s));
            }

            let pad_chars: Vec<char> = pad.chars().collect();
            let needed = target_len - chars.len();
            let mut prefix = String::with_capacity(needed);
            for i in 0..needed {
                prefix.push(pad_chars[i % pad_chars.len()]);
            }
            prefix.push_str(&s);
            Ok(JSValue::String(prefix))
        })),
    );

    // String.prototype.padEnd(targetLength, padString)
    JSObject::set_property(
        &proto,
        "padEnd",
        JSValue::Function(JSFunction::new_native("padEnd", |this, args| {
            let s = this.to_string_val();
            let target_len = args.get(0).map(|v| v.to_number() as usize).unwrap_or(0);
            let pad = args.get(1).map(|v| v.to_string_val()).unwrap_or_else(|| " ".to_string());

            let chars: Vec<char> = s.chars().collect();
            if chars.len() >= target_len || pad.is_empty() {
                return Ok(JSValue::String(s));
            }

            let pad_chars: Vec<char> = pad.chars().collect();
            let needed = target_len - chars.len();
            let mut result = s;
            for i in 0..needed {
                result.push(pad_chars[i % pad_chars.len()]);
            }
            Ok(JSValue::String(result))
        })),
    );

    // String.prototype.concat(...strings)
    JSObject::set_property(
        &proto,
        "concat",
        JSValue::Function(JSFunction::new_native("concat", |this, args| {
            let mut s = this.to_string_val();
            for arg in args {
                s.push_str(&arg.to_string_val());
            }
            Ok(JSValue::String(s))
        })),
    );

    // String.prototype.at(index) (ES2022)
    JSObject::set_property(
        &proto,
        "at",
        JSValue::Function(JSFunction::new_native("at", |this, args| {
            let s = this.to_string_val();
            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as isize;
            let idx_arg = args.first().map(|v| v.to_number() as isize).unwrap_or(0);
            let actual_idx = if idx_arg < 0 { len + idx_arg } else { idx_arg };
            if actual_idx >= 0 && actual_idx < len {
                Ok(JSValue::String(chars[actual_idx as usize].to_string()))
            } else {
                Ok(JSValue::Undefined)
            }
        })),
    );

    // String.prototype.isWellFormed() (ES2024)
    JSObject::set_property(
        &proto,
        "isWellFormed",
        JSValue::Function(JSFunction::new_native("isWellFormed", |this, _args| {
            let s = this.to_string_val();
            let mut is_well_formed = true;
            let mut iter = s.encode_utf16().peekable();
            while let Some(u) = iter.next() {
                if (0xd800..=0xdbff).contains(&u) {
                    if let Some(&next_u) = iter.peek() {
                        if (0xdc00..=0xdfff).contains(&next_u) {
                            iter.next();
                        } else {
                            is_well_formed = false;
                            break;
                        }
                    } else {
                        is_well_formed = false;
                        break;
                    }
                } else if (0xdc00..=0xdfff).contains(&u) {
                    is_well_formed = false;
                    break;
                }
            }
            Ok(JSValue::Boolean(is_well_formed))
        })),
    );

    // String.prototype.toWellFormed() (ES2024)
    JSObject::set_property(
        &proto,
        "toWellFormed",
        JSValue::Function(JSFunction::new_native("toWellFormed", |this, _args| {
            let s = this.to_string_val();
            let utf16: Vec<u16> = s.encode_utf16().collect();
            let mut cleaned: Vec<u16> = Vec::with_capacity(utf16.len());
            let mut i = 0;
            while i < utf16.len() {
                let u = utf16[i];
                if (0xd800..=0xdbff).contains(&u) {
                    if i + 1 < utf16.len() && (0xdc00..=0xdfff).contains(&utf16[i + 1]) {
                        cleaned.push(u);
                        cleaned.push(utf16[i + 1]);
                        i += 2;
                    } else {
                        cleaned.push(0xfffd);
                        i += 1;
                    }
                } else if (0xdc00..=0xdfff).contains(&u) {
                    cleaned.push(0xfffd);
                    i += 1;
                } else {
                    cleaned.push(u);
                    i += 1;
                }
            }
            let out = String::from_utf16_lossy(&cleaned);
            Ok(JSValue::String(out))
        })),
    );

    // String.prototype[Symbol.iterator]()
    let str_iter_fn = JSFunction::new_native("[Symbol.iterator]", |this, _args| {
        let s = this.to_string_val();
        Ok(crate::objects::generator::new_string_iterator(s))
    });
    JSObject::set_property(&proto, "Symbol(Symbol.iterator)", JSValue::Function(str_iter_fn.clone()));
    JSObject::set_property(&proto, "[Symbol.iterator]", JSValue::Function(str_iter_fn.clone()));
    JSObject::set_property(&proto, "iterator", JSValue::Function(str_iter_fn));

    // Annex B.2.3 HTML wrapper methods
    for &(name, tag) in &[
        ("big", "big"),
        ("blink", "blink"),
        ("bold", "b"),
        ("fixed", "tt"),
        ("italics", "i"),
        ("small", "small"),
        ("strike", "strike"),
        ("sub", "sub"),
        ("sup", "sup"),
    ] {
        let tag_str = tag.to_string();
        JSObject::set_property(
            &proto,
            name,
            JSValue::Function(JSFunction::new_closure(name, move |this, _args| {
                let s = this.to_string_val();
                Ok(JSValue::String(format!("<{}>{}</{}>", tag_str, s, tag_str)))
            })),
        );
    }

    JSObject::set_property(
        &proto,
        "anchor",
        JSValue::Function(JSFunction::new_native("anchor", |this, args| {
            let s = this.to_string_val();
            let name = args.first().map(|v| v.to_string_val()).unwrap_or_default();
            Ok(JSValue::String(format!("<a name=\"{}\">{}</a>", name, s)))
        })),
    );

    JSObject::set_property(
        &proto,
        "link",
        JSValue::Function(JSFunction::new_native("link", |this, args| {
            let s = this.to_string_val();
            let href = args.first().map(|v| v.to_string_val()).unwrap_or_default();
            Ok(JSValue::String(format!("<a href=\"{}\">{}</a>", href, s)))
        })),
    );

    JSObject::set_property(
        &proto,
        "fontcolor",
        JSValue::Function(JSFunction::new_native("fontcolor", |this, args| {
            let s = this.to_string_val();
            let color = args.first().map(|v| v.to_string_val()).unwrap_or_default();
            Ok(JSValue::String(format!("<font color=\"{}\">{}</font>", color, s)))
        })),
    );

    JSObject::set_property(
        &proto,
        "fontsize",
        JSValue::Function(JSFunction::new_native("fontsize", |this, args| {
            let s = this.to_string_val();
            let size = args.first().map(|v| v.to_string_val()).unwrap_or_default();
            Ok(JSValue::String(format!("<font size=\"{}\">{}</font>", size, s)))
        })),
    );

    proto
}

/// Creates the `String` constructor object.
pub fn create_string_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let string_ctor = JSObject::new_empty(None);

    JSObject::set_property(&string_ctor, "prototype", JSValue::Object(prototype));

    // String.fromCharCode(...codes)
    JSObject::set_property(
        &string_ctor,
        "fromCharCode",
        JSValue::Function(JSFunction::new_native("fromCharCode", |_this, args| {
            let mut result = String::with_capacity(args.len());
            for arg in args {
                let code = (arg.to_number() as u32) & 0xffff;
                if let Some(ch) = char::from_u32(code) {
                    result.push(ch);
                } else {
                    result.push('\u{fffd}');
                }
            }
            Ok(JSValue::String(result))
        })),
    );

    // String.fromCodePoint(...codePoints) (ES2015)
    JSObject::set_property(
        &string_ctor,
        "fromCodePoint",
        JSValue::Function(JSFunction::new_native("fromCodePoint", |_this, args| {
            let mut result = String::with_capacity(args.len());
            for arg in args {
                let num = arg.to_number();
                if num < 0.0 || num > 0x10ffff as f64 || num != (num as u32 as f64) {
                    return Err(format!("RangeError: Invalid code point {}", num));
                }
                let cp = num as u32;
                if let Some(ch) = char::from_u32(cp) {
                    result.push(ch);
                } else {
                    return Err(format!("RangeError: Invalid code point {}", cp));
                }
            }
            Ok(JSValue::String(result))
        })),
    );

    // String.raw(template, ...substitutions) (ES2015)
    JSObject::set_property(
        &string_ctor,
        "raw",
        JSValue::Function(JSFunction::new_native("raw", |_this, args| {
            let template = match args.first() {
                Some(JSValue::Object(obj)) => obj.clone(),
                _ => return Err("TypeError: Cannot convert undefined or null to object".to_string()),
            };
            let raw_prop = template.borrow().get_property("raw");
            let raw_arr = match raw_prop {
                JSValue::Array(arr) => arr.borrow().elements.clone(),
                JSValue::Object(obj) => {
                    let borrowed = obj.borrow();
                    let len = borrowed.get_property("length").to_number() as usize;
                    let mut elems = Vec::with_capacity(len);
                    for i in 0..len {
                        elems.push(borrowed.get_property(&i.to_string()));
                    }
                    elems
                }
                _ => return Err("TypeError: Cannot convert undefined or null to object".to_string()),
            };

            let substitutions = if args.len() > 1 { &args[1..] } else { &[] };
            let mut result = String::new();
            for (i, seg) in raw_arr.iter().enumerate() {
                result.push_str(&seg.to_string_val());
                if i < substitutions.len() && i + 1 < raw_arr.len() {
                    result.push_str(&substitutions[i].to_string_val());
                }
            }
            Ok(JSValue::String(result))
        })),
    );

    // Direct invocation `String(value)`
    JSObject::set_property(
        &string_ctor,
        "__call__",
        JSValue::Function(JSFunction::new_native("String", |_this, args| {
            let s = args.first().map(|v| v.to_string_val()).unwrap_or_default();
            Ok(JSValue::String(s))
        })),
    );

    string_ctor
}
