//! Safe Rust reimplementation of Google V8's ECMAScript `Array` built-in constructor and prototype.
//!
//! Implements `Array.isArray`, `Array.of`, `Array.from`, and array prototype methods:
//! `push`, `pop`, `shift`, `unshift`, `join`, `slice`, `reverse`, `indexOf`,
//! `map`, `filter`, `reduce`, `reduceRight`, `forEach`, `find`, `findIndex`,
//! `includes`, `concat`, `some`, `every`, `flat`, and `fill`.

use crate::objects::{JSArray, JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

/// Helper function for ECMAScript `SameValueZero` equality comparison.
fn same_value_zero(a: &JSValue, b: &JSValue) -> bool {
    if let (JSValue::Number(x), JSValue::Number(y)) = (a, b) {
        if x.is_nan() && y.is_nan() {
            return true;
        }
    }
    a.strict_equal(b)
}

/// Helper function to sort JSValue elements according to an optional ECMAScript compareFn.
pub fn sort_elements(elements: &mut [JSValue], compare_fn: Option<&Rc<JSFunction>>) -> Result<(), String> {
    if let Some(cf) = compare_fn {
        let mut err: Option<String> = None;
        elements.sort_by(|a, b| {
            if err.is_some() {
                return std::cmp::Ordering::Equal;
            }
            match cf.call(&JSValue::Undefined, &[a.clone(), b.clone()]) {
                Ok(res) => {
                    let num = res.to_number();
                    if num.is_nan() || num == 0.0 {
                        std::cmp::Ordering::Equal
                    } else if num < 0.0 {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    }
                }
                Err(e) => {
                    err = Some(e);
                    std::cmp::Ordering::Equal
                }
            }
        });
        if let Some(e) = err {
            return Err(e);
        }
    } else {
        elements.sort_by(|a, b| {
            let sa = a.to_string_val();
            let sb = b.to_string_val();
            sa.cmp(&sb)
        });
    }
    Ok(())
}

/// Creates the `Array.prototype` object equipped with ECMAScript array methods.

pub fn create_array_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // Array.prototype.push(...items)
    JSObject::set_property(
        &proto,
        "push",
        JSValue::Function(JSFunction::new_native("push", |this, args| {
            if let JSValue::Array(arr) = this {
                let mut borrowed = arr.borrow_mut();
                for arg in args {
                    borrowed.elements.push(arg.clone());
                }
                Ok(JSValue::Smi(borrowed.elements.len() as i32))
            } else {
                Ok(JSValue::Smi(0))
            }
        })),
    );

    // Array.prototype.pop()
    JSObject::set_property(
        &proto,
        "pop",
        JSValue::Function(JSFunction::new_native("pop", |this, _args| {
            if let JSValue::Array(arr) = this {
                let val = arr.borrow_mut().elements.pop().unwrap_or(JSValue::Undefined);
                Ok(val)
            } else {
                Ok(JSValue::Undefined)
            }
        })),
    );

    // Array.prototype.shift()
    JSObject::set_property(
        &proto,
        "shift",
        JSValue::Function(JSFunction::new_native("shift", |this, _args| {
            if let JSValue::Array(arr) = this {
                let mut borrowed = arr.borrow_mut();
                if borrowed.elements.is_empty() {
                    Ok(JSValue::Undefined)
                } else {
                    Ok(borrowed.elements.remove(0))
                }
            } else {
                Ok(JSValue::Undefined)
            }
        })),
    );

    // Array.prototype.unshift(...items)
    JSObject::set_property(
        &proto,
        "unshift",
        JSValue::Function(JSFunction::new_native("unshift", |this, args| {
            if let JSValue::Array(arr) = this {
                let mut borrowed = arr.borrow_mut();
                for (i, arg) in args.iter().enumerate() {
                    borrowed.elements.insert(i, arg.clone());
                }
                Ok(JSValue::Smi(borrowed.elements.len() as i32))
            } else {
                Ok(JSValue::Smi(0))
            }
        })),
    );

    // Array.prototype.join(separator)
    JSObject::set_property(
        &proto,
        "join",
        JSValue::Function(JSFunction::new_native("join", |this, args| {
            if let JSValue::Array(arr) = this {
                let sep = args
                    .first()
                    .map(|v| v.to_string_val())
                    .unwrap_or_else(|| ",".to_string());
                let borrowed = arr.borrow();
                let parts: Vec<String> = borrowed
                    .elements
                    .iter()
                    .map(|e| match e {
                        JSValue::Null | JSValue::Undefined => String::new(),
                        _ => e.to_string_val(),
                    })
                    .collect();
                Ok(JSValue::String(parts.join(&sep)))
            } else {
                Ok(JSValue::String(String::new()))
            }
        })),
    );

    // Array.prototype.slice(start, end)
    JSObject::set_property(
        &proto,
        "slice",
        JSValue::Function(JSFunction::new_native("slice", |this, args| {
            if let JSValue::Array(arr) = this {
                let borrowed = arr.borrow();
                let len = borrowed.elements.len() as isize;

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

                let mut sliced = Vec::new();
                if norm_start < norm_end {
                    for elem in &borrowed.elements[norm_start..norm_end] {
                        sliced.push(elem.clone());
                    }
                }

                let proto = borrowed.map.borrow().prototype.clone();
                Ok(JSValue::Array(JSArray::new_array_with_proto(sliced, proto)))
            } else {
                Ok(JSValue::Array(JSArray::new_array(Vec::new())))
            }
        })),
    );

    // Array.prototype.reverse()
    JSObject::set_property(
        &proto,
        "reverse",
        JSValue::Function(JSFunction::new_native("reverse", |this, _args| {
            if let JSValue::Array(arr) = this {
                arr.borrow_mut().elements.reverse();
                Ok(this.clone())
            } else {
                Ok(this.clone())
            }
        })),
    );

    // Array.prototype.indexOf(searchElement, fromIndex)
    JSObject::set_property(
        &proto,
        "indexOf",
        JSValue::Function(JSFunction::new_native("indexOf", |this, args| {
            if let JSValue::Array(arr) = this {
                let search = args.get(0).cloned().unwrap_or(JSValue::Undefined);
                let from_idx = args.get(1).map(|v| {
                    let n = v.to_number();
                    if n < 0.0 { 0 } else { n as usize }
                }).unwrap_or(0);

                let borrowed = arr.borrow();
                for (i, elem) in borrowed.elements.iter().enumerate().skip(from_idx) {
                    if elem.strict_equal(&search) {
                        return Ok(JSValue::Smi(i as i32));
                    }
                }
                Ok(JSValue::Smi(-1))
            } else {
                Ok(JSValue::Smi(-1))
            }
        })),
    );

    // Array.prototype.includes(searchElement, fromIndex)
    JSObject::set_property(
        &proto,
        "includes",
        JSValue::Function(JSFunction::new_native("includes", |this, args| {
            if let JSValue::Array(arr) = this {
                let search = args.get(0).cloned().unwrap_or(JSValue::Undefined);
                let borrowed = arr.borrow();
                let len = borrowed.elements.len() as isize;

                let from_arg = args.get(1).map(|v| v.to_number() as isize).unwrap_or(0);
                let from_idx = if from_arg < 0 {
                    (len + from_arg).max(0) as usize
                } else {
                    from_arg as usize
                };

                for elem in borrowed.elements.iter().skip(from_idx) {
                    if same_value_zero(elem, &search) {
                        return Ok(JSValue::Boolean(true));
                    }
                }
                Ok(JSValue::Boolean(false))
            } else {
                Ok(JSValue::Boolean(false))
            }
        })),
    );

    // Array.prototype.map(callback, thisArg)
    JSObject::set_property(
        &proto,
        "map",
        JSValue::Function(JSFunction::new_native("map", |this, args| {
            if let JSValue::Array(arr) = this {
                let callback = match args.first() {
                    Some(JSValue::Function(cb)) => cb.clone(),
                    _ => return Err("TypeError: callback is not a function".to_string()),
                };
                let this_arg = args.get(1).cloned().unwrap_or(JSValue::Undefined);
                let elements = arr.borrow().elements.clone();
                let mut mapped = Vec::with_capacity(elements.len());

                for (i, elem) in elements.into_iter().enumerate() {
                    let call_args = [elem, JSValue::Smi(i as i32), this.clone()];
                    let res = callback.call(&this_arg, &call_args)?;
                    mapped.push(res);
                }

                let proto = arr.borrow().map.borrow().prototype.clone();
                Ok(JSValue::Array(JSArray::new_array_with_proto(mapped, proto)))
            } else {
                Ok(JSValue::Array(JSArray::new_array(Vec::new())))
            }
        })),
    );

    // Array.prototype.filter(callback, thisArg)
    JSObject::set_property(
        &proto,
        "filter",
        JSValue::Function(JSFunction::new_native("filter", |this, args| {
            if let JSValue::Array(arr) = this {
                let callback = match args.first() {
                    Some(JSValue::Function(cb)) => cb.clone(),
                    _ => return Err("TypeError: callback is not a function".to_string()),
                };
                let this_arg = args.get(1).cloned().unwrap_or(JSValue::Undefined);
                let elements = arr.borrow().elements.clone();
                let mut filtered = Vec::new();

                for (i, elem) in elements.into_iter().enumerate() {
                    let call_args = [elem.clone(), JSValue::Smi(i as i32), this.clone()];
                    let res = callback.call(&this_arg, &call_args)?;
                    if res.to_boolean() {
                        filtered.push(elem);
                    }
                }

                let proto = arr.borrow().map.borrow().prototype.clone();
                Ok(JSValue::Array(JSArray::new_array_with_proto(filtered, proto)))
            } else {
                Ok(JSValue::Array(JSArray::new_array(Vec::new())))
            }
        })),
    );

    // Array.prototype.reduce(callback, initialValue)
    JSObject::set_property(
        &proto,
        "reduce",
        JSValue::Function(JSFunction::new_native("reduce", |this, args| {
            if let JSValue::Array(arr) = this {
                let callback = match args.first() {
                    Some(JSValue::Function(cb)) => cb.clone(),
                    _ => return Err("TypeError: callback is not a function".to_string()),
                };

                let elements = arr.borrow().elements.clone();
                if elements.is_empty() && args.len() < 2 {
                    return Err("TypeError: Reduce of empty array with no initial value".to_string());
                }

                let (mut accumulator, start_idx) = if args.len() >= 2 {
                    (args[1].clone(), 0)
                } else {
                    (elements[0].clone(), 1)
                };

                for (i, elem) in elements.into_iter().enumerate().skip(start_idx) {
                    let call_args = [accumulator, elem, JSValue::Smi(i as i32), this.clone()];
                    accumulator = callback.call(&JSValue::Undefined, &call_args)?;
                }

                Ok(accumulator)
            } else {
                Ok(JSValue::Undefined)
            }
        })),
    );

    // Array.prototype.reduceRight(callback, initialValue)
    JSObject::set_property(
        &proto,
        "reduceRight",
        JSValue::Function(JSFunction::new_native("reduceRight", |this, args| {
            if let JSValue::Array(arr) = this {
                let callback = match args.first() {
                    Some(JSValue::Function(cb)) => cb.clone(),
                    _ => return Err("TypeError: callback is not a function".to_string()),
                };

                let elements = arr.borrow().elements.clone();
                if elements.is_empty() && args.len() < 2 {
                    return Err("TypeError: Reduce of empty array with no initial value".to_string());
                }

                let len = elements.len();
                let (mut accumulator, count) = if args.len() >= 2 {
                    (args[1].clone(), len)
                } else {
                    (elements[len - 1].clone(), len - 1)
                };

                for i in (0..count).rev() {
                    let call_args = [accumulator, elements[i].clone(), JSValue::Smi(i as i32), this.clone()];
                    accumulator = callback.call(&JSValue::Undefined, &call_args)?;
                }

                Ok(accumulator)
            } else {
                Ok(JSValue::Undefined)
            }
        })),
    );

    // Array.prototype.forEach(callback, thisArg)
    JSObject::set_property(
        &proto,
        "forEach",
        JSValue::Function(JSFunction::new_native("forEach", |this, args| {
            if let JSValue::Array(arr) = this {
                let callback = match args.first() {
                    Some(JSValue::Function(cb)) => cb.clone(),
                    _ => return Err("TypeError: callback is not a function".to_string()),
                };
                let this_arg = args.get(1).cloned().unwrap_or(JSValue::Undefined);
                let elements = arr.borrow().elements.clone();

                for (i, elem) in elements.into_iter().enumerate() {
                    let call_args = [elem, JSValue::Smi(i as i32), this.clone()];
                    callback.call(&this_arg, &call_args)?;
                }
            }
            Ok(JSValue::Undefined)
        })),
    );

    // Array.prototype.find(callback, thisArg)
    JSObject::set_property(
        &proto,
        "find",
        JSValue::Function(JSFunction::new_native("find", |this, args| {
            if let JSValue::Array(arr) = this {
                let callback = match args.first() {
                    Some(JSValue::Function(cb)) => cb.clone(),
                    _ => return Err("TypeError: callback is not a function".to_string()),
                };
                let this_arg = args.get(1).cloned().unwrap_or(JSValue::Undefined);
                let elements = arr.borrow().elements.clone();

                for (i, elem) in elements.into_iter().enumerate() {
                    let call_args = [elem.clone(), JSValue::Smi(i as i32), this.clone()];
                    let res = callback.call(&this_arg, &call_args)?;
                    if res.to_boolean() {
                        return Ok(elem);
                    }
                }
            }
            Ok(JSValue::Undefined)
        })),
    );

    // Array.prototype.findIndex(callback, thisArg)
    JSObject::set_property(
        &proto,
        "findIndex",
        JSValue::Function(JSFunction::new_native("findIndex", |this, args| {
            if let JSValue::Array(arr) = this {
                let callback = match args.first() {
                    Some(JSValue::Function(cb)) => cb.clone(),
                    _ => return Err("TypeError: callback is not a function".to_string()),
                };
                let this_arg = args.get(1).cloned().unwrap_or(JSValue::Undefined);
                let elements = arr.borrow().elements.clone();

                for (i, elem) in elements.into_iter().enumerate() {
                    let call_args = [elem, JSValue::Smi(i as i32), this.clone()];
                    let res = callback.call(&this_arg, &call_args)?;
                    if res.to_boolean() {
                        return Ok(JSValue::Smi(i as i32));
                    }
                }
            }
            Ok(JSValue::Smi(-1))
        })),
    );

    // Array.prototype.concat(...items)
    JSObject::set_property(
        &proto,
        "concat",
        JSValue::Function(JSFunction::new_native("concat", |this, args| {
            let mut result = Vec::new();
            if let JSValue::Array(arr) = this {
                result.extend(arr.borrow().elements.clone());
            } else {
                result.push(this.clone());
            }

            for arg in args {
                if let JSValue::Array(arg_arr) = arg {
                    result.extend(arg_arr.borrow().elements.clone());
                } else {
                    result.push(arg.clone());
                }
            }

            let proto = if let JSValue::Array(arr) = this {
                arr.borrow().map.borrow().prototype.clone()
            } else {
                None
            };
            Ok(JSValue::Array(JSArray::new_array_with_proto(result, proto)))
        })),
    );

    // Array.prototype.some(callback, thisArg)
    JSObject::set_property(
        &proto,
        "some",
        JSValue::Function(JSFunction::new_native("some", |this, args| {
            if let JSValue::Array(arr) = this {
                let callback = match args.first() {
                    Some(JSValue::Function(cb)) => cb.clone(),
                    _ => return Err("TypeError: callback is not a function".to_string()),
                };
                let this_arg = args.get(1).cloned().unwrap_or(JSValue::Undefined);
                let elements = arr.borrow().elements.clone();

                for (i, elem) in elements.into_iter().enumerate() {
                    let call_args = [elem, JSValue::Smi(i as i32), this.clone()];
                    let res = callback.call(&this_arg, &call_args)?;
                    if res.to_boolean() {
                        return Ok(JSValue::Boolean(true));
                    }
                }
            }
            Ok(JSValue::Boolean(false))
        })),
    );

    // Array.prototype.every(callback, thisArg)
    JSObject::set_property(
        &proto,
        "every",
        JSValue::Function(JSFunction::new_native("every", |this, args| {
            if let JSValue::Array(arr) = this {
                let callback = match args.first() {
                    Some(JSValue::Function(cb)) => cb.clone(),
                    _ => return Err("TypeError: callback is not a function".to_string()),
                };
                let this_arg = args.get(1).cloned().unwrap_or(JSValue::Undefined);
                let elements = arr.borrow().elements.clone();

                for (i, elem) in elements.into_iter().enumerate() {
                    let call_args = [elem, JSValue::Smi(i as i32), this.clone()];
                    let res = callback.call(&this_arg, &call_args)?;
                    if !res.to_boolean() {
                        return Ok(JSValue::Boolean(false));
                    }
                }
            }
            Ok(JSValue::Boolean(true))
        })),
    );

    // Array.prototype.flat(depth)
    JSObject::set_property(
        &proto,
        "flat",
        JSValue::Function(JSFunction::new_native("flat", |this, args| {
            fn flatten(elements: &[JSValue], depth: usize, out: &mut Vec<JSValue>) {
                for item in elements {
                    if depth > 0 {
                        if let JSValue::Array(sub_arr) = item {
                            flatten(&sub_arr.borrow().elements, depth - 1, out);
                            continue;
                        }
                    }
                    out.push(item.clone());
                }
            }

            if let JSValue::Array(arr) = this {
                let depth = args.get(0).map(|v| {
                    let n = v.to_number();
                    if n < 0.0 { 0 } else { n as usize }
                }).unwrap_or(1);

                let borrowed = arr.borrow();
                let mut flattened = Vec::new();
                flatten(&borrowed.elements, depth, &mut flattened);

                let proto = borrowed.map.borrow().prototype.clone();
                Ok(JSValue::Array(JSArray::new_array_with_proto(flattened, proto)))
            } else {
                Ok(JSValue::Array(JSArray::new_array(Vec::new())))
            }
        })),
    );

    // Array.prototype.fill(value, start, end)
    JSObject::set_property(
        &proto,
        "fill",
        JSValue::Function(JSFunction::new_native("fill", |this, args| {
            if let JSValue::Array(arr) = this {
                let fill_val = args.get(0).cloned().unwrap_or(JSValue::Undefined);
                let mut borrowed = arr.borrow_mut();
                let len = borrowed.elements.len() as isize;

                let start_arg = args.get(1).map(|v| v.to_number() as isize).unwrap_or(0);
                let end_arg = args.get(2).map(|v| v.to_number() as isize).unwrap_or(len);

                let norm_start = if start_arg < 0 {
                    (len + start_arg).max(0) as usize
                } else {
                    start_arg.min(len) as usize
                };

                let norm_end = if end_arg < 0 {
                    (len + end_arg).max(0) as usize
                } else {
                    end_arg.min(len) as usize
                };

                for i in norm_start..norm_end {
                    borrowed.elements[i] = fill_val.clone();
                }
            }
            Ok(this.clone())
        })),
    );

    // Array.prototype.values() / [Symbol.iterator]()
    let values_fn = JSFunction::new_native("values", |this, _args| {
        if let JSValue::Array(ref arr) = this {
            Ok(crate::objects::generator::new_array_iterator(arr.clone()))
        } else {
            Err("TypeError: Array.prototype.values called on non-array".to_string())
        }
    });
    JSObject::set_property(&proto, "values", JSValue::Function(values_fn.clone()));
    JSObject::set_property(&proto, "Symbol(Symbol.iterator)", JSValue::Function(values_fn.clone()));
    JSObject::set_property(&proto, "[Symbol.iterator]", JSValue::Function(values_fn.clone()));
    JSObject::set_property(&proto, "iterator", JSValue::Function(values_fn));

    // Array.prototype.reverse()
    JSObject::set_property(
        &proto,
        "reverse",
        JSValue::Function(JSFunction::new_native("reverse", |this, _args| {
            if let JSValue::Array(arr) = this {
                arr.borrow_mut().elements.reverse();
            }
            Ok(this.clone())
        })),
    );

    // Array.prototype.toReversed() (ES2023)
    JSObject::set_property(
        &proto,
        "toReversed",
        JSValue::Function(JSFunction::new_native("toReversed", |this, _args| {
            if let JSValue::Array(arr) = this {
                let mut elems = arr.borrow().elements.clone();
                elems.reverse();
                let p = arr.borrow().map.borrow().prototype.clone();
                Ok(JSValue::Array(JSArray::new_array_with_proto(elems, p)))
            } else {
                Err("TypeError: Array.prototype.toReversed called on non-array".to_string())
            }
        })),
    );

    // Array.prototype.sort(compareFn)
    JSObject::set_property(
        &proto,
        "sort",
        JSValue::Function(JSFunction::new_native("sort", |this, args| {
            if let JSValue::Array(arr) = this {
                let compare_fn = match args.first() {
                    Some(JSValue::Function(f)) => Some(f),
                    _ => None,
                };
                let mut borrowed = arr.borrow_mut();
                sort_elements(&mut borrowed.elements, compare_fn)?;
            }
            Ok(this.clone())
        })),
    );

    // Array.prototype.toSorted(compareFn) (ES2023)
    JSObject::set_property(
        &proto,
        "toSorted",
        JSValue::Function(JSFunction::new_native("toSorted", |this, args| {
            if let JSValue::Array(arr) = this {
                let compare_fn = match args.first() {
                    Some(JSValue::Function(f)) => Some(f),
                    _ => None,
                };
                let mut elems = arr.borrow().elements.clone();
                sort_elements(&mut elems, compare_fn)?;
                let p = arr.borrow().map.borrow().prototype.clone();
                Ok(JSValue::Array(JSArray::new_array_with_proto(elems, p)))
            } else {
                Err("TypeError: Array.prototype.toSorted called on non-array".to_string())
            }
        })),
    );

    // Array.prototype.toSpliced(start, deleteCount, ...items) (ES2023)
    JSObject::set_property(
        &proto,
        "toSpliced",
        JSValue::Function(JSFunction::new_native("toSpliced", |this, args| {
            if let JSValue::Array(arr) = this {
                let elems = arr.borrow().elements.clone();
                let len = elems.len() as isize;
                let start_arg = args.get(0).map(|v| v.to_number() as isize).unwrap_or(0);
                let actual_start = if start_arg < 0 {
                    (len + start_arg).max(0) as usize
                } else {
                    start_arg.min(len) as usize
                };

                let skip_count = if args.len() > 1 {
                    let sc = args[1].to_number() as isize;
                    sc.max(0).min(len - actual_start as isize) as usize
                } else {
                    (len as usize).saturating_sub(actual_start)
                };

                let mut new_elems = Vec::new();
                new_elems.extend_from_slice(&elems[..actual_start]);
                for item in args.iter().skip(2) {
                    new_elems.push(item.clone());
                }
                if actual_start + skip_count < elems.len() {
                    new_elems.extend_from_slice(&elems[actual_start + skip_count..]);
                }

                let p = arr.borrow().map.borrow().prototype.clone();
                Ok(JSValue::Array(JSArray::new_array_with_proto(new_elems, p)))
            } else {
                Err("TypeError: Array.prototype.toSpliced called on non-array".to_string())
            }
        })),
    );

    // Array.prototype.with(index, value) (ES2023)
    JSObject::set_property(
        &proto,
        "with",
        JSValue::Function(JSFunction::new_native("with", |this, args| {
            if let JSValue::Array(arr) = this {
                let mut elems = arr.borrow().elements.clone();
                let len = elems.len() as isize;
                let index_arg = args.get(0).map(|v| v.to_number() as isize).unwrap_or(0);
                let actual_idx = if index_arg < 0 {
                    len + index_arg
                } else {
                    index_arg
                };

                if actual_idx < 0 || actual_idx >= len {
                    return Err("RangeError: Invalid index".to_string());
                }

                let val = args.get(1).cloned().unwrap_or(JSValue::Undefined);
                elems[actual_idx as usize] = val;
                let p = arr.borrow().map.borrow().prototype.clone();
                Ok(JSValue::Array(JSArray::new_array_with_proto(elems, p)))
            } else {
                Err("TypeError: Array.prototype.with called on non-array".to_string())
            }
        })),
    );

    // Array.prototype.at(index) (ES2022)
    JSObject::set_property(
        &proto,
        "at",
        JSValue::Function(JSFunction::new_native("at", |this, args| {
            if let JSValue::Array(arr) = this {
                let borrowed = arr.borrow();
                let len = borrowed.elements.len() as isize;
                let idx_arg = args.first().map(|v| v.to_number() as isize).unwrap_or(0);
                let actual_idx = if idx_arg < 0 { len + idx_arg } else { idx_arg };
                if actual_idx >= 0 && actual_idx < len {
                    Ok(borrowed.elements[actual_idx as usize].clone())
                } else {
                    Ok(JSValue::Undefined)
                }
            } else {
                Ok(JSValue::Undefined)
            }
        })),
    );

    // Array.prototype.flatMap(callback, thisArg?) (ES2019)
    JSObject::set_property(
        &proto,
        "flatMap",
        JSValue::Function(JSFunction::new_native("flatMap", |this, args| {
            if let JSValue::Array(arr) = this {
                let callback = match args.first() {
                    Some(JSValue::Function(cb)) => cb.clone(),
                    _ => return Err("TypeError: Array.prototype.flatMap callback must be a function".to_string()),
                };
                let this_arg = args.get(1).unwrap_or(&JSValue::Undefined);
                let elements = arr.borrow().elements.clone();
                let mut result_elems = Vec::new();

                for (i, elem) in elements.into_iter().enumerate() {
                    let mapped = callback.call(this_arg, &[elem, JSValue::Smi(i as i32), this.clone()])?;
                    match mapped {
                        JSValue::Array(inner_arr) => {
                            result_elems.extend(inner_arr.borrow().elements.clone());
                        }
                        other => {
                            result_elems.push(other);
                        }
                    }
                }

                let p = arr.borrow().map.borrow().prototype.clone();
                Ok(JSValue::Array(JSArray::new_array_with_proto(result_elems, p)))
            } else {
                Err("TypeError: Array.prototype.flatMap called on non-array".to_string())
            }
        })),
    );

    // Array.prototype.findLast(callback, thisArg?) (ES2023)
    JSObject::set_property(
        &proto,
        "findLast",
        JSValue::Function(JSFunction::new_native("findLast", |this, args| {
            if let JSValue::Array(arr) = this {
                let callback = match args.first() {
                    Some(JSValue::Function(cb)) => cb.clone(),
                    _ => return Err("TypeError: Array.prototype.findLast callback must be a function".to_string()),
                };
                let this_arg = args.get(1).unwrap_or(&JSValue::Undefined);
                let elements = arr.borrow().elements.clone();

                for i in (0..elements.len()).rev() {
                    let elem = &elements[i];
                    let matched = callback.call(this_arg, &[elem.clone(), JSValue::Smi(i as i32), this.clone()])?;
                    if matched.to_boolean() {
                        return Ok(elem.clone());
                    }
                }
                Ok(JSValue::Undefined)
            } else {
                Err("TypeError: Array.prototype.findLast called on non-array".to_string())
            }
        })),
    );

    // Array.prototype.findLastIndex(callback, thisArg?) (ES2023)
    JSObject::set_property(
        &proto,
        "findLastIndex",
        JSValue::Function(JSFunction::new_native("findLastIndex", |this, args| {
            if let JSValue::Array(arr) = this {
                let callback = match args.first() {
                    Some(JSValue::Function(cb)) => cb.clone(),
                    _ => return Err("TypeError: Array.prototype.findLastIndex callback must be a function".to_string()),
                };
                let this_arg = args.get(1).unwrap_or(&JSValue::Undefined);
                let elements = arr.borrow().elements.clone();

                for i in (0..elements.len()).rev() {
                    let elem = &elements[i];
                    let matched = callback.call(this_arg, &[elem.clone(), JSValue::Smi(i as i32), this.clone()])?;
                    if matched.to_boolean() {
                        return Ok(JSValue::Smi(i as i32));
                    }
                }
                Ok(JSValue::Smi(-1))
            } else {
                Err("TypeError: Array.prototype.findLastIndex called on non-array".to_string())
            }
        })),
    );

    // Array.prototype.copyWithin(target, start, end?)
    JSObject::set_property(
        &proto,
        "copyWithin",
        JSValue::Function(JSFunction::new_native("copyWithin", |this, args| {
            if let JSValue::Array(arr) = this {
                let mut borrowed = arr.borrow_mut();
                let len = borrowed.elements.len() as isize;
                let target_arg = args.get(0).map(|v| v.to_number() as isize).unwrap_or(0);
                let start_arg = args.get(1).map(|v| v.to_number() as isize).unwrap_or(0);
                let end_arg = args.get(2).map(|v| v.to_number() as isize).unwrap_or(len);

                let target = if target_arg < 0 { (len + target_arg).max(0) } else { target_arg.min(len) } as usize;
                let start = if start_arg < 0 { (len + start_arg).max(0) } else { start_arg.min(len) } as usize;
                let end = if end_arg < 0 { (len + end_arg).max(0) } else { end_arg.min(len) } as usize;

                let count = if end > start { (end - start).min(borrowed.elements.len().saturating_sub(target)) } else { 0 };

                if count > 0 && start < borrowed.elements.len() {
                    let slice: Vec<JSValue> = borrowed.elements[start..start + count].to_vec();
                    for (i, val) in slice.into_iter().enumerate() {
                        if target + i < borrowed.elements.len() {
                            borrowed.elements[target + i] = val;
                        }
                    }
                }

                drop(borrowed);
                Ok(this.clone())
            } else {
                Err("TypeError: Array.prototype.copyWithin called on non-array".to_string())
            }
        })),
    );

    proto
}

/// Creates the `Array` constructor object.
pub fn create_array_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let arr_ctor = JSObject::new_empty(None);

    JSObject::set_property(&arr_ctor, "prototype", JSValue::Object(prototype.clone()));

    // Array.isArray(arg)
    JSObject::set_property(
        &arr_ctor,
        "isArray",
        JSValue::Function(JSFunction::new_native("isArray", |_this, args| {
            let is_arr = matches!(args.first(), Some(JSValue::Array(_)));
            Ok(JSValue::Boolean(is_arr))
        })),
    );

    // Array.of(...items)
    JSObject::set_property(
        &arr_ctor,
        "of",
        JSValue::Function(JSFunction::new_native("of", |_this, args| {
            Ok(JSValue::Array(JSArray::new_array(args.to_vec())))
        })),
    );

    // Array.from(arrayLike)
    JSObject::set_property(
        &arr_ctor,
        "from",
        JSValue::Function(JSFunction::new_native("from", |_this, args| {
            let elements = match args.first() {
                Some(JSValue::Array(arr)) => arr.borrow().elements.clone(),
                Some(JSValue::String(s)) => s.chars().map(|c| JSValue::String(c.to_string())).collect(),
                _ => Vec::new(),
            };
            Ok(JSValue::Array(JSArray::new_array(elements)))
        })),
    );

    // Array.fromAsync(asyncItems, mapFn?) (ES2024)
    JSObject::set_property(
        &arr_ctor,
        "fromAsync",
        JSValue::Function(JSFunction::new_native("fromAsync", |_this, args| {
            let (promise, resolve, _reject) = crate::builtins::promise::new_promise_capability(None);
            let items = match args.first() {
                Some(JSValue::Array(arr)) => arr.borrow().elements.clone(),
                Some(JSValue::String(s)) => s.chars().map(|c| JSValue::String(c.to_string())).collect(),
                Some(other) => vec![other.clone()],
                None => Vec::new(),
            };
            let map_fn = match args.get(1) {
                Some(JSValue::Function(f)) => Some(f.clone()),
                _ => None,
            };

            let mut resolved_items = Vec::with_capacity(items.len());
            for (idx, item) in items.into_iter().enumerate() {
                let mapped = if let Some(ref f) = map_fn {
                    f.call(&JSValue::Undefined, &[item, JSValue::Smi(idx as i32)])?
                } else {
                    item
                };
                resolved_items.push(mapped);
            }
            let res_arr = JSArray::new_array(resolved_items);
            let _ = resolve.call(&JSValue::Undefined, &[JSValue::Array(res_arr)]);
            Ok(JSValue::Object(promise))
        })),
    );

    // Direct invocation `Array(...items)`
    JSObject::set_property(
        &arr_ctor,
        "__call__",
        JSValue::Function(JSFunction::new_native("Array", |_this, args| {
            let elements = if args.len() == 1 {
                if let Some(JSValue::Smi(n)) = args.first() {
                    if *n >= 0 {
                        vec![JSValue::Undefined; *n as usize]
                    } else {
                        return Err("RangeError: Invalid array length".to_string());
                    }
                } else {
                    args.to_vec()
                }
            } else {
                args.to_vec()
            };
            Ok(JSValue::Array(JSArray::new_array(elements)))
        })),
    );

    arr_ctor
}
