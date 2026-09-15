//! Safe Rust reimplementation of Google V8's TypedArray, ArrayBuffer, and DataView built-ins.
//!
//! Exposes ArrayBuffer, DataView, and 9 standard TypedArray constructors with full prototype methods.

use crate::objects::js_object::JSObject;
use crate::objects::map::InstanceType;
use crate::objects::typed_array::TypedArrayKind;
use crate::objects::value::JSValue;
use crate::objects::JSFunction;
use std::cell::RefCell;
use std::rc::Rc;

pub fn create_array_buffer_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    let slice_fn = JSFunction::new_native("slice", |this, args| {
        let buffer_rc = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("ArrayBuffer.prototype.slice called on non-object".to_string()),
        };

        let is_buf = buffer_rc.borrow().map.borrow().instance_type == InstanceType::JSArrayBuffer;
        if !is_buf {
            return Err("Incompatible receiver for ArrayBuffer.prototype.slice".to_string());
        }

        let bytes_opt = buffer_rc.borrow().ext_or_default().array_buffer_data.clone();
        let bytes = bytes_opt.ok_or_else(|| "ArrayBuffer has no data".to_string())?;
        let src_bytes = bytes.borrow();
        let len = src_bytes.len();

        let start_arg = args.get(0).map(|v| v.to_number() as i64).unwrap_or(0);
        let end_arg = args.get(1).map(|v| v.to_number() as i64).unwrap_or(len as i64);

        let start = if start_arg < 0 {
            (len as i64 + start_arg).max(0) as usize
        } else {
            start_arg.min(len as i64) as usize
        };

        let end = if end_arg < 0 {
            (len as i64 + end_arg).max(0) as usize
        } else {
            end_arg.min(len as i64) as usize
        };

        let slice_len = if end > start { end - start } else { 0 };
        let mut new_bytes = vec![0u8; slice_len];
        if slice_len > 0 {
            new_bytes.copy_from_slice(&src_bytes[start..end]);
        }

        Ok(JSValue::Object(JSObject::new_array_buffer_from_bytes(new_bytes, None)))
    });
    JSObject::set_property(&proto, "slice", JSValue::Function(slice_fn));

    proto
}

pub fn create_array_buffer_constructor(proto: &Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let proto_clone = proto.clone();
    let ctor = JSFunction::new_closure("ArrayBuffer", move |_this, args| {
        let len = args.get(0).map(|v| v.to_number().max(0.0) as usize).unwrap_or(0);
        Ok(JSValue::Object(JSObject::new_array_buffer(len, Some(proto_clone.clone()))))
    });

    let is_view_fn = JSFunction::new_native("isView", |_this, args| {
        let arg = args.get(0).unwrap_or(&JSValue::Undefined);
        if let JSValue::Object(o) = arg {
            let t = o.borrow().map.borrow().instance_type;
            let is_v = t == InstanceType::JSTypedArray || t == InstanceType::JSDataView;
            Ok(JSValue::Boolean(is_v))
        } else {
            Ok(JSValue::Boolean(false))
        }
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(proto.clone()));
    JSObject::set_property(&ctor_obj, "isView", JSValue::Function(is_view_fn));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor));
    JSObject::set_property(proto, "constructor", JSValue::Object(ctor_obj.clone()));

    ctor_obj
}

pub fn create_typed_array_prototype(_kind: TypedArrayKind) -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // .subarray(begin, end)
    let subarray_fn = JSFunction::new_native("subarray", move |this, args| {
        let ta_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("Method called on non-object".to_string()),
        };

        let ta_data_opt = ta_obj.borrow().ext_or_default().typed_array_data.clone();
        let ta_data = ta_data_opt.ok_or_else(|| "Receiver is not a TypedArray".to_string())?;

        let len = ta_data.length;
        let start_arg = args.get(0).map(|v| v.to_number() as i64).unwrap_or(0);
        let end_arg = args.get(1).map(|v| v.to_number() as i64).unwrap_or(len as i64);

        let start = if start_arg < 0 {
            (len as i64 + start_arg).max(0) as usize
        } else {
            start_arg.min(len as i64) as usize
        };

        let end = if end_arg < 0 {
            (len as i64 + end_arg).max(0) as usize
        } else {
            end_arg.min(len as i64) as usize
        };

        let sub_len = if end > start { end - start } else { 0 };
        let new_offset = ta_data.byte_offset + start * ta_data.kind.element_size();

        let sub_ta = JSObject::new_typed_array(
            ta_data.kind,
            ta_data.buffer.clone(),
            new_offset,
            sub_len,
            None,
        );
        Ok(JSValue::Object(sub_ta))
    });
    JSObject::set_property(&proto, "subarray", JSValue::Function(subarray_fn));

    // .set(arrayLike, offset)
    let set_fn = JSFunction::new_native("set", move |this, args| {
        let ta_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("Method called on non-object".to_string()),
        };

        let ta_data = ta_obj.borrow().ext_or_default().typed_array_data.clone()
            .ok_or_else(|| "Receiver is not a TypedArray".to_string())?;

        let src = args.get(0).ok_or_else(|| "Source array required for .set()".to_string())?;
        let offset = args.get(1).map(|v| v.to_number().max(0.0) as usize).unwrap_or(0);

        let buf_rc = ta_data.buffer.clone();
        let buf_obj = buf_rc.borrow();
        let buf_bytes_rc = buf_obj.ext_or_default().array_buffer_data.clone()
            .ok_or_else(|| "ArrayBuffer has no data".to_string())?;
        let mut buf_bytes = buf_bytes_rc.borrow_mut();

        match src {
            JSValue::Array(arr) => {
                let elems = arr.borrow().elements.clone();
                for (i, elem) in elems.iter().enumerate() {
                    let target_idx = offset + i;
                    if target_idx < ta_data.length {
                        ta_data.kind.write_element(&mut buf_bytes, ta_data.byte_offset, target_idx, elem);
                    }
                }
            }
            JSValue::Object(src_obj) => {
                if let Some(ref src_ta) = src_obj.borrow().ext_or_default().typed_array_data {
                    let src_buf_rc = src_ta.buffer.clone();
                    let src_buf = src_buf_rc.borrow();
                    if let Some(ref src_bytes_rc) = src_buf.ext_or_default().array_buffer_data {
                        let src_bytes = src_bytes_rc.borrow();
                        for i in 0..src_ta.length {
                            let target_idx = offset + i;
                            if target_idx < ta_data.length {
                                let val = src_ta.kind.read_element(&src_bytes, src_ta.byte_offset, i);
                                ta_data.kind.write_element(&mut buf_bytes, ta_data.byte_offset, target_idx, &val);
                            }
                        }
                    }
                }
            }
            _ => return Err("Invalid source for TypedArray.set".to_string()),
        }

        Ok(JSValue::Undefined)
    });
    JSObject::set_property(&proto, "set", JSValue::Function(set_fn));

    // .fill(value, start, end)
    let fill_fn = JSFunction::new_native("fill", move |this, args| {
        let ta_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("Method called on non-object".to_string()),
        };

        let ta_data = ta_obj.borrow().ext_or_default().typed_array_data.clone()
            .ok_or_else(|| "Receiver is not a TypedArray".to_string())?;

        let val = args.get(0).cloned().unwrap_or(JSValue::Smi(0));
        let len = ta_data.length;
        let start_arg = args.get(1).map(|v| v.to_number() as i64).unwrap_or(0);
        let end_arg = args.get(2).map(|v| v.to_number() as i64).unwrap_or(len as i64);

        let start = if start_arg < 0 { (len as i64 + start_arg).max(0) as usize } else { start_arg.min(len as i64) as usize };
        let end = if end_arg < 0 { (len as i64 + end_arg).max(0) as usize } else { end_arg.min(len as i64) as usize };

        let buf_rc = ta_data.buffer.clone();
        let buf_obj = buf_rc.borrow();
        if let Some(ref buf_bytes_rc) = buf_obj.ext_or_default().array_buffer_data {
            let mut buf_bytes = buf_bytes_rc.borrow_mut();
            for i in start..end {
                ta_data.kind.write_element(&mut buf_bytes, ta_data.byte_offset, i, &val);
            }
        }

        Ok(this.clone())
    });
    JSObject::set_property(&proto, "fill", JSValue::Function(fill_fn));

    // .join(separator)
    let join_fn = JSFunction::new_native("join", move |this, args| {
        let ta_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("Method called on non-object".to_string()),
        };
        let ta_data = ta_obj.borrow().ext_or_default().typed_array_data.clone()
            .ok_or_else(|| "Receiver is not a TypedArray".to_string())?;

        let sep = args.get(0).map(|v| v.to_string()).unwrap_or_else(|| ",".to_string());
        let buf_rc = ta_data.buffer.clone();
        let buf_obj = buf_rc.borrow();
        let buf_bytes_rc = buf_obj.ext_or_default().array_buffer_data.clone().ok_or_else(|| "Buffer error".to_string())?;
        let buf_bytes = buf_bytes_rc.borrow();

        let mut parts = Vec::new();
        for i in 0..ta_data.length {
            let v = ta_data.kind.read_element(&buf_bytes, ta_data.byte_offset, i);
            parts.push(v.to_string());
        }
        Ok(JSValue::String(parts.join(&sep)))
    });
    JSObject::set_property(&proto, "join", JSValue::Function(join_fn));

    // .toReversed() (ES2023)
    let to_reversed_fn = JSFunction::new_native("toReversed", |this, _args| {
        let ta_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("TypeError: Method called on non-object".to_string()),
        };

        let (kind, len, mut elems) = read_all_elements(&ta_obj)?;
        elems.reverse();

        let new_ta = new_typed_array_instance(kind, len, &elems, ta_obj.borrow().map.borrow().prototype.clone());
        Ok(JSValue::Object(new_ta))
    });
    JSObject::set_property(&proto, "toReversed", JSValue::Function(to_reversed_fn));

    // .toSorted(compareFn) (ES2023)
    let to_sorted_fn = JSFunction::new_native("toSorted", |this, args| {
        let ta_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("TypeError: Method called on non-object".to_string()),
        };

        let (kind, len, mut elems) = read_all_elements(&ta_obj)?;
        let compare_fn = match args.first() {
            Some(JSValue::Function(f)) => Some(f),
            _ => None,
        };
        crate::builtins::array::sort_elements(&mut elems, compare_fn)?;

        let new_ta = new_typed_array_instance(kind, len, &elems, ta_obj.borrow().map.borrow().prototype.clone());
        Ok(JSValue::Object(new_ta))
    });
    JSObject::set_property(&proto, "toSorted", JSValue::Function(to_sorted_fn));

    // .with(index, value) (ES2023)
    let with_fn = JSFunction::new_native("with", |this, args| {
        let ta_obj = match this {
            JSValue::Object(o) => o.clone(),
            _ => return Err("TypeError: Method called on non-object".to_string()),
        };

        let (kind, len, mut elems) = read_all_elements(&ta_obj)?;
        let index_arg = args.get(0).map(|v| v.to_number() as isize).unwrap_or(0);
        let actual_idx = if index_arg < 0 {
            len as isize + index_arg
        } else {
            index_arg
        };

        if actual_idx < 0 || actual_idx >= len as isize {
            return Err("RangeError: Invalid index".to_string());
        }

        let val = args.get(1).cloned().unwrap_or(JSValue::Smi(0));
        elems[actual_idx as usize] = val;

        let new_ta = new_typed_array_instance(kind, len, &elems, ta_obj.borrow().map.borrow().prototype.clone());
        Ok(JSValue::Object(new_ta))
    });
    JSObject::set_property(&proto, "with", JSValue::Function(with_fn));

    proto
}

fn read_all_elements(ta_obj: &Rc<RefCell<JSObject>>) -> Result<(TypedArrayKind, usize, Vec<JSValue>), String> {
    let borrowed = ta_obj.borrow();
    let ta_data = borrowed
        .ext_or_default()
        .typed_array_data
        .clone()
        .ok_or_else(|| "TypeError: Receiver is not a TypedArray".to_string())?;

    let buf_rc = ta_data.buffer.clone();
    let buf_obj = buf_rc.borrow();
    let buf_bytes_rc = buf_obj
        .ext_or_default()
        .array_buffer_data
        .clone()
        .ok_or_else(|| "Buffer error".to_string())?;
    let buf_bytes = buf_bytes_rc.borrow();

    let mut elems = Vec::with_capacity(ta_data.length);
    for i in 0..ta_data.length {
        let v = ta_data.kind.read_element(&buf_bytes, ta_data.byte_offset, i);
        elems.push(v);
    }
    Ok((ta_data.kind, ta_data.length, elems))
}

fn new_typed_array_instance(
    kind: TypedArrayKind,
    length: usize,
    elements: &[JSValue],
    proto: Option<Rc<RefCell<JSObject>>>,
) -> Rc<RefCell<JSObject>> {
    let byte_len = length * kind.element_size();
    let buffer = JSObject::new_array_buffer(byte_len, None);
    if let Some(ref buf_bytes) = buffer.borrow().ext_or_default().array_buffer_data {
        let mut b = buf_bytes.borrow_mut();
        for (i, elem) in elements.iter().enumerate() {
            kind.write_element(&mut b, 0, i, elem);
        }
    }
    JSObject::new_typed_array(kind, buffer, 0, length, proto)
}

pub fn create_typed_array_constructor(kind: TypedArrayKind, proto: &Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let proto_clone = proto.clone();
    let name = kind.name();

    let ctor = JSFunction::new_closure(name, move |_this, args| {
        let first_arg = args.get(0);
        match first_arg {
            Some(JSValue::Object(o)) if {
                let it = o.borrow().map.borrow().instance_type;
                it == InstanceType::JSArrayBuffer || it == InstanceType::JSSharedArrayBuffer
            } => {
                let buffer = o.clone();
                let byte_offset = args.get(1).map(|v| v.to_number().max(0.0) as usize).unwrap_or(0);
                let buf_len = {
                    let b = buffer.borrow();
                    let ext = b.ext_or_default();
                    if let Some(ref ab) = ext.array_buffer_data {
                        ab.borrow().len()
                    } else if let Some(ref sab) = ext.shared_array_buffer_data {
                        sab.read().unwrap().len()
                    } else {
                        0
                    }
                };
                let remaining = if buf_len > byte_offset { buf_len - byte_offset } else { 0 };
                let max_length = remaining / kind.element_size();
                let length = args.get(2).map(|v| (v.to_number() as usize).min(max_length)).unwrap_or(max_length);

                Ok(JSValue::Object(JSObject::new_typed_array(
                    kind,
                    buffer,
                    byte_offset,
                    length,
                    Some(proto_clone.clone()),
                )))
            }
            Some(JSValue::Array(arr)) => {
                let elems = arr.borrow().elements.clone();
                let length = elems.len();
                let byte_len = length * kind.element_size();
                let buffer = JSObject::new_array_buffer(byte_len, None);

                if let Some(ref buf_bytes) = buffer.borrow().ext_or_default().array_buffer_data {
                    let mut b = buf_bytes.borrow_mut();
                    for (i, elem) in elems.iter().enumerate() {
                        kind.write_element(&mut b, 0, i, elem);
                    }
                }

                Ok(JSValue::Object(JSObject::new_typed_array(
                    kind,
                    buffer,
                    0,
                    length,
                    Some(proto_clone.clone()),
                )))
            }
            Some(JSValue::Smi(len)) => {
                let length = (*len).max(0) as usize;
                let byte_len = length * kind.element_size();
                let buffer = JSObject::new_array_buffer(byte_len, None);

                Ok(JSValue::Object(JSObject::new_typed_array(
                    kind,
                    buffer,
                    0,
                    length,
                    Some(proto_clone.clone()),
                )))
            }
            Some(JSValue::Number(len)) => {
                let length = (*len).max(0.0) as usize;
                let byte_len = length * kind.element_size();
                let buffer = JSObject::new_array_buffer(byte_len, None);

                Ok(JSValue::Object(JSObject::new_typed_array(
                    kind,
                    buffer,
                    0,
                    length,
                    Some(proto_clone.clone()),
                )))
            }
            _ => {
                let buffer = JSObject::new_array_buffer(0, None);
                Ok(JSValue::Object(JSObject::new_typed_array(
                    kind,
                    buffer,
                    0,
                    0,
                    Some(proto_clone.clone()),
                )))
            }
        }
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(proto.clone()));
    JSObject::set_property(&ctor_obj, "BYTES_PER_ELEMENT", JSValue::Smi(kind.element_size() as i32));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor));
    JSObject::set_property(proto, "BYTES_PER_ELEMENT", JSValue::Smi(kind.element_size() as i32));
    JSObject::set_property(proto, "constructor", JSValue::Object(ctor_obj.clone()));

    ctor_obj
}

pub fn create_data_view_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // Macro for generating DataView getters and setters
    macro_rules! define_getter_setter {
        ($proto:expr, $get_name:literal, $set_name:literal, $size:literal, $get_expr:expr, $set_expr:expr) => {
            let get_fn = JSFunction::new_native($get_name, move |this, args| {
                let dv_obj = match this {
                    JSValue::Object(o) => o,
                    _ => return Err("Method called on non-object".to_string()),
                };
                let dv_borrow = dv_obj.borrow();
                let ext = dv_borrow.ext_or_default();
                let dv = ext.data_view_data.as_ref()
                    .ok_or_else(|| "Receiver is not a DataView".to_string())?;

                let offset = match args.get(0) {
                    Some(JSValue::Smi(s)) => if *s >= 0 { *s as usize } else { 0 },
                    Some(v) => v.to_number().max(0.0) as usize,
                    None => 0,
                };
                let little_endian = match args.get(1) {
                    Some(v) => v.to_boolean(),
                    None => false,
                };

                if offset + $size > dv.byte_length {
                    return Err("Offset is outside the bounds of the DataView".to_string());
                }

                let buf_obj = dv.buffer.borrow();
                let ext_buf = buf_obj.ext_or_default();
                let bytes_rc = ext_buf.array_buffer_data.as_ref().ok_or_else(|| "Buffer error".to_string())?;
                let bytes = bytes_rc.borrow();
                let pos = dv.byte_offset + offset;

                $get_expr(&bytes[pos..pos + $size], little_endian)
            });
            JSObject::set_property($proto, $get_name, JSValue::Function(get_fn));

            let set_fn = JSFunction::new_native($set_name, move |this, args| {
                let dv_obj = match this {
                    JSValue::Object(o) => o,
                    _ => return Err("Method called on non-object".to_string()),
                };
                let dv_borrow = dv_obj.borrow();
                let ext = dv_borrow.ext_or_default();
                let dv = ext.data_view_data.as_ref()
                    .ok_or_else(|| "Receiver is not a DataView".to_string())?;

                let offset = match args.get(0) {
                    Some(JSValue::Smi(s)) => if *s >= 0 { *s as usize } else { 0 },
                    Some(v) => v.to_number().max(0.0) as usize,
                    None => 0,
                };
                let default_val = JSValue::Smi(0);
                let val = args.get(1).unwrap_or(&default_val);
                let little_endian = match args.get(2) {
                    Some(v) => v.to_boolean(),
                    None => false,
                };

                if offset + $size > dv.byte_length {
                    return Err("Offset is outside the bounds of the DataView".to_string());
                }

                let buf_obj = dv.buffer.borrow();
                let ext_buf = buf_obj.ext_or_default();
                let bytes_rc = ext_buf.array_buffer_data.as_ref().ok_or_else(|| "Buffer error".to_string())?;
                let mut bytes = bytes_rc.borrow_mut();
                let pos = dv.byte_offset + offset;

                $set_expr(&mut bytes[pos..pos + $size], val, little_endian);
                Ok(JSValue::Undefined)
            });
            JSObject::set_property($proto, $set_name, JSValue::Function(set_fn));
        };
    }

    define_getter_setter!(
        &proto, "getInt8", "setInt8", 1,
        |slice: &[u8], _le: bool| Ok(JSValue::Smi(slice[0] as i8 as i32)),
        |slice: &mut [u8], val: &JSValue, _le: bool| { slice[0] = val.to_number() as i64 as i8 as u8; }
    );

    define_getter_setter!(
        &proto, "getUint8", "setUint8", 1,
        |slice: &[u8], _le: bool| Ok(JSValue::Smi(slice[0] as i32)),
        |slice: &mut [u8], val: &JSValue, _le: bool| { slice[0] = val.to_number() as i64 as u8; }
    );

    define_getter_setter!(
        &proto, "getInt16", "setInt16", 2,
        |slice: &[u8], le: bool| {
            let v = if le { i16::from_le_bytes([slice[0], slice[1]]) } else { i16::from_be_bytes([slice[0], slice[1]]) };
            Ok(JSValue::Smi(v as i32))
        },
        |slice: &mut [u8], val: &JSValue, le: bool| {
            let n = val.to_number() as i64 as i16;
            let b = if le { n.to_le_bytes() } else { n.to_be_bytes() };
            slice.copy_from_slice(&b);
        }
    );

    define_getter_setter!(
        &proto, "getUint16", "setUint16", 2,
        |slice: &[u8], le: bool| {
            let v = if le { u16::from_le_bytes([slice[0], slice[1]]) } else { u16::from_be_bytes([slice[0], slice[1]]) };
            Ok(JSValue::Smi(v as i32))
        },
        |slice: &mut [u8], val: &JSValue, le: bool| {
            let n = val.to_number() as i64 as u16;
            let b = if le { n.to_le_bytes() } else { n.to_be_bytes() };
            slice.copy_from_slice(&b);
        }
    );

    define_getter_setter!(
        &proto, "getInt32", "setInt32", 4,
        |slice: &[u8], le: bool| {
            let mut b = [0u8; 4];
            b.copy_from_slice(slice);
            let v = if le { i32::from_le_bytes(b) } else { i32::from_be_bytes(b) };
            Ok(JSValue::Smi(v))
        },
        |slice: &mut [u8], val: &JSValue, le: bool| {
            let n = val.to_number() as i64 as i32;
            let b = if le { n.to_le_bytes() } else { n.to_be_bytes() };
            slice.copy_from_slice(&b);
        }
    );

    define_getter_setter!(
        &proto, "getUint32", "setUint32", 4,
        |slice: &[u8], le: bool| {
            let mut b = [0u8; 4];
            b.copy_from_slice(slice);
            let v = if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) };
            Ok(JSValue::Number(v as f64))
        },
        |slice: &mut [u8], val: &JSValue, le: bool| {
            let n = val.to_number() as i64 as u32;
            let b = if le { n.to_le_bytes() } else { n.to_be_bytes() };
            slice.copy_from_slice(&b);
        }
    );

    define_getter_setter!(
        &proto, "getFloat16", "setFloat16", 2,
        |slice: &[u8], le: bool| {
            let u = if le { u16::from_le_bytes([slice[0], slice[1]]) } else { u16::from_be_bytes([slice[0], slice[1]]) };
            let f = crate::objects::typed_array::f16_to_f32(u);
            Ok(JSValue::Number(f as f64))
        },
        |slice: &mut [u8], val: &JSValue, le: bool| {
            let f = val.to_number() as f32;
            let u = crate::objects::typed_array::f32_to_f16(f);
            let b = if le { u.to_le_bytes() } else { u.to_be_bytes() };
            slice.copy_from_slice(&b);
        }
    );

    define_getter_setter!(
        &proto, "getFloat32", "setFloat32", 4,
        |slice: &[u8], le: bool| {
            let mut b = [0u8; 4];
            b.copy_from_slice(slice);
            let v = if le { f32::from_le_bytes(b) } else { f32::from_be_bytes(b) };
            Ok(JSValue::Number(v as f64))
        },
        |slice: &mut [u8], val: &JSValue, le: bool| {
            let f = val.to_number() as f32;
            let b = if le { f.to_le_bytes() } else { f.to_be_bytes() };
            slice.copy_from_slice(&b);
        }
    );

    define_getter_setter!(
        &proto, "getFloat64", "setFloat64", 8,
        |slice: &[u8], le: bool| {
            let mut b = [0u8; 8];
            b.copy_from_slice(slice);
            let v = if le { f64::from_le_bytes(b) } else { f64::from_be_bytes(b) };
            Ok(JSValue::Number(v))
        },
        |slice: &mut [u8], val: &JSValue, le: bool| {
            let f = val.to_number();
            let b = if le { f.to_le_bytes() } else { f.to_be_bytes() };
            slice.copy_from_slice(&b);
        }
    );

    proto
}

pub fn create_data_view_constructor(proto: &Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let proto_clone = proto.clone();
    let ctor = JSFunction::new_closure("DataView", move |_this, args| {
        let buffer_arg = args.get(0).ok_or_else(|| "DataView constructor requires ArrayBuffer".to_string())?;
        let buffer = match buffer_arg {
            JSValue::Object(o) if o.borrow().map.borrow().instance_type == InstanceType::JSArrayBuffer => o.clone(),
            _ => return Err("First argument to DataView constructor must be an ArrayBuffer".to_string()),
        };

        let byte_offset = args.get(1).map(|v| v.to_number().max(0.0) as usize).unwrap_or(0);
        let buf_len = buffer.borrow().ext_or_default().array_buffer_data.as_ref().map(|b| b.borrow().len()).unwrap_or(0);
        let remaining = if buf_len > byte_offset { buf_len - byte_offset } else { 0 };
        let byte_length = args.get(2).map(|v| (v.to_number() as usize).min(remaining)).unwrap_or(remaining);

        Ok(JSValue::Object(JSObject::new_data_view(
            buffer,
            byte_offset,
            byte_length,
            Some(proto_clone.clone()),
        )))
    });

    let ctor_obj = JSObject::new_empty(None);
    JSObject::set_property(&ctor_obj, "prototype", JSValue::Object(proto.clone()));
    JSObject::set_property(&ctor_obj, "__call__", JSValue::Function(ctor));
    JSObject::set_property(proto, "constructor", JSValue::Object(ctor_obj.clone()));

    ctor_obj
}
