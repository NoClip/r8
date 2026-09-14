//! Safe Rust reimplementation of Google V8's function objects.
//!
//! Provides `JSFunction`, supporting both native runtime callbacks and
//! interpreted bytecode functions, with automated Turbofan JIT tiering.

use super::value::JSValue;
use crate::compiler::backend::code_allocator::NativeExecutable;
use crate::compiler::backend::registers::CallingConvention;
use crate::compiler::CompilerPipeline;
use crate::interpreter::bytecode_array::BytecodeArray;
use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

/// Native callback signature: receives `this` and arguments, returns result or error message.
pub type NativeCallback = fn(this: &JSValue, args: &[JSValue]) -> Result<JSValue, String>;

/// Dynamic closure callback signature capable of capturing environment variables.
pub type ClosureCallback = Rc<dyn Fn(&JSValue, &[JSValue]) -> Result<JSValue, String>>;

#[derive(Clone)]
pub enum FunctionKind {
    Native(NativeCallback),
    Closure(ClosureCallback),
    Bytecode(Rc<BytecodeArray>),
    BaselineJit {
        bytecode: Rc<BytecodeArray>,
        executable: Rc<NativeExecutable>,
    },
    JitCompiled {
        bytecode: Rc<BytecodeArray>,
        executable: Rc<NativeExecutable>,
    },
}

/// A callable JavaScript function instance with automatic JIT tiering.
#[derive(Clone)]
pub struct JSFunction {
    pub name: String,
    pub bytecode: Option<Rc<BytecodeArray>>,
    pub is_jit: Cell<bool>,
    pub kind: RefCell<FunctionKind>,
    pub invocation_count: Cell<usize>,
}

impl JSFunction {
    /// Baseline JIT tiering threshold (Sparkplug).
    pub const SPARKPLUG_HOT_THRESHOLD: usize = 2;
    /// Optimizing JIT tiering threshold (Turbofan).
    pub const JIT_HOT_THRESHOLD: usize = 5;

    /// Creates a new native function wrapper.
    pub fn new_native(name: &str, callback: NativeCallback) -> Rc<Self> {
        Rc::new(Self {
            name: name.to_string(),
            bytecode: None,
            is_jit: Cell::new(false),
            kind: RefCell::new(FunctionKind::Native(callback)),
            invocation_count: Cell::new(0),
        })
    }

    /// Creates a new native closure wrapper capturing arbitrary environment state.
    pub fn new_closure<F>(name: &str, callback: F) -> Rc<Self>
    where
        F: Fn(&JSValue, &[JSValue]) -> Result<JSValue, String> + 'static,
    {
        Rc::new(Self {
            name: name.to_string(),
            bytecode: None,
            is_jit: Cell::new(false),
            kind: RefCell::new(FunctionKind::Closure(Rc::new(callback))),
            invocation_count: Cell::new(0),
        })
    }

    /// Creates a new interpreted bytecode function wrapper.
    pub fn new_bytecode(name: &str, bytecode: Rc<BytecodeArray>) -> Rc<Self> {
        Rc::new(Self {
            name: name.to_string(),
            bytecode: Some(bytecode.clone()),
            is_jit: Cell::new(false),
            kind: RefCell::new(FunctionKind::Bytecode(bytecode)),
            invocation_count: Cell::new(0),
        })
    }

    /// Creates a new JIT-compiled function wrapper.
    pub fn new_jit_compiled(
        name: &str,
        bytecode: Rc<BytecodeArray>,
        executable: Rc<NativeExecutable>,
    ) -> Rc<Self> {
        Rc::new(Self {
            name: name.to_string(),
            bytecode: Some(bytecode.clone()),
            is_jit: Cell::new(true),
            kind: RefCell::new(FunctionKind::JitCompiled {
                bytecode,
                executable,
            }),
            invocation_count: Cell::new(0),
        })
    }

    /// Tells whether this function has been compiled to native code.
    pub fn is_jit_compiled(&self) -> bool {
        matches!(*self.kind.borrow(), FunctionKind::JitCompiled { .. } | FunctionKind::BaselineJit { .. })
    }

    /// Attempts to compile a bytecode function into Sparkplug baseline machine code.
    pub fn compile_to_sparkplug(&self) -> bool {
        let bytecode_opt = match *self.kind.borrow() {
            FunctionKind::Bytecode(ref bc) => Some(bc.clone()),
            _ => None,
        };

        if let Some(bytecode) = bytecode_opt {
            if !crate::compiler::sparkplug::SparkplugCompiler::can_compile(&bytecode) {
                return false;
            }
            let executable = crate::compiler::sparkplug::SparkplugCompiler::compile(&bytecode);
            *self.kind.borrow_mut() = FunctionKind::BaselineJit {
                bytecode,
                executable: Rc::new(executable),
            };
            self.is_jit.set(true);
            true
        } else {
            false
        }
    }

    /// Attempts to compile a bytecode function into Turbofan native machine code.
    pub fn compile_to_jit(&self) -> bool {
        let bytecode_opt = match *self.kind.borrow() {
            FunctionKind::Bytecode(ref bc) => Some(bc.clone()),
            FunctionKind::BaselineJit { ref bytecode, .. } => Some(bytecode.clone()),
            _ => None,
        };

        if let Some(bytecode) = bytecode_opt {
            if !crate::compiler::graph_builder::BytecodeGraphBuilder::can_compile(&bytecode) {
                return false;
            }
            let (executable, _) = CompilerPipeline::compile_to_native(
                &bytecode,
                CallingConvention::host_default(),
            );
            *self.kind.borrow_mut() = FunctionKind::JitCompiled {
                bytecode,
                executable: Rc::new(executable),
            };
            self.is_jit.set(true);
            true
        } else {
            false
        }
    }

    /// Invokes the function with the given `this` receiver and arguments.
    #[inline(always)]
    pub fn call(&self, this: &JSValue, args: &[JSValue]) -> Result<JSValue, String> {
        self.call_with_global(this, args, None)
    }

    /// Invokes the function with the given `this` receiver, arguments, and optional cached global context.
    #[inline(always)]
    pub fn call_with_global(
        &self,
        this: &JSValue,
        args: &[JSValue],
        global: Option<&Rc<RefCell<crate::objects::js_object::JSObject>>>,
    ) -> Result<JSValue, String> {
        let count = self.invocation_count.get();
        if count <= Self::JIT_HOT_THRESHOLD {
            self.invocation_count.set(count + 1);
            if count + 1 == Self::SPARKPLUG_HOT_THRESHOLD {
                if matches!(*self.kind.borrow(), FunctionKind::Bytecode(_)) {
                    if self.compile_to_sparkplug() {
                        self.is_jit.set(true);
                    }
                }
            } else if count + 1 == Self::JIT_HOT_THRESHOLD {
                if matches!(*self.kind.borrow(), FunctionKind::Bytecode(_) | FunctionKind::BaselineJit { .. }) {
                    if self.compile_to_jit() {
                        self.is_jit.set(true);
                    }
                }
            }
        }

        // Fast path: pure bytecode execution without RefCell borrow overhead
        if !self.is_jit.get() {
            if let Some(ref bytecode) = self.bytecode {
                if let Some(imm) = bytecode.quick_smi_base_case {
                    if args.len() == 1 && matches!(this, JSValue::Undefined) {
                        if let JSValue::Smi(n) = args[0] {
                            if n <= imm {
                                return Ok(JSValue::Smi(n));
                            }
                        }
                    }
                }
                return crate::interpreter::InterpreterVM::execute_with_receiver(bytecode, this, args, global)
                    .map_err(|e| e.message);
            }
        }

        let kind_borrow = self.kind.borrow();
        match &*kind_borrow {
            FunctionKind::Native(cb) => {
                let cb = *cb;
                drop(kind_borrow);
                cb(this, args)
            }
            FunctionKind::Closure(cb) => {
                let cb = cb.clone();
                drop(kind_borrow);
                cb(this, args)
            }
            FunctionKind::Bytecode(ref bytecode) => {
                crate::interpreter::InterpreterVM::execute_with_receiver(bytecode, this, args, global)
                    .map_err(|e| e.message)
            }
            FunctionKind::JitCompiled { bytecode, executable }
            | FunctionKind::BaselineJit { bytecode, executable } => {
                let executable = executable.clone();
                let bytecode = bytecode.clone();
                drop(kind_borrow);

                // Try native fast path if arguments are numeric/integers
                if args.len() < 8 {
                    let mut stack_args = [0i64; 9];
                    stack_args[0] = 0; // Parameter 0 is `this` (receiver)
                    let mut can_use_native = true;
                    for (i, arg) in args.iter().enumerate() {
                        match arg {
                            JSValue::Smi(n) => stack_args[1 + i] = *n as i64,
                            JSValue::Number(f) => stack_args[1 + i] = *f as i64,
                            JSValue::Boolean(b) => stack_args[1 + i] = if *b { 1 } else { 0 },
                            _ => {
                                can_use_native = false;
                                break;
                            }
                        }
                    }

                    if can_use_native {
                        if let Ok(res) = executable.execute(&stack_args[..1 + args.len()]) {
                            return Ok(JSValue::Smi(res.return_value as i32));
                        }
                    }
                } else {
                    let mut int_args = Vec::with_capacity(1 + args.len());
                    int_args.push(0i64);
                    let mut can_use_native = true;
                    for arg in args {
                        match arg {
                            JSValue::Smi(n) => int_args.push(*n as i64),
                            JSValue::Number(f) => int_args.push(*f as i64),
                            JSValue::Boolean(b) => int_args.push(if *b { 1 } else { 0 }),
                            _ => {
                                can_use_native = false;
                                break;
                            }
                        }
                    }

                    if can_use_native {
                        if let Ok(res) = executable.execute(&int_args) {
                            return Ok(JSValue::Smi(res.return_value as i32));
                        }
                    }
                }

                // Deoptimization / Interpreter fallback
                crate::interpreter::InterpreterVM::execute_with_receiver(&bytecode, this, args, global)
                    .map_err(|e| e.message)
            }
        }
    }
}

impl fmt::Debug for JSFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind_str = match *self.kind.borrow() {
            FunctionKind::Native(_) => "native",
            FunctionKind::Closure(_) => "closure",
            FunctionKind::Bytecode(_) => "interpreted",
            FunctionKind::BaselineJit { .. } => "baseline-jit",
            FunctionKind::JitCompiled { .. } => "jit-compiled",
        };
        write!(f, "[Function: {} ({})]", self.name, kind_str)
    }
}

impl fmt::Display for JSFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self.kind.borrow() {
            FunctionKind::JitCompiled { .. } | FunctionKind::BaselineJit { .. } => {
                write!(f, "function {}() {{ [native machine code] }}", self.name)
            }
            _ => write!(f, "function {}() {{ [native code] }}", self.name),
        }
    }
}
