//! Safe Rust reimplementation of WHATWG URL and URLSearchParams APIs (RFC 3986).
//!
//! Exposes `URL`, `URLSearchParams`, and global percent-encoding utilities:
//! `encodeURI`, `decodeURI`, `encodeURIComponent`, `decodeURIComponent`.

use crate::objects::js_array::JSArray;
use crate::objects::js_object::JSObject;
use crate::objects::value::JSValue;
use crate::objects::JSFunction;
use std::cell::RefCell;
use std::rc::Rc;

/// Percent-encodes a character for URI component conforming to RFC 3986.
pub fn percent_encode(s: &str, is_component: bool) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        let is_unreserved = b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~';
        let is_reserved_uri = !is_component && (b == b';' || b == b',' || b == b'/' || b == b'?' || b == b':'
            || b == b'@' || b == b'&' || b == b'=' || b == b'+' || b == b'$' || b == b'#');

        if is_unreserved || is_reserved_uri {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Decodes percent-encoded hex sequences in a URI string.
pub fn percent_decode(s: &str, plus_as_space: bool) -> Result<String, String> {
    let mut bytes = Vec::with_capacity(s.len());
    let mut chars = s.bytes().peekable();

    while let Some(b) = chars.next() {
        if b == b'+' && plus_as_space {
            bytes.push(b' ');
        } else if b == b'%' {
            let h1 = chars.next().ok_or_else(|| "URIError: Malformed URI".to_string())?;
            let h2 = chars.next().ok_or_else(|| "URIError: Malformed URI".to_string())?;
            let hex_str = [h1, h2];
            let hex_val = std::str::from_utf8(&hex_str).map_err(|_| "URIError: Malformed URI".to_string())?;
            let byte_val = u8::from_str_radix(hex_val, 16).map_err(|_| "URIError: Malformed URI".to_string())?;
            bytes.push(byte_val);
        } else {
            bytes.push(b);
        }
    }

    String::from_utf8(bytes).map_err(|_| "URIError: Malformed URI sequence".to_string())
}

// -----------------------------------------------------------------------------
// URLSearchParams
// -----------------------------------------------------------------------------

fn parse_query_pairs(query: &str) -> Vec<(String, String)> {
    let q = query.strip_prefix('?').unwrap_or(query);
    if q.is_empty() {
        return Vec::new();
    }
    let mut pairs = Vec::new();
    for part in q.split('&') {
        if part.is_empty() {
            continue;
        }
        let mut split = part.splitn(2, '=');
        let key_enc = split.next().unwrap_or_default();
        let val_enc = split.next().unwrap_or_default();
        let key = percent_decode(key_enc, true).unwrap_or_else(|_| key_enc.to_string());
        let val = percent_decode(val_enc, true).unwrap_or_else(|_| val_enc.to_string());
        pairs.push((key, val));
    }
    pairs
}

fn pairs_to_query_string(pairs: &[(String, String)]) -> String {
    let mut parts = Vec::with_capacity(pairs.len());
    for (k, v) in pairs {
        let enc_k = percent_encode(k, true);
        let enc_v = percent_encode(v, true);
        parts.push(format!("{}={}", enc_k, enc_v));
    }
    parts.join("&")
}

fn get_sp_pairs(obj: &Rc<RefCell<JSObject>>) -> Vec<(String, String)> {
    let prop = obj.borrow().get_property("__query_entries__");
    match prop {
        JSValue::Array(arr) => {
            let mut pairs = Vec::new();
            for item in &arr.borrow().elements {
                if let JSValue::Array(sub) = item {
                    let k = sub.borrow().elements.first().map(|v| v.to_string_val()).unwrap_or_default();
                    let v = sub.borrow().elements.get(1).map(|v| v.to_string_val()).unwrap_or_default();
                    pairs.push((k, v));
                }
            }
            pairs
        }
        _ => Vec::new(),
    }
}

fn set_sp_pairs(obj: &Rc<RefCell<JSObject>>, pairs: Vec<(String, String)>) {
    let mut arr_items = Vec::with_capacity(pairs.len());
    for (k, v) in &pairs {
        let pair = JSArray::new_array(vec![JSValue::String(k.clone()), JSValue::String(v.clone())]);
        arr_items.push(JSValue::Array(pair));
    }
    JSObject::set_property(obj, "__query_entries__", JSValue::Array(JSArray::new_array(arr_items)));

    // If parent URL is linked, update parent URL
    let parent_prop = obj.borrow().get_property("__parent_url__");
    if let JSValue::Object(parent) = parent_prop {
        let qs = pairs_to_query_string(&pairs);
        update_url_from_search(&parent, &qs);
    }
}

/// Creates the `URLSearchParams` prototype and constructor.
pub fn create_url_search_params_constructor() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // URLSearchParams.prototype.append(name, value)
    let append_fn = JSFunction::new_native("append", |this, args| {
        let name = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let value = args.get(1).map(|v| v.to_string_val()).unwrap_or_default();
        if let JSValue::Object(o) = this {
            let mut pairs = get_sp_pairs(o);
            pairs.push((name, value));
            set_sp_pairs(o, pairs);
        }
        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&proto, "append", JSValue::Function(append_fn));

    // URLSearchParams.prototype.delete(name)
    let delete_fn = JSFunction::new_native("delete", |this, args| {
        let name = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        if let JSValue::Object(o) = this {
            let mut pairs = get_sp_pairs(o);
            pairs.retain(|(k, _)| k != &name);
            set_sp_pairs(o, pairs);
        }
        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&proto, "delete", JSValue::Function(delete_fn));

    // URLSearchParams.prototype.get(name)
    let get_fn = JSFunction::new_native("get", |this, args| {
        let name = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        if let JSValue::Object(o) = this {
            let pairs = get_sp_pairs(o);
            for (k, v) in pairs {
                if k == name {
                    return Ok(JSValue::String(v));
                }
            }
        }
        Ok(JSValue::Null)
    });
    JSObject::set_property(&proto, "get", JSValue::Function(get_fn));

    // URLSearchParams.prototype.getAll(name)
    let get_all_fn = JSFunction::new_native("getAll", |this, args| {
        let name = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let mut results = Vec::new();
        if let JSValue::Object(o) = this {
            let pairs = get_sp_pairs(o);
            for (k, v) in pairs {
                if k == name {
                    results.push(JSValue::String(v));
                }
            }
        }
        Ok(JSValue::Array(JSArray::new_array(results)))
    });
    JSObject::set_property(&proto, "getAll", JSValue::Function(get_all_fn));

    // URLSearchParams.prototype.has(name)
    let has_fn = JSFunction::new_native("has", |this, args| {
        let name = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        if let JSValue::Object(o) = this {
            let pairs = get_sp_pairs(o);
            let has = pairs.iter().any(|(k, _)| k == &name);
            return Ok(JSValue::Boolean(has));
        }
        Ok(JSValue::Boolean(false))
    });
    JSObject::set_property(&proto, "has", JSValue::Function(has_fn));

    // URLSearchParams.prototype.set(name, value)
    let set_fn = JSFunction::new_native("set", |this, args| {
        let name = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let value = args.get(1).map(|v| v.to_string_val()).unwrap_or_default();
        if let JSValue::Object(o) = this {
            let pairs = get_sp_pairs(o);
            let mut found = false;
            let mut new_pairs = Vec::new();
            for (k, v) in pairs {
                if k == name {
                    if !found {
                        new_pairs.push((name.clone(), value.clone()));
                        found = true;
                    }
                } else {
                    new_pairs.push((k, v));
                }
            }
            if !found {
                new_pairs.push((name, value));
            }
            set_sp_pairs(o, new_pairs);
        }
        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&proto, "set", JSValue::Function(set_fn));

    // URLSearchParams.prototype.sort()
    let sort_fn = JSFunction::new_native("sort", |this, _args| {
        if let JSValue::Object(o) = this {
            let mut pairs = get_sp_pairs(o);
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            set_sp_pairs(o, pairs);
        }
        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&proto, "sort", JSValue::Function(sort_fn));

    // URLSearchParams.prototype.toString()
    let to_string_fn = JSFunction::new_native("toString", |this, _args| {
        if let JSValue::Object(o) = this {
            let pairs = get_sp_pairs(o);
            return Ok(JSValue::String(pairs_to_query_string(&pairs)));
        }
        Ok(JSValue::String(String::new()))
    });
    JSObject::set_property(&proto, "toString", JSValue::Function(to_string_fn));

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(proto.clone()));

    let proto_clone = proto.clone();
    let ctor_fn = JSFunction::new_closure("URLSearchParams", move |_this, args| {
        let pairs = match args.first() {
            Some(JSValue::String(s)) => parse_query_pairs(s),
            Some(JSValue::Array(arr)) => {
                let mut p = Vec::new();
                for item in &arr.borrow().elements {
                    if let JSValue::Array(sub_arr) = item {
                        let k = sub_arr.borrow().elements.first().map(|v| v.to_string_val()).unwrap_or_default();
                        let v = sub_arr.borrow().elements.get(1).map(|v| v.to_string_val()).unwrap_or_default();
                        p.push((k, v));
                    }
                }
                p
            }
            Some(JSValue::Object(o)) => {
                let borrowed = o.borrow();
                let mut p = Vec::new();
                for desc in &borrowed.map.borrow().descriptors {
                    if let Some(val) = borrowed.properties.get(desc.field_index) {
                        p.push((desc.name.clone(), val.to_string_val()));
                    }
                }
                p
            }
            _ => Vec::new(),
        };

        let instance = JSObject::new_empty(Some(proto_clone.clone()));
        set_sp_pairs(&instance, pairs);
        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor_fn));
    JSObject::set_property(&proto, "constructor", JSValue::Object(ctor_obj.clone()));

    ctor_obj
}

// -----------------------------------------------------------------------------
// URL Implementation
// -----------------------------------------------------------------------------

#[derive(Clone, Default)]
pub struct ParsedUrl {
    pub protocol: String,
    pub username: String,
    pub password: String,
    pub hostname: String,
    pub port: String,
    pub pathname: String,
    pub search: String,
    pub hash: String,
}

impl ParsedUrl {
    pub fn parse(input: &str, base: Option<&str>) -> Result<Self, String> {
        let trimmed = input.trim();
        let full_input = if let Some(base_str) = base {
            let base_trimmed = base_str.trim();
            if trimmed.contains("://") {
                trimmed.to_string()
            } else if trimmed.starts_with("//") {
                let scheme_end = base_trimmed.find("://").unwrap_or(0);
                format!("{}:{}", &base_trimmed[..scheme_end], trimmed)
            } else if trimmed.starts_with('/') {
                let base_parsed = Self::parse_absolute(base_trimmed)?;
                let origin = base_parsed.origin();
                format!("{}{}", origin, trimmed)
            } else {
                let base_parsed = Self::parse_absolute(base_trimmed)?;
                let mut path = base_parsed.pathname.clone();
                if let Some(idx) = path.rfind('/') {
                    path.truncate(idx + 1);
                } else {
                    path = "/".to_string();
                }
                format!("{}{}{}", base_parsed.origin(), path, trimmed)
            }
        } else {
            trimmed.to_string()
        };

        Self::parse_absolute(&full_input)
    }

    fn parse_absolute(s: &str) -> Result<Self, String> {
        let scheme_end = s.find("://").ok_or_else(|| format!("TypeError: Invalid URL: {}", s))?;
        let protocol = format!("{}:", s[..scheme_end].to_ascii_lowercase());

        let rest = &s[scheme_end + 3..];
        let (authority_and_path, hash) = match rest.find('#') {
            Some(idx) => (&rest[..idx], format!("#{}", &rest[idx + 1..])),
            None => (rest, String::new()),
        };

        let (authority_and_path, search) = match authority_and_path.find('?') {
            Some(idx) => (&authority_and_path[..idx], format!("?{}", &authority_and_path[idx + 1..])),
            None => (authority_and_path, String::new()),
        };

        let (authority, pathname) = match authority_and_path.find('/') {
            Some(idx) => (&authority_and_path[..idx], authority_and_path[idx..].to_string()),
            None => (authority_and_path, "/".to_string()),
        };

        let (userinfo, host_port) = match authority.find('@') {
            Some(idx) => (Some(&authority[..idx]), &authority[idx + 1..]),
            None => (None, authority),
        };

        let mut username = String::new();
        let mut password = String::new();
        if let Some(ui) = userinfo {
            if let Some(colon) = ui.find(':') {
                username = ui[..colon].to_string();
                password = ui[colon + 1..].to_string();
            } else {
                username = ui.to_string();
            }
        }

        let (hostname, port) = match host_port.find(':') {
            Some(idx) => (host_port[..idx].to_ascii_lowercase(), host_port[idx + 1..].to_string()),
            None => (host_port.to_ascii_lowercase(), String::new()),
        };

        Ok(Self {
            protocol,
            username,
            password,
            hostname,
            port,
            pathname: if pathname.is_empty() { "/".to_string() } else { pathname },
            search,
            hash,
        })
    }

    pub fn host(&self) -> String {
        if self.port.is_empty() {
            self.hostname.clone()
        } else {
            format!("{}:{}", self.hostname, self.port)
        }
    }

    pub fn origin(&self) -> String {
        format!("{}//{}", self.protocol, self.host())
    }

    pub fn href(&self) -> String {
        let userinfo = if !self.username.is_empty() || !self.password.is_empty() {
            if !self.password.is_empty() {
                format!("{}:{}@", self.username, self.password)
            } else {
                format!("{}@", self.username)
            }
        } else {
            String::new()
        };

        format!("{}//{}{}{}{}{}", self.protocol, userinfo, self.host(), self.pathname, self.search, self.hash)
    }
}

fn update_url_from_search(url_obj: &Rc<RefCell<JSObject>>, query: &str) {
    let search = if query.is_empty() {
        String::new()
    } else {
        format!("?{}", query)
    };
    JSObject::set_property(url_obj, "search", JSValue::String(search));
    sync_url_href(url_obj);
}

fn sync_url_href(url_obj: &Rc<RefCell<JSObject>>) {
    let borrowed = url_obj.borrow();
    let proto = borrowed.get_property("protocol").to_string_val();
    let host = borrowed.get_property("host").to_string_val();
    let pathname = borrowed.get_property("pathname").to_string_val();
    let search = borrowed.get_property("search").to_string_val();
    let hash = borrowed.get_property("hash").to_string_val();
    let href = format!("{}//{foo}{bar}{baz}{qux}", proto, foo = host, bar = pathname, baz = search, qux = hash);
    drop(borrowed);
    JSObject::set_property(url_obj, "href", JSValue::String(href));
}

/// Creates the `URL` constructor object.
pub fn create_url_constructor(search_params_ctor: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    let to_string_fn = JSFunction::new_native("toString", |this, _args| {
        if let JSValue::Object(o) = this {
            let href = o.borrow().get_property("href");
            return Ok(href);
        }
        Ok(JSValue::String(String::new()))
    });
    JSObject::set_property(&proto, "toString", JSValue::Function(to_string_fn.clone()));
    JSObject::set_property(&proto, "toJSON", JSValue::Function(to_string_fn));

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(proto.clone()));

    let proto_clone = proto.clone();
    let sp_ctor = search_params_ctor.clone();

    let ctor_fn = JSFunction::new_closure("URL", move |_this, args| {
        let input = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let base = args.get(1).map(|v| v.to_string_val());
        let parsed = ParsedUrl::parse(&input, base.as_deref())?;

        let instance = JSObject::new_empty(Some(proto_clone.clone()));
        JSObject::set_property(&instance, "protocol", JSValue::String(parsed.protocol.clone()));
        JSObject::set_property(&instance, "username", JSValue::String(parsed.username.clone()));
        JSObject::set_property(&instance, "password", JSValue::String(parsed.password.clone()));
        JSObject::set_property(&instance, "host", JSValue::String(parsed.host()));
        JSObject::set_property(&instance, "hostname", JSValue::String(parsed.hostname.clone()));
        JSObject::set_property(&instance, "port", JSValue::String(parsed.port.clone()));
        JSObject::set_property(&instance, "pathname", JSValue::String(parsed.pathname.clone()));
        JSObject::set_property(&instance, "search", JSValue::String(parsed.search.clone()));
        JSObject::set_property(&instance, "hash", JSValue::String(parsed.hash.clone()));
        JSObject::set_property(&instance, "origin", JSValue::String(parsed.origin()));
        JSObject::set_property(&instance, "href", JSValue::String(parsed.href()));

        // Create associated searchParams instance
        let sp_call = sp_ctor.borrow().get_property("__call__");
        let sp_instance = match sp_call {
            JSValue::Function(f) => {
                match f.call(&JSValue::Undefined, &[JSValue::String(parsed.search.clone())]) {
                    Ok(JSValue::Object(sp_obj)) => sp_obj,
                    _ => JSObject::new_empty(None),
                }
            }
            _ => JSObject::new_empty(None),
        };

        // Link searchParams back to parent url instance
        JSObject::set_property(&sp_instance, "__parent_url__", JSValue::Object(instance.clone()));
        JSObject::set_property(&instance, "searchParams", JSValue::Object(sp_instance));

        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor_fn));
    JSObject::set_property(&proto, "constructor", JSValue::Object(ctor_obj.clone()));

    ctor_obj
}

// -----------------------------------------------------------------------------
// Global URI Utilities
// -----------------------------------------------------------------------------

pub fn encode_uri_function() -> Rc<JSFunction> {
    JSFunction::new_native("encodeURI", |_this, args| {
        let uri = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        Ok(JSValue::String(percent_encode(&uri, false)))
    })
}

pub fn decode_uri_function() -> Rc<JSFunction> {
    JSFunction::new_native("decodeURI", |_this, args| {
        let uri = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        percent_decode(&uri, false).map(JSValue::String)
    })
}

pub fn encode_uri_component_function() -> Rc<JSFunction> {
    JSFunction::new_native("encodeURIComponent", |_this, args| {
        let comp = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        Ok(JSValue::String(percent_encode(&comp, true)))
    })
}

pub fn decode_uri_component_function() -> Rc<JSFunction> {
    JSFunction::new_native("decodeURIComponent", |_this, args| {
        let comp = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        percent_decode(&comp, false).map(JSValue::String)
    })
}
