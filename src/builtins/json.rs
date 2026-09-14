//! Safe Rust reimplementation of Google V8's ECMAScript `JSON` built-in object.
//!
//! Implements RFC 8259 compliant `JSON.parse()` and `JSON.stringify()` with indentation support.

use crate::objects::{JSArray, JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

/// A recursive-descent JSON parser adhering to RFC 8259.
struct JsonParser {
    chars: Vec<char>,
    pos: usize,
}

impl JsonParser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse(&mut self) -> Result<JSValue, String> {
        self.skip_whitespace();
        if self.pos >= self.chars.len() {
            return Err("Unexpected end of JSON input".to_string());
        }
        let val = self.parse_value()?;
        self.skip_whitespace();
        if self.pos < self.chars.len() {
            return Err(format!(
                "Unexpected character '{}' after JSON at position {}",
                self.chars[self.pos], self.pos
            ));
        }
        Ok(val)
    }

    fn parse_value(&mut self) -> Result<JSValue, String> {
        self.skip_whitespace();
        let ch = match self.peek() {
            Some(c) => c,
            None => return Err("Unexpected end of JSON input".to_string()),
        };

        match ch {
            '{' => self.parse_object(),
            '[' => self.parse_array(),
            '"' => self.parse_string().map(JSValue::String),
            't' | 'f' => self.parse_boolean(),
            'n' => self.parse_null(),
            '-' | '0'..='9' => self.parse_number(),
            _ => Err(format!("Unexpected token '{}' at position {}", ch, self.pos)),
        }
    }

    fn parse_object(&mut self) -> Result<JSValue, String> {
        self.advance(); // consume '{'
        self.skip_whitespace();

        let obj = JSObject::new_empty(None);
        if self.peek() == Some('}') {
            self.advance();
            return Ok(JSValue::Object(obj));
        }

        loop {
            self.skip_whitespace();
            if self.peek() != Some('"') {
                return Err(format!(
                    "Expected string key in object at position {}",
                    self.pos
                ));
            }

            let key = self.parse_string()?;
            self.skip_whitespace();

            if self.peek() != Some(':') {
                return Err(format!("Expected ':' after key at position {}", self.pos));
            }
            self.advance(); // consume ':'

            let value = self.parse_value()?;
            JSObject::set_property(&obj, &key, value);

            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.advance();
                    self.skip_whitespace();
                    if self.peek() == Some('}') {
                        return Err(format!(
                            "Trailing comma not allowed in JSON object at position {}",
                            self.pos
                        ));
                    }
                }
                Some('}') => {
                    self.advance();
                    break;
                }
                _ => {
                    return Err(format!(
                        "Expected ',' or '}}' in object at position {}",
                        self.pos
                    ));
                }
            }
        }

        Ok(JSValue::Object(obj))
    }

    fn parse_array(&mut self) -> Result<JSValue, String> {
        self.advance(); // consume '['
        self.skip_whitespace();

        let mut elements = Vec::new();
        if self.peek() == Some(']') {
            self.advance();
            return Ok(JSValue::Array(JSArray::new_array(elements)));
        }

        loop {
            let elem = self.parse_value()?;
            elements.push(elem);

            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.advance();
                    self.skip_whitespace();
                    if self.peek() == Some(']') {
                        return Err(format!(
                            "Trailing comma not allowed in JSON array at position {}",
                            self.pos
                        ));
                    }
                }
                Some(']') => {
                    self.advance();
                    break;
                }
                _ => {
                    return Err(format!(
                        "Expected ',' or ']' in array at position {}",
                        self.pos
                    ));
                }
            }
        }

        Ok(JSValue::Array(JSArray::new_array(elements)))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.advance(); // consume opening '"'
        let mut s = String::new();

        while let Some(ch) = self.advance() {
            match ch {
                '"' => return Ok(s),
                '\\' => {
                    let esc = match self.advance() {
                        Some(c) => c,
                        None => return Err("Unterminated string escape in JSON".to_string()),
                    };
                    match esc {
                        '"' => s.push('"'),
                        '\\' => s.push('\\'),
                        '/' => s.push('/'),
                        'b' => s.push('\u{0008}'),
                        'f' => s.push('\u{000c}'),
                        'n' => s.push('\n'),
                        'r' => s.push('\r'),
                        't' => s.push('\t'),
                        'u' => {
                            let mut hex = String::with_capacity(4);
                            for _ in 0..4 {
                                if let Some(h) = self.advance() {
                                    hex.push(h);
                                } else {
                                    return Err("Incomplete \\u hex escape in JSON".to_string());
                                }
                            }
                            if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                if let Some(unicode_char) = char::from_u32(code) {
                                    s.push(unicode_char);
                                } else {
                                    s.push('\u{fffd}');
                                }
                            } else {
                                return Err(format!("Invalid \\u hex escape in JSON: {}", hex));
                            }
                        }
                        _ => return Err(format!("Invalid escape sequence '\\{}' in JSON", esc)),
                    }
                }
                '\n' | '\r' => {
                    return Err("Unescaped control character in JSON string".to_string());
                }
                _ => s.push(ch),
            }
        }

        Err("Unterminated string in JSON".to_string())
    }

    fn parse_boolean(&mut self) -> Result<JSValue, String> {
        if self.chars[self.pos..].starts_with(&['t', 'r', 'u', 'e']) {
            self.pos += 4;
            Ok(JSValue::Boolean(true))
        } else if self.chars[self.pos..].starts_with(&['f', 'a', 'l', 's', 'e']) {
            self.pos += 5;
            Ok(JSValue::Boolean(false))
        } else {
            Err(format!("Invalid token at position {}", self.pos))
        }
    }

    fn parse_null(&mut self) -> Result<JSValue, String> {
        if self.chars[self.pos..].starts_with(&['n', 'u', 'l', 'l']) {
            self.pos += 4;
            Ok(JSValue::Null)
        } else {
            Err(format!("Invalid token at position {}", self.pos))
        }
    }

    fn parse_number(&mut self) -> Result<JSValue, String> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.advance();
        }

        let mut has_digits = false;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                has_digits = true;
                self.advance();
            } else {
                break;
            }
        }

        if !has_digits {
            return Err(format!("Invalid number at position {}", start));
        }

        let mut is_float = false;
        if self.peek() == Some('.') {
            is_float = true;
            self.advance();
            let mut has_frac_digits = false;
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    has_frac_digits = true;
                    self.advance();
                } else {
                    break;
                }
            }
            if !has_frac_digits {
                return Err(format!("Invalid fractional number at position {}", start));
            }
        }

        if let Some('e') | Some('E') = self.peek() {
            is_float = true;
            self.advance();
            if let Some('+') | Some('-') = self.peek() {
                self.advance();
            }
            let mut has_exp_digits = false;
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    has_exp_digits = true;
                    self.advance();
                } else {
                    break;
                }
            }
            if !has_exp_digits {
                return Err(format!("Invalid exponent in number at position {}", start));
            }
        }

        let num_str: String = self.chars[start..self.pos].iter().collect();
        if !is_float {
            if let Ok(n) = num_str.parse::<i32>() {
                return Ok(JSValue::Smi(n));
            }
        }

        if let Ok(f) = num_str.parse::<f64>() {
            Ok(JSValue::Number(f))
        } else {
            Err(format!("Could not parse number '{}'", num_str))
        }
    }
}

/// Serializes a JSValue to a JSON string.
fn stringify_value(
    value: &JSValue,
    indent_step: &Option<String>,
    current_depth: usize,
) -> Result<String, String> {
    match value {
        JSValue::Null => Ok("null".to_string()),
        JSValue::Boolean(b) => Ok(if *b { "true".to_string() } else { "false".to_string() }),
        JSValue::Smi(n) => Ok(n.to_string()),
        JSValue::Number(f) => {
            if f.is_nan() || f.is_infinite() {
                Ok("null".to_string())
            } else if f.fract() == 0.0 && *f >= i32::MIN as f64 && *f <= i32::MAX as f64 {
                Ok((*f as i64).to_string())
            } else {
                Ok(f.to_string())
            }
        }
        JSValue::String(s) => Ok(escape_json_string(s)),
        JSValue::Array(arr) => {
            let borrowed = arr.borrow();
            if borrowed.elements.is_empty() {
                return Ok("[]".to_string());
            }

            let next_depth = current_depth + 1;
            let mut items = Vec::new();
            for elem in &borrowed.elements {
                let serialized = match elem {
                    JSValue::Undefined | JSValue::Function(_) => "null".to_string(),
                    other => stringify_value(other, indent_step, next_depth)?,
                };
                items.push(serialized);
            }

            if let Some(ref step) = indent_step {
                let indent_curr = step.repeat(current_depth);
                let indent_next = step.repeat(next_depth);
                let joined = items.join(&format!(",\n{}", indent_next));
                Ok(format!("[\n{}{}\n{}]", indent_next, joined, indent_curr))
            } else {
                Ok(format!("[{}]", items.join(",")))
            }
        }
        JSValue::Object(obj) => {
            let borrowed = obj.borrow();
            if borrowed.get_property("__is_raw_json__") == JSValue::Boolean(true) {
                return Ok(borrowed.get_property("rawJSON").to_string_val());
            }
            let keys = if borrowed.map.borrow().is_dictionary_map {
                borrowed.ext_or_default().dictionary_properties.keys().cloned().collect::<Vec<_>>()
            } else {
                borrowed
                    .map
                    .borrow()
                    .descriptors
                    .iter()
                    .filter(|d| !d.details.is_dont_enum())
                    .map(|d| d.name.clone())
                    .collect()
            };

            let next_depth = current_depth + 1;
            let mut entries = Vec::new();
            for key in keys {
                let prop_val = borrowed.get_property(&key);
                if matches!(prop_val, JSValue::Undefined | JSValue::Function(_)) {
                    continue;
                }
                let key_str = escape_json_string(&key);
                let val_str = stringify_value(&prop_val, indent_step, next_depth)?;
                if indent_step.is_some() {
                    entries.push(format!("{}: {}", key_str, val_str));
                } else {
                    entries.push(format!("{}:{}", key_str, val_str));
                }
            }

            if entries.is_empty() {
                return Ok("{}".to_string());
            }

            if let Some(ref step) = indent_step {
                let indent_curr = step.repeat(current_depth);
                let indent_next = step.repeat(next_depth);
                let joined = entries.join(&format!(",\n{}", indent_next));
                Ok(format!("{{\n{}{}\n{}}}", indent_next, joined, indent_curr))
            } else {
                Ok(format!("{{{}}}", entries.join(",")))
            }
        }
        JSValue::Undefined | JSValue::Function(_) | JSValue::Symbol(_) => Ok(String::new()),
        JSValue::BigInt(_) => Err("TypeError: Do not know how to serialize a BigInt".to_string()),
    }
}

fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            c if c.is_control() => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Allocates the global `JSON` object with `parse` and `stringify` methods.
pub fn create_json_object() -> Rc<RefCell<JSObject>> {
    let json_obj = JSObject::new_empty(None);

    // JSON.parse(text)
    JSObject::set_property(
        &json_obj,
        "parse",
        JSValue::Function(JSFunction::new_native("parse", |_this, args| {
            let input = match args.first() {
                Some(v) => v.to_string_val(),
                None => return Err("JSON.parse requires at least 1 argument".to_string()),
            };

            let mut parser = JsonParser::new(&input);
            parser.parse().map_err(|e| format!("SyntaxError: {}", e))
        })),
    );

    // JSON.stringify(value, replacer, space)
    JSObject::set_property(
        &json_obj,
        "stringify",
        JSValue::Function(JSFunction::new_native("stringify", |_this, args| {
            let value = match args.first() {
                Some(v) => v,
                None => return Ok(JSValue::Undefined),
            };

            if matches!(value, JSValue::Undefined | JSValue::Function(_)) {
                return Ok(JSValue::Undefined);
            }

            let space_arg = args.get(2);
            let indent_step = match space_arg {
                Some(JSValue::Smi(n)) => {
                    let count = (*n).clamp(0, 10) as usize;
                    if count > 0 {
                        Some(" ".repeat(count))
                    } else {
                        None
                    }
                }
                Some(JSValue::Number(f)) => {
                    let count = (*f as i64).clamp(0, 10) as usize;
                    if count > 0 {
                        Some(" ".repeat(count))
                    } else {
                        None
                    }
                }
                Some(JSValue::String(s)) => {
                    if !s.is_empty() {
                        Some(s.chars().take(10).collect())
                    } else {
                        None
                    }
                }
                _ => None,
            };

            let result = stringify_value(value, &indent_step, 0)?;
            Ok(JSValue::String(result))
        })),
    );

    // JSON.rawJSON(text) (ES2024)
    JSObject::set_property(
        &json_obj,
        "rawJSON",
        JSValue::Function(JSFunction::new_native("rawJSON", |_this, args| {
            let text_val = args.first().ok_or_else(|| "TypeError: JSON.rawJSON requires a string argument".to_string())?;
            let text = match text_val {
                JSValue::String(s) => s.clone(),
                _ => return Err("TypeError: JSON.rawJSON requires a string argument".to_string()),
            };

            // Validate that text is valid JSON syntax
            let mut parser = JsonParser::new(&text);
            parser.parse().map_err(|e| format!("SyntaxError: Invalid JSON text in JSON.rawJSON: {}", e))?;

            let raw_obj = JSObject::new_empty(None);
            JSObject::set_property(&raw_obj, "rawJSON", JSValue::String(text));
            JSObject::set_property(&raw_obj, "__is_raw_json__", JSValue::Boolean(true));
            Ok(JSValue::Object(raw_obj))
        })),
    );

    // JSON.isRawJSON(value) (ES2024)
    JSObject::set_property(
        &json_obj,
        "isRawJSON",
        JSValue::Function(JSFunction::new_native("isRawJSON", |_this, args| {
            if let Some(JSValue::Object(o)) = args.first() {
                let is_raw = o.borrow().get_property("__is_raw_json__") == JSValue::Boolean(true);
                Ok(JSValue::Boolean(is_raw))
            } else {
                Ok(JSValue::Boolean(false))
            }
        })),
    );

    json_obj
}

/// Helper function to parse raw JSON into a JSValue.
pub fn parse_json(s: &str) -> Result<JSValue, String> {
    let mut parser = JsonParser::new(s);
    parser.parse().map_err(|e| format!("SyntaxError: {}", e))
}

/// Helper function to stringify a JSValue to raw JSON.
pub fn stringify_json(v: &JSValue) -> Result<String, String> {
    stringify_value(v, &None, 0)
}
