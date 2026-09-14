//! Safe Rust reimplementation of Google V8's ECMAScript Modules (ESM) subsystem (`src/objects/source-text-module.h`).
//!
//! Provides the module state machine (Unlinked, Linking, Linked, Evaluating, Evaluated, Errored),
//! static import/export resolution, dynamic `import()` promises, module namespace objects,
//! and top-level await integration.

use crate::ast::ast::{Program, Statement};
use crate::builtins::promise::new_promise_capability;
use crate::interpreter::bytecode_generator::BytecodeGenerator;
use crate::interpreter::interpreter::InterpreterVM;
use crate::objects::js_object::JSObject;
use crate::objects::value::JSValue;
use crate::parsing::parser::Parser;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// ECMAScript Module lifecycle status.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ModuleStatus {
    Unlinked,
    Linking,
    Linked,
    Evaluating,
    Evaluated,
    Errored,
}

/// A parsed and resolved ECMAScript Module Record.
#[derive(Clone, Debug)]
pub struct ModuleRecord {
    pub specifier: String,
    pub source: String,
    pub status: ModuleStatus,
    pub program: Program,
    pub exports: HashMap<String, JSValue>,
    pub star_exports: Vec<String>,
    pub namespace: Option<Rc<RefCell<JSObject>>>,
    pub error: Option<String>,
}

impl ModuleRecord {
    /// Parses an ECMAScript module source string into a ModuleRecord.
    pub fn parse(specifier: &str, source: &str) -> Result<Self, String> {
        let mut parser = Parser::new(source);
        let program = parser.parse_program().map_err(|e| e.to_string())?;

        let mut star_exports = Vec::new();
        for stmt in &program.statements {
            if let Statement::ExportDeclaration { specifier: Some(ref s), specifiers, .. } = stmt {
                if specifiers.iter().any(|sp| sp.exported == "*") {
                    star_exports.push(s.clone());
                }
            }
        }

        Ok(Self {
            specifier: specifier.to_string(),
            source: source.to_string(),
            status: ModuleStatus::Unlinked,
            program,
            exports: HashMap::new(),
            star_exports,
            namespace: None,
            error: None,
        })
    }

    /// Creates or returns the cached Module Namespace Exotic Object for this module.
    pub fn get_namespace(&mut self) -> Rc<RefCell<JSObject>> {
        if let Some(ref ns) = self.namespace {
            return ns.clone();
        }

        let ns = JSObject::new_empty(None);
        let mut names: Vec<_> = self.exports.keys().cloned().collect();
        names.sort();
        for name in names {
            if let Some(val) = self.exports.get(&name) {
                JSObject::set_property(&ns, &name, val.clone());
            }
        }

        JSObject::set_property(&ns, "Symbol(Symbol.toStringTag)", JSValue::String("Module".to_string()));

        self.namespace = Some(ns.clone());
        ns
    }
}

thread_local! {
    static MODULE_REGISTRY: RefCell<HashMap<String, Rc<RefCell<ModuleRecord>>>> = RefCell::new(HashMap::new());
}

/// Registers a module source code under the given specifier.
pub fn register_module(specifier: &str, source: &str) -> Result<Rc<RefCell<ModuleRecord>>, String> {
    let record = ModuleRecord::parse(specifier, source)?;
    let rc = Rc::new(RefCell::new(record));
    MODULE_REGISTRY.with(|reg| {
        reg.borrow_mut().insert(specifier.to_string(), rc.clone());
    });
    Ok(rc)
}

/// Retrieves a registered module record if present.
pub fn get_module(specifier: &str) -> Option<Rc<RefCell<ModuleRecord>>> {
    MODULE_REGISTRY.with(|reg| reg.borrow().get(specifier).cloned())
}

/// Clears all loaded modules from the registry.
pub fn clear_module_registry() {
    MODULE_REGISTRY.with(|reg| reg.borrow_mut().clear());
}

/// Links and evaluates a module by specifier.
pub fn link_and_evaluate_module(
    specifier: &str,
    global: Option<&Rc<RefCell<JSObject>>>,
) -> Result<JSValue, String> {
    let module_rc = match get_module(specifier) {
        Some(m) => m,
        None => return Err(format!("Module not found: {}", specifier)),
    };

    let status = module_rc.borrow().status;
    if status == ModuleStatus::Evaluated {
        let ns = module_rc.borrow_mut().get_namespace();
        return Ok(JSValue::Object(ns));
    }
    if status == ModuleStatus::Evaluating {
        return Err(format!("Circular dependency detected evaluating module: {}", specifier));
    }
    if status == ModuleStatus::Errored {
        return Err(module_rc.borrow().error.clone().unwrap_or_else(|| "Module evaluation failed".to_string()));
    }

    module_rc.borrow_mut().status = ModuleStatus::Linking;

    let import_specifiers: Vec<String> = {
        let borrowed = module_rc.borrow();
        let mut list = Vec::new();
        for stmt in &borrowed.program.statements {
            match stmt {
                Statement::ImportDeclaration { specifier, .. } => {
                    list.push(specifier.clone());
                }
                Statement::ExportDeclaration { specifier: Some(ref s), .. } => {
                    list.push(s.clone());
                }
                _ => {}
            }
        }
        list
    };

    for dep_spec in &import_specifiers {
        link_and_evaluate_module(dep_spec, global)?;
    }

    module_rc.borrow_mut().status = ModuleStatus::Evaluating;

    let global_obj = global.cloned().or_else(crate::runtime::current_global).unwrap_or_else(|| {
        JSObject::new_empty(None)
    });

    let stmts = module_rc.borrow().program.statements.clone();

    // Hoist top-level functions and exported functions
    for stmt in &stmts {
        match stmt {
            Statement::FunctionDeclaration { .. } => {
                crate::runtime::Context::hoist_declarations(&global_obj, &global_obj, std::slice::from_ref(stmt));
            }
            Statement::ExportDeclaration { declaration: Some(ref decl), .. } => {
                if matches!(**decl, Statement::FunctionDeclaration { .. }) {
                    crate::runtime::Context::hoist_declarations(&global_obj, &global_obj, std::slice::from_ref(decl));
                }
            }
            _ => {}
        }
    }

    for stmt in &stmts {
        match stmt {
            Statement::ImportDeclaration { specifier: dep_spec, specifiers } => {
                if let Some(dep_rc) = get_module(dep_spec) {
                    let dep = dep_rc.borrow();
                    for spec in specifiers {
                        if spec.imported == "*" {
                            let mut dep_mut = dep_rc.borrow_mut();
                            let ns = dep_mut.get_namespace();
                            JSObject::set_property(&global_obj, &spec.local, JSValue::Object(ns));
                        } else if spec.imported == "default" {
                            let val = dep.exports.get("default").cloned().unwrap_or(JSValue::Undefined);
                            JSObject::set_property(&global_obj, &spec.local, val);
                        } else {
                            let val = dep.exports.get(&spec.imported).cloned().unwrap_or(JSValue::Undefined);
                            JSObject::set_property(&global_obj, &spec.local, val);
                        }
                    }
                }
            }
            Statement::ExportDeclaration { specifier, specifiers, declaration, is_default } => {
                if let Some(ref dep_spec) = specifier {
                    if let Some(dep_rc) = get_module(dep_spec) {
                        let dep = dep_rc.borrow();
                        for sp in specifiers {
                            if sp.exported == "*" {
                                for (k, v) in &dep.exports {
                                    module_rc.borrow_mut().exports.insert(k.clone(), v.clone());
                                }
                            } else {
                                let val = dep.exports.get(&sp.local).cloned().unwrap_or(JSValue::Undefined);
                                module_rc.borrow_mut().exports.insert(sp.exported.clone(), val);
                            }
                        }
                    }
                } else if let Some(decl) = declaration {
                    let val = match &**decl {
                        Statement::FunctionDeclaration { name, .. } => {
                            global_obj.borrow().get_property(name)
                        }
                        _ => {
                            let prog = Program::new(vec![(**decl).clone()]);
                            let bc = BytecodeGenerator::compile_program(&prog);
                            match InterpreterVM::execute_with_context(&bc, &[], Some(&global_obj)) {
                                Ok(v) => v,
                                Err(e) => {
                                    module_rc.borrow_mut().status = ModuleStatus::Errored;
                                    module_rc.borrow_mut().error = Some(e.message.clone());
                                    return Err(e.message);
                                }
                            }
                        }
                    };
                    if *is_default {
                        module_rc.borrow_mut().exports.insert("default".to_string(), val);
                    } else {
                        for sp in specifiers {
                            let v = global_obj.borrow().get_property(&sp.local);
                            let export_val = if v != JSValue::Undefined { v } else { val.clone() };
                            module_rc.borrow_mut().exports.insert(sp.exported.clone(), export_val);
                        }
                    }
                } else {
                    for sp in specifiers {
                        let v = global_obj.borrow().get_property(&sp.local);
                        module_rc.borrow_mut().exports.insert(sp.exported.clone(), v);
                    }
                }
            }
            other => {
                let prog = Program::new(vec![other.clone()]);
                let bc = BytecodeGenerator::compile_program(&prog);
                if let Err(e) = InterpreterVM::execute_with_context(&bc, &[], Some(&global_obj)) {
                    module_rc.borrow_mut().status = ModuleStatus::Errored;
                    module_rc.borrow_mut().error = Some(e.message.clone());
                    return Err(e.message);
                }
            }
        }
    }

    module_rc.borrow_mut().status = ModuleStatus::Evaluated;
    let ns = module_rc.borrow_mut().get_namespace();
    Ok(JSValue::Object(ns))
}

/// Implements dynamic `import(specifier)` returning a Promise resolved to the Module Namespace Object.
pub fn dynamic_import(specifier: &str) -> Result<JSValue, String> {
    let proto = crate::runtime::current_global().and_then(|g| {
        match g.borrow().get_property("Promise") {
            JSValue::Object(p) => match p.borrow().get_property("prototype") {
                JSValue::Object(proto) => Some(proto),
                _ => None,
            },
            _ => None,
        }
    });
    let (promise, resolve_fn, reject_fn) = new_promise_capability(proto);
    match link_and_evaluate_module(specifier, None) {
        Ok(ns_val) => {
            let _ = resolve_fn.call(&JSValue::Undefined, &[ns_val]);
        }
        Err(err_msg) => {
            let _ = reject_fn.call(&JSValue::Undefined, &[JSValue::String(err_msg)]);
        }
    }
    Ok(JSValue::Object(promise))
}
