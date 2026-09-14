//! Safe Rust reimplementation of Google V8's Abstract Syntax Tree (`src/ast/ast.h`).
//!
//! Provides the AST node hierarchy representing parsed ECMAScript programs:
//! expressions, statements, declarations, and a visitor pattern for tree traversals.

use std::fmt;

/// Literal values representable in AST literal nodes.
#[derive(Clone, Debug, PartialEq)]
pub enum LiteralValue {
    Smi(i32),
    Number(f64),
    String(String),
    Boolean(bool),
    BigInt(String),
    Null,
    Undefined,
}

impl fmt::Display for LiteralValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiteralValue::Smi(val) => write!(f, "{}", val),
            LiteralValue::Number(val) => write!(f, "{}", val),
            LiteralValue::String(val) => write!(f, "\"{}\"", val),
            LiteralValue::Boolean(val) => write!(f, "{}", val),
            LiteralValue::BigInt(val) => write!(f, "{}n", val),
            LiteralValue::Null => write!(f, "null"),
            LiteralValue::Undefined => write!(f, "undefined"),
        }
    }
}

/// Binary operators in AST expressions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
    BitwiseOr,
    BitwiseXor,
    BitwiseAnd,
    ShiftLeft,
    ShiftRight,
    ShiftRightLogical,
    Eq,
    EqStrict,
    NotEq,
    NotEqStrict,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
    LogicalAnd,
    LogicalOr,
    NullishCoalescing,
    InstanceOf,
    In,
}

impl BinaryOperator {
    pub fn symbol(&self) -> &'static str {
        match self {
            BinaryOperator::Add => "+",
            BinaryOperator::Sub => "-",
            BinaryOperator::Mul => "*",
            BinaryOperator::Div => "/",
            BinaryOperator::Mod => "%",
            BinaryOperator::Exp => "**",
            BinaryOperator::BitwiseOr => "|",
            BinaryOperator::BitwiseXor => "^",
            BinaryOperator::BitwiseAnd => "&",
            BinaryOperator::ShiftLeft => "<<",
            BinaryOperator::ShiftRight => ">>",
            BinaryOperator::ShiftRightLogical => ">>>",
            BinaryOperator::Eq => "==",
            BinaryOperator::EqStrict => "===",
            BinaryOperator::NotEq => "!=",
            BinaryOperator::NotEqStrict => "!==",
            BinaryOperator::LessThan => "<",
            BinaryOperator::GreaterThan => ">",
            BinaryOperator::LessThanOrEqual => "<=",
            BinaryOperator::GreaterThanOrEqual => ">=",
            BinaryOperator::LogicalAnd => "&&",
            BinaryOperator::LogicalOr => "||",
            BinaryOperator::NullishCoalescing => "??",
            BinaryOperator::InstanceOf => "instanceof",
            BinaryOperator::In => "in",
        }
    }
}

/// Unary operators in AST expressions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOperator {
    Plus,
    Minus,
    Not,
    BitwiseNot,
    TypeOf,
    Void,
    Delete,
}

impl UnaryOperator {
    pub fn symbol(&self) -> &'static str {
        match self {
            UnaryOperator::Plus => "+",
            UnaryOperator::Minus => "-",
            UnaryOperator::Not => "!",
            UnaryOperator::BitwiseNot => "~",
            UnaryOperator::TypeOf => "typeof ",
            UnaryOperator::Void => "void ",
            UnaryOperator::Delete => "delete ",
        }
    }
}

/// Abstract Syntax Tree expressions.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    Literal(LiteralValue),
    Variable(String),
    Binary {
        op: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Unary {
        op: UnaryOperator,
        expr: Box<Expression>,
    },
    Assignment {
        target: String,
        value: Box<Expression>,
    },
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },
    PropertyAccess {
        object: Box<Expression>,
        property: String,
    },
    KeyedAccess {
        object: Box<Expression>,
        key: Box<Expression>,
    },
    PropertyAssignment {
        object: Box<Expression>,
        property: String,
        value: Box<Expression>,
    },
    KeyedAssignment {
        object: Box<Expression>,
        key: Box<Expression>,
        value: Box<Expression>,
    },
    ObjectLiteral(Vec<(String, Expression)>),
    ArrayLiteral(Vec<Expression>),
    Await(Box<Expression>),
    This,
    Super,
    New {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },
    RegExpLiteral {
        pattern: String,
        flags: String,
    },
    Spread(Box<Expression>),
    OptionalPropertyAccess {
        object: Box<Expression>,
        property: String,
    },
    OptionalKeyedAccess {
        object: Box<Expression>,
        key: Box<Expression>,
    },
    OptionalCall {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },
    Yield {
        value: Option<Box<Expression>>,
        delegate: bool,
    },
    DynamicImport(Box<Expression>),
    Conditional {
        condition: Box<Expression>,
        then_expr: Box<Expression>,
        else_expr: Box<Expression>,
    },
    NewTarget,
    ImportMeta,
    ArrayDestructureAssignment {
        targets: Vec<Expression>,
        value: Box<Expression>,
    },
    ObjectDestructureAssignment {
        pairs: Vec<(String, Expression)>,
        value: Box<Expression>,
    },
    ComputedProperty {
        key: Box<Expression>,
        value: Box<Expression>,
    },
}

/// An imported binding inside an `import` declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportSpecifier {
    pub imported: String,
    pub local: String,
}

/// An exported binding inside an `export` declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportSpecifier {
    pub local: String,
    pub exported: String,
}

/// Abstract Syntax Tree statements.
#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    Block(Vec<Statement>),
    Expression(Expression),
    VariableDeclaration {
        name: String,
        init: Option<Expression>,
        is_const: bool,
    },
    UsingDeclaration {
        name: String,
        init: Expression,
        is_await: bool,
    },
    If {
        condition: Expression,
        then_branch: Box<Statement>,
        else_branch: Option<Box<Statement>>,
    },
    While {
        condition: Expression,
        body: Box<Statement>,
    },
    For {
        init: Option<Box<Statement>>,
        condition: Option<Expression>,
        update: Option<Expression>,
        body: Box<Statement>,
    },
    ForIn {
        var_name: String,
        is_decl: bool,
        object: Expression,
        body: Box<Statement>,
    },
    ForOf {
        var_name: String,
        is_decl: bool,
        iterable: Expression,
        body: Box<Statement>,
        is_await: bool,
    },
    DoWhile {
        body: Box<Statement>,
        condition: Expression,
    },
    Switch {
        discriminant: Expression,
        cases: Vec<SwitchCase>,
    },
    Break(Option<String>),
    Continue(Option<String>),
    Debugger,
    With {
        object: Expression,
        body: Box<Statement>,
    },
    Labeled {
        label: String,
        body: Box<Statement>,
    },
    TryCatch {
        try_block: Box<Statement>,
        catch_param: Option<String>,
        catch_block: Option<Box<Statement>>,
        finally_block: Option<Box<Statement>>,
    },
    Throw(Expression),
    Return(Option<Expression>),
    FunctionDeclaration {
        name: String,
        params: Vec<String>,
        body: Vec<Statement>,
        is_async: bool,
        is_generator: bool,
    },
    ClassDeclaration {
        name: String,
        super_class: Option<String>,
        constructor: Option<ClassMethod>,
        methods: Vec<ClassMethod>,
        fields: Vec<ClassField>,
        static_blocks: Vec<ClassStaticBlock>,
    },
    ImportDeclaration {
        specifier: String,
        specifiers: Vec<ImportSpecifier>,
    },
    ExportDeclaration {
        specifier: Option<String>,
        specifiers: Vec<ExportSpecifier>,
        declaration: Option<Box<Statement>>,
        is_default: bool,
    },
    Empty,
}

/// A field definition inside an ECMAScript class declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassField {
    pub name: String,
    pub initializer: Option<Expression>,
    pub is_static: bool,
}

/// A static initialization block inside an ECMAScript class declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassStaticBlock {
    pub body: Vec<Statement>,
}

/// A method definition inside an ECMAScript class declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassMethod {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Statement>,
    pub is_static: bool,
    pub is_getter: bool,
    pub is_setter: bool,
}

/// A case or default clause inside a `switch` statement.
#[derive(Clone, Debug, PartialEq)]
pub struct SwitchCase {
    pub test: Option<Expression>,
    pub statements: Vec<Statement>,
}

/// Root node of an AST representing an ECMAScript script or module.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub statements: Vec<Statement>,
}

impl Program {
    pub fn new(statements: Vec<Statement>) -> Self {
        Self { statements }
    }

    pub fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }
}

/// Visitor pattern interface for traversing AST structures.
pub trait AstVisitor {
    fn visit_program(&mut self, program: &Program) {
        for stmt in &program.statements {
            self.visit_statement(stmt);
        }
    }

    fn visit_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Block(stmts) => {
                for s in stmts {
                    self.visit_statement(s);
                }
            }
            Statement::Expression(expr) => self.visit_expression(expr),
            Statement::VariableDeclaration { init, .. } => {
                if let Some(init_expr) = init {
                    self.visit_expression(init_expr);
                }
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.visit_expression(condition);
                self.visit_statement(then_branch);
                if let Some(else_stmt) = else_branch {
                    self.visit_statement(else_stmt);
                }
            }
            Statement::While { condition, body } => {
                self.visit_expression(condition);
                self.visit_statement(body);
            }
            Statement::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(i) = init {
                    self.visit_statement(i);
                }
                if let Some(c) = condition {
                    self.visit_expression(c);
                }
                if let Some(u) = update {
                    self.visit_expression(u);
                }
                self.visit_statement(body);
            }
            Statement::ForIn { object, body, .. } => {
                self.visit_expression(object);
                self.visit_statement(body);
            }
            Statement::ForOf { iterable, body, .. } => {
                self.visit_expression(iterable);
                self.visit_statement(body);
            }
            Statement::DoWhile { body, condition } => {
                self.visit_statement(body);
                self.visit_expression(condition);
            }
            Statement::Switch { discriminant, cases } => {
                self.visit_expression(discriminant);
                for c in cases {
                    if let Some(t) = &c.test {
                        self.visit_expression(t);
                    }
                    for s in &c.statements {
                        self.visit_statement(s);
                    }
                }
            }
            Statement::Break(_) | Statement::Continue(_) | Statement::Debugger => {}
            Statement::With { object, body } => {
                self.visit_expression(object);
                self.visit_statement(body);
            }
            Statement::UsingDeclaration { init, .. } => {
                self.visit_expression(init);
            }
            Statement::Labeled { body, .. } => {
                self.visit_statement(body);
            }
            Statement::TryCatch {
                try_block,
                catch_block,
                finally_block,
                ..
            } => {
                self.visit_statement(try_block);
                if let Some(cb) = catch_block {
                    self.visit_statement(cb);
                }
                if let Some(fb) = finally_block {
                    self.visit_statement(fb);
                }
            }
            Statement::Throw(expr) => {
                self.visit_expression(expr);
            }
            Statement::Return(opt_expr) => {
                if let Some(expr) = opt_expr {
                    self.visit_expression(expr);
                }
            }
            Statement::FunctionDeclaration { body, .. } => {
                for s in body {
                    self.visit_statement(s);
                }
            }
            Statement::ClassDeclaration {
                constructor,
                methods,
                fields,
                static_blocks,
                ..
            } => {
                if let Some(ctor) = constructor {
                    for s in &ctor.body {
                        self.visit_statement(s);
                    }
                }
                for m in methods {
                    for s in &m.body {
                        self.visit_statement(s);
                    }
                }
                for f in fields {
                    if let Some(ref init) = f.initializer {
                        self.visit_expression(init);
                    }
                }
                for b in static_blocks {
                    for s in &b.body {
                        self.visit_statement(s);
                    }
                }
            }
            Statement::ImportDeclaration { .. } => {}
            Statement::ExportDeclaration { declaration, .. } => {
                if let Some(decl) = declaration {
                    self.visit_statement(decl);
                }
            }
            Statement::Empty => {}
        }
    }

    fn visit_expression(&mut self, expr: &Expression) {
        match expr {
            Expression::Literal(_)
            | Expression::Variable(_)
            | Expression::This
            | Expression::Super
            | Expression::NewTarget
            | Expression::ImportMeta
            | Expression::RegExpLiteral { .. } => {}
            Expression::Conditional { condition, then_expr, else_expr } => {
                self.visit_expression(condition);
                self.visit_expression(then_expr);
                self.visit_expression(else_expr);
            }
            Expression::ArrayDestructureAssignment { targets, value } => {
                for t in targets {
                    self.visit_expression(t);
                }
                self.visit_expression(value);
            }
            Expression::ObjectDestructureAssignment { pairs, value } => {
                for (_, val) in pairs {
                    self.visit_expression(val);
                }
                self.visit_expression(value);
            }
            Expression::Binary { left, right, .. } => {
                self.visit_expression(left);
                self.visit_expression(right);
            }
            Expression::Unary { expr, .. } => {
                self.visit_expression(expr);
            }
            Expression::Assignment { value, .. } => {
                self.visit_expression(value);
            }
            Expression::Call { callee, arguments } => {
                self.visit_expression(callee);
                for arg in arguments {
                    self.visit_expression(arg);
                }
            }
            Expression::PropertyAccess { object, .. } => {
                self.visit_expression(object);
            }
            Expression::KeyedAccess { object, key } => {
                self.visit_expression(object);
                self.visit_expression(key);
            }
            Expression::PropertyAssignment { object, value, .. } => {
                self.visit_expression(object);
                self.visit_expression(value);
            }
            Expression::KeyedAssignment { object, key, value } => {
                self.visit_expression(object);
                self.visit_expression(key);
                self.visit_expression(value);
            }
            Expression::ObjectLiteral(props) => {
                for (_, val) in props {
                    self.visit_expression(val);
                }
            }
            Expression::ArrayLiteral(elements) => {
                for elem in elements {
                    self.visit_expression(elem);
                }
            }
            Expression::Await(expr) => {
                self.visit_expression(expr);
            }
            Expression::New { callee, arguments } => {
                self.visit_expression(callee);
                for arg in arguments {
                    self.visit_expression(arg);
                }
            }
            Expression::Spread(expr) => {
                self.visit_expression(expr);
            }
            Expression::OptionalPropertyAccess { object, .. } => {
                self.visit_expression(object);
            }
            Expression::OptionalKeyedAccess { object, key } => {
                self.visit_expression(object);
                self.visit_expression(key);
            }
            Expression::OptionalCall { callee, arguments } => {
                self.visit_expression(callee);
                for arg in arguments {
                    self.visit_expression(arg);
                }
            }
            Expression::Yield { value, .. } => {
                if let Some(v) = value {
                    self.visit_expression(v);
                }
            }
            Expression::DynamicImport(expr) => {
                self.visit_expression(expr);
            }
            Expression::ComputedProperty { key, value } => {
                self.visit_expression(key);
                self.visit_expression(value);
            }
        }
    }
}
