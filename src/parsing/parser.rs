//! Safe Rust reimplementation of Google V8's Parser (`src/parsing/parser.h`).
//!
//! Provides a recursive-descent and precedence-climbing (Pratt) parser that turns
//! ECMAScript source code tokens into an Abstract Syntax Tree (AST).

use super::scanner::{ScannedToken, Scanner};
use super::token::Token;
use crate::ast::*;
use std::fmt;

/// An error encountered during parsing with source character offset.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SyntaxError at position {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Recursive-descent and Pratt parser for ECMAScript syntax.
pub struct Parser<'a> {
    scanner: Scanner<'a>,
    current_token: ScannedToken,
    peeked_token: Option<ScannedToken>,
    pub anon_counter: usize,
    pub anon_functions: Vec<Statement>,
    pub enclosing_private_names: Vec<std::collections::HashSet<String>>,
}

impl<'a> Parser<'a> {
    /// Creates a new Parser initialized with the given source string.
    pub fn new(source: &'a str) -> Self {
        let mut scanner = Scanner::new(source);
        let current_token = scanner.next_token();
        Self {
            scanner,
            current_token,
            peeked_token: None,
            anon_counter: 0,
            anon_functions: Vec::new(),
            enclosing_private_names: Vec::new(),
        }
    }

    /// Returns a reference to the current token.
    #[inline]
    pub fn current(&self) -> &ScannedToken {
        &self.current_token
    }

    /// Peeks at the next token ahead in the stream.
    pub fn peek_token(&mut self) -> &ScannedToken {
        if self.peeked_token.is_none() {
            self.peeked_token = Some(self.scanner.next_token());
        }
        self.peeked_token.as_ref().unwrap()
    }

    /// Advances the scanner to the next token and returns the previous token.
    pub fn advance(&mut self) -> ScannedToken {
        let next = if let Some(peeked) = self.peeked_token.take() {
            peeked
        } else {
            self.scanner.next_token()
        };
        std::mem::replace(&mut self.current_token, next)
    }

    /// Consumes the expected token or returns a syntax error.
    pub fn consume(&mut self, expected: Token) -> Result<ScannedToken, ParseError> {
        if self.current().token == expected {
            Ok(self.advance())
        } else {
            Err(ParseError {
                message: format!(
                    "Unexpected token {:?}, expected {:?}",
                    self.current().token,
                    expected
                ),
                offset: self.current().start,
            })
        }
    }

    /// Parses a property identifier, allowing standard identifiers, keywords, strings, or numbers.
    pub fn parse_property_name(&mut self) -> Result<String, ParseError> {
        let tok = self.current().clone();
        if tok.token == Token::Identifier || tok.token.is_keyword() || tok.token == Token::String || tok.token == Token::Number {
            self.advance();
            if tok.literal.starts_with('#') {
                let in_scope = self.enclosing_private_names.iter().any(|set| set.contains(&tok.literal));
                if !in_scope {
                    return Err(ParseError {
                        message: format!("SyntaxError: Private field '{}' must be declared in an enclosing class", tok.literal),
                        offset: tok.start,
                    });
                }
            }
            Ok(tok.literal)
        } else {
            Err(ParseError {
                message: format!("Expected property identifier, got {:?}", tok.token),
                offset: tok.start,
            })
        }
    }

    /// Matches and consumes the token if it matches `expected`.
    pub fn match_token(&mut self, expected: Token) -> bool {
        if self.current().token == expected {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Consumes a semicolon or applies Automatic Semicolon Insertion (ASI).
    pub fn consume_semicolon(&mut self) {
        if self.current().token == Token::Semicolon {
            self.advance();
        } else if self.current().token == Token::RightBrace || self.current().token == Token::Eos {
            // Semicolon insertion rule
        }
    }

    /// Parses an entire ECMAScript program.
    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut statements = Vec::new();
        while self.current().token != Token::Eos {
            statements.push(self.parse_statement()?);
        }
        let mut all_stmts = std::mem::take(&mut self.anon_functions);
        all_stmts.extend(statements);
        let all_stmts = self.desugar_using_in_block(all_stmts);
        Ok(Program::new(all_stmts))
    }

    /// Parses a single statement.
    pub fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        if self.current().token == Token::Identifier && self.peek_token().token == Token::Colon {
            let label = self.advance().literal;
            self.advance(); // consume ':'
            let body = self.parse_statement()?;
            return Ok(Statement::Labeled {
                label,
                body: Box::new(body),
            });
        }

        match self.current().token {
            Token::LeftBrace => self.parse_block(),
            Token::Var | Token::Let | Token::Const => self.parse_variable_declaration(),
            Token::Using => self.parse_using_declaration(false),
            Token::Await => {
                if self.peek_token().token == Token::Using {
                    self.advance(); // consume Token::Await
                    self.parse_using_declaration(true)
                } else {
                    self.parse_expression_statement()
                }
            }
            Token::Class => self.parse_class_declaration(),
            Token::Function => self.parse_function_declaration(false),
            Token::Async => {
                if self.peek_token().token == Token::Function {
                    self.advance(); // consume Token::Async
                    self.parse_function_declaration(true)
                } else {
                    self.parse_expression_statement()
                }
            }
            Token::If => self.parse_if_statement(),
            Token::While => self.parse_while_statement(),
            Token::Do => self.parse_do_while_statement(),
            Token::For => self.parse_for_statement(),
            Token::Switch => self.parse_switch_statement(),
            Token::Break => self.parse_break_statement(),
            Token::Continue => self.parse_continue_statement(),
            Token::Debugger => self.parse_debugger_statement(),
            Token::With => self.parse_with_statement(),
            Token::Try => self.parse_try_statement(),
            Token::Throw => self.parse_throw_statement(),
            Token::Return => self.parse_return_statement(),
            Token::Import => {
                if self.peek_token().token == Token::LeftParen {
                    self.parse_expression_statement()
                } else {
                    self.parse_import_statement()
                }
            }
            Token::Export => self.parse_export_statement(),
            Token::Semicolon => {
                self.advance();
                Ok(Statement::Empty)
            }
            _ => self.parse_expression_statement(),
        }
    }

    /// Parses a `{ ... }` block statement.
    pub fn parse_block(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::LeftBrace)?;
        let mut stmts = Vec::new();
        while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
            stmts.push(self.parse_statement()?);
        }
        self.consume(Token::RightBrace)?;
        let stmts = self.desugar_using_in_block(stmts);
        Ok(Statement::Block(stmts))
    }

    /// Parses variable declarations (`let x = 10;`, `const y = 20;`, `var z;`).
    pub fn parse_variable_declaration(&mut self) -> Result<Statement, ParseError> {
        let kw = self.advance();
        let is_const = kw.token == Token::Const;

        if self.current().token == Token::LeftBracket {
            // Array destructuring: let [a, b, ...rest] = expr;
            self.advance();
            let mut items = Vec::new();
            while self.current().token != Token::RightBracket && self.current().token != Token::Eos {
                if self.match_token(Token::Ellipsis) {
                    let rest_name = self.consume(Token::Identifier)?.literal;
                    items.push((format!("...{}", rest_name), None));
                    break;
                }
                let name = self.consume(Token::Identifier)?.literal;
                let def = if self.match_token(Token::Assign) {
                    Some(self.parse_precedence(2)?)
                } else {
                    None
                };
                items.push((name, def));
                if !self.match_token(Token::Comma) {
                    break;
                }
            }
            self.consume(Token::RightBracket)?;
            self.consume(Token::Assign)?;
            let init = self.parse_expression()?;
            self.consume_semicolon();

            self.anon_counter += 1;
            let tmp_var = format!("__destruct_arr_{}__", self.anon_counter);
            let mut stmts = Vec::new();
            stmts.push(Statement::VariableDeclaration {
                name: tmp_var.clone(),
                init: Some(init),
                is_const: true,
            });
            for (idx, (item_name, default_expr)) in items.into_iter().enumerate() {
                if item_name.starts_with("...") {
                    let rest_name = item_name[3..].to_string();
                    stmts.push(Statement::VariableDeclaration {
                        name: rest_name,
                        init: Some(Expression::Call {
                            callee: Box::new(Expression::PropertyAccess {
                                object: Box::new(Expression::Variable(tmp_var.clone())),
                                property: "slice".to_string(),
                            }),
                            arguments: vec![Expression::Literal(LiteralValue::Smi(idx as i32))],
                        }),
                        is_const,
                    });
                } else {
                    stmts.push(Statement::VariableDeclaration {
                        name: item_name.clone(),
                        init: Some(Expression::KeyedAccess {
                            object: Box::new(Expression::Variable(tmp_var.clone())),
                            key: Box::new(Expression::Literal(LiteralValue::Smi(idx as i32))),
                        }),
                        is_const,
                    });
                    if let Some(def) = default_expr {
                        stmts.push(Statement::If {
                            condition: Expression::Binary {
                                op: BinaryOperator::EqStrict,
                                left: Box::new(Expression::Variable(item_name.clone())),
                                right: Box::new(Expression::Literal(LiteralValue::Undefined)),
                            },
                            then_branch: Box::new(Statement::Expression(Expression::Assignment {
                                target: item_name,
                                value: Box::new(def),
                            })),
                            else_branch: None,
                        });
                    }
                }
            }
            return Ok(Statement::Block(stmts));
        } else if self.current().token == Token::LeftBrace {
            // Object destructuring: let { x, y: new_y, z = 5 } = expr;
            self.advance();
            let mut props = Vec::new();
            while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
                let key = self.parse_property_name()?;
                let (target_var, default_expr) = if self.match_token(Token::Colon) {
                    let target = self.consume(Token::Identifier)?.literal;
                    let def = if self.match_token(Token::Assign) {
                        Some(self.parse_precedence(2)?)
                    } else {
                        None
                    };
                    (target, def)
                } else {
                    let def = if self.match_token(Token::Assign) {
                        Some(self.parse_precedence(2)?)
                    } else {
                        None
                    };
                    (key.clone(), def)
                };
                props.push((key, target_var, default_expr));
                if !self.match_token(Token::Comma) {
                    break;
                }
            }
            self.consume(Token::RightBrace)?;
            self.consume(Token::Assign)?;
            let init = self.parse_expression()?;
            self.consume_semicolon();

            self.anon_counter += 1;
            let tmp_var = format!("__destruct_obj_{}__", self.anon_counter);
            let mut stmts = Vec::new();
            stmts.push(Statement::VariableDeclaration {
                name: tmp_var.clone(),
                init: Some(init),
                is_const: true,
            });
            for (prop_name, target_var, default_expr) in props {
                stmts.push(Statement::VariableDeclaration {
                    name: target_var.clone(),
                    init: Some(Expression::PropertyAccess {
                        object: Box::new(Expression::Variable(tmp_var.clone())),
                        property: prop_name,
                    }),
                    is_const,
                });
                if let Some(def) = default_expr {
                    stmts.push(Statement::If {
                        condition: Expression::Binary {
                            op: BinaryOperator::EqStrict,
                            left: Box::new(Expression::Variable(target_var.clone())),
                            right: Box::new(Expression::Literal(LiteralValue::Undefined)),
                        },
                        then_branch: Box::new(Statement::Expression(Expression::Assignment {
                            target: target_var,
                            value: Box::new(def),
                        })),
                        else_branch: None,
                    });
                }
            }
            return Ok(Statement::Block(stmts));
        }

        let ident = self.consume(Token::Identifier)?;
        let init = if self.match_token(Token::Assign) {
            Some(self.parse_expression()?)
        } else {
            None
        };

        self.consume_semicolon();
        Ok(Statement::VariableDeclaration {
            name: ident.literal,
            init,
            is_const,
        })
    }

    /// Parses `using x = expr;` or `await using x = expr;`.
    pub fn parse_using_declaration(&mut self, is_await: bool) -> Result<Statement, ParseError> {
        self.consume(Token::Using)?;
        let mut stmts = Vec::new();
        loop {
            let ident = self.consume(Token::Identifier)?;
            self.consume(Token::Assign)?;
            let init = self.parse_expression()?;
            stmts.push(Statement::UsingDeclaration {
                name: ident.literal,
                init,
                is_await,
            });
            if !self.match_token(Token::Comma) {
                break;
            }
        }
        self.consume_semicolon();
        if stmts.len() == 1 {
            Ok(stmts.pop().unwrap())
        } else {
            Ok(Statement::Block(stmts))
        }
    }

    /// Desugars `using` and `await using` declarations in a list of statements into an implicit DisposableStack
    /// guarded by a `try ... finally { stack.dispose() }` block.
    pub fn desugar_using_in_block(&mut self, stmts: Vec<Statement>) -> Vec<Statement> {
        let has_using = stmts.iter().any(|s| match s {
            Statement::UsingDeclaration { .. } => true,
            Statement::Block(inner) => inner.iter().any(|is| matches!(is, Statement::UsingDeclaration { .. })),
            _ => false,
        });

        if !has_using {
            return stmts;
        }

        // Flatten any immediate nested blocks from comma-separated using
        let mut flat_stmts = Vec::new();
        for s in stmts {
            if let Statement::Block(inner) = s {
                if inner.iter().any(|is| matches!(is, Statement::UsingDeclaration { .. })) {
                    flat_stmts.extend(inner);
                    continue;
                }
                flat_stmts.push(Statement::Block(inner));
            } else {
                flat_stmts.push(s);
            }
        }

        let first_idx = match flat_stmts.iter().position(|s| matches!(s, Statement::UsingDeclaration { .. })) {
            Some(idx) => idx,
            None => return flat_stmts,
        };

        let mut result = flat_stmts[..first_idx].to_vec();
        let rest = &flat_stmts[first_idx..];

        let has_await = rest.iter().any(|s| matches!(s, Statement::UsingDeclaration { is_await: true, .. }));

        self.anon_counter += 1;
        let stack_var = format!("__using_stack_{}__", self.anon_counter);

        let stack_ctor_name = if has_await {
            "AsyncDisposableStack"
        } else {
            "DisposableStack"
        };

        let stack_decl = Statement::VariableDeclaration {
            name: stack_var.clone(),
            init: Some(Expression::New {
                callee: Box::new(Expression::Variable(stack_ctor_name.to_string())),
                arguments: Vec::new(),
            }),
            is_const: true,
        };
        result.push(stack_decl);

        let mut try_body = Vec::new();
        for s in rest {
            match s {
                Statement::UsingDeclaration { name, init, .. } => {
                    let wrapped_init = Expression::Call {
                        callee: Box::new(Expression::PropertyAccess {
                            object: Box::new(Expression::Variable(stack_var.clone())),
                            property: "use".to_string(),
                        }),
                        arguments: vec![init.clone()],
                    };
                    try_body.push(Statement::VariableDeclaration {
                        name: name.clone(),
                        init: Some(wrapped_init),
                        is_const: true,
                    });
                }
                other => {
                    try_body.push(other.clone());
                }
            }
        }

        let dispose_prop = if has_await { "disposeAsync" } else { "dispose" };
        let dispose_call = Expression::Call {
            callee: Box::new(Expression::PropertyAccess {
                object: Box::new(Expression::Variable(stack_var)),
                property: dispose_prop.to_string(),
            }),
            arguments: Vec::new(),
        };

        let finally_expr = if has_await {
            Expression::Await(Box::new(dispose_call))
        } else {
            dispose_call
        };

        let try_catch = Statement::TryCatch {
            try_block: Box::new(Statement::Block(try_body)),
            catch_param: None,
            catch_block: None,
            finally_block: Some(Box::new(Statement::Expression(finally_expr))),
        };
        result.push(try_catch);

        result
    }

    /// Parses `if (cond) stmt else stmt`.
    pub fn parse_if_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::If)?;
        self.consume(Token::LeftParen)?;
        let condition = self.parse_expression()?;
        self.consume(Token::RightParen)?;

        let then_branch = Box::new(self.parse_statement()?);
        let else_branch = if self.match_token(Token::Else) {
            Some(Box::new(self.parse_statement()?))
        } else {
            None
        };

        Ok(Statement::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    /// Parses `while (cond) stmt`.
    pub fn parse_while_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::While)?;
        self.consume(Token::LeftParen)?;
        let condition = self.parse_expression()?;
        self.consume(Token::RightParen)?;

        let body = Box::new(self.parse_statement()?);
        Ok(Statement::While { condition, body })
    }

    /// Parses `do stmt while (cond);`.
    pub fn parse_do_while_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::Do)?;
        let body = Box::new(self.parse_statement()?);
        self.consume(Token::While)?;
        self.consume(Token::LeftParen)?;
        let condition = self.parse_expression()?;
        self.consume(Token::RightParen)?;
        self.consume_semicolon();
        Ok(Statement::DoWhile { body, condition })
    }

    /// Parses `for (init; cond; update) stmt`, `for (var in obj) stmt`, or `for (var of iterable) stmt`.
    pub fn parse_for_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::For)?;
        let is_await = self.match_token(Token::Await);
        self.consume(Token::LeftParen)?;

        if matches!(self.current().token, Token::Var | Token::Let | Token::Const) {
            let kw = self.advance();
            let is_const = kw.token == Token::Const;
            let ident = self.consume(Token::Identifier)?.literal;

            if self.match_token(Token::In) {
                let object = self.parse_expression()?;
                self.consume(Token::RightParen)?;
                let body = Box::new(self.parse_statement()?);
                return Ok(Statement::ForIn {
                    var_name: ident,
                    is_decl: true,
                    object,
                    body,
                });
            }

            if self.match_token(Token::Of) {
                let iterable = self.parse_expression()?;
                self.consume(Token::RightParen)?;
                let body = Box::new(self.parse_statement()?);
                return Ok(Statement::ForOf {
                    var_name: ident,
                    is_decl: true,
                    iterable,
                    body,
                    is_await,
                });
            }

            let init = if self.match_token(Token::Assign) {
                Some(self.parse_expression()?)
            } else {
                None
            };
            self.consume(Token::Semicolon)?;
            let init_stmt = Some(Box::new(Statement::VariableDeclaration {
                name: ident,
                init,
                is_const,
            }));

            let condition = if self.current().token != Token::Semicolon {
                Some(self.parse_expression()?)
            } else {
                None
            };
            self.consume(Token::Semicolon)?;

            let update = if self.current().token != Token::RightParen {
                Some(self.parse_expression()?)
            } else {
                None
            };
            self.consume(Token::RightParen)?;

            let body = Box::new(self.parse_statement()?);
            return Ok(Statement::For {
                init: init_stmt,
                condition,
                update,
                body,
            });
        }

        let init_stmt = if self.match_token(Token::Semicolon) {
            None
        } else {
            let expr = self.parse_expression()?;
            if let Expression::Variable(ref ident) = expr {
                if self.match_token(Token::In) {
                    let object = self.parse_expression()?;
                    self.consume(Token::RightParen)?;
                    let body = Box::new(self.parse_statement()?);
                    return Ok(Statement::ForIn {
                        var_name: ident.clone(),
                        is_decl: false,
                        object,
                        body,
                    });
                }
                if self.match_token(Token::Of) {
                    let iterable = self.parse_expression()?;
                    self.consume(Token::RightParen)?;
                    let body = Box::new(self.parse_statement()?);
                    return Ok(Statement::ForOf {
                        var_name: ident.clone(),
                        is_decl: false,
                        iterable,
                        body,
                        is_await,
                    });
                }
            }
            self.consume(Token::Semicolon)?;
            Some(Box::new(Statement::Expression(expr)))
        };

        let condition = if self.current().token != Token::Semicolon {
            Some(self.parse_expression()?)
        } else {
            None
        };
        self.consume(Token::Semicolon)?;

        let update = if self.current().token != Token::RightParen {
            Some(self.parse_expression()?)
        } else {
            None
        };
        self.consume(Token::RightParen)?;

        let body = Box::new(self.parse_statement()?);
        Ok(Statement::For {
            init: init_stmt,
            condition,
            update,
            body,
        })
    }

    /// Parses `switch (expr) { case c: ... default: ... }`.
    pub fn parse_switch_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::Switch)?;
        self.consume(Token::LeftParen)?;
        let discriminant = self.parse_expression()?;
        self.consume(Token::RightParen)?;
        self.consume(Token::LeftBrace)?;

        let mut cases = Vec::new();
        while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
            if self.match_token(Token::Case) {
                let test = Some(self.parse_expression()?);
                self.consume(Token::Colon)?;
                let mut statements = Vec::new();
                while !matches!(
                    self.current().token,
                    Token::Case | Token::Default | Token::RightBrace | Token::Eos
                ) {
                    statements.push(self.parse_statement()?);
                }
                cases.push(SwitchCase { test, statements });
            } else if self.match_token(Token::Default) {
                self.consume(Token::Colon)?;
                let mut statements = Vec::new();
                while !matches!(
                    self.current().token,
                    Token::Case | Token::Default | Token::RightBrace | Token::Eos
                ) {
                    statements.push(self.parse_statement()?);
                }
                cases.push(SwitchCase {
                    test: None,
                    statements,
                });
            } else {
                return Err(ParseError {
                    message: format!("Expected 'case' or 'default', found {:?}", self.current().token),
                    offset: self.current().start,
                });
            }
        }
        self.consume(Token::RightBrace)?;
        Ok(Statement::Switch { discriminant, cases })
    }

    /// Parses `break [label];`.
    pub fn parse_break_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::Break)?;
        let label = if self.current().token == Token::Identifier {
            Some(self.advance().literal)
        } else {
            None
        };
        self.consume_semicolon();
        Ok(Statement::Break(label))
    }

    /// Parses `continue [label];`.
    pub fn parse_continue_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::Continue)?;
        let label = if self.current().token == Token::Identifier {
            Some(self.advance().literal)
        } else {
            None
        };
        self.consume_semicolon();
        Ok(Statement::Continue(label))
    }

    /// Parses `debugger;`.
    pub fn parse_debugger_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::Debugger)?;
        self.consume_semicolon();
        Ok(Statement::Debugger)
    }

    /// Parses `with (expr) statement`.
    pub fn parse_with_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::With)?;
        self.consume(Token::LeftParen)?;
        let object = self.parse_expression()?;
        self.consume(Token::RightParen)?;
        let body = Box::new(self.parse_statement()?);
        Ok(Statement::With { object, body })
    }

    /// Parses `try { ... } catch (e) { ... } finally { ... }`.
    pub fn parse_try_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::Try)?;
        let try_block = Box::new(self.parse_block()?);

        let mut catch_param = None;
        let mut catch_block = None;
        if self.match_token(Token::Catch) {
            if self.match_token(Token::LeftParen) {
                let ident = self.consume(Token::Identifier)?;
                catch_param = Some(ident.literal);
                self.consume(Token::RightParen)?;
            }
            catch_block = Some(Box::new(self.parse_block()?));
        }

        let mut finally_block = None;
        if self.match_token(Token::Finally) {
            finally_block = Some(Box::new(self.parse_block()?));
        }

        if catch_block.is_none() && finally_block.is_none() {
            return Err(ParseError {
                message: "Missing 'catch' or 'finally' clause after 'try'".to_string(),
                offset: self.current().start,
            });
        }

        Ok(Statement::TryCatch {
            try_block,
            catch_param,
            catch_block,
            finally_block,
        })
    }

    /// Parses `throw expr;`.
    pub fn parse_throw_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::Throw)?;
        let expr = self.parse_expression()?;
        self.consume_semicolon();
        Ok(Statement::Throw(expr))
    }

    /// Parses `return [expr];`.
    pub fn parse_return_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::Return)?;
        let expr = if self.current().token == Token::Semicolon
            || self.current().token == Token::RightBrace
            || self.current().token == Token::Eos
        {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.consume_semicolon();
        Ok(Statement::Return(expr))
    }

    /// Parses an `import` declaration (`import def from "spec"`, `import * as ns from "spec"`, `import { a, b as c } from "spec"`).
    pub fn parse_import_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::Import)?;
        let mut specifiers = Vec::new();

        // Bare import: import "specifier";
        if self.current().token == Token::String {
            let specifier = self.advance().literal;
            self.consume_semicolon();
            return Ok(Statement::ImportDeclaration {
                specifier,
                specifiers: Vec::new(),
            });
        }

        // Default import
        if self.current().token == Token::Identifier {
            let default_name = self.advance().literal;
            specifiers.push(ImportSpecifier {
                imported: "default".to_string(),
                local: default_name,
            });
            let _ = self.match_token(Token::Comma);
        }

        if self.match_token(Token::Mul) {
            let as_tok = self.consume(Token::Identifier)?;
            if as_tok.literal != "as" {
                return Err(ParseError {
                    message: "Expected 'as' after '*' in import".to_string(),
                    offset: as_tok.start,
                });
            }
            let local_name = self.consume(Token::Identifier)?.literal;
            specifiers.push(ImportSpecifier {
                imported: "*".to_string(),
                local: local_name,
            });
        } else if self.current().token == Token::LeftBrace {
            self.advance();
            while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
                let imported = self.consume(Token::Identifier)?.literal;
                let local = if self.current().token == Token::Identifier && self.current().literal == "as" {
                    self.advance();
                    self.consume(Token::Identifier)?.literal
                } else {
                    imported.clone()
                };
                specifiers.push(ImportSpecifier { imported, local });
                if !self.match_token(Token::Comma) {
                    break;
                }
            }
            self.consume(Token::RightBrace)?;
        }

        let from_tok = self.consume(Token::Identifier)?;
        if from_tok.literal != "from" {
            return Err(ParseError {
                message: "Expected 'from' after import specifiers".to_string(),
                offset: from_tok.start,
            });
        }

        let specifier = self.consume(Token::String)?.literal;
        self.consume_semicolon();

        Ok(Statement::ImportDeclaration {
            specifier,
            specifiers,
        })
    }

    /// Parses an `export` declaration (`export default ...`, `export const ...`, `export { a }`).
    pub fn parse_export_statement(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::Export)?;

        // Case 1: export default ...
        if self.match_token(Token::Default) {
            if self.current().token == Token::Function {
                let func_stmt = self.parse_function_declaration(false)?;
                return Ok(Statement::ExportDeclaration {
                    specifier: None,
                    specifiers: vec![ExportSpecifier {
                        local: "default".to_string(),
                        exported: "default".to_string(),
                    }],
                    declaration: Some(Box::new(func_stmt)),
                    is_default: true,
                });
            }
            if self.current().token == Token::Class {
                let class_stmt = self.parse_class_declaration()?;
                return Ok(Statement::ExportDeclaration {
                    specifier: None,
                    specifiers: vec![ExportSpecifier {
                        local: "default".to_string(),
                        exported: "default".to_string(),
                    }],
                    declaration: Some(Box::new(class_stmt)),
                    is_default: true,
                });
            }
            let expr = self.parse_expression()?;
            self.consume_semicolon();
            return Ok(Statement::ExportDeclaration {
                specifier: None,
                specifiers: vec![ExportSpecifier {
                    local: "default".to_string(),
                    exported: "default".to_string(),
                }],
                declaration: Some(Box::new(Statement::Expression(expr))),
                is_default: true,
            });
        }

        // Case 2: export * from "specifier"
        if self.match_token(Token::Mul) {
            let from_tok = self.consume(Token::Identifier)?;
            if from_tok.literal != "from" {
                return Err(ParseError {
                    message: "Expected 'from' after '*' in export".to_string(),
                    offset: from_tok.start,
                });
            }
            let specifier = self.consume(Token::String)?.literal;
            self.consume_semicolon();
            return Ok(Statement::ExportDeclaration {
                specifier: Some(specifier),
                specifiers: vec![ExportSpecifier {
                    local: "*".to_string(),
                    exported: "*".to_string(),
                }],
                declaration: None,
                is_default: false,
            });
        }

        // Case 3: export const / let / var / function / class / async function
        if matches!(self.current().token, Token::Const | Token::Let | Token::Var | Token::Function | Token::Class | Token::Async) {
            let decl = match self.current().token {
                Token::Const | Token::Let | Token::Var => self.parse_variable_declaration()?,
                Token::Class => self.parse_class_declaration()?,
                Token::Function => self.parse_function_declaration(false)?,
                Token::Async => {
                    self.advance();
                    self.parse_function_declaration(true)?
                }
                _ => unreachable!(),
            };
            let mut specifiers = Vec::new();
            match &decl {
                Statement::VariableDeclaration { name, .. } => {
                    specifiers.push(ExportSpecifier {
                        local: name.clone(),
                        exported: name.clone(),
                    });
                }
                Statement::FunctionDeclaration { name, .. } => {
                    specifiers.push(ExportSpecifier {
                        local: name.clone(),
                        exported: name.clone(),
                    });
                }
                Statement::ClassDeclaration { name, .. } => {
                    specifiers.push(ExportSpecifier {
                        local: name.clone(),
                        exported: name.clone(),
                    });
                }
                _ => {}
            }
            return Ok(Statement::ExportDeclaration {
                specifier: None,
                specifiers,
                declaration: Some(Box::new(decl)),
                is_default: false,
            });
        }

        // Case 4: export { a, b as c } [from "specifier"]
        if self.current().token == Token::LeftBrace {
            self.advance();
            let mut specifiers = Vec::new();
            while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
                let local = self.consume(Token::Identifier)?.literal;
                let exported = if self.current().token == Token::Identifier && self.current().literal == "as" {
                    self.advance();
                    self.consume(Token::Identifier)?.literal
                } else {
                    local.clone()
                };
                specifiers.push(ExportSpecifier { local, exported });
                if !self.match_token(Token::Comma) {
                    break;
                }
            }
            self.consume(Token::RightBrace)?;

            let specifier = if self.current().token == Token::Identifier && self.current().literal == "from" {
                self.advance();
                Some(self.consume(Token::String)?.literal)
            } else {
                None
            };
            self.consume_semicolon();

            return Ok(Statement::ExportDeclaration {
                specifier,
                specifiers,
                declaration: None,
                is_default: false,
            });
        }

        Err(ParseError {
            message: format!("Unexpected token in export: {:?}", self.current().token),
            offset: self.current().start,
        })
    }

    /// Parses `function name(arg1, arg2) { ... }`, `function* ...`, or `async function ...`.
    pub fn parse_function_declaration(&mut self, is_async: bool) -> Result<Statement, ParseError> {
        self.consume(Token::Function)?;
        let is_generator = self.match_token(Token::Mul);
        let name_tok = self.consume(Token::Identifier)?;

        self.consume(Token::LeftParen)?;
        let mut params = Vec::new();
        let mut prologue = Vec::new();
        while self.current().token != Token::RightParen && self.current().token != Token::Eos {
            if self.match_token(Token::Ellipsis) {
                let param_tok = self.consume(Token::Identifier)?;
                params.push(format!("...{}", param_tok.literal));
                break;
            }
            let param_tok = self.consume(Token::Identifier)?;
            let p_name = param_tok.literal;
            params.push(p_name.clone());
            if self.match_token(Token::Assign) {
                let default_expr = self.parse_precedence(2)?;
                prologue.push(Statement::If {
                    condition: Expression::Binary {
                        op: BinaryOperator::EqStrict,
                        left: Box::new(Expression::Variable(p_name.clone())),
                        right: Box::new(Expression::Literal(LiteralValue::Undefined)),
                    },
                    then_branch: Box::new(Statement::Expression(Expression::Assignment {
                        target: p_name,
                        value: Box::new(default_expr),
                    })),
                    else_branch: None,
                });
            }
            if !self.match_token(Token::Comma) {
                break;
            }
        }
        self.consume(Token::RightParen)?;

        self.consume(Token::LeftBrace)?;
        let mut body = Vec::new();
        while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
            body.push(self.parse_statement()?);
        }
        self.consume(Token::RightBrace)?;

        prologue.append(&mut body);
        let body = self.desugar_using_in_block(prologue);

        Ok(Statement::FunctionDeclaration {
            name: name_tok.literal,
            params,
            body,
            is_async,
            is_generator,
        })
    }

    /// Helper to pre-scan and collect all declared private member names in the class body.
    fn collect_class_private_names(&self) -> std::collections::HashSet<String> {
        let mut names = std::collections::HashSet::new();
        let mut scan = self.scanner.clone();
        let mut depth = 1usize;

        if depth == 1 && self.current_token.token == Token::Identifier && self.current_token.literal.starts_with('#') {
            names.insert(self.current_token.literal.clone());
        }
        if let Some(ref pt) = self.peeked_token {
            if depth == 1 && pt.token == Token::Identifier && pt.literal.starts_with('#') {
                names.insert(pt.literal.clone());
            }
        }

        loop {
            let tok = scan.next_token();
            if tok.token == Token::Eos {
                break;
            }
            if tok.token == Token::LeftBrace {
                depth += 1;
            } else if tok.token == Token::RightBrace {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            } else if depth == 1 && tok.token == Token::Identifier && tok.literal.starts_with('#') {
                names.insert(tok.literal);
            }
        }
        names
    }

    /// Parses an ECMAScript class declaration (`class Name extends Super { ... }`).
    pub fn parse_class_declaration(&mut self) -> Result<Statement, ParseError> {
        self.consume(Token::Class)?;
        let name_tok = self.consume(Token::Identifier)?;
        let class_name = name_tok.literal;

        let super_class = if self.match_token(Token::Extends) {
            let super_tok = self.consume(Token::Identifier)?;
            Some(super_tok.literal)
        } else {
            None
        };

        self.consume(Token::LeftBrace)?;

        let declared_privates = self.collect_class_private_names();
        self.enclosing_private_names.push(declared_privates);

        let mut constructor = None;
        let mut methods = Vec::new();
        let mut fields = Vec::new();
        let mut static_blocks = Vec::new();

        while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
            if self.match_token(Token::Semicolon) {
                continue;
            }

            // Static initialization block: static { ... stmts ... }
            if self.current().token == Token::Static && self.peek_token().token == Token::LeftBrace {
                self.advance(); // consume 'static'
                self.consume(Token::LeftBrace)?;
                let mut block_body = Vec::new();
                while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
                    block_body.push(self.parse_statement()?);
                }
                self.consume(Token::RightBrace)?;
                static_blocks.push(ClassStaticBlock { body: block_body });
                continue;
            }

            let mut is_static = false;
            let mut is_getter = false;
            let mut is_setter = false;

            if self.current().token == Token::Static {
                let next_tok = self.peek_token();
                if next_tok.token != Token::LeftParen
                    && next_tok.token != Token::Assign
                    && next_tok.token != Token::Semicolon
                    && next_tok.token != Token::RightBrace
                {
                    self.advance();
                    is_static = true;
                }
            }

            if self.current().token == Token::Identifier {
                if self.current().literal == "get" {
                    let next_tok = self.peek_token();
                    if next_tok.token != Token::LeftParen
                        && next_tok.token != Token::Assign
                        && next_tok.token != Token::Semicolon
                    {
                        self.advance();
                        is_getter = true;
                    }
                } else if self.current().literal == "set" {
                    let next_tok = self.peek_token();
                    if next_tok.token != Token::LeftParen
                        && next_tok.token != Token::Assign
                        && next_tok.token != Token::Semicolon
                    {
                        self.advance();
                        is_setter = true;
                    }
                }
            }

            let member_name = self.parse_property_name()?;

            if self.current().token == Token::LeftParen {
                self.consume(Token::LeftParen)?;
                let mut params = Vec::new();
                while self.current().token != Token::RightParen && self.current().token != Token::Eos {
                    let p = self.consume(Token::Identifier)?;
                    params.push(p.literal);
                    if !self.match_token(Token::Comma) {
                        break;
                    }
                }
                self.consume(Token::RightParen)?;

                self.consume(Token::LeftBrace)?;
                let mut body = Vec::new();
                while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
                    body.push(self.parse_statement()?);
                }
                self.consume(Token::RightBrace)?;

                let method = ClassMethod {
                    name: member_name.clone(),
                    params,
                    body,
                    is_static,
                    is_getter,
                    is_setter,
                };

                if member_name == "constructor" && !is_static {
                    constructor = Some(method);
                } else {
                    methods.push(method);
                }
            } else {
                // Class field: [static] [#]name [= expr];
                let initializer = if self.match_token(Token::Assign) {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                self.consume_semicolon();
                fields.push(ClassField {
                    name: member_name,
                    initializer,
                    is_static,
                });
            }
        }

        self.consume(Token::RightBrace)?;
        self.enclosing_private_names.pop();

        Ok(Statement::ClassDeclaration {
            name: class_name,
            super_class,
            constructor,
            methods,
            fields,
            static_blocks,
        })
    }

    /// Parses an expression statement (`expr;`).
    pub fn parse_expression_statement(&mut self) -> Result<Statement, ParseError> {
        let expr = self.parse_expression()?;
        self.consume_semicolon();
        Ok(Statement::Expression(expr))
    }

    /// Parses an expression starting at the lowest precedence level.
    pub fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_precedence(2)
    }

    /// Precedence climbing (Pratt parsing) implementation.
    pub fn parse_precedence(&mut self, min_prec: i32) -> Result<Expression, ParseError> {
        let mut left = self.parse_primary()?;

        loop {
            // Postfix increment / decrement (e.g. i++, i--)
            if (self.current().token == Token::Inc || self.current().token == Token::Dec) && min_prec <= 16 {
                let is_inc = self.advance().token == Token::Inc;
                let op = if is_inc { BinaryOperator::Add } else { BinaryOperator::Sub };
                left = match left {
                    Expression::Variable(ref name) => Expression::Assignment {
                        target: name.clone(),
                        value: Box::new(Expression::Binary {
                            op,
                            left: Box::new(Expression::Variable(name.clone())),
                            right: Box::new(Expression::Literal(LiteralValue::Smi(1))),
                        }),
                    },
                    Expression::PropertyAccess { ref object, ref property } => Expression::PropertyAssignment {
                        object: object.clone(),
                        property: property.clone(),
                        value: Box::new(Expression::Binary {
                            op,
                            left: Box::new(Expression::PropertyAccess {
                                object: object.clone(),
                                property: property.clone(),
                            }),
                            right: Box::new(Expression::Literal(LiteralValue::Smi(1))),
                        }),
                    },
                    Expression::KeyedAccess { ref object, ref key } => Expression::KeyedAssignment {
                        object: object.clone(),
                        key: key.clone(),
                        value: Box::new(Expression::Binary {
                            op,
                            left: Box::new(Expression::KeyedAccess {
                                object: object.clone(),
                                key: key.clone(),
                            }),
                            right: Box::new(Expression::Literal(LiteralValue::Smi(1))),
                        }),
                    },
                    _ => {
                        return Err(ParseError {
                            message: "Invalid left-hand side in postfix operation".to_string(),
                            offset: self.current().start,
                        });
                    }
                };
                continue;
            }

            // Check for call expressions: left(arg1, arg2)
            if self.current().token == Token::LeftParen && min_prec <= 16 {
                self.advance();
                let mut arguments = Vec::new();
                while self.current().token != Token::RightParen && self.current().token != Token::Eos {
                    if self.match_token(Token::Ellipsis) {
                        let inner = self.parse_expression()?;
                        arguments.push(Expression::Spread(Box::new(inner)));
                    } else {
                        arguments.push(self.parse_expression()?);
                    }
                    if !self.match_token(Token::Comma) {
                        break;
                    }
                }
                self.consume(Token::RightParen)?;
                left = Expression::Call {
                    callee: Box::new(left),
                    arguments,
                };
                continue;
            }

            // Check for optional chaining: left?.prop, left?.[expr], left?.(arg1, arg2)
            if self.current().token == Token::QuestionPeriod && min_prec <= 16 {
                self.advance();
                if self.current().token == Token::LeftBracket {
                    self.advance();
                    let key = self.parse_expression()?;
                    self.consume(Token::RightBracket)?;
                    left = Expression::OptionalKeyedAccess {
                        object: Box::new(left),
                        key: Box::new(key),
                    };
                    continue;
                } else if self.current().token == Token::LeftParen {
                    self.advance();
                    let mut arguments = Vec::new();
                    while self.current().token != Token::RightParen && self.current().token != Token::Eos {
                        if self.match_token(Token::Ellipsis) {
                            let inner = self.parse_expression()?;
                            arguments.push(Expression::Spread(Box::new(inner)));
                        } else {
                            arguments.push(self.parse_expression()?);
                        }
                        if !self.match_token(Token::Comma) {
                            break;
                        }
                    }
                    self.consume(Token::RightParen)?;
                    left = Expression::OptionalCall {
                        callee: Box::new(left),
                        arguments,
                    };
                    continue;
                } else {
                    let prop_name = self.parse_property_name()?;
                    left = Expression::OptionalPropertyAccess {
                        object: Box::new(left),
                        property: prop_name,
                    };
                    continue;
                }
            }

            // Check for property access: left.prop
            if self.current().token == Token::Period && min_prec <= 16 {
                self.advance();
                let prop_name = self.parse_property_name()?;
                left = Expression::PropertyAccess {
                    object: Box::new(left),
                    property: prop_name,
                };
                continue;
            }

            // Check for keyed access: left[expr]
            if self.current().token == Token::LeftBracket && min_prec <= 16 {
                self.advance();
                let key = self.parse_expression()?;
                self.consume(Token::RightBracket)?;
                left = Expression::KeyedAccess {
                    object: Box::new(left),
                    key: Box::new(key),
                };
                continue;
            }

            let tok = self.current().token;
            let prec = tok.precedence(true);

            if prec < min_prec {
                break;
            }

            // Assignment and compound assignment operators (right-associative)
            if matches!(
                tok,
                Token::Assign
                    | Token::AssignAdd
                    | Token::AssignSub
                    | Token::AssignMul
                    | Token::AssignDiv
                    | Token::AssignMod
                    | Token::AssignExp
                    | Token::AssignShl
                    | Token::AssignSar
                    | Token::AssignShr
                    | Token::AssignAnd
                    | Token::AssignOr
                    | Token::AssignXor
                    | Token::AssignNullish
                    | Token::AssignLogicalAnd
                    | Token::AssignLogicalOr
            ) {
                self.advance();
                let right = self.parse_precedence(prec)?;
                let value = match tok {
                    Token::Assign => right,
                    Token::AssignAdd => Expression::Binary {
                        op: BinaryOperator::Add,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignSub => Expression::Binary {
                        op: BinaryOperator::Sub,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignMul => Expression::Binary {
                        op: BinaryOperator::Mul,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignDiv => Expression::Binary {
                        op: BinaryOperator::Div,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignMod => Expression::Binary {
                        op: BinaryOperator::Mod,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignExp => Expression::Binary {
                        op: BinaryOperator::Exp,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignShl => Expression::Binary {
                        op: BinaryOperator::ShiftLeft,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignSar => Expression::Binary {
                        op: BinaryOperator::ShiftRight,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignShr => Expression::Binary {
                        op: BinaryOperator::ShiftRightLogical,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignXor => Expression::Binary {
                        op: BinaryOperator::BitwiseXor,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignAnd => Expression::Binary {
                        op: BinaryOperator::BitwiseAnd,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignOr => Expression::Binary {
                        op: BinaryOperator::BitwiseOr,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignLogicalAnd => Expression::Binary {
                        op: BinaryOperator::LogicalAnd,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignLogicalOr => Expression::Binary {
                        op: BinaryOperator::LogicalOr,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    Token::AssignNullish => Expression::Binary {
                        op: BinaryOperator::NullishCoalescing,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    },
                    _ => unreachable!(),
                };
                left = match left {
                    Expression::Variable(name) => Expression::Assignment {
                        target: name,
                        value: Box::new(value),
                    },
                    Expression::PropertyAccess { object, property } => Expression::PropertyAssignment {
                        object,
                        property,
                        value: Box::new(value),
                    },
                    Expression::KeyedAccess { object, key } => Expression::KeyedAssignment {
                        object,
                        key,
                        value: Box::new(value),
                    },
                    Expression::ArrayLiteral(elements) if tok == Token::Assign => Expression::ArrayDestructureAssignment {
                        targets: elements,
                        value: Box::new(value),
                    },
                    Expression::ObjectLiteral(pairs) if tok == Token::Assign => Expression::ObjectDestructureAssignment {
                        pairs,
                        value: Box::new(value),
                    },
                    _ => {
                        return Err(ParseError {
                            message: "Invalid left-hand side in assignment".to_string(),
                            offset: self.current().start,
                        });
                    }
                };
                continue;
            }

            // Conditional (ternary) operator: condition ? then_expr : else_expr
            if tok == Token::Conditional && min_prec <= 3 {
                self.advance(); // consume '?'
                let then_expr = self.parse_precedence(2)?;
                self.consume(Token::Colon)?;
                let else_expr = self.parse_precedence(2)?;
                left = Expression::Conditional {
                    condition: Box::new(left),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                };
                continue;
            }

            // Binary operator (left-associative, except Exp which is right-associative)
            if let Some(op) = Self::token_to_binary_op(tok) {
                self.advance();
                let next_min_prec = if tok == Token::Exp { prec } else { prec + 1 };
                let right = self.parse_precedence(next_min_prec)?;
                left = Expression::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
                continue;
            }

            break;
        }

        Ok(left)
    }

    /// Speculatively attempts to parse an arrow function parameter list `(a, b = 1, ...rest) =>`.
    fn try_parse_arrow_params(&mut self) -> Option<(Vec<String>, Vec<Statement>)> {
        self.advance(); // consume LeftParen
        let mut params = Vec::new();
        let mut prologue = Vec::new();
        while self.current().token != Token::RightParen && self.current().token != Token::Eos {
            if self.match_token(Token::Ellipsis) {
                if self.current().token != Token::Identifier {
                    return None;
                }
                let param_name = self.advance().literal;
                params.push(format!("...{}", param_name));
                break;
            }
            if self.current().token != Token::Identifier {
                return None;
            }
            let param_name = self.advance().literal;
            params.push(param_name.clone());
            if self.match_token(Token::Assign) {
                let default_expr = self.parse_precedence(2).ok()?;
                prologue.push(Statement::If {
                    condition: Expression::Binary {
                        op: BinaryOperator::EqStrict,
                        left: Box::new(Expression::Variable(param_name.clone())),
                        right: Box::new(Expression::Literal(LiteralValue::Undefined)),
                    },
                    then_branch: Box::new(Statement::Expression(Expression::Assignment {
                        target: param_name,
                        value: Box::new(default_expr),
                    })),
                    else_branch: None,
                });
            }
            if !self.match_token(Token::Comma) {
                break;
            }
        }
        if !self.match_token(Token::RightParen) {
            return None;
        }
        if !self.match_token(Token::Arrow) {
            return None;
        }
        Some((params, prologue))
    }

    /// Parses prefix/primary expressions.
    pub fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        let tok = self.current().clone();

        match tok.token {
            Token::Number => {
                self.advance();
                let literal = tok.literal.trim();
                let val = if literal.starts_with("0x") || literal.starts_with("0X") {
                    if let Ok(i) = i64::from_str_radix(&literal[2..], 16) {
                        if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                            LiteralValue::Smi(i as i32)
                        } else {
                            LiteralValue::Number(i as f64)
                        }
                    } else {
                        LiteralValue::Smi(0)
                    }
                } else if literal.starts_with("0b") || literal.starts_with("0B") {
                    if let Ok(i) = i64::from_str_radix(&literal[2..], 2) {
                        if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                            LiteralValue::Smi(i as i32)
                        } else {
                            LiteralValue::Number(i as f64)
                        }
                    } else {
                        LiteralValue::Smi(0)
                    }
                } else if literal.starts_with("0o") || literal.starts_with("0O") {
                    if let Ok(i) = i64::from_str_radix(&literal[2..], 8) {
                        if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                            LiteralValue::Smi(i as i32)
                        } else {
                            LiteralValue::Number(i as f64)
                        }
                    } else {
                        LiteralValue::Smi(0)
                    }
                } else if let Ok(i) = literal.parse::<i32>() {
                    LiteralValue::Smi(i)
                } else if let Ok(f) = literal.parse::<f64>() {
                    LiteralValue::Number(f)
                } else {
                    LiteralValue::Smi(0)
                };
                Ok(Expression::Literal(val))
            }
            Token::BigInt => {
                self.advance();
                Ok(Expression::Literal(LiteralValue::BigInt(tok.literal)))
            }
            Token::String => {
                self.advance();
                Ok(Expression::Literal(LiteralValue::String(tok.literal)))
            }
            Token::TrueLiteral => {
                self.advance();
                Ok(Expression::Literal(LiteralValue::Boolean(true)))
            }
            Token::FalseLiteral => {
                self.advance();
                Ok(Expression::Literal(LiteralValue::Boolean(false)))
            }
            Token::NullLiteral => {
                self.advance();
                Ok(Expression::Literal(LiteralValue::Null))
            }
            Token::Identifier => {
                self.advance();
                if self.current().token == Token::Arrow {
                    self.advance(); // consume =>
                    let param_name = tok.literal;
                    let (params, body) = if self.current().token == Token::LeftBrace {
                        self.advance();
                        let mut stmts = Vec::new();
                        while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
                            stmts.push(self.parse_statement()?);
                        }
                        self.consume(Token::RightBrace)?;
                        (vec![param_name], stmts)
                    } else {
                        let expr = self.parse_precedence(2)?;
                        (vec![param_name], vec![Statement::Return(Some(expr))])
                    };
                    self.anon_counter += 1;
                    let fn_name = format!("__anon_arrow_{}__", self.anon_counter);
                    self.anon_functions.push(Statement::FunctionDeclaration {
                        name: fn_name.clone(),
                        params,
                        body,
                        is_async: false,
                        is_generator: false,
                    });
                    return Ok(Expression::Variable(fn_name));
                }
                if tok.literal == "undefined" {
                    Ok(Expression::Literal(LiteralValue::Undefined))
                } else if tok.literal.starts_with('#') {
                    let in_scope = self.enclosing_private_names.iter().any(|set| set.contains(&tok.literal));
                    if !in_scope {
                        return Err(ParseError {
                            message: format!("SyntaxError: Private field '{}' must be declared in an enclosing class", tok.literal),
                            offset: tok.start,
                        });
                    }
                    Ok(Expression::Literal(LiteralValue::String(tok.literal)))
                } else {
                    Ok(Expression::Variable(tok.literal))
                }
            }
            Token::Function => {
                self.advance();
                let is_generator = self.match_token(Token::Mul);
                let name = if self.current().token == Token::Identifier {
                    self.advance().literal
                } else {
                    self.anon_counter += 1;
                    format!("__anon_fn_{}__", self.anon_counter)
                };

                self.consume(Token::LeftParen)?;
                let mut params = Vec::new();
                let mut prologue = Vec::new();
                while self.current().token != Token::RightParen && self.current().token != Token::Eos {
                    if self.match_token(Token::Ellipsis) {
                        let param_tok = self.consume(Token::Identifier)?;
                        params.push(format!("...{}", param_tok.literal));
                        break;
                    }
                    let param_tok = self.consume(Token::Identifier)?;
                    let p_name = param_tok.literal;
                    params.push(p_name.clone());
                    if self.match_token(Token::Assign) {
                        let default_expr = self.parse_precedence(2)?;
                        prologue.push(Statement::If {
                            condition: Expression::Binary {
                                op: BinaryOperator::EqStrict,
                                left: Box::new(Expression::Variable(p_name.clone())),
                                right: Box::new(Expression::Literal(LiteralValue::Undefined)),
                            },
                            then_branch: Box::new(Statement::Expression(Expression::Assignment {
                                target: p_name,
                                value: Box::new(default_expr),
                            })),
                            else_branch: None,
                        });
                    }
                    if !self.match_token(Token::Comma) {
                        break;
                    }
                }
                self.consume(Token::RightParen)?;

                self.consume(Token::LeftBrace)?;
                let mut body = Vec::new();
                while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
                    body.push(self.parse_statement()?);
                }
                self.consume(Token::RightBrace)?;
                prologue.append(&mut body);
                let body = prologue;

                self.anon_functions.push(Statement::FunctionDeclaration {
                    name: name.clone(),
                    params,
                    body,
                    is_async: false,
                    is_generator,
                });

                Ok(Expression::Variable(name))
            }
            Token::Async => {
                if self.peek_token().token == Token::Function {
                    self.advance(); // consume async
                    self.advance(); // consume function
                    let is_generator = self.match_token(Token::Mul);
                    let name = if self.current().token == Token::Identifier {
                        self.advance().literal
                    } else {
                        self.anon_counter += 1;
                        format!("__anon_async_fn_{}__", self.anon_counter)
                    };

                    self.consume(Token::LeftParen)?;
                    let mut params = Vec::new();
                    let mut prologue = Vec::new();
                    while self.current().token != Token::RightParen && self.current().token != Token::Eos {
                        if self.match_token(Token::Ellipsis) {
                            let param_tok = self.consume(Token::Identifier)?;
                            params.push(format!("...{}", param_tok.literal));
                            break;
                        }
                        let param_tok = self.consume(Token::Identifier)?;
                        let p_name = param_tok.literal;
                        params.push(p_name.clone());
                        if self.match_token(Token::Assign) {
                            let default_expr = self.parse_precedence(2)?;
                            prologue.push(Statement::If {
                                condition: Expression::Binary {
                                op: BinaryOperator::EqStrict,
                                left: Box::new(Expression::Variable(p_name.clone())),
                                right: Box::new(Expression::Literal(LiteralValue::Undefined)),
                            },
                            then_branch: Box::new(Statement::Expression(Expression::Assignment {
                                target: p_name,
                                value: Box::new(default_expr),
                            })),
                            else_branch: None,
                        });
                        }
                        if !self.match_token(Token::Comma) {
                            break;
                        }
                    }
                    self.consume(Token::RightParen)?;

                    self.consume(Token::LeftBrace)?;
                    let mut body = Vec::new();
                    while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
                        body.push(self.parse_statement()?);
                    }
                    self.consume(Token::RightBrace)?;
                    prologue.append(&mut body);
                    let body = prologue;

                    self.anon_functions.push(Statement::FunctionDeclaration {
                        name: name.clone(),
                        params,
                        body,
                        is_async: true,
                        is_generator,
                    });

                    Ok(Expression::Variable(name))
                } else {
                    self.advance();
                    Ok(Expression::Variable("async".to_string()))
                }
            }
            Token::Await => {
                self.advance();
                let operand = self.parse_precedence(15)?;
                Ok(Expression::Await(Box::new(operand)))
            }
            Token::Yield => {
                self.advance();
                let delegate = self.match_token(Token::Mul);
                let value = if matches!(
                    self.current().token,
                    Token::Semicolon
                        | Token::RightParen
                        | Token::RightBracket
                        | Token::RightBrace
                        | Token::Comma
                        | Token::Colon
                        | Token::Eos
                ) {
                    None
                } else {
                    Some(Box::new(self.parse_precedence(2)?))
                };
                Ok(Expression::Yield { value, delegate })
            }
            Token::Import => {
                self.advance();
                if self.match_token(Token::Period) {
                    let meta_tok = self.consume(Token::Identifier)?;
                    if meta_tok.literal == "meta" {
                        return Ok(Expression::ImportMeta);
                    } else {
                        return Err(ParseError {
                            message: format!("Unexpected identifier after 'import.': {}", meta_tok.literal),
                            offset: meta_tok.start,
                        });
                    }
                }
                self.consume(Token::LeftParen)?;
                let spec_expr = self.parse_expression()?;
                self.consume(Token::RightParen)?;
                Ok(Expression::DynamicImport(Box::new(spec_expr)))
            }
            Token::This => {
                self.advance();
                Ok(Expression::This)
            }
            Token::Super => {
                self.advance();
                Ok(Expression::Super)
            }
            Token::New => {
                self.advance();
                if self.match_token(Token::Period) {
                    let target_tok = self.consume(Token::Identifier)?;
                    if target_tok.literal == "target" {
                        return Ok(Expression::NewTarget);
                    } else {
                        return Err(ParseError {
                            message: format!("Unexpected identifier after 'new.': {}", target_tok.literal),
                            offset: target_tok.start,
                        });
                    }
                }
                let callee = self.parse_primary()?;
                let mut target = callee;
                while self.current().token == Token::Period {
                    self.advance();
                    let prop = self.parse_property_name()?;
                    target = Expression::PropertyAccess {
                        object: Box::new(target),
                        property: prop,
                    };
                }
                let mut arguments = Vec::new();
                if self.match_token(Token::LeftParen) {
                    while self.current().token != Token::RightParen && self.current().token != Token::Eos {
                        arguments.push(self.parse_expression()?);
                        if !self.match_token(Token::Comma) {
                            break;
                        }
                    }
                    self.consume(Token::RightParen)?;
                }
                Ok(Expression::New {
                    callee: Box::new(target),
                    arguments,
                })
            }
            Token::LeftParen => {
                let snapshot = (self.scanner.clone(), self.current_token.clone(), self.peeked_token.clone());
                if let Some((params, mut prologue)) = self.try_parse_arrow_params() {
                    let (params, body) = if self.current().token == Token::LeftBrace {
                        self.advance();
                        let mut stmts = Vec::new();
                        while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
                            stmts.push(self.parse_statement()?);
                        }
                        self.consume(Token::RightBrace)?;
                        prologue.append(&mut stmts);
                        (params, prologue)
                    } else {
                        let expr = self.parse_precedence(2)?;
                        prologue.push(Statement::Return(Some(expr)));
                        (params, prologue)
                    };
                    self.anon_counter += 1;
                    let fn_name = format!("__anon_arrow_{}__", self.anon_counter);
                    self.anon_functions.push(Statement::FunctionDeclaration {
                        name: fn_name.clone(),
                        params,
                        body,
                        is_async: false,
                        is_generator: false,
                    });
                    return Ok(Expression::Variable(fn_name));
                } else {
                    self.scanner = snapshot.0;
                    self.current_token = snapshot.1;
                    self.peeked_token = snapshot.2;
                }
                self.advance();
                let expr = self.parse_expression()?;
                self.consume(Token::RightParen)?;
                Ok(expr)
            }
            Token::LeftBrace => {
                self.advance();
                let mut properties = Vec::new();
                while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
                    if self.match_token(Token::Ellipsis) {
                        let val = self.parse_expression()?;
                        properties.push(("...".to_string(), Expression::Spread(Box::new(val))));
                    } else if self.match_token(Token::LeftBracket) {
                        let key_expr = self.parse_expression()?;
                        self.consume(Token::RightBracket)?;
                        let val = if self.match_token(Token::Colon) {
                            self.parse_expression()?
                        } else if self.current().token == Token::LeftParen {
                            self.consume(Token::LeftParen)?;
                            let mut params = Vec::new();
                            while self.current().token != Token::RightParen && self.current().token != Token::Eos {
                                let p = self.consume(Token::Identifier)?.literal;
                                params.push(p);
                                if !self.match_token(Token::Comma) {
                                    break;
                                }
                            }
                            self.consume(Token::RightParen)?;
                            self.consume(Token::LeftBrace)?;
                            let mut body = Vec::new();
                            while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
                                body.push(self.parse_statement()?);
                            }
                            self.consume(Token::RightBrace)?;
                            let body = self.desugar_using_in_block(body);
                            self.anon_counter += 1;
                            let fn_name = format!("__anon_computed_method_{}__", self.anon_counter);
                            self.anon_functions.push(Statement::FunctionDeclaration {
                                name: fn_name.clone(),
                                params,
                                body,
                                is_async: false,
                                is_generator: false,
                            });
                            Expression::Variable(fn_name)
                        } else {
                            return Err(ParseError {
                                message: "Expected ':' or '(' after computed property key".to_string(),
                                offset: self.current().start,
                            });
                        };
                        properties.push((
                            "__computed__".to_string(),
                            Expression::ComputedProperty {
                                key: Box::new(key_expr),
                                value: Box::new(val),
                            },
                        ));
                    } else {
                        let key = self.parse_property_name()?;
                        let val = if self.match_token(Token::Colon) {
                            self.parse_expression()?
                        } else if self.current().token == Token::LeftParen {
                            self.consume(Token::LeftParen)?;
                            let mut params = Vec::new();
                            while self.current().token != Token::RightParen && self.current().token != Token::Eos {
                                let p = self.consume(Token::Identifier)?.literal;
                                params.push(p);
                                if !self.match_token(Token::Comma) {
                                    break;
                                }
                            }
                            self.consume(Token::RightParen)?;
                            self.consume(Token::LeftBrace)?;
                            let mut body = Vec::new();
                            while self.current().token != Token::RightBrace && self.current().token != Token::Eos {
                                body.push(self.parse_statement()?);
                            }
                            self.consume(Token::RightBrace)?;
                            let body = self.desugar_using_in_block(body);
                            self.anon_counter += 1;
                            let fn_name = format!("__anon_method_{}__", self.anon_counter);
                            self.anon_functions.push(Statement::FunctionDeclaration {
                                name: fn_name.clone(),
                                params,
                                body,
                                is_async: false,
                                is_generator: false,
                            });
                            Expression::Variable(fn_name)
                        } else {
                            Expression::Variable(key.clone())
                        };
                        properties.push((key, val));
                    }
                    if !self.match_token(Token::Comma) {
                        break;
                    }
                }
                self.consume(Token::RightBrace)?;
                Ok(Expression::ObjectLiteral(properties))
            }
            Token::LeftBracket => {
                self.advance();
                let mut elements = Vec::new();
                while self.current().token != Token::RightBracket && self.current().token != Token::Eos {
                    if self.match_token(Token::Ellipsis) {
                        let val = self.parse_expression()?;
                        elements.push(Expression::Spread(Box::new(val)));
                    } else {
                        elements.push(self.parse_expression()?);
                    }
                    if !self.match_token(Token::Comma) {
                        break;
                    }
                }
                self.consume(Token::RightBracket)?;
                Ok(Expression::ArrayLiteral(elements))
            }
            Token::TemplateTail => {
                self.advance();
                Ok(Expression::Literal(LiteralValue::String(tok.literal)))
            }
            Token::TemplateSpan => {
                self.advance();
                let mut expr = Expression::Literal(LiteralValue::String(tok.literal));
                loop {
                    let mid_expr = self.parse_expression()?;
                    expr = Expression::Binary {
                        op: BinaryOperator::Add,
                        left: Box::new(expr),
                        right: Box::new(mid_expr),
                    };
                    if self.current().token == Token::TemplateSpan {
                        let next_span = self.advance();
                        if !next_span.literal.is_empty() {
                            expr = Expression::Binary {
                                op: BinaryOperator::Add,
                                left: Box::new(expr),
                                right: Box::new(Expression::Literal(LiteralValue::String(next_span.literal))),
                            };
                        }
                    } else if self.current().token == Token::TemplateTail {
                        let tail = self.advance();
                        if !tail.literal.is_empty() {
                            expr = Expression::Binary {
                                op: BinaryOperator::Add,
                                left: Box::new(expr),
                                right: Box::new(Expression::Literal(LiteralValue::String(tail.literal))),
                            };
                        }
                        break;
                    } else {
                        break;
                    }
                }
                Ok(expr)
            }
            // Unary operators
            Token::Add => {
                self.advance();
                let operand = self.parse_precedence(15)?;
                Ok(Expression::Unary {
                    op: UnaryOperator::Plus,
                    expr: Box::new(operand),
                })
            }
            Token::Sub => {
                self.advance();
                let operand = self.parse_precedence(15)?;
                Ok(Expression::Unary {
                    op: UnaryOperator::Minus,
                    expr: Box::new(operand),
                })
            }
            Token::Not => {
                self.advance();
                let operand = self.parse_precedence(15)?;
                Ok(Expression::Unary {
                    op: UnaryOperator::Not,
                    expr: Box::new(operand),
                })
            }
            Token::BitNot => {
                self.advance();
                let operand = self.parse_precedence(15)?;
                Ok(Expression::Unary {
                    op: UnaryOperator::BitwiseNot,
                    expr: Box::new(operand),
                })
            }
            Token::Typeof => {
                self.advance();
                let operand = self.parse_precedence(15)?;
                Ok(Expression::Unary {
                    op: UnaryOperator::TypeOf,
                    expr: Box::new(operand),
                })
            }
            Token::Void => {
                self.advance();
                let operand = self.parse_precedence(15)?;
                Ok(Expression::Unary {
                    op: UnaryOperator::Void,
                    expr: Box::new(operand),
                })
            }
            Token::Delete => {
                self.advance();
                let operand = self.parse_precedence(15)?;
                Ok(Expression::Unary {
                    op: UnaryOperator::Delete,
                    expr: Box::new(operand),
                })
            }
            Token::Inc | Token::Dec => {
                let is_inc = self.advance().token == Token::Inc;
                let op = if is_inc { BinaryOperator::Add } else { BinaryOperator::Sub };
                let operand = self.parse_primary()?;
                let expr = match operand {
                    Expression::Variable(ref name) => Expression::Assignment {
                        target: name.clone(),
                        value: Box::new(Expression::Binary {
                            op,
                            left: Box::new(Expression::Variable(name.clone())),
                            right: Box::new(Expression::Literal(LiteralValue::Smi(1))),
                        }),
                    },
                    Expression::PropertyAccess { ref object, ref property } => Expression::PropertyAssignment {
                        object: object.clone(),
                        property: property.clone(),
                        value: Box::new(Expression::Binary {
                            op,
                            left: Box::new(Expression::PropertyAccess {
                                object: object.clone(),
                                property: property.clone(),
                            }),
                            right: Box::new(Expression::Literal(LiteralValue::Smi(1))),
                        }),
                    },
                    Expression::KeyedAccess { ref object, ref key } => Expression::KeyedAssignment {
                        object: object.clone(),
                        key: key.clone(),
                        value: Box::new(Expression::Binary {
                            op,
                            left: Box::new(Expression::KeyedAccess {
                                object: object.clone(),
                                key: key.clone(),
                            }),
                            right: Box::new(Expression::Literal(LiteralValue::Smi(1))),
                        }),
                    },
                    _ => {
                        return Err(ParseError {
                            message: "Invalid left-hand side in prefix operation".to_string(),
                            offset: tok.start,
                        });
                    }
                };
                Ok(expr)
            }
            Token::Div | Token::AssignDiv => {
                let is_assign = tok.token == Token::AssignDiv;
                let (pattern, flags) = self.scanner.scan_regexp_body_and_flags(is_assign).map_err(|msg| ParseError {
                    message: msg,
                    offset: tok.start,
                })?;
                self.current_token = self.scanner.next_token();
                self.peeked_token = None;
                Ok(Expression::RegExpLiteral { pattern, flags })
            }
            _ => Err(ParseError {
                message: format!("Unexpected token in expression: {:?}", tok.token),
                offset: tok.start,
            }),
        }
    }

    /// Maps token to binary operator.
    fn token_to_binary_op(tok: Token) -> Option<BinaryOperator> {
        match tok {
            Token::Add => Some(BinaryOperator::Add),
            Token::Sub => Some(BinaryOperator::Sub),
            Token::Mul => Some(BinaryOperator::Mul),
            Token::Div => Some(BinaryOperator::Div),
            Token::Mod => Some(BinaryOperator::Mod),
            Token::Exp => Some(BinaryOperator::Exp),
            Token::BitOr => Some(BinaryOperator::BitwiseOr),
            Token::BitXor => Some(BinaryOperator::BitwiseXor),
            Token::BitAnd => Some(BinaryOperator::BitwiseAnd),
            Token::Shl => Some(BinaryOperator::ShiftLeft),
            Token::Sar => Some(BinaryOperator::ShiftRight),
            Token::Shr => Some(BinaryOperator::ShiftRightLogical),
            Token::Eq => Some(BinaryOperator::Eq),
            Token::EqStrict => Some(BinaryOperator::EqStrict),
            Token::NotEq => Some(BinaryOperator::NotEq),
            Token::NotEqStrict => Some(BinaryOperator::NotEqStrict),
            Token::Lt => Some(BinaryOperator::LessThan),
            Token::Gt => Some(BinaryOperator::GreaterThan),
            Token::Lte => Some(BinaryOperator::LessThanOrEqual),
            Token::Gte => Some(BinaryOperator::GreaterThanOrEqual),
            Token::And => Some(BinaryOperator::LogicalAnd),
            Token::Or => Some(BinaryOperator::LogicalOr),
            Token::Nullish => Some(BinaryOperator::NullishCoalescing),
            Token::Instanceof => Some(BinaryOperator::InstanceOf),
            Token::In => Some(BinaryOperator::In),
            _ => None,
        }
    }
}
