//! Safe Rust reimplementation of Google V8's Generator & Iterator Objects (`src/objects/js-generator.h`).
//!
//! Provides the generator execution state machine (SuspendedStart, Executing,
//! SuspendedYield, Completed) and standard ECMAScript iterator data structures.

use crate::interpreter::bytecode_array::BytecodeArray;
use crate::interpreter::interpreter::InterpreterFrame;
use crate::objects::js_object::JSObject;
use crate::objects::value::JSValue;
use std::cell::RefCell;
use std::rc::Rc;

/// ECMAScript generator execution states.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GeneratorState {
    SuspendedStart,
    Executing,
    SuspendedYield,
    Completed,
}

/// Runtime execution payload stored inside a suspended or running generator object.
#[derive(Clone, Debug)]
pub struct GeneratorData {
    pub state: GeneratorState,
    pub bytecode_array: Rc<BytecodeArray>,
    pub pc: usize,
    pub frame: Option<InterpreterFrame>,
    pub receiver: JSValue,
    pub arguments: Vec<JSValue>,
    pub is_async: bool,
}

impl GeneratorData {
    pub fn new(
        bytecode_array: Rc<BytecodeArray>,
        receiver: JSValue,
        arguments: Vec<JSValue>,
        is_async: bool,
    ) -> Self {
        Self {
            state: GeneratorState::SuspendedStart,
            bytecode_array,
            pc: 0,
            frame: None,
            receiver,
            arguments,
            is_async,
        }
    }
}

/// Internal state for an Array Iterator object (`arr.values()`, `arr[Symbol.iterator]()`).
#[derive(Clone, Debug)]
pub struct ArrayIteratorData {
    pub array: Rc<RefCell<JSObject>>,
    pub index: usize,
}

/// Internal state for a String Iterator object (`str[Symbol.iterator]()`).
#[derive(Clone, Debug)]
pub struct StringIteratorData {
    pub chars: Vec<char>,
    pub index: usize,
}

/// Creates a standard ECMAScript IteratorResult object `{ value: JSValue, done: bool }`.
pub fn create_iter_result(value: JSValue, done: bool) -> JSValue {
    let obj = JSObject::new_empty(None);
    JSObject::set_property(&obj, "value", value);
    JSObject::set_property(&obj, "done", JSValue::Boolean(done));
    JSValue::Object(obj)
}

fn array_iterator_next(this: &JSValue, _args: &[JSValue]) -> Result<JSValue, String> {
    if let JSValue::Object(ref obj) = this {
        let has_arr = obj.borrow().ext_ref().and_then(|e| e.array_iterator_data.clone());
        if let Some(arr_data) = has_arr {
            let mut it = arr_data.borrow_mut();
            let (has_more, val) = {
                let arr_borrow = it.array.borrow();
                if it.index < arr_borrow.elements.len() {
                    (true, Some(arr_borrow.elements[it.index].clone()))
                } else {
                    (false, None)
                }
            };
            if has_more {
                it.index += 1;
                return Ok(create_iter_result(val.unwrap(), false));
            } else {
                return Ok(create_iter_result(JSValue::Undefined, true));
            }
        }
    }
    Ok(create_iter_result(JSValue::Undefined, true))
}

fn iterator_return_self(this: &JSValue, _args: &[JSValue]) -> Result<JSValue, String> {
    Ok(this.clone())
}

/// Creates a new Array Iterator object over the specified array.
pub fn new_array_iterator(arr: Rc<RefCell<JSObject>>) -> JSValue {
    let iter_obj = JSObject::new_empty(None);
    iter_obj.borrow_mut().ext_mut().array_iterator_data = Some(Rc::new(RefCell::new(ArrayIteratorData {
        array: arr,
        index: 0,
    })));

    let next_fn = crate::objects::function::JSFunction::new_native("next", array_iterator_next);
    JSObject::set_property(&iter_obj, "next", JSValue::Function(next_fn));

    let sym_iter_fn = crate::objects::function::JSFunction::new_native("[Symbol.iterator]", iterator_return_self);
    JSObject::set_property(&iter_obj, "Symbol(Symbol.iterator)", JSValue::Function(sym_iter_fn.clone()));
    JSObject::set_property(&iter_obj, "[Symbol.iterator]", JSValue::Function(sym_iter_fn.clone()));
    JSObject::set_property(&iter_obj, "iterator", JSValue::Function(sym_iter_fn));

    JSValue::Object(iter_obj)
}

fn string_iterator_next(this: &JSValue, _args: &[JSValue]) -> Result<JSValue, String> {
    if let JSValue::Object(ref obj) = this {
        let has_str = obj.borrow().ext_ref().and_then(|e| e.string_iterator_data.clone());
        if let Some(str_data) = has_str {
            let mut it = str_data.borrow_mut();
            if it.index < it.chars.len() {
                let val = JSValue::String(it.chars[it.index].to_string());
                it.index += 1;
                return Ok(create_iter_result(val, false));
            } else {
                return Ok(create_iter_result(JSValue::Undefined, true));
            }
        }
    }
    Ok(create_iter_result(JSValue::Undefined, true))
}

/// Creates a new String Iterator object over the specified string.
pub fn new_string_iterator(s: String) -> JSValue {
    let chars: Vec<char> = s.chars().collect();
    let iter_obj = JSObject::new_empty(None);
    iter_obj.borrow_mut().ext_mut().string_iterator_data = Some(Rc::new(RefCell::new(StringIteratorData {
        chars,
        index: 0,
    })));

    let next_fn = crate::objects::function::JSFunction::new_native("next", string_iterator_next);
    JSObject::set_property(&iter_obj, "next", JSValue::Function(next_fn));

    let sym_iter_fn = crate::objects::function::JSFunction::new_native("[Symbol.iterator]", iterator_return_self);
    JSObject::set_property(&iter_obj, "Symbol(Symbol.iterator)", JSValue::Function(sym_iter_fn.clone()));
    JSObject::set_property(&iter_obj, "[Symbol.iterator]", JSValue::Function(sym_iter_fn.clone()));
    JSObject::set_property(&iter_obj, "iterator", JSValue::Function(sym_iter_fn));

    JSValue::Object(iter_obj)
}
