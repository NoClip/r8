//! Safe Rust reimplementation of the WHATWG Encoding API (`TextEncoder` and `TextDecoder`).
//!
//! Provides zero-copy capable UTF-8 string encoding to `Uint8Array` and decoding
//! from typed arrays / ArrayBuffers with Byte Order Mark (BOM) handling.

use crate::objects::js_object::JSObject;
use crate::objects::map::InstanceType;
use crate::objects::typed_array::TypedArrayKind;
use crate::objects::value::JSValue;
use crate::objects::JSFunction;
use std::cell::RefCell;
use std::rc::Rc;

/// Creates the `TextEncoder` constructor object and its prototype.
pub fn create_text_encoder_constructor() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);
    JSObject::set_property(&proto, "encoding", JSValue::String("utf-8".to_string()));

    // TextEncoder.prototype.encode(input)
    let encode_fn = JSFunction::new_native("encode", |_this, args| {
        let input_str = args
            .first()
            .map(|v| match v {
                JSValue::Undefined => String::new(),
                _ => v.to_string_val(),
            })
            .unwrap_or_default();

        let bytes = input_str.into_bytes();
        let len = bytes.len();
        let buffer = JSObject::new_array_buffer_from_bytes(bytes, None);
        let uint8_array = JSObject::new_typed_array(
            TypedArrayKind::Uint8,
            buffer,
            0,
            len,
            None,
        );

        Ok(JSValue::Object(uint8_array))
    });
    JSObject::set_property(&proto, "encode", JSValue::Function(encode_fn));

    // TextEncoder.prototype.encodeInto(source, destination)
    let encode_into_fn = JSFunction::new_native("encodeInto", |_this, args| {
        let src_str = args.first().map(|v| v.to_string_val()).unwrap_or_default();
        let dest_obj = match args.get(1) {
            Some(JSValue::Object(o)) => o.clone(),
            _ => return Err("TypeError: Destination must be a Uint8Array".to_string()),
        };

        let is_typed_array = dest_obj.borrow().map.borrow().instance_type == InstanceType::JSTypedArray;
        if !is_typed_array {
            return Err("TypeError: Destination must be a Uint8Array".to_string());
        }

        let ta_data = dest_obj
            .borrow()
            .ext_or_default()
            .typed_array_data
            .clone()
            .ok_or_else(|| "TypeError: Invalid TypedArray".to_string())?;

        if ta_data.kind != TypedArrayKind::Uint8 {
            return Err("TypeError: Destination must be a Uint8Array".to_string());
        }

        let buf_rc = ta_data.buffer.clone();
        let buf_obj = buf_rc.borrow();
        let buf_bytes_rc = buf_obj
            .ext_or_default()
            .array_buffer_data
            .clone()
            .ok_or_else(|| "ArrayBuffer has no data".to_string())?;
        let mut buf_bytes = buf_bytes_rc.borrow_mut();

        let mut read_chars = 0;
        let mut written_bytes = 0;
        let dest_capacity = ta_data.length;

        for ch in src_str.chars() {
            let mut enc_buf = [0u8; 4];
            let encoded = ch.encode_utf8(&mut enc_buf).as_bytes();
            if written_bytes + encoded.len() > dest_capacity {
                break;
            }
            for &b in encoded {
                buf_bytes[ta_data.byte_offset + written_bytes] = b;
                written_bytes += 1;
            }
            read_chars += 1;
        }

        let result = JSObject::new_empty(None);
        JSObject::set_property(&result, "read", JSValue::Smi(read_chars as i32));
        JSObject::set_property(&result, "written", JSValue::Smi(written_bytes as i32));
        Ok(JSValue::Object(result))
    });
    JSObject::set_property(&proto, "encodeInto", JSValue::Function(encode_into_fn));

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(proto.clone()));

    let proto_clone = proto.clone();
    let ctor_fn = JSFunction::new_closure("TextEncoder", move |_this, _args| {
        let instance = JSObject::new_empty(Some(proto_clone.clone()));
        JSObject::set_property(&instance, "encoding", JSValue::String("utf-8".to_string()));
        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor_fn));
    JSObject::set_property(&proto, "constructor", JSValue::Object(ctor_obj.clone()));

    ctor_obj
}

/// Creates the `TextDecoder` constructor object and its prototype.
pub fn create_text_decoder_constructor() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);
    JSObject::set_property(&proto, "encoding", JSValue::String("utf-8".to_string()));
    JSObject::set_property(&proto, "fatal", JSValue::Boolean(false));
    JSObject::set_property(&proto, "ignoreBOM", JSValue::Boolean(false));

    // TextDecoder.prototype.decode(input, options)
    let decode_fn = JSFunction::new_native("decode", |this, args| {
        let (fatal, ignore_bom) = match this {
            JSValue::Object(o) => {
                let fatal = match o.borrow().get_property("fatal") {
                    JSValue::Boolean(b) => b,
                    _ => false,
                };
                let ignore_bom = match o.borrow().get_property("ignoreBOM") {
                    JSValue::Boolean(b) => b,
                    _ => false,
                };
                (fatal, ignore_bom)
            }
            _ => (false, false),
        };

        let raw_bytes: Vec<u8> = match args.first() {
            None | Some(JSValue::Undefined) => Vec::new(),
            Some(JSValue::Object(o)) => {
                let borrowed = o.borrow();
                let it = borrowed.map.borrow().instance_type;
                if it == InstanceType::JSTypedArray {
                    if let Some(ref ta) = borrowed.ext_or_default().typed_array_data {
                        let buf = ta.buffer.borrow();
                        if let Some(ref data) = buf.ext_or_default().array_buffer_data {
                            let b = data.borrow();
                            let start = ta.byte_offset;
                            let end = (start + ta.length * ta.kind.element_size()).min(b.len());
                            if start <= end {
                                b[start..end].to_vec()
                            } else {
                                Vec::new()
                            }
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    }
                } else if it == InstanceType::JSArrayBuffer {
                    if let Some(ref data) = borrowed.ext_or_default().array_buffer_data {
                        data.borrow().clone()
                    } else {
                        Vec::new()
                    }
                } else if it == InstanceType::JSDataView {
                    if let Some(ref dv) = borrowed.ext_or_default().data_view_data {
                        let buf = dv.buffer.borrow();
                        if let Some(ref data) = buf.ext_or_default().array_buffer_data {
                            let b = data.borrow();
                            let start = dv.byte_offset;
                            let end = (start + dv.byte_length).min(b.len());
                            if start <= end {
                                b[start..end].to_vec()
                            } else {
                                Vec::new()
                            }
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        };

        // Handle BOM: UTF-8 BOM is [0xEF, 0xBB, 0xBF]
        let slice = if !ignore_bom && raw_bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &raw_bytes[3..]
        } else {
            &raw_bytes[..]
        };

        if fatal {
            match std::str::from_utf8(slice) {
                Ok(s) => Ok(JSValue::String(s.to_string())),
                Err(_) => Err("TypeError: The encoded data was not valid UTF-8".to_string()),
            }
        } else {
            let decoded = String::from_utf8_lossy(slice).to_string();
            Ok(JSValue::String(decoded))
        }
    });
    JSObject::set_property(&proto, "decode", JSValue::Function(decode_fn));

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(proto.clone()));

    let proto_clone = proto.clone();
    let ctor_fn = JSFunction::new_closure("TextDecoder", move |_this, args| {
        let label = args
            .first()
            .map(|v| v.to_string_val().to_lowercase())
            .unwrap_or_else(|| "utf-8".to_string());

        if label != "utf-8" && label != "utf8" && label != "unicode-1-1-utf-8" {
            return Err(format!("RangeError: The \"{}\" encoding is not supported", label));
        }

        let mut fatal = false;
        let mut ignore_bom = false;

        if let Some(JSValue::Object(opts)) = args.get(1) {
            let borrowed = opts.borrow();
            if let JSValue::Boolean(b) = borrowed.get_property("fatal") {
                fatal = b;
            }
            if let JSValue::Boolean(b) = borrowed.get_property("ignoreBOM") {
                ignore_bom = b;
            }
        }

        let instance = JSObject::new_empty(Some(proto_clone.clone()));
        JSObject::set_property(&instance, "encoding", JSValue::String("utf-8".to_string()));
        JSObject::set_property(&instance, "fatal", JSValue::Boolean(fatal));
        JSObject::set_property(&instance, "ignoreBOM", JSValue::Boolean(ignore_bom));
        Ok(JSValue::Object(instance))
    });

    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor_fn));
    JSObject::set_property(&proto, "constructor", JSValue::Object(ctor_obj.clone()));

    ctor_obj
}
