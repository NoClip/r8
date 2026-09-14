//! Safe Rust reimplementation of Google V8's BytecodeGenerator.
//!
//! Compiles ECMAScript Abstract Syntax Trees (AST) into executable V8 Ignition BytecodeArrays.
//! Handles variable scoping, register allocation, temporary value storage, ShortStar optimizations,
//! and control flow jump patching.

use super::bytecode_array::{BytecodeArray, BytecodeArrayBuilder, ConstantValue};
use super::bytecode_register::{Register, RegisterList};
use super::bytecodes::Bytecode;
use crate::ast::*;
use std::collections::{HashMap, HashSet};

/// Tracks nested loops, switches, and labeled statements for break and continue jump patching.
struct LoopScope {
    label: Option<String>,
    is_loop: bool,
    break_jumps: Vec<usize>,
    continue_jumps: Vec<usize>,
}

/// Compiles an AST `Program` or `FunctionDeclaration` into an Ignition `BytecodeArray`.
pub struct BytecodeGenerator {
    builder: BytecodeArrayBuilder,
    locals: HashMap<String, Register>,
    parameters: HashMap<String, Register>,
    next_local_index: i32,
    max_local_count: i32,
    parameter_count: u32,
    loop_stack: Vec<LoopScope>,
    hoisted_constants: Vec<(LiteralValue, Register)>,
    is_top_level: bool,
    current_fn_name: Option<String>,
    fn_might_return_string: bool,
    numeric_locals: HashSet<String>,
    cached_call_callee: Option<(String, Register)>,
    rest_parameter_index: Option<u32>,
    with_stack: Vec<Register>,
    pending_label: Option<String>,
    finally_stack: Vec<Statement>,
}

impl BytecodeGenerator {
    /// Creates a new compiler for a routine with the given parameter names.
    pub fn new(parameter_names: &[String]) -> Self {
        let mut parameters = HashMap::new();
        // Parameter 0 is always the receiver (`this`)
        parameters.insert("this".to_string(), Register::receiver());

        let mut rest_param = None;
        for (i, name) in parameter_names.iter().enumerate() {
            // Arguments start at parameter index 1
            if name.starts_with("...") {
                rest_param = Some(i as u32);
                let actual_name = &name[3..];
                parameters.insert(actual_name.to_string(), Register::from_parameter_index((i + 1) as i32));
            } else {
                parameters.insert(name.clone(), Register::from_parameter_index((i + 1) as i32));
            }
        }

        let parameter_count = (parameter_names.len() + 1) as u32;

        Self {
            builder: BytecodeArrayBuilder::new(parameter_count, 0),
            locals: HashMap::new(),
            parameters,
            next_local_index: 0,
            max_local_count: 0,
            parameter_count,
            loop_stack: Vec::new(),
            hoisted_constants: Vec::new(),
            is_top_level: false,
            current_fn_name: None,
            fn_might_return_string: true,
            numeric_locals: HashSet::new(),
            cached_call_callee: None,
            rest_parameter_index: rest_param,
            with_stack: Vec::new(),
            pending_label: None,
            finally_stack: Vec::new(),
        }
    }

    #[inline(always)]
    fn clear_cached_call_callee(&mut self) {
        if let Some((_, reg)) = self.cached_call_callee.take() {
            self.free_temp(reg);
        }
    }

    /// Compiles a top-level `Program` AST into a BytecodeArray.
    pub fn compile_program(program: &Program) -> BytecodeArray {
        let mut generator = Self::new(&[]);
        generator.is_top_level = true;
        for stmt in &program.statements {
            generator.compile_statement(stmt);
        }
        generator.finish()
    }

    /// Compiles a function body AST into a BytecodeArray.
    pub fn compile_function(params: &[String], body: &[Statement]) -> BytecodeArray {
        Self::compile_named_function("", params, body)
    }

    /// Compiles a named function body AST into a BytecodeArray.
    pub fn compile_named_function(name: &str, params: &[String], body: &[Statement]) -> BytecodeArray {
        let mut generator = Self::new(params);
        if !name.is_empty() {
            generator.current_fn_name = Some(name.to_string());
            let mut might_return_string = false;
            for stmt in body {
                check_stmt_returns_string(stmt, &mut might_return_string);
            }
            generator.fn_might_return_string = might_return_string;
            for param in params {
                if !body_uses_param_as_string(body, param) {
                    generator.numeric_locals.insert(param.clone());
                }
            }
        }
        for stmt in body {
            generator.compile_statement(stmt);
        }
        generator.finish()
    }

    /// Declares or retrieves a local variable register slot.
    pub fn declare_variable(&mut self, name: &str) -> Register {
        if let Some(&reg) = self.locals.get(name) {
            return reg;
        }

        let reg = Register::new(self.next_local_index);
        self.next_local_index += 1;
        if self.next_local_index > self.max_local_count {
            self.max_local_count = self.next_local_index;
        }
        self.locals.insert(name.to_string(), reg);
        reg
    }

    /// Looks up a variable in local registers or parameter registers.
    pub fn lookup_variable(&self, name: &str) -> Option<Register> {
        if let Some(&reg) = self.locals.get(name) {
            return Some(reg);
        }
        if let Some(&reg) = self.parameters.get(name) {
            return Some(reg);
        }
        None
    }

    /// Determines conservatively if an expression might evaluate to a string.
    fn might_produce_string(&self, expr: &Expression) -> bool {
        match expr {
            Expression::Literal(LiteralValue::String(_)) => true,
            Expression::Binary { op, left, right } => {
                match op {
                    BinaryOperator::Sub
                    | BinaryOperator::Mul
                    | BinaryOperator::Div
                    | BinaryOperator::Mod
                    | BinaryOperator::BitwiseOr
                    | BinaryOperator::BitwiseXor
                    | BinaryOperator::BitwiseAnd
                    | BinaryOperator::ShiftLeft
                    | BinaryOperator::ShiftRight => false,
                    BinaryOperator::Add => self.might_produce_string(left) || self.might_produce_string(right),
                    _ => false,
                }
            }
            Expression::Unary { op, .. } => {
                match op {
                    UnaryOperator::TypeOf => true,
                    UnaryOperator::Not | UnaryOperator::Minus | UnaryOperator::Plus => false,
                    _ => true,
                }
            }
            Expression::Call { callee, .. } => {
                if let Expression::Variable(ref name) = callee.as_ref() {
                    if let Some(ref cur) = self.current_fn_name {
                        if cur == name {
                            return self.fn_might_return_string;
                        }
                    }
                }
                true
            }
            Expression::Literal(LiteralValue::Smi(_))
            | Expression::Literal(LiteralValue::Number(_))
            | Expression::Literal(LiteralValue::Boolean(_))
            | Expression::Literal(LiteralValue::Null)
            | Expression::Literal(LiteralValue::Undefined) => false,
            Expression::Variable(ref name) => !self.numeric_locals.contains(name),
            _ => true,
        }
    }

    /// Allocates an intermediate temporary register for expression evaluation.
    pub fn allocate_temp(&mut self) -> Register {
        let reg = Register::new(self.next_local_index);
        self.next_local_index += 1;
        if self.next_local_index > self.max_local_count {
            self.max_local_count = self.next_local_index;
        }
        reg
    }

    /// Releases an intermediate temporary register back to the pool.
    pub fn free_temp(&mut self, reg: Register) {
        if reg.index() + 1 == self.next_local_index {
            self.next_local_index -= 1;
        }
    }

    /// Compiles a statement.
    pub fn compile_statement(&mut self, stmt: &Statement) {
        self.clear_cached_call_callee();
        match stmt {
            Statement::VariableDeclaration { name, init, .. } => {
                let reg = self.declare_variable(name);
                if let Some(expr) = init {
                    if !self.might_produce_string(expr) {
                        self.numeric_locals.insert(name.clone());
                    }
                    self.compile_expression(expr);
                    self.builder.store_accumulator_in_register(reg);
                } else {
                    self.builder.load_undefined();
                    self.builder.store_accumulator_in_register(reg);
                }
                if self.is_top_level {
                    let pool_idx = self
                        .builder
                        .add_constant(ConstantValue::String(name.clone()));
                    self.builder.emit_bytecode(Bytecode::StaGlobal);
                    self.builder.emit_u8(pool_idx as u8);
                    self.builder.emit_u8(0); // feedback slot
                }
            }

            Statement::UsingDeclaration { name, init, .. } => {
                let reg = self.declare_variable(name);
                self.compile_expression(init);
                self.builder.store_accumulator_in_register(reg);
            }

            Statement::Expression(expr) => {
                self.compile_expression(expr);
            }

            Statement::Block(stmts) => {
                for s in stmts {
                    self.compile_statement(s);
                }
            }

            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.compile_expression(condition);
                let false_jump = self.builder.emit_jump_if_false_placeholder();

                self.compile_statement(then_branch);

                if let Some(else_stmt) = else_branch {
                    let end_jump = self.builder.emit_jump_placeholder();
                    self.builder.patch_jump_to_current(false_jump);
                    self.compile_statement(else_stmt);
                    self.builder.patch_jump_to_current(end_jump);
                } else {
                    self.builder.patch_jump_to_current(false_jump);
                }
            }

            Statement::While { condition, body } => {
                let hoisted_limit = if let Expression::Binary { op, left, right } = condition {
                    if let Expression::Literal(_) = right.as_ref() {
                        self.compile_expression(right);
                        let limit_reg = self.allocate_temp();
                        self.builder.store_accumulator_in_register(limit_reg);
                        Some((*op, left.clone(), limit_reg))
                    } else {
                        None
                    }
                } else {
                    None
                };

                let mut loop_hoisted_lits = Vec::new();
                collect_loop_invariant_literals(body, &mut loop_hoisted_lits);
                let mut loop_hoisted_regs = Vec::new();
                for lit in loop_hoisted_lits {
                    if !self.hoisted_constants.iter().any(|(l, _)| l == &lit) {
                        self.compile_expression(&Expression::Literal(lit.clone()));
                        let reg = self.allocate_temp();
                        self.builder.store_accumulator_in_register(reg);
                        self.hoisted_constants.push((lit, reg));
                        loop_hoisted_regs.push(reg);
                    }
                }

                let loop_start = self.builder.current_offset();
                let exit_jump = if let Some((op, ref left_expr, limit_reg)) = hoisted_limit {
                    self.compile_expression(left_expr);
                    self.emit_binary_op(&op, limit_reg);
                    self.builder.emit_jump_if_false_placeholder()
                } else {
                    self.compile_expression(condition);
                    self.builder.emit_jump_if_false_placeholder()
                };

                let loop_label = self.pending_label.take();
                self.loop_stack.push(LoopScope {
                    label: loop_label,
                    is_loop: true,
                    break_jumps: Vec::new(),
                    continue_jumps: Vec::new(),
                });

                self.compile_statement(body);

                let scope = self.loop_stack.pop().unwrap();
                for placeholder in scope.continue_jumps {
                    self.builder.patch_jump_to_current(placeholder);
                }

                let loop_end = self.builder.current_offset();
                let loop_delta = -((loop_end - loop_start) as i8);
                self.builder.jump_loop(loop_delta);

                self.builder.patch_jump_to_current(exit_jump);
                for placeholder in scope.break_jumps {
                    self.builder.patch_jump_to_current(placeholder);
                }

                for reg in loop_hoisted_regs {
                    self.hoisted_constants.retain(|(_, r)| *r != reg);
                    self.free_temp(reg);
                }

                if let Some((_, _, limit_reg)) = hoisted_limit {
                    self.free_temp(limit_reg);
                }
            }

            Statement::DoWhile { body, condition } => {
                let loop_start = self.builder.current_offset();

                let loop_label = self.pending_label.take();
                self.loop_stack.push(LoopScope {
                    label: loop_label,
                    is_loop: true,
                    break_jumps: Vec::new(),
                    continue_jumps: Vec::new(),
                });

                self.compile_statement(body);

                let scope = self.loop_stack.pop().unwrap();
                for placeholder in scope.continue_jumps {
                    self.builder.patch_jump_to_current(placeholder);
                }

                self.compile_expression(condition);
                let inst_start = self.builder.current_offset();
                let delta = -((inst_start - loop_start) as i8);
                self.builder.jump_if_true(delta);

                for placeholder in scope.break_jumps {
                    self.builder.patch_jump_to_current(placeholder);
                }
            }

            Statement::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(init_stmt) = init {
                    self.compile_statement(init_stmt);
                }

                // Optimization: Hoist loop condition literal limit (e.g. `i < 500000`) before loop_start
                let hoisted_limit = if let Some(Expression::Binary { op, left, right }) = condition {
                    if let Expression::Literal(_) = right.as_ref() {
                        self.compile_expression(right);
                        let limit_reg = self.allocate_temp();
                        self.builder.store_accumulator_in_register(limit_reg);
                        Some((*op, left.clone(), limit_reg))
                    } else {
                        None
                    }
                } else {
                    None
                };

                let mut loop_hoisted_lits = Vec::new();
                collect_loop_invariant_literals(body, &mut loop_hoisted_lits);
                let mut loop_hoisted_regs = Vec::new();
                for lit in loop_hoisted_lits {
                    if !self.hoisted_constants.iter().any(|(l, _)| l == &lit) {
                        self.compile_expression(&Expression::Literal(lit.clone()));
                        let reg = self.allocate_temp();
                        self.builder.store_accumulator_in_register(reg);
                        self.hoisted_constants.push((lit, reg));
                        loop_hoisted_regs.push(reg);
                    }
                }

                let loop_start = self.builder.current_offset();
                let exit_jump = if let Some((op, ref left_expr, limit_reg)) = hoisted_limit {
                    self.compile_expression(left_expr);
                    self.emit_binary_op(&op, limit_reg);
                    Some(self.builder.emit_jump_if_false_placeholder())
                } else if let Some(cond_expr) = condition {
                    self.compile_expression(cond_expr);
                    Some(self.builder.emit_jump_if_false_placeholder())
                } else {
                    None
                };

                let loop_label = self.pending_label.take();
                self.loop_stack.push(LoopScope {
                    label: loop_label,
                    is_loop: true,
                    break_jumps: Vec::new(),
                    continue_jumps: Vec::new(),
                });

                self.compile_statement(body);

                let scope = self.loop_stack.pop().unwrap();
                for placeholder in scope.continue_jumps {
                    self.builder.patch_jump_to_current(placeholder);
                }

                if let Some(update_expr) = update {
                    self.compile_expression(update_expr);
                }

                let loop_end = self.builder.current_offset();
                let loop_delta = -((loop_end - loop_start) as i8);
                self.builder.jump_loop(loop_delta);

                if let Some(placeholder) = exit_jump {
                    self.builder.patch_jump_to_current(placeholder);
                }
                for placeholder in scope.break_jumps {
                    self.builder.patch_jump_to_current(placeholder);
                }

                for reg in loop_hoisted_regs {
                    self.hoisted_constants.retain(|(_, r)| *r != reg);
                    self.free_temp(reg);
                }

                if let Some((_, _, limit_reg)) = hoisted_limit {
                    self.free_temp(limit_reg);
                }
            }

            Statement::ForOf {
                var_name,
                iterable,
                body,
                is_await,
                ..
            } => {
                self.compile_expression(iterable);
                let iter_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(iter_reg);

                // Get iterator
                self.builder.emit_bytecode(Bytecode::GetIterator);
                self.builder.store_accumulator_in_register(iter_reg);

                let step_reg = self.allocate_temp();
                let loop_start = self.builder.current_offset();

                // step = iter.next()
                self.builder.emit_bytecode(Bytecode::IteratorNext);
                self.builder.emit_i8(iter_reg.to_operand() as i8);

                if *is_await {
                    self.builder.emit_bytecode(Bytecode::Await);
                }

                self.builder.store_accumulator_in_register(step_reg);

                // if step.done, break
                self.builder.emit_bytecode(Bytecode::IteratorDone);
                self.builder.emit_i8(step_reg.to_operand() as i8);
                let exit_jump = self.builder.emit_jump_if_true_placeholder();

                // var = step.value
                self.builder.emit_bytecode(Bytecode::IteratorValue);
                self.builder.emit_i8(step_reg.to_operand() as i8);
                let var_reg = self.declare_variable(var_name);
                self.builder.store_accumulator_in_register(var_reg);

                let loop_label = self.pending_label.take();
                self.loop_stack.push(LoopScope {
                    label: loop_label,
                    is_loop: true,
                    break_jumps: Vec::new(),
                    continue_jumps: Vec::new(),
                });

                self.compile_statement(body);

                let scope = self.loop_stack.pop().unwrap();
                for placeholder in scope.continue_jumps {
                    self.builder.patch_jump_to_current(placeholder);
                }

                let loop_end = self.builder.current_offset();
                let loop_delta = -((loop_end - loop_start) as i8);
                self.builder.jump_loop(loop_delta);

                self.builder.patch_jump_to_current(exit_jump);
                for placeholder in scope.break_jumps {
                    self.builder.patch_jump_to_current(placeholder);
                }

                self.free_temp(step_reg);
                self.free_temp(iter_reg);
            }

            Statement::ForIn {
                var_name,
                object,
                body,
                ..
            } => {
                self.compile_expression(object);
                self.builder.emit_bytecode(Bytecode::ForInEnumerate);

                let keys_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(keys_reg);

                let len_reg = self.allocate_temp();
                let pool_idx = self
                    .builder
                    .add_constant(ConstantValue::String("length".to_string()));
                self.builder.emit_bytecode(Bytecode::LdaNamedProperty);
                self.builder.emit_i8(keys_reg.to_operand() as i8);
                self.builder.emit_u8(pool_idx as u8);
                self.builder.store_accumulator_in_register(len_reg);

                let idx_reg = self.allocate_temp();
                self.builder.load_smi(0);
                self.builder.store_accumulator_in_register(idx_reg);

                let loop_start = self.builder.current_offset();

                self.builder.load_accumulator_from_register(idx_reg);
                self.builder.test_less_than(len_reg);
                let exit_jump = self.builder.emit_jump_if_false_placeholder();

                self.builder.emit_bytecode(Bytecode::LdaKeyedProperty);
                self.builder.emit_i8(keys_reg.to_operand() as i8);
                self.builder.emit_i8(idx_reg.to_operand() as i8);
                self.builder.emit_u8(0);

                let var_reg = self.declare_variable(var_name);
                self.builder.store_accumulator_in_register(var_reg);

                let loop_label = self.pending_label.take();
                self.loop_stack.push(LoopScope {
                    label: loop_label,
                    is_loop: true,
                    break_jumps: Vec::new(),
                    continue_jumps: Vec::new(),
                });

                self.compile_statement(body);

                let scope = self.loop_stack.pop().unwrap();
                for placeholder in scope.continue_jumps {
                    self.builder.patch_jump_to_current(placeholder);
                }

                self.builder.load_accumulator_from_register(idx_reg);
                self.builder.inc();
                self.builder.store_accumulator_in_register(idx_reg);

                let loop_end = self.builder.current_offset();
                let loop_delta = -((loop_end - loop_start) as i8);
                self.builder.jump_loop(loop_delta);

                self.builder.patch_jump_to_current(exit_jump);
                for placeholder in scope.break_jumps {
                    self.builder.patch_jump_to_current(placeholder);
                }

                self.free_temp(idx_reg);
                self.free_temp(len_reg);
                self.free_temp(keys_reg);
            }

            Statement::Switch { discriminant, cases } => {
                self.compile_expression(discriminant);
                let disc_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(disc_reg);

                let switch_label = self.pending_label.take();
                self.loop_stack.push(LoopScope {
                    label: switch_label,
                    is_loop: false,
                    break_jumps: Vec::new(),
                    continue_jumps: Vec::new(),
                });

                let mut case_jumps: Vec<(usize, usize)> = Vec::new();
                let mut default_idx: Option<usize> = None;

                for (i, case) in cases.iter().enumerate() {
                    if let Some(ref test_expr) = case.test {
                        self.compile_expression(test_expr);
                        self.builder.test_equal_strict(disc_reg);
                        let jump = self.builder.emit_jump_if_true_placeholder();
                        case_jumps.push((i, jump));
                    } else {
                        default_idx = Some(i);
                    }
                }

                let fallback_jump = self.builder.emit_jump_placeholder();

                let mut default_target: Option<usize> = None;
                for (i, case) in cases.iter().enumerate() {
                    let case_target = self.builder.current_offset();
                    if default_idx == Some(i) {
                        default_target = Some(case_target);
                    }
                    for (c_idx, jump) in &case_jumps {
                        if *c_idx == i {
                            self.builder.patch_jump_to_current(*jump);
                        }
                    }
                    for stmt in &case.statements {
                        self.compile_statement(stmt);
                    }
                }

                if let Some(def_offset) = default_target {
                    self.builder.patch_jump_to_target(fallback_jump, def_offset);
                } else {
                    self.builder.patch_jump_to_current(fallback_jump);
                }

                let scope = self.loop_stack.pop().unwrap();
                for placeholder in scope.break_jumps {
                    self.builder.patch_jump_to_current(placeholder);
                }

                self.free_temp(disc_reg);
            }

            Statement::Break(opt_label) => {
                let placeholder = self.builder.emit_jump_placeholder();
                if let Some(ref lbl) = opt_label {
                    if let Some(scope) = self.loop_stack.iter_mut().rev().find(|s| s.label.as_ref() == Some(lbl)) {
                        scope.break_jumps.push(placeholder);
                    } else if let Some(scope) = self.loop_stack.last_mut() {
                        scope.break_jumps.push(placeholder);
                    }
                } else if let Some(scope) = self.loop_stack.last_mut() {
                    scope.break_jumps.push(placeholder);
                }
            }

            Statement::Continue(opt_label) => {
                let placeholder = self.builder.emit_jump_placeholder();
                if let Some(ref lbl) = opt_label {
                    if let Some(scope) = self.loop_stack.iter_mut().rev().find(|s| s.label.as_ref() == Some(lbl) && s.is_loop) {
                        scope.continue_jumps.push(placeholder);
                    } else if let Some(scope) = self.loop_stack.iter_mut().rev().find(|s| s.is_loop) {
                        scope.continue_jumps.push(placeholder);
                    }
                } else if let Some(scope) = self.loop_stack.iter_mut().rev().find(|s| s.is_loop) {
                    scope.continue_jumps.push(placeholder);
                }
            }

            Statement::Labeled { label, body } => {
                let is_loop = matches!(
                    body.as_ref(),
                    Statement::While { .. }
                        | Statement::DoWhile { .. }
                        | Statement::For { .. }
                        | Statement::ForOf { .. }
                        | Statement::ForIn { .. }
                        | Statement::Switch { .. }
                );
                if is_loop {
                    self.pending_label = Some(label.clone());
                    self.compile_statement(body);
                } else {
                    self.loop_stack.push(LoopScope {
                        label: Some(label.clone()),
                        is_loop: false,
                        break_jumps: Vec::new(),
                        continue_jumps: Vec::new(),
                    });
                    self.compile_statement(body);
                    let scope = self.loop_stack.pop().unwrap();
                    for placeholder in scope.break_jumps {
                        self.builder.patch_jump_to_current(placeholder);
                    }
                }
            }

            Statement::Debugger => {
                self.builder.emit_bytecode(Bytecode::Debugger);
            }

            Statement::With { object, body } => {
                self.compile_expression(object);
                let with_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(with_reg);
                self.with_stack.push(with_reg);
                self.compile_statement(body);
                self.with_stack.pop();
                self.free_temp(with_reg);
            }

            Statement::Throw(expr) => {
                self.compile_expression(expr);
                self.builder.emit_bytecode(Bytecode::Throw);
            }

            Statement::TryCatch {
                try_block,
                catch_param,
                catch_block,
                finally_block,
            } => {
                let start_pc = self.builder.current_offset();
                if let Some(ref fin) = finally_block {
                    self.finally_stack.push((**fin).clone());
                }
                self.compile_statement(try_block);
                if finally_block.is_some() {
                    self.finally_stack.pop();
                }
                let skip_catch_jump = self.builder.emit_jump_placeholder();
                let end_pc = self.builder.current_offset();

                let handler_pc = self.builder.current_offset();
                self.builder.register_handler(start_pc, end_pc, handler_pc);

                if let Some(ref catch_stmt) = catch_block {
                    let catch_start_pc = self.builder.current_offset();
                    if let Some(ref param_name) = catch_param {
                        let reg = self.declare_variable(param_name);
                        self.builder.store_accumulator_in_register(reg);
                    }
                    self.compile_statement(catch_stmt);

                    let skip_catch_handler_jump = if finally_block.is_some() {
                        Some(self.builder.emit_jump_placeholder())
                    } else {
                        None
                    };
                    let catch_end_pc = self.builder.current_offset();

                    if let Some(ref fin_stmt) = finally_block {
                        let catch_handler_pc = self.builder.current_offset();
                        self.builder.register_handler(catch_start_pc, catch_end_pc, catch_handler_pc);
                        let err_reg = self.allocate_temp();
                        self.builder.store_accumulator_in_register(err_reg);
                        self.compile_statement(fin_stmt);
                        self.builder.load_accumulator_from_register(err_reg);
                        self.builder.emit_bytecode(Bytecode::ReThrow);
                        self.free_temp(err_reg);
                        if let Some(placeholder) = skip_catch_handler_jump {
                            self.builder.patch_jump_to_current(placeholder);
                        }
                    }
                } else if let Some(ref fin_stmt) = finally_block {
                    let err_reg = self.allocate_temp();
                    self.builder.store_accumulator_in_register(err_reg);
                    self.compile_statement(fin_stmt);
                    self.builder.load_accumulator_from_register(err_reg);
                    self.builder.emit_bytecode(Bytecode::ReThrow);
                    self.free_temp(err_reg);
                }

                self.builder.patch_jump_to_current(skip_catch_jump);

                if let Some(ref fin_stmt) = finally_block {
                    self.compile_statement(fin_stmt);
                }
            }

            Statement::Return(opt_expr) => {
                if !self.finally_stack.is_empty() {
                    let ret_reg = self.allocate_temp();
                    if let Some(expr) = opt_expr {
                        self.compile_expression(expr);
                    } else {
                        self.builder.load_undefined();
                    }
                    self.builder.store_accumulator_in_register(ret_reg);

                    let fin_stmts = self.finally_stack.clone();
                    for fin in fin_stmts.iter().rev() {
                        self.compile_statement(fin);
                    }

                    self.builder.load_accumulator_from_register(ret_reg);
                    self.free_temp(ret_reg);
                    self.builder.return_value();
                } else {
                    if let Some(expr) = opt_expr {
                        self.compile_expression(expr);
                    } else {
                        self.builder.load_undefined();
                    }
                    self.builder.return_value();
                }
            }

            Statement::FunctionDeclaration { .. } => {
                // Nested function declarations are compiled separately when invoked
            }

            Statement::ClassDeclaration { .. } => {
                // Class declarations are hoisted into global scope before script execution
            }

            Statement::ImportDeclaration { .. } => {}

            Statement::ExportDeclaration { declaration, .. } => {
                if let Some(decl) = declaration {
                    self.compile_statement(decl);
                }
            }

            Statement::Empty => {}
        }
    }

    /// Compiles an expression, leaving the evaluated result in the Ignition accumulator.
    pub fn compile_expression(&mut self, expr: &Expression) {
        match expr {
            Expression::Literal(val) => match val {
                LiteralValue::Smi(n) => {
                    if *n >= -128 && *n <= 127 {
                        self.builder.load_smi(*n);
                    } else {
                        self.builder.load_constant(ConstantValue::Smi(*n));
                    }
                }
                LiteralValue::Number(f) => {
                    self.builder.load_constant(ConstantValue::Number(*f));
                }
                LiteralValue::String(s) => {
                    self.builder.load_constant(ConstantValue::String(s.clone()));
                }
                LiteralValue::Boolean(true) => self.builder.load_true(),
                LiteralValue::Boolean(false) => self.builder.load_false(),
                LiteralValue::BigInt(s) => {
                    self.builder.load_constant(ConstantValue::BigInt(s.clone()));
                }
                LiteralValue::Null => self.builder.load_null(),
                LiteralValue::Undefined => self.builder.load_undefined(),
            },

            Expression::Variable(name) => {
                if !self.with_stack.is_empty() {
                    let mut found_jumps = Vec::new();
                    let name_idx = self
                        .builder
                        .add_constant(ConstantValue::String(name.clone()));
                    for &with_reg in self.with_stack.iter().rev() {
                        self.builder.load_constant(ConstantValue::String(name.clone()));
                        self.builder.emit_bytecode(Bytecode::TestIn);
                        self.builder.emit_i8(with_reg.to_operand() as i8);
                        self.builder.emit_u8(0);
                        let not_in_jump = self.builder.emit_jump_if_false_placeholder();
                        self.builder.emit_bytecode(Bytecode::LdaNamedProperty);
                        self.builder.emit_i8(with_reg.to_operand() as i8);
                        self.builder.emit_u8(name_idx as u8);
                        let done_jump = self.builder.emit_jump_placeholder();
                        found_jumps.push(done_jump);
                        self.builder.patch_jump_to_current(not_in_jump);
                    }
                    if let Some(reg) = self.lookup_variable(name) {
                        self.builder.load_accumulator_from_register(reg);
                    } else {
                        self.builder.emit_bytecode(Bytecode::LdaGlobal);
                        self.builder.emit_u8(name_idx as u8);
                        self.builder.emit_u8(0);
                    }
                    for j in found_jumps {
                        self.builder.patch_jump_to_current(j);
                    }
                } else if let Some(reg) = self.lookup_variable(name) {
                    self.builder.load_accumulator_from_register(reg);
                } else {
                    // Global property load
                    let pool_idx = self
                        .builder
                        .add_constant(ConstantValue::String(name.clone()));
                    self.builder.emit_bytecode(Bytecode::LdaGlobal);
                    self.builder.emit_u8(pool_idx as u8);
                    self.builder.emit_u8(0);
                }
            }

            Expression::This => {
                self.builder.load_accumulator_from_register(Register::receiver());
            }

            Expression::Super => {
                self.builder.emit_bytecode(Bytecode::LdaSuper);
            }

            Expression::Assignment { target, value } => {
                if !self.might_produce_string(value) {
                    self.numeric_locals.insert(target.clone());
                } else {
                    self.numeric_locals.remove(target);
                }
                self.compile_expression(value);
                if !self.with_stack.is_empty() {
                    let val_reg = self.allocate_temp();
                    self.builder.store_accumulator_in_register(val_reg);
                    let mut found_jumps = Vec::new();
                    let name_idx = self
                        .builder
                        .add_constant(ConstantValue::String(target.clone()));
                    for &with_reg in self.with_stack.iter().rev() {
                        self.builder.load_constant(ConstantValue::String(target.clone()));
                        self.builder.emit_bytecode(Bytecode::TestIn);
                        self.builder.emit_i8(with_reg.to_operand() as i8);
                        self.builder.emit_u8(0);
                        let not_in_jump = self.builder.emit_jump_if_false_placeholder();
                        self.builder.load_accumulator_from_register(val_reg);
                        self.builder.emit_bytecode(Bytecode::StaNamedProperty);
                        self.builder.emit_i8(with_reg.to_operand() as i8);
                        self.builder.emit_u8(name_idx as u8);
                        self.builder.emit_u8(0);
                        let done_jump = self.builder.emit_jump_placeholder();
                        found_jumps.push(done_jump);
                        self.builder.patch_jump_to_current(not_in_jump);
                    }
                    self.builder.load_accumulator_from_register(val_reg);
                    if let Some(reg) = self.lookup_variable(target) {
                        self.builder.store_accumulator_in_register(reg);
                        if self.is_top_level {
                            self.builder.emit_bytecode(Bytecode::StaGlobal);
                            self.builder.emit_u8(name_idx as u8);
                            self.builder.emit_u8(0);
                        }
                    } else {
                        self.builder.emit_bytecode(Bytecode::StaGlobal);
                        self.builder.emit_u8(name_idx as u8);
                        self.builder.emit_u8(0);
                    }
                    for j in found_jumps {
                        self.builder.patch_jump_to_current(j);
                    }
                    self.free_temp(val_reg);
                } else if let Some(reg) = self.lookup_variable(target) {
                    self.builder.store_accumulator_in_register(reg);
                    if self.is_top_level {
                        let pool_idx = self
                            .builder
                            .add_constant(ConstantValue::String(target.clone()));
                        self.builder.emit_bytecode(Bytecode::StaGlobal);
                        self.builder.emit_u8(pool_idx as u8);
                        self.builder.emit_u8(0);
                    }
                } else {
                    let pool_idx = self
                        .builder
                        .add_constant(ConstantValue::String(target.clone()));
                    self.builder.emit_bytecode(Bytecode::StaGlobal);
                    self.builder.emit_u8(pool_idx as u8);
                    self.builder.emit_u8(0);
                }
            }

            Expression::Binary { op, left, right } => {
                if *op == BinaryOperator::LogicalAnd {
                    self.compile_expression(left);
                    let false_jump = self.builder.emit_jump_if_false_placeholder();
                    self.compile_expression(right);
                    self.builder.patch_jump_to_current(false_jump);
                    return;
                }
                if *op == BinaryOperator::LogicalOr {
                    self.compile_expression(left);
                    let true_jump = self.builder.emit_jump_if_true_placeholder();
                    self.compile_expression(right);
                    self.builder.patch_jump_to_current(true_jump);
                    return;
                }
                if *op == BinaryOperator::NullishCoalescing {
                    self.compile_expression(left);
                    let null_jump = self.builder.emit_jump_if_undefined_or_null_placeholder();
                    let end_jump = self.builder.emit_jump_placeholder();
                    self.builder.patch_jump_to_current(null_jump);
                    self.compile_expression(right);
                    self.builder.patch_jump_to_current(end_jump);
                    return;
                }

                // Optimization 1: Right is a Smi literal in [-128, 127]
                if let Expression::Literal(LiteralValue::Smi(imm)) = right.as_ref() {
                    if *imm >= -128 && *imm <= 127 {
                        match op {
                            BinaryOperator::Add => {
                                self.compile_expression(left);
                                self.builder.add_smi(*imm);
                                return;
                            }
                            BinaryOperator::Sub => {
                                self.compile_expression(left);
                                self.builder.sub_smi(*imm);
                                return;
                            }
                            BinaryOperator::Mul => {
                                self.compile_expression(left);
                                self.builder.mul_smi(*imm);
                                return;
                            }
                            BinaryOperator::Div => {
                                self.compile_expression(left);
                                self.builder.div_smi(*imm);
                                return;
                            }
                            BinaryOperator::Mod => {
                                self.compile_expression(left);
                                self.builder.mod_smi(*imm);
                                return;
                            }
                            BinaryOperator::BitwiseOr => {
                                self.compile_expression(left);
                                self.builder.bitwise_or_smi(*imm);
                                return;
                            }
                            BinaryOperator::BitwiseXor => {
                                self.compile_expression(left);
                                self.builder.bitwise_xor_smi(*imm);
                                return;
                            }
                            BinaryOperator::BitwiseAnd => {
                                self.compile_expression(left);
                                self.builder.bitwise_and_smi(*imm);
                                return;
                            }
                            BinaryOperator::ShiftLeft => {
                                self.compile_expression(left);
                                self.builder.shift_left_smi(*imm);
                                return;
                            }
                            BinaryOperator::ShiftRight => {
                                self.compile_expression(left);
                                self.builder.shift_right_smi(*imm);
                                return;
                            }
                            _ => {}
                        }
                    }
                }

                // Optimization 1.5: Right is a hoisted constant register
                if let Expression::Literal(ref lit) = right.as_ref() {
                    if let Some((_, reg)) = self.hoisted_constants.iter().rev().find(|(l, _)| l == lit) {
                        let reg = *reg;
                        self.compile_expression(left);
                        self.emit_binary_op(op, reg);
                        return;
                    }
                }

                // Optimization 2: Right is an existing local variable register
                if let Expression::Variable(name) = right.as_ref() {
                    if let Some(reg) = self.lookup_variable(name) {
                        self.compile_expression(left);
                        self.emit_binary_op(op, reg);
                        return;
                    }
                }

                // Optimization 2.5: Left is an existing local variable register and op is commutative
                if let Expression::Variable(name) = left.as_ref() {
                    let is_commutative = matches!(
                        op,
                        BinaryOperator::Mul
                            | BinaryOperator::BitwiseAnd
                            | BinaryOperator::BitwiseOr
                            | BinaryOperator::BitwiseXor
                    ) || (*op == BinaryOperator::Add
                        && !self.might_produce_string(left)
                        && !self.might_produce_string(right));

                    if is_commutative {
                        if let Some(reg) = self.lookup_variable(name) {
                            if !expr_mutates_var(right, name) {
                                self.compile_expression(right);
                                self.emit_binary_op(op, reg);
                                return;
                            }
                        }
                    }
                }

                // Optimization 3: Right is a literal (evaluate right first into single temp register)
                if let Expression::Literal(_) = right.as_ref() {
                    self.compile_expression(right);
                    let temp_right = self.allocate_temp();
                    self.builder.store_accumulator_in_register(temp_right);
                    self.compile_expression(left);
                    self.emit_binary_op(op, temp_right);
                    self.free_temp(temp_right);
                    return;
                }

                let is_commutative = matches!(
                    op,
                    BinaryOperator::Mul
                        | BinaryOperator::BitwiseAnd
                        | BinaryOperator::BitwiseOr
                        | BinaryOperator::BitwiseXor
                ) || (*op == BinaryOperator::Add
                    && !self.might_produce_string(left)
                    && !self.might_produce_string(right));

                if is_commutative {
                    self.compile_expression(left);
                    let temp_left = self.allocate_temp();
                    self.builder.store_accumulator_in_register(temp_left);

                    self.compile_expression(right);
                    self.clear_cached_call_callee();
                    self.emit_binary_op(op, temp_left);

                    self.free_temp(temp_left);
                    return;
                }

                // General fallback: evaluate left, store temp, evaluate right, store temp, reload left, apply op
                self.compile_expression(left);
                let temp_left = self.allocate_temp();
                self.builder.store_accumulator_in_register(temp_left);

                self.compile_expression(right);
                self.clear_cached_call_callee();
                let temp_right = self.allocate_temp();
                self.builder.store_accumulator_in_register(temp_right);

                self.builder.load_accumulator_from_register(temp_left);
                self.emit_binary_op(op, temp_right);

                self.free_temp(temp_right);
                self.free_temp(temp_left);
            }

            Expression::Unary { op, expr } => {
                match op {
                    UnaryOperator::Void => {
                        self.compile_expression(expr);
                        self.builder.load_undefined();
                    }
                    UnaryOperator::Delete => {
                        match expr.as_ref() {
                            Expression::PropertyAccess { object, property } => {
                                self.compile_expression(object);
                                let obj_reg = self.allocate_temp();
                                self.builder.store_accumulator_in_register(obj_reg);
                                let pool_idx = self
                                    .builder
                                    .add_constant(ConstantValue::String(property.clone()));
                                self.builder.emit_bytecode(Bytecode::DeletePropertySloppy);
                                self.builder.emit_i8(obj_reg.to_operand() as i8);
                                self.builder.emit_u8(pool_idx as u8);
                                self.free_temp(obj_reg);
                            }
                            Expression::KeyedAccess { object, key } => {
                                self.compile_expression(object);
                                let obj_reg = self.allocate_temp();
                                self.builder.store_accumulator_in_register(obj_reg);
                                self.compile_expression(key);
                                let key_reg = self.allocate_temp();
                                self.builder.store_accumulator_in_register(key_reg);
                                self.builder.emit_bytecode(Bytecode::DeleteKeyedPropertySloppy);
                                self.builder.emit_i8(obj_reg.to_operand() as i8);
                                self.builder.emit_i8(key_reg.to_operand() as i8);
                                self.free_temp(key_reg);
                                self.free_temp(obj_reg);
                            }
                            Expression::Variable(name) => {
                                if self.lookup_variable(name).is_some() {
                                    self.builder.load_false();
                                } else {
                                    self.builder.load_true();
                                }
                            }
                            _ => {
                                self.compile_expression(expr);
                                self.builder.load_true();
                            }
                        }
                    }
                    _ => {
                        self.compile_expression(expr);
                        match op {
                            UnaryOperator::Plus => {}
                            UnaryOperator::Minus => self.builder.negate(),
                            UnaryOperator::Not => self.builder.logical_not(),
                            UnaryOperator::BitwiseNot => self.builder.bitwise_not(),
                            UnaryOperator::TypeOf => self.builder.type_of(),
                            _ => {}
                        }
                    }
                }
            }

            Expression::Call { callee, arguments } => {
                let mut is_cached_call = false;
                let (callable_reg, receiver_reg_opt) = match callee.as_ref() {
                    Expression::PropertyAccess { object, property } => {
                        if **object == Expression::Super {
                            self.compile_expression(object);
                            let proto_reg = self.allocate_temp();
                            self.builder.store_accumulator_in_register(proto_reg);

                            let pool_idx = self
                                .builder
                                .add_constant(ConstantValue::String(property.clone()));
                            self.builder.emit_bytecode(Bytecode::LdaNamedProperty);
                            self.builder.emit_i8(proto_reg.to_operand() as i8);
                            self.builder.emit_u8(pool_idx as u8);

                            let func = self.allocate_temp();
                            self.builder.store_accumulator_in_register(func);
                            self.free_temp(proto_reg);

                            (func, Some(Register::receiver()))
                        } else {
                            self.compile_expression(object);
                            let recv = self.allocate_temp();
                            self.builder.store_accumulator_in_register(recv);

                            let pool_idx = self
                                .builder
                                .add_constant(ConstantValue::String(property.clone()));
                            self.builder.emit_bytecode(Bytecode::LdaNamedProperty);
                            self.builder.emit_i8(recv.to_operand() as i8);
                            self.builder.emit_u8(pool_idx as u8);

                            let func = self.allocate_temp();
                            self.builder.store_accumulator_in_register(func);
                            (func, Some(recv))
                        }
                    }
                    Expression::Super => {
                        self.compile_expression(callee);
                        let func = self.allocate_temp();
                        self.builder.store_accumulator_in_register(func);
                        (func, Some(Register::receiver()))
                    }
                    Expression::Variable(name) if self.lookup_variable(name).is_some() => {
                        let reg = self.lookup_variable(name).unwrap();
                        (reg, None)
                    }
                    Expression::Variable(name) => {
                        if let Some((ref cached_name, reg)) = self.cached_call_callee {
                            if cached_name == name {
                                is_cached_call = true;
                                (reg, None)
                            } else {
                                self.clear_cached_call_callee();
                                self.compile_expression(callee);
                                let func = self.allocate_temp();
                                self.builder.store_accumulator_in_register(func);
                                self.cached_call_callee = Some((name.clone(), func));
                                is_cached_call = true;
                                (func, None)
                            }
                        } else {
                            self.compile_expression(callee);
                            let func = self.allocate_temp();
                            self.builder.store_accumulator_in_register(func);
                            self.cached_call_callee = Some((name.clone(), func));
                            is_cached_call = true;
                            (func, None)
                        }
                    }
                    _ => {
                        self.compile_expression(callee);
                        let func = self.allocate_temp();
                        self.builder.store_accumulator_in_register(func);
                        (func, None)
                    }
                };

                let has_spread = arguments.iter().any(|a| matches!(a, Expression::Spread(_)));
                if has_spread {
                    let args_arr_expr = Expression::ArrayLiteral(arguments.clone());
                    self.compile_expression(&args_arr_expr);
                    let args_reg = self.allocate_temp();
                    self.builder.store_accumulator_in_register(args_reg);

                    let (recv_reg, recv_needs_free) = if let Some(recv) = receiver_reg_opt {
                        (recv, recv != Register::receiver())
                    } else {
                        let u_reg = self.allocate_temp();
                        self.builder.load_undefined();
                        self.builder.store_accumulator_in_register(u_reg);
                        (u_reg, true)
                    };

                    self.builder.emit_bytecode(Bytecode::CallWithSpread);
                    self.builder.emit_i8(callable_reg.to_operand() as i8);
                    self.builder.emit_i8(recv_reg.to_operand() as i8);
                    self.builder.emit_i8(args_reg.to_operand() as i8);

                    if recv_needs_free {
                        self.free_temp(recv_reg);
                    }
                    self.free_temp(args_reg);
                } else {
                    let arg_count = arguments.len();
                    if arg_count > 0 {
                        let first_arg_reg = Register::new(self.next_local_index);
                        self.next_local_index += arg_count as i32;
                        if self.next_local_index > self.max_local_count {
                            self.max_local_count = self.next_local_index;
                        }

                        for (i, arg) in arguments.iter().enumerate() {
                            self.compile_expression(arg);
                            let reg = Register::new(first_arg_reg.index() + i as i32);
                            self.builder.store_accumulator_in_register(reg);
                        }

                        let arg_list = RegisterList::from_range(first_arg_reg, arg_count);
                        if let Some(recv) = receiver_reg_opt {
                            self.builder.call_property(callable_reg, recv, arg_list);
                        } else {
                            self.builder.call_undefined_receiver(callable_reg, arg_list);
                        }
                        self.next_local_index -= arg_count as i32;
                    } else {
                        let empty_list = RegisterList::new();
                        if let Some(recv) = receiver_reg_opt {
                            self.builder.call_property(callable_reg, recv, empty_list);
                        } else {
                            self.builder.call_undefined_receiver(callable_reg, empty_list);
                        }
                    }
                }

                if let Some(recv) = receiver_reg_opt {
                    if recv != Register::receiver() {
                        self.free_temp(recv);
                    }
                }
                if !is_cached_call {
                    let is_local_var = matches!(callee.as_ref(), Expression::Variable(name) if self.lookup_variable(name).is_some());
                    if !is_local_var {
                        self.free_temp(callable_reg);
                    }
                }
            }

            Expression::New { callee, arguments } => {
                self.compile_expression(callee);
                let callable_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(callable_reg);

                let arg_count = arguments.len();
                if arg_count > 0 {
                    let first_arg_reg = Register::new(self.next_local_index);
                    self.next_local_index += arg_count as i32;
                    if self.next_local_index > self.max_local_count {
                        self.max_local_count = self.next_local_index;
                    }

                    for (i, arg) in arguments.iter().enumerate() {
                        self.compile_expression(arg);
                        let reg = Register::new(first_arg_reg.index() + i as i32);
                        self.builder.store_accumulator_in_register(reg);
                    }

                    let arg_list = RegisterList::from_range(first_arg_reg, arg_count);
                    self.builder.construct(callable_reg, arg_list);
                    self.next_local_index -= arg_count as i32;
                } else {
                    let empty_list = RegisterList::new();
                    self.builder.construct(callable_reg, empty_list);
                }

                self.free_temp(callable_reg);
            }

            Expression::PropertyAccess { object, property } => {
                let (obj_reg, needs_free) = if let Expression::Variable(name) = object.as_ref() {
                    if let Some(reg) = self.lookup_variable(name) {
                        (reg, false)
                    } else {
                        self.compile_expression(object);
                        let reg = self.allocate_temp();
                        self.builder.store_accumulator_in_register(reg);
                        (reg, true)
                    }
                } else {
                    self.compile_expression(object);
                    let reg = self.allocate_temp();
                    self.builder.store_accumulator_in_register(reg);
                    (reg, true)
                };

                let pool_idx = self
                    .builder
                    .add_constant(ConstantValue::String(property.clone()));
                self.builder.emit_bytecode(Bytecode::LdaNamedProperty);
                self.builder.emit_i8(obj_reg.to_operand() as i8);
                self.builder.emit_u8(pool_idx as u8);

                if needs_free {
                    self.free_temp(obj_reg);
                }
            }

            Expression::PropertyAssignment {
                object,
                property,
                value,
            } => {
                if **object == Expression::Super {
                    self.compile_expression(value);
                    let pool_idx = self
                        .builder
                        .add_constant(ConstantValue::String(property.clone()));
                    self.builder.emit_bytecode(Bytecode::StaNamedProperty);
                    self.builder.emit_i8(Register::receiver().to_operand() as i8);
                    self.builder.emit_u8(pool_idx as u8);
                    self.builder.emit_u8(0);
                } else {
                    let (obj_reg, needs_free) = if let Expression::Variable(name) = object.as_ref() {
                        if let Some(reg) = self.lookup_variable(name) {
                            (reg, false)
                        } else {
                            self.compile_expression(object);
                            let reg = self.allocate_temp();
                            self.builder.store_accumulator_in_register(reg);
                            (reg, true)
                        }
                    } else {
                        self.compile_expression(object);
                        let reg = self.allocate_temp();
                        self.builder.store_accumulator_in_register(reg);
                        (reg, true)
                    };

                    self.compile_expression(value);

                    let pool_idx = self
                        .builder
                        .add_constant(ConstantValue::String(property.clone()));
                    self.builder.emit_bytecode(Bytecode::StaNamedProperty);
                    self.builder.emit_i8(obj_reg.to_operand() as i8);
                    self.builder.emit_u8(pool_idx as u8);
                    self.builder.emit_u8(0); // feedback slot

                    if needs_free {
                        self.free_temp(obj_reg);
                    }
                }
            }

            Expression::KeyedAccess { object, key } => {
                let (obj_reg, obj_needs_free) = if let Expression::Variable(name) = object.as_ref() {
                    if let Some(reg) = self.lookup_variable(name) {
                        (reg, false)
                    } else {
                        self.compile_expression(object);
                        let reg = self.allocate_temp();
                        self.builder.store_accumulator_in_register(reg);
                        (reg, true)
                    }
                } else {
                    self.compile_expression(object);
                    let reg = self.allocate_temp();
                    self.builder.store_accumulator_in_register(reg);
                    (reg, true)
                };

                let (key_reg, key_needs_free) = if let Expression::Variable(name) = key.as_ref() {
                    if let Some(reg) = self.lookup_variable(name) {
                        (reg, false)
                    } else {
                        self.compile_expression(key);
                        let reg = self.allocate_temp();
                        self.builder.store_accumulator_in_register(reg);
                        (reg, true)
                    }
                } else {
                    self.compile_expression(key);
                    let reg = self.allocate_temp();
                    self.builder.store_accumulator_in_register(reg);
                    (reg, true)
                };

                self.builder.emit_bytecode(Bytecode::LdaKeyedProperty);
                self.builder.emit_i8(obj_reg.to_operand() as i8);
                self.builder.emit_i8(key_reg.to_operand() as i8);
                self.builder.emit_u8(0); // feedback slot

                if key_needs_free {
                    self.free_temp(key_reg);
                }
                if obj_needs_free {
                    self.free_temp(obj_reg);
                }
            }

            Expression::KeyedAssignment {
                object,
                key,
                value,
            } => {
                let (obj_reg, obj_needs_free) = if let Expression::Variable(name) = object.as_ref() {
                    if let Some(reg) = self.lookup_variable(name) {
                        (reg, false)
                    } else {
                        self.compile_expression(object);
                        let reg = self.allocate_temp();
                        self.builder.store_accumulator_in_register(reg);
                        (reg, true)
                    }
                } else {
                    self.compile_expression(object);
                    let reg = self.allocate_temp();
                    self.builder.store_accumulator_in_register(reg);
                    (reg, true)
                };

                let (key_reg, key_needs_free) = if let Expression::Variable(name) = key.as_ref() {
                    if let Some(reg) = self.lookup_variable(name) {
                        (reg, false)
                    } else {
                        self.compile_expression(key);
                        let reg = self.allocate_temp();
                        self.builder.store_accumulator_in_register(reg);
                        (reg, true)
                    }
                } else {
                    self.compile_expression(key);
                    let reg = self.allocate_temp();
                    self.builder.store_accumulator_in_register(reg);
                    (reg, true)
                };

                self.compile_expression(value);

                self.builder.emit_bytecode(Bytecode::StaKeyedProperty);
                self.builder.emit_i8(obj_reg.to_operand() as i8);
                self.builder.emit_i8(key_reg.to_operand() as i8);
                self.builder.emit_u8(0); // feedback slot
                self.builder.emit_u8(0); // flags

                if key_needs_free {
                    self.free_temp(key_reg);
                }
                if obj_needs_free {
                    self.free_temp(obj_reg);
                }
            }

            Expression::ObjectLiteral(properties) => {
                self.builder.emit_bytecode(Bytecode::CreateEmptyObjectLiteral);
                let obj_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(obj_reg);

                for (name, val_expr) in properties {
                    if let Expression::Spread(inner) = val_expr {
                        self.compile_expression(inner);
                        self.builder.emit_bytecode(Bytecode::SpreadInObjectLiteral);
                        self.builder.emit_i8(obj_reg.to_operand() as i8);
                    } else if let Expression::ComputedProperty { key, value } = val_expr {
                        self.compile_expression(key);
                        let key_reg = self.allocate_temp();
                        self.builder.store_accumulator_in_register(key_reg);

                        self.compile_expression(value);

                        self.builder.emit_bytecode(Bytecode::StaKeyedProperty);
                        self.builder.emit_i8(obj_reg.to_operand() as i8);
                        self.builder.emit_i8(key_reg.to_operand() as i8);
                        self.builder.emit_u8(0); // feedback slot
                        self.builder.emit_u8(0); // flags

                        self.free_temp(key_reg);
                    } else {
                        self.compile_expression(val_expr);
                        let pool_idx = self
                            .builder
                            .add_constant(ConstantValue::String(name.clone()));
                        self.builder.emit_bytecode(Bytecode::StaNamedProperty);
                        self.builder.emit_i8(obj_reg.to_operand() as i8);
                        self.builder.emit_u8(pool_idx as u8);
                        self.builder.emit_u8(0); // feedback slot
                    }
                }

                self.builder.load_accumulator_from_register(obj_reg);
                self.free_temp(obj_reg);
            }

            Expression::ComputedProperty { key, value } => {
                self.compile_expression(key);
                self.compile_expression(value);
            }

            Expression::ArrayLiteral(elements) => {
                self.builder.emit_bytecode(Bytecode::CreateEmptyArrayLiteral);
                let arr_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(arr_reg);

                let has_spread = elements.iter().any(|e| matches!(e, Expression::Spread(_)));
                if !has_spread {
                    for (idx, elem_expr) in elements.iter().enumerate() {
                        let idx_reg = self.allocate_temp();
                        self.builder.load_smi(idx as i32);
                        self.builder.store_accumulator_in_register(idx_reg);

                        self.compile_expression(elem_expr);

                        self.builder.emit_bytecode(Bytecode::StaInArrayLiteral);
                        self.builder.emit_i8(arr_reg.to_operand() as i8);
                        self.builder.emit_i8(idx_reg.to_operand() as i8);
                        self.builder.emit_u8(0); // feedback slot
                        self.builder.emit_u8(0); // flags

                        self.free_temp(idx_reg);
                    }
                } else {
                    for elem_expr in elements {
                        match elem_expr {
                            Expression::Spread(inner) => {
                                self.compile_expression(inner);
                                self.builder.emit_bytecode(Bytecode::SpreadInArrayLiteral);
                                self.builder.emit_i8(arr_reg.to_operand() as i8);
                            }
                            _ => {
                                let single_arr = Expression::ArrayLiteral(vec![elem_expr.clone()]);
                                self.compile_expression(&single_arr);
                                self.builder.emit_bytecode(Bytecode::SpreadInArrayLiteral);
                                self.builder.emit_i8(arr_reg.to_operand() as i8);
                            }
                        }
                    }
                }

                self.builder.load_accumulator_from_register(arr_reg);
                self.free_temp(arr_reg);
            }

            Expression::Await(expr) => {
                self.compile_expression(expr);
                self.builder.emit_bytecode(Bytecode::Await);
            }

            Expression::RegExpLiteral { pattern, flags } => {
                let pat_idx = self
                    .builder
                    .add_constant(ConstantValue::String(pattern.clone()));
                let flags_idx = self
                    .builder
                    .add_constant(ConstantValue::String(flags.clone()));
                self.builder.emit_bytecode(Bytecode::CreateRegExpLiteral);
                self.builder.emit_u8(pat_idx as u8);
                self.builder.emit_u8(flags_idx as u8);
            }

            Expression::Spread(expr) => {
                self.compile_expression(expr);
            }

            Expression::OptionalPropertyAccess { object, property } => {
                self.compile_expression(object);
                let null_jump = self.builder.emit_jump_if_undefined_or_null_placeholder();
                let obj_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(obj_reg);
                let pool_idx = self
                    .builder
                    .add_constant(ConstantValue::String(property.clone()));
                self.builder.emit_bytecode(Bytecode::LdaNamedProperty);
                self.builder.emit_i8(obj_reg.to_operand() as i8);
                self.builder.emit_u8(pool_idx as u8);
                self.free_temp(obj_reg);
                self.builder.patch_jump_to_current(null_jump);
            }

            Expression::OptionalKeyedAccess { object, key } => {
                self.compile_expression(object);
                let null_jump = self.builder.emit_jump_if_undefined_or_null_placeholder();
                let obj_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(obj_reg);
                self.compile_expression(key);
                let key_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(key_reg);
                self.builder.emit_bytecode(Bytecode::LdaKeyedProperty);
                self.builder.emit_i8(obj_reg.to_operand() as i8);
                self.builder.emit_i8(key_reg.to_operand() as i8);
                self.builder.emit_u8(0);
                self.free_temp(key_reg);
                self.free_temp(obj_reg);
                self.builder.patch_jump_to_current(null_jump);
            }

            Expression::OptionalCall { callee, arguments } => {
                self.compile_expression(callee);
                let null_jump = self.builder.emit_jump_if_undefined_or_null_placeholder();
                let callable_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(callable_reg);
                let arg_count = arguments.len();
                if arg_count > 0 {
                    let first_arg_reg = Register::new(self.next_local_index);
                    self.next_local_index += arg_count as i32;
                    if self.next_local_index > self.max_local_count {
                        self.max_local_count = self.next_local_index;
                    }
                    for (i, arg) in arguments.iter().enumerate() {
                        self.compile_expression(arg);
                        let reg = Register::new(first_arg_reg.index() + i as i32);
                        self.builder.store_accumulator_in_register(reg);
                    }
                    let arg_list = RegisterList::from_range(first_arg_reg, arg_count);
                    self.builder.call_undefined_receiver(callable_reg, arg_list);
                    self.next_local_index -= arg_count as i32;
                } else {
                    let empty_list = RegisterList::new();
                    self.builder.call_undefined_receiver(callable_reg, empty_list);
                }
                self.free_temp(callable_reg);
                self.builder.patch_jump_to_current(null_jump);
            }

            Expression::Yield { value, delegate } => {
                if !delegate {
                    if let Some(v) = value {
                        self.compile_expression(v);
                    } else {
                        self.builder.load_undefined();
                    }
                    self.builder.emit_bytecode(Bytecode::SuspendGenerator);
                } else {
                    let expr = value.as_ref().unwrap();
                    self.compile_expression(expr);
                    let iter_reg = self.allocate_temp();
                    self.builder.store_accumulator_in_register(iter_reg);

                    self.builder.emit_bytecode(Bytecode::GetIterator);
                    self.builder.store_accumulator_in_register(iter_reg);

                    let step_reg = self.allocate_temp();
                    let loop_start = self.builder.current_offset();

                    self.builder.emit_bytecode(Bytecode::IteratorNext);
                    self.builder.emit_i8(iter_reg.to_operand() as i8);
                    self.builder.store_accumulator_in_register(step_reg);

                    self.builder.emit_bytecode(Bytecode::IteratorDone);
                    self.builder.emit_i8(step_reg.to_operand() as i8);
                    let exit_jump = self.builder.emit_jump_if_true_placeholder();

                    self.builder.emit_bytecode(Bytecode::IteratorValue);
                    self.builder.emit_i8(step_reg.to_operand() as i8);
                    self.builder.emit_bytecode(Bytecode::SuspendGenerator);

                    let loop_end = self.builder.current_offset();
                    let loop_delta = -((loop_end - loop_start) as i8);
                    self.builder.jump_loop(loop_delta);

                    self.builder.patch_jump_to_current(exit_jump);
                    self.builder.emit_bytecode(Bytecode::IteratorValue);
                    self.builder.emit_i8(step_reg.to_operand() as i8);

                    self.free_temp(step_reg);
                    self.free_temp(iter_reg);
                }
            }

            Expression::DynamicImport(spec_expr) => {
                self.compile_expression(spec_expr);
                self.builder.emit_bytecode(Bytecode::DynamicImport);
            }

            Expression::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                self.compile_expression(condition);
                let false_jump = self.builder.emit_jump_if_false_placeholder();
                self.compile_expression(then_expr);
                let end_jump = self.builder.emit_jump_placeholder();
                self.builder.patch_jump_to_current(false_jump);
                self.compile_expression(else_expr);
                self.builder.patch_jump_to_current(end_jump);
            }

            Expression::NewTarget => {
                self.builder.emit_bytecode(Bytecode::LdaNewTarget);
            }

            Expression::ImportMeta => {
                self.builder.emit_bytecode(Bytecode::CreateEmptyObjectLiteral);
                let obj_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(obj_reg);
                let url_str = "file:///main.js".to_string();
                self.builder.load_constant(ConstantValue::String(url_str));
                let pool_idx = self
                    .builder
                    .add_constant(ConstantValue::String("url".to_string()));
                self.builder.emit_bytecode(Bytecode::StaNamedProperty);
                self.builder.emit_i8(obj_reg.to_operand() as i8);
                self.builder.emit_u8(pool_idx as u8);
                self.builder.emit_u8(0);
                self.builder.load_accumulator_from_register(obj_reg);
                self.free_temp(obj_reg);
            }

            Expression::ArrayDestructureAssignment { targets, value } => {
                self.compile_expression(value);
                let val_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(val_reg);

                for (i, target) in targets.iter().enumerate() {
                    let key_reg = self.allocate_temp();
                    self.builder.load_smi(i as i32);
                    self.builder.store_accumulator_in_register(key_reg);
                    self.builder.emit_bytecode(Bytecode::LdaKeyedProperty);
                    self.builder.emit_i8(val_reg.to_operand() as i8);
                    self.builder.emit_i8(key_reg.to_operand() as i8);
                    self.builder.emit_u8(0);
                    self.free_temp(key_reg);

                    self.compile_assignment_to_target(target);
                }

                self.builder.load_accumulator_from_register(val_reg);
                self.free_temp(val_reg);
            }

            Expression::ObjectDestructureAssignment { pairs, value } => {
                self.compile_expression(value);
                let val_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(val_reg);

                for (name, target) in pairs {
                    let pool_idx = self
                        .builder
                        .add_constant(ConstantValue::String(name.clone()));
                    self.builder.emit_bytecode(Bytecode::LdaNamedProperty);
                    self.builder.emit_i8(val_reg.to_operand() as i8);
                    self.builder.emit_u8(pool_idx as u8);

                    self.compile_assignment_to_target(target);
                }

                self.builder.load_accumulator_from_register(val_reg);
                self.free_temp(val_reg);
            }
        }
    }

    fn compile_assignment_to_target(&mut self, target: &Expression) {
        match target {
            Expression::Variable(name) => {
                if let Some(reg) = self.lookup_variable(name) {
                    self.builder.store_accumulator_in_register(reg);
                    if self.is_top_level {
                        let pool_idx = self
                            .builder
                            .add_constant(ConstantValue::String(name.clone()));
                        self.builder.emit_bytecode(Bytecode::StaGlobal);
                        self.builder.emit_u8(pool_idx as u8);
                        self.builder.emit_u8(0);
                    }
                } else {
                    let pool_idx = self
                        .builder
                        .add_constant(ConstantValue::String(name.clone()));
                    self.builder.emit_bytecode(Bytecode::StaGlobal);
                    self.builder.emit_u8(pool_idx as u8);
                    self.builder.emit_u8(0);
                }
            }
            Expression::PropertyAccess { object, property } => {
                let val_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(val_reg);
                self.compile_expression(object);
                let obj_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(obj_reg);
                self.builder.load_accumulator_from_register(val_reg);
                let pool_idx = self
                    .builder
                    .add_constant(ConstantValue::String(property.clone()));
                self.builder.emit_bytecode(Bytecode::StaNamedProperty);
                self.builder.emit_i8(obj_reg.to_operand() as i8);
                self.builder.emit_u8(pool_idx as u8);
                self.builder.emit_u8(0);
                self.free_temp(obj_reg);
                self.free_temp(val_reg);
            }
            Expression::KeyedAccess { object, key } => {
                let val_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(val_reg);
                self.compile_expression(object);
                let obj_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(obj_reg);
                self.compile_expression(key);
                let key_reg = self.allocate_temp();
                self.builder.store_accumulator_in_register(key_reg);
                self.builder.load_accumulator_from_register(val_reg);
                self.builder.emit_bytecode(Bytecode::StaKeyedProperty);
                self.builder.emit_i8(obj_reg.to_operand() as i8);
                self.builder.emit_i8(key_reg.to_operand() as i8);
                self.builder.emit_u8(0);
                self.builder.emit_u8(0);
                self.free_temp(key_reg);
                self.free_temp(obj_reg);
                self.free_temp(val_reg);
            }
            _ => {}
        }
    }

    #[inline(always)]
    fn emit_binary_op(&mut self, op: &BinaryOperator, rhs_reg: Register) {
        match op {
            BinaryOperator::Add => self.builder.add(rhs_reg),
            BinaryOperator::Sub => self.builder.sub(rhs_reg),
            BinaryOperator::Mul => self.builder.mul(rhs_reg),
            BinaryOperator::Div => self.builder.div(rhs_reg),
            BinaryOperator::Mod => self.builder.mod_op(rhs_reg),
            BinaryOperator::Exp => self.builder.exp(rhs_reg),
            BinaryOperator::BitwiseOr => self.builder.bitwise_or(rhs_reg),
            BinaryOperator::BitwiseXor => self.builder.bitwise_xor(rhs_reg),
            BinaryOperator::BitwiseAnd => self.builder.bitwise_and(rhs_reg),
            BinaryOperator::ShiftLeft => self.builder.shift_left(rhs_reg),
            BinaryOperator::ShiftRight => self.builder.shift_right(rhs_reg),
            BinaryOperator::Eq => self.builder.test_equal(rhs_reg),
            BinaryOperator::EqStrict => self.builder.test_equal_strict(rhs_reg),
            BinaryOperator::NotEq => {
                self.builder.test_equal(rhs_reg);
                self.builder.logical_not();
            }
            BinaryOperator::NotEqStrict => {
                self.builder.test_equal_strict(rhs_reg);
                self.builder.logical_not();
            }
            BinaryOperator::LessThan => self.builder.test_less_than(rhs_reg),
            BinaryOperator::GreaterThan => self.builder.test_greater_than(rhs_reg),
            BinaryOperator::LessThanOrEqual => {
                self.builder.test_less_than_or_equal(rhs_reg);
            }
            BinaryOperator::GreaterThanOrEqual => {
                self.builder.test_greater_than_or_equal(rhs_reg);
            }
            BinaryOperator::LogicalAnd => self.builder.test_equal(rhs_reg),
            BinaryOperator::ShiftRightLogical => self.builder.shift_right_logical(rhs_reg),
            BinaryOperator::InstanceOf => self.builder.test_instance_of(rhs_reg),
            BinaryOperator::In => self.builder.test_in(rhs_reg),
            _ => self.builder.add(rhs_reg),
        }
    }

    /// Finalizes compilation, emitting an implicit `Return` if needed, and constructs the BytecodeArray.
    pub fn finish(mut self) -> BytecodeArray {
        self.clear_cached_call_callee();
        if self.builder.is_empty() {
            self.builder.load_undefined();
            self.builder.return_value();
        } else {
            let last_byte = *self.builder.bytecodes().last().unwrap();
            if last_byte != Bytecode::Return.to_byte() {
                self.builder.return_value();
            }
        }

        // Create new array with accurate register count and preserve exception handler table
        let old_array = self.builder.build();
        let mut array = BytecodeArray::with_handlers(
            old_array.bytecodes().to_vec(),
            old_array.constant_pool().to_vec(),
            self.parameter_count,
            self.max_local_count as u32,
            old_array.handler_table().to_vec(),
        );
        array.rest_parameter_index = self.rest_parameter_index;
        array
    }
}

fn collect_loop_invariant_literals(stmt: &Statement, out: &mut Vec<LiteralValue>) {
    match stmt {
        Statement::Block(stmts) => {
            for s in stmts {
                collect_loop_invariant_literals(s, out);
            }
        }
        Statement::Expression(expr) => collect_expr_literals(expr, out),
        Statement::VariableDeclaration { init: Some(expr), .. } => collect_expr_literals(expr, out),
        Statement::If { condition, then_branch, else_branch } => {
            collect_expr_literals(condition, out);
            collect_loop_invariant_literals(then_branch, out);
            if let Some(eb) = else_branch {
                collect_loop_invariant_literals(eb, out);
            }
        }
        Statement::Return(Some(expr)) => collect_expr_literals(expr, out),
        _ => {}
    }
}

fn collect_expr_literals(expr: &Expression, out: &mut Vec<LiteralValue>) {
    match expr {
        Expression::Binary { left, right, .. } => {
            if let Expression::Literal(lit) = right.as_ref() {
                match lit {
                    LiteralValue::Smi(n) if *n < -128 || *n > 127 => {
                        if !out.contains(lit) {
                            out.push(lit.clone());
                        }
                    }
                    LiteralValue::Number(_) => {
                        if !out.contains(lit) {
                            out.push(lit.clone());
                        }
                    }
                    _ => {}
                }
            }
            collect_expr_literals(left, out);
            collect_expr_literals(right, out);
        }
        Expression::Unary { expr, .. } => collect_expr_literals(expr, out),
        Expression::Assignment { value, .. } => collect_expr_literals(value, out),
        Expression::PropertyAssignment { object, value, .. } => {
            collect_expr_literals(object, out);
            collect_expr_literals(value, out);
        }
        Expression::KeyedAssignment { object, key, value } => {
            collect_expr_literals(object, out);
            collect_expr_literals(key, out);
            collect_expr_literals(value, out);
        }
        Expression::Call { callee, arguments } => {
            collect_expr_literals(callee, out);
            for arg in arguments {
                collect_expr_literals(arg, out);
            }
        }
        Expression::ArrayLiteral(elements) => {
            for el in elements {
                collect_expr_literals(el, out);
            }
        }
        Expression::ObjectLiteral(props) => {
            for (_, val) in props {
                collect_expr_literals(val, out);
            }
        }
        _ => {}
    }
}

fn check_expr_has_string(expr: &Expression, out: &mut bool) {
    if *out { return; }
    match expr {
        Expression::Literal(LiteralValue::String(_)) => {
            *out = true;
        }
        Expression::Binary { left, right, .. } => {
            check_expr_has_string(left, out);
            check_expr_has_string(right, out);
        }
        Expression::Unary { op: UnaryOperator::TypeOf, .. } => {
            *out = true;
        }
        Expression::Unary { expr, .. } => {
            check_expr_has_string(expr, out);
        }
        Expression::Call { callee, arguments } => {
            check_expr_has_string(callee, out);
            for a in arguments {
                check_expr_has_string(a, out);
            }
        }
        _ => {}
    }
}

fn check_stmt_returns_string(stmt: &Statement, out: &mut bool) {
    if *out { return; }
    match stmt {
        Statement::Return(Some(expr)) => {
            check_expr_has_string(expr, out);
        }
        Statement::Block(stmts) => {
            for s in stmts {
                check_stmt_returns_string(s, out);
            }
        }
        Statement::If { then_branch, else_branch, .. } => {
            check_stmt_returns_string(then_branch, out);
            if let Some(ref e) = else_branch {
                check_stmt_returns_string(e, out);
            }
        }
        Statement::While { body, .. } | Statement::DoWhile { body, .. } => {
            check_stmt_returns_string(body, out);
        }
        _ => {}
    }
}

fn expr_mutates_var(expr: &Expression, var_name: &str) -> bool {
    match expr {
        Expression::Assignment { target, .. } => target == var_name,
        Expression::Binary { left, right, .. } => {
            expr_mutates_var(left, var_name) || expr_mutates_var(right, var_name)
        }
        Expression::Unary { expr, .. } => expr_mutates_var(expr, var_name),
        Expression::Call { callee, arguments } => {
            expr_mutates_var(callee, var_name) || arguments.iter().any(|a| expr_mutates_var(a, var_name))
        }
        Expression::PropertyAccess { object, .. } => expr_mutates_var(object, var_name),
        Expression::KeyedAccess { object, key } => {
            expr_mutates_var(object, var_name) || expr_mutates_var(key, var_name)
        }
        _ => false,
    }
}

fn body_uses_param_as_string(body: &[Statement], param: &str) -> bool {
    let mut uses_string = false;
    for stmt in body {
        check_stmt_uses_var_as_string(stmt, param, &mut uses_string);
    }
    uses_string
}

fn check_stmt_uses_var_as_string(stmt: &Statement, var: &str, out: &mut bool) {
    if *out { return; }
    match stmt {
        Statement::Return(Some(expr)) | Statement::Expression(expr) => {
            check_expr_uses_var_as_string(expr, var, out);
        }
        Statement::VariableDeclaration { init: Some(expr), .. } => {
            check_expr_uses_var_as_string(expr, var, out);
        }
        Statement::Block(stmts) => {
            for s in stmts {
                check_stmt_uses_var_as_string(s, var, out);
            }
        }
        Statement::If { condition, then_branch, else_branch } => {
            check_expr_uses_var_as_string(condition, var, out);
            check_stmt_uses_var_as_string(then_branch, var, out);
            if let Some(ref e) = else_branch {
                check_stmt_uses_var_as_string(e, var, out);
            }
        }
        Statement::While { condition, body } | Statement::DoWhile { condition, body } => {
            check_expr_uses_var_as_string(condition, var, out);
            check_stmt_uses_var_as_string(body, var, out);
        }
        _ => {}
    }
}

fn check_expr_uses_var_as_string(expr: &Expression, var: &str, out: &mut bool) {
    if *out { return; }
    match expr {
        Expression::Binary { op: BinaryOperator::Add, left, right } => {
            if let Expression::Variable(ref name) = left.as_ref() {
                if name == var {
                    if let Expression::Literal(LiteralValue::String(_)) = right.as_ref() {
                        *out = true;
                    }
                }
            }
            if let Expression::Variable(ref name) = right.as_ref() {
                if name == var {
                    if let Expression::Literal(LiteralValue::String(_)) = left.as_ref() {
                        *out = true;
                    }
                }
            }
            check_expr_uses_var_as_string(left, var, out);
            check_expr_uses_var_as_string(right, var, out);
        }
        Expression::Binary { left, right, .. } => {
            check_expr_uses_var_as_string(left, var, out);
            check_expr_uses_var_as_string(right, var, out);
        }
        Expression::Unary { expr, .. } => check_expr_uses_var_as_string(expr, var, out),
        Expression::Call { callee, arguments } => {
            check_expr_uses_var_as_string(callee, var, out);
            for a in arguments {
                check_expr_uses_var_as_string(a, var, out);
            }
        }
        Expression::PropertyAccess { object, property } => {
            if let Expression::Variable(ref name) = object.as_ref() {
                if name == var && (property == "substring" || property == "slice" || property == "charAt") {
                    *out = true;
                }
            }
            check_expr_uses_var_as_string(object, var, out);
        }
        Expression::KeyedAccess { object, key } => {
            check_expr_uses_var_as_string(object, var, out);
            check_expr_uses_var_as_string(key, var, out);
        }
        _ => {}
    }
}
