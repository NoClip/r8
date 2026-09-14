//! Safe Rust reimplementation of Google V8's `src/parsing/token.h`.
//!
//! Provides the complete ECMAScript token definitions, keyword categorization,
//! operator precedence calculation, and fast keyword lookup matching V8 semantics.

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Token {
    // Special
    Eos,
    Illegal,

    // Punctuators
    Period,
    QuestionPeriod,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Colon,
    Semicolon,
    Comma,
    Ellipsis,
    Conditional,
    Arrow,

    // Assignment operators
    Assign,
    AssignAdd,
    AssignSub,
    AssignMul,
    AssignDiv,
    AssignMod,
    AssignExp,
    AssignShl,
    AssignSar,
    AssignShr,
    AssignAnd,
    AssignOr,
    AssignXor,
    AssignNullish,
    AssignLogicalAnd,
    AssignLogicalOr,

    // Binary / Unary Operators
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
    Shl,
    Sar,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Not,
    BitNot,
    Eq,
    NotEq,
    EqStrict,
    NotEqStrict,
    Lt,
    Lte,
    Gt,
    Gte,
    Inc,
    Dec,
    And,
    Or,
    Nullish,

    // Literals
    Identifier,
    String,
    Number,
    BigInt,
    RegExpLiteral,
    TemplateSpan,
    TemplateTail,

    // Keywords
    If,
    Else,
    Function,
    Return,
    Var,
    Let,
    Const,
    Class,
    Extends,
    Async,
    Await,
    Yield,
    Try,
    Catch,
    Finally,
    For,
    While,
    Do,
    Switch,
    Case,
    Default,
    Break,
    Continue,
    New,
    This,
    Super,
    Delete,
    Typeof,
    Void,
    Instanceof,
    In,
    Of,
    Import,
    Export,
    Throw,
    Debugger,
    With,
    Using,
    Static,
    NullLiteral,
    TrueLiteral,
    FalseLiteral,
}

impl Token {
    /// Tells whether this token is an ECMAScript keyword or reserved word.
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            Token::If
                | Token::Else
                | Token::Function
                | Token::Return
                | Token::Var
                | Token::Let
                | Token::Const
                | Token::Class
                | Token::Extends
                | Token::Async
                | Token::Await
                | Token::Yield
                | Token::Try
                | Token::Catch
                | Token::Finally
                | Token::For
                | Token::While
                | Token::Do
                | Token::Switch
                | Token::Case
                | Token::Default
                | Token::Break
                | Token::Continue
                | Token::New
                | Token::This
                | Token::Super
                | Token::Delete
                | Token::Typeof
                | Token::Void
                | Token::Instanceof
                | Token::In
                | Token::Of
                | Token::Import
                | Token::Export
                | Token::Throw
                | Token::Debugger
                | Token::With
                | Token::Using
                | Token::Static
                | Token::NullLiteral
                | Token::TrueLiteral
                | Token::FalseLiteral
        )
    }

    /// Tells whether this token is a binary operator.
    pub fn is_binary_op(&self) -> bool {
        matches!(
            self,
            Token::Add
                | Token::Sub
                | Token::Mul
                | Token::Div
                | Token::Mod
                | Token::Exp
                | Token::Shl
                | Token::Sar
                | Token::Shr
                | Token::BitAnd
                | Token::BitOr
                | Token::BitXor
                | Token::Eq
                | Token::NotEq
                | Token::EqStrict
                | Token::NotEqStrict
                | Token::Lt
                | Token::Lte
                | Token::Gt
                | Token::Gte
                | Token::And
                | Token::Or
                | Token::Nullish
                | Token::In
                | Token::Instanceof
        )
    }

    /// Tells whether this token is an assignment or compound assignment operator.
    pub fn is_assignment_op(&self) -> bool {
        matches!(
            self,
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
        )
    }

    /// Tells whether this token is a unary operator.
    pub fn is_unary_op(&self) -> bool {
        matches!(
            self,
            Token::Not
                | Token::BitNot
                | Token::Add
                | Token::Sub
                | Token::Typeof
                | Token::Void
                | Token::Delete
                | Token::Inc
                | Token::Dec
        )
    }

    /// Returns the binary operator precedence according to ECMAScript specification.
    pub fn precedence(&self, accept_in: bool) -> i32 {
        match self {
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
            | Token::AssignLogicalOr => 2,

            Token::Nullish | Token::Conditional => 3,
            Token::Or => 4,
            Token::And => 5,
            Token::BitOr => 6,
            Token::BitXor => 7,
            Token::BitAnd => 8,

            Token::Eq | Token::NotEq | Token::EqStrict | Token::NotEqStrict => 9,

            Token::Lt | Token::Lte | Token::Gt | Token::Gte | Token::Instanceof => 10,
            Token::In => {
                if accept_in {
                    10
                } else {
                    0
                }
            }

            Token::Shl | Token::Sar | Token::Shr => 11,
            Token::Add | Token::Sub => 12,
            Token::Mul | Token::Div | Token::Mod => 13,
            Token::Exp => 14,

            _ => 0,
        }
    }

    /// Returns the canonical human-readable symbol or keyword string.
    pub fn symbol(&self) -> Option<&'static str> {
        match self {
            Token::Period => Some("."),
            Token::QuestionPeriod => Some("?."),
            Token::LeftParen => Some("("),
            Token::RightParen => Some(")"),
            Token::LeftBracket => Some("["),
            Token::RightBracket => Some("]"),
            Token::LeftBrace => Some("{"),
            Token::RightBrace => Some("}"),
            Token::Colon => Some(":"),
            Token::Semicolon => Some(";"),
            Token::Comma => Some(","),
            Token::Ellipsis => Some("..."),
            Token::Conditional => Some("?"),
            Token::Arrow => Some("=>"),
            Token::Assign => Some("="),
            Token::AssignAdd => Some("+="),
            Token::AssignSub => Some("-="),
            Token::AssignMul => Some("*="),
            Token::AssignDiv => Some("/="),
            Token::AssignMod => Some("%="),
            Token::AssignExp => Some("**="),
            Token::AssignShl => Some("<<="),
            Token::AssignSar => Some(">>="),
            Token::AssignShr => Some(">>>="),
            Token::AssignAnd => Some("&="),
            Token::AssignOr => Some("|="),
            Token::AssignXor => Some("^="),
            Token::AssignNullish => Some("??="),
            Token::AssignLogicalAnd => Some("&&="),
            Token::AssignLogicalOr => Some("||="),
            Token::Add => Some("+"),
            Token::Sub => Some("-"),
            Token::Mul => Some("*"),
            Token::Div => Some("/"),
            Token::Mod => Some("%"),
            Token::Exp => Some("**"),
            Token::Shl => Some("<<"),
            Token::Sar => Some(">>"),
            Token::Shr => Some(">>>"),
            Token::BitAnd => Some("&"),
            Token::BitOr => Some("|"),
            Token::BitXor => Some("^"),
            Token::Not => Some("!"),
            Token::BitNot => Some("~"),
            Token::Eq => Some("=="),
            Token::NotEq => Some("!="),
            Token::EqStrict => Some("==="),
            Token::NotEqStrict => Some("!=="),
            Token::Lt => Some("<"),
            Token::Lte => Some("<="),
            Token::Gt => Some(">"),
            Token::Gte => Some(">="),
            Token::Inc => Some("++"),
            Token::Dec => Some("--"),
            Token::And => Some("&&"),
            Token::Or => Some("||"),
            Token::Nullish => Some("??"),
            Token::If => Some("if"),
            Token::Else => Some("else"),
            Token::Function => Some("function"),
            Token::Return => Some("return"),
            Token::Var => Some("var"),
            Token::Let => Some("let"),
            Token::Const => Some("const"),
            Token::Class => Some("class"),
            Token::Extends => Some("extends"),
            Token::Async => Some("async"),
            Token::Await => Some("await"),
            Token::Yield => Some("yield"),
            Token::Try => Some("try"),
            Token::Catch => Some("catch"),
            Token::Finally => Some("finally"),
            Token::For => Some("for"),
            Token::While => Some("while"),
            Token::Do => Some("do"),
            Token::Switch => Some("switch"),
            Token::Case => Some("case"),
            Token::Default => Some("default"),
            Token::Break => Some("break"),
            Token::Continue => Some("continue"),
            Token::New => Some("new"),
            Token::This => Some("this"),
            Token::Super => Some("super"),
            Token::Delete => Some("delete"),
            Token::Typeof => Some("typeof"),
            Token::Void => Some("void"),
            Token::Instanceof => Some("instanceof"),
            Token::In => Some("in"),
            Token::Of => Some("of"),
            Token::Import => Some("import"),
            Token::Export => Some("export"),
            Token::Throw => Some("throw"),
            Token::Debugger => Some("debugger"),
            Token::With => Some("with"),
            Token::Using => Some("using"),
            Token::Static => Some("static"),
            Token::NullLiteral => Some("null"),
            Token::TrueLiteral => Some("true"),
            Token::FalseLiteral => Some("false"),
            _ => None,
        }
    }

    /// Fast lookup resolving identifier string to keyword token.
    pub fn lookup_keyword(ident: &str) -> Option<Token> {
        match ident {
            "if" => Some(Token::If),
            "else" => Some(Token::Else),
            "function" => Some(Token::Function),
            "return" => Some(Token::Return),
            "var" => Some(Token::Var),
            "let" => Some(Token::Let),
            "const" => Some(Token::Const),
            "class" => Some(Token::Class),
            "extends" => Some(Token::Extends),
            "async" => Some(Token::Async),
            "await" => Some(Token::Await),
            "yield" => Some(Token::Yield),
            "try" => Some(Token::Try),
            "catch" => Some(Token::Catch),
            "finally" => Some(Token::Finally),
            "for" => Some(Token::For),
            "while" => Some(Token::While),
            "do" => Some(Token::Do),
            "switch" => Some(Token::Switch),
            "case" => Some(Token::Case),
            "default" => Some(Token::Default),
            "break" => Some(Token::Break),
            "continue" => Some(Token::Continue),
            "new" => Some(Token::New),
            "this" => Some(Token::This),
            "super" => Some(Token::Super),
            "delete" => Some(Token::Delete),
            "typeof" => Some(Token::Typeof),
            "void" => Some(Token::Void),
            "instanceof" => Some(Token::Instanceof),
            "in" => Some(Token::In),
            "of" => Some(Token::Of),
            "import" => Some(Token::Import),
            "export" => Some(Token::Export),
            "throw" => Some(Token::Throw),
            "debugger" => Some(Token::Debugger),
            "with" => Some(Token::With),
            "using" => Some(Token::Using),
            "static" => Some(Token::Static),
            "null" => Some(Token::NullLiteral),
            "true" => Some(Token::TrueLiteral),
            "false" => Some(Token::FalseLiteral),
            _ => None,
        }
    }
}
