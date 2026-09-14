//! Safe Rust reimplementation of Google V8's `src/parsing/scanner.h`.
//!
//! Provides a high-performance, safe JavaScript lexical analyzer (Lexer/Scanner)
//! supporting numbers (hex, octal, binary, floats, BigInt), strings with escape sequences,
//! template literals, comments, whitespace, Automatic Semicolon Insertion (ASI) tracking,
//! and identifier/keyword resolution matching ECMAScript standard.

use super::token::Token;

#[derive(Clone, Debug, PartialEq)]
pub struct ScannedToken {
    pub token: Token,
    pub start: usize,
    pub end: usize,
    pub literal: String,
}

#[derive(Clone)]
pub struct Scanner<'a> {
    source: &'a str,
    chars: Vec<(usize, char)>,
    cursor: usize,
    has_preceding_newline: bool,
    brace_depth: usize,
    template_brace_stack: Vec<usize>,
}

impl<'a> Scanner<'a> {
    pub fn new(source: &'a str) -> Self {
        let chars: Vec<(usize, char)> = source.char_indices().collect();
        Self {
            source,
            chars,
            cursor: 0,
            has_preceding_newline: false,
            brace_depth: 0,
            template_brace_stack: Vec::new(),
        }
    }

    #[inline]
    fn peek_char(&self) -> Option<char> {
        self.chars.get(self.cursor).map(|&(_, c)| c)
    }

    #[inline]
    fn peek_char_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.cursor + offset).map(|&(_, c)| c)
    }

    #[inline]
    fn advance_char(&mut self) -> Option<char> {
        if self.cursor < self.chars.len() {
            let c = self.chars[self.cursor].1;
            self.cursor += 1;
            Some(c)
        } else {
            None
        }
    }

    #[inline]
    fn current_pos(&self) -> usize {
        if self.cursor < self.chars.len() {
            self.chars[self.cursor].0
        } else {
            self.source.len()
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        self.has_preceding_newline = false;
        while let Some(c) = self.peek_char() {
            match c {
                ' ' | '\t' | '\r' | '\x0C' => {
                    self.advance_char();
                }
                '\n' => {
                    self.has_preceding_newline = true;
                    self.advance_char();
                }
                '/' => {
                    if let Some(next) = self.peek_char_at(1) {
                        if next == '/' {
                            // Single line comment
                            self.advance_char();
                            self.advance_char();
                            while let Some(sc) = self.peek_char() {
                                if sc == '\n' {
                                    self.has_preceding_newline = true;
                                    self.advance_char();
                                    break;
                                }
                                self.advance_char();
                            }
                        } else if next == '*' {
                            // Multi line comment
                            self.advance_char();
                            self.advance_char();
                            while let Some(mc) = self.peek_char() {
                                if mc == '\n' {
                                    self.has_preceding_newline = true;
                                }
                                if mc == '*' && self.peek_char_at(1) == Some('/') {
                                    self.advance_char();
                                    self.advance_char();
                                    break;
                                }
                                self.advance_char();
                            }
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
    }

    pub fn next_token(&mut self) -> ScannedToken {
        self.skip_whitespace_and_comments();
        let start = self.current_pos();

        let ch = match self.advance_char() {
            Some(c) => c,
            None => {
                return ScannedToken {
                    token: Token::Eos,
                    start,
                    end: start,
                    literal: String::new(),
                };
            }
        };

        // Private identifiers (#field)
        if ch == '#' {
            if self.peek_char().map_or(false, |c| c.is_alphabetic() || c == '_' || c == '$') {
                return self.scan_identifier(start, ch);
            } else {
                return ScannedToken {
                    token: Token::Illegal,
                    start,
                    end: self.current_pos(),
                    literal: "#".to_string(),
                };
            }
        }

        // Identifiers and Keywords
        if ch.is_alphabetic() || ch == '_' || ch == '$' {
            return self.scan_identifier(start, ch);
        }

        // Numbers
        if ch.is_ascii_digit() || (ch == '.' && self.peek_char().map_or(false, |c| c.is_ascii_digit())) {
            return self.scan_number(start, ch);
        }

        // Strings
        if ch == '"' || ch == '\'' {
            return self.scan_string(start, ch);
        }

        // Template Literals
        if ch == '`' {
            return self.scan_template(start);
        }

        // Punctuators & Operators
        self.scan_operator(start, ch)
    }

    fn scan_identifier(&mut self, start: usize, first: char) -> ScannedToken {
        let mut literal = String::new();
        literal.push(first);

        while let Some(c) = self.peek_char() {
            if c.is_alphanumeric() || c == '_' || c == '$' {
                literal.push(c);
                self.advance_char();
            } else {
                break;
            }
        }

        let end = self.current_pos();
        let token = Token::lookup_keyword(&literal).unwrap_or(Token::Identifier);

        ScannedToken {
            token,
            start,
            end,
            literal,
        }
    }

    fn scan_number(&mut self, start: usize, first: char) -> ScannedToken {
        let mut literal = String::new();
        literal.push(first);

        // Hex, binary, octal prefix
        if first == '0' {
            if let Some(c) = self.peek_char() {
                if matches!(c, 'x' | 'X' | 'b' | 'B' | 'o' | 'O') {
                    literal.push(c);
                    self.advance_char();
                    while let Some(digit) = self.peek_char() {
                        if digit.is_alphanumeric() || digit == '_' {
                            literal.push(digit);
                            self.advance_char();
                        } else {
                            break;
                        }
                    }
                    let end = self.current_pos();
                    let is_bigint = literal.ends_with('n');
                    return ScannedToken {
                        token: if is_bigint { Token::BigInt } else { Token::Number },
                        start,
                        end,
                        literal,
                    };
                }
            }
        }

        // Decimal / floating point numbers
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() || c == '.' || c == '_' || c == 'e' || c == 'E' {
                literal.push(c);
                self.advance_char();
                if matches!(c, 'e' | 'E') {
                    if let Some(sign) = self.peek_char() {
                        if sign == '+' || sign == '-' {
                            literal.push(sign);
                            self.advance_char();
                        }
                    }
                }
            } else if c == 'n' {
                literal.push(c);
                self.advance_char();
                break;
            } else {
                break;
            }
        }

        let end = self.current_pos();
        let is_bigint = literal.ends_with('n');

        ScannedToken {
            token: if is_bigint { Token::BigInt } else { Token::Number },
            start,
            end,
            literal,
        }
    }

    fn scan_string(&mut self, start: usize, quote: char) -> ScannedToken {
        let mut literal = String::new();
        while let Some(c) = self.advance_char() {
            if c == quote {
                let end = self.current_pos();
                return ScannedToken {
                    token: Token::String,
                    start,
                    end,
                    literal,
                };
            }
            if c == '\\' {
                if let Some(escaped) = self.advance_char() {
                    match escaped {
                        'n' => literal.push('\n'),
                        't' => literal.push('\t'),
                        'r' => literal.push('\r'),
                        '\\' => literal.push('\\'),
                        '\'' => literal.push('\''),
                        '"' => literal.push('"'),
                        '0' => literal.push('\0'),
                        'x' => {
                            let mut hex = String::new();
                            for _ in 0..2 {
                                if let Some(h) = self.peek_char() {
                                    if h.is_ascii_hexdigit() {
                                        hex.push(h);
                                        self.advance_char();
                                    } else {
                                        break;
                                    }
                                }
                            }
                            if let Ok(val) = u8::from_str_radix(&hex, 16) {
                                literal.push(val as char);
                            } else {
                                literal.push_str(&hex);
                            }
                        }
                        'u' => {
                            if self.peek_char() == Some('{') {
                                self.advance_char();
                                let mut hex = String::new();
                                while let Some(h) = self.peek_char() {
                                    if h == '}' {
                                        self.advance_char();
                                        break;
                                    } else if h.is_ascii_hexdigit() {
                                        hex.push(h);
                                        self.advance_char();
                                    } else {
                                        break;
                                    }
                                }
                                if let Ok(val) = u32::from_str_radix(&hex, 16) {
                                    if let Some(ch) = char::from_u32(val) {
                                        literal.push(ch);
                                    }
                                }
                            } else {
                                let mut hex = String::new();
                                for _ in 0..4 {
                                    if let Some(h) = self.peek_char() {
                                        if h.is_ascii_hexdigit() {
                                            hex.push(h);
                                            self.advance_char();
                                        } else {
                                            break;
                                        }
                                    }
                                }
                                if let Ok(val) = u32::from_str_radix(&hex, 16) {
                                    if let Some(ch) = char::from_u32(val) {
                                        literal.push(ch);
                                    }
                                }
                            }
                        }
                        other => literal.push(other),
                    }
                }
            } else {
                literal.push(c);
            }
        }

        ScannedToken {
            token: Token::Illegal,
            start,
            end: self.current_pos(),
            literal,
        }
    }

    fn scan_template(&mut self, start: usize) -> ScannedToken {
        let mut literal = String::new();
        while let Some(c) = self.advance_char() {
            if c == '`' {
                let end = self.current_pos();
                return ScannedToken {
                    token: Token::TemplateTail,
                    start,
                    end,
                    literal,
                };
            }
            if c == '$' && self.peek_char() == Some('{') {
                self.advance_char();
                let end = self.current_pos();
                self.template_brace_stack.push(self.brace_depth);
                return ScannedToken {
                    token: Token::TemplateSpan,
                    start,
                    end,
                    literal,
                };
            }
            literal.push(c);
        }

        ScannedToken {
            token: Token::Illegal,
            start,
            end: self.current_pos(),
            literal,
        }
    }

    fn scan_template_continuation(&mut self, start: usize) -> ScannedToken {
        let mut literal = String::new();
        while let Some(c) = self.advance_char() {
            if c == '`' {
                let end = self.current_pos();
                return ScannedToken {
                    token: Token::TemplateTail,
                    start,
                    end,
                    literal,
                };
            }
            if c == '$' && self.peek_char() == Some('{') {
                self.advance_char();
                let end = self.current_pos();
                self.template_brace_stack.push(self.brace_depth);
                return ScannedToken {
                    token: Token::TemplateSpan,
                    start,
                    end,
                    literal,
                };
            }
            literal.push(c);
        }

        ScannedToken {
            token: Token::Illegal,
            start,
            end: self.current_pos(),
            literal,
        }
    }

    fn scan_operator(&mut self, start: usize, first: char) -> ScannedToken {
        let mut lit = String::new();
        lit.push(first);

        let tok = match first {
            '.' => {
                if self.peek_char() == Some('.') && self.peek_char_at(1) == Some('.') {
                    self.advance_char();
                    self.advance_char();
                    lit.push_str("..");
                    Token::Ellipsis
                } else {
                    Token::Period
                }
            }
            '?' => match self.peek_char() {
                Some('.') => {
                    self.advance_char();
                    lit.push('.');
                    Token::QuestionPeriod
                }
                Some('?') => {
                    self.advance_char();
                    lit.push('?');
                    if self.peek_char() == Some('=') {
                        self.advance_char();
                        lit.push('=');
                        Token::AssignNullish
                    } else {
                        Token::Nullish
                    }
                }
                _ => Token::Conditional,
            },
            '(' => Token::LeftParen,
            ')' => Token::RightParen,
            '[' => Token::LeftBracket,
            ']' => Token::RightBracket,
            '{' => {
                self.brace_depth += 1;
                Token::LeftBrace
            }
            '}' => {
                if self.template_brace_stack.last() == Some(&self.brace_depth) {
                    self.template_brace_stack.pop();
                    return self.scan_template_continuation(start);
                }
                if self.brace_depth > 0 {
                    self.brace_depth -= 1;
                }
                Token::RightBrace
            }
            ':' => Token::Colon,
            ';' => Token::Semicolon,
            ',' => Token::Comma,
            '~' => Token::BitNot,

            '=' => match self.peek_char() {
                Some('=') => {
                    self.advance_char();
                    lit.push('=');
                    if self.peek_char() == Some('=') {
                        self.advance_char();
                        lit.push('=');
                        Token::EqStrict
                    } else {
                        Token::Eq
                    }
                }
                Some('>') => {
                    self.advance_char();
                    lit.push('>');
                    Token::Arrow
                }
                _ => Token::Assign,
            },

            '+' => match self.peek_char() {
                Some('+') => {
                    self.advance_char();
                    lit.push('+');
                    Token::Inc
                }
                Some('=') => {
                    self.advance_char();
                    lit.push('=');
                    Token::AssignAdd
                }
                _ => Token::Add,
            },

            '-' => match self.peek_char() {
                Some('-') => {
                    self.advance_char();
                    lit.push('-');
                    Token::Dec
                }
                Some('=') => {
                    self.advance_char();
                    lit.push('=');
                    Token::AssignSub
                }
                _ => Token::Sub,
            },

            '*' => match self.peek_char() {
                Some('*') => {
                    self.advance_char();
                    lit.push('*');
                    if self.peek_char() == Some('=') {
                        self.advance_char();
                        lit.push('=');
                        Token::AssignExp
                    } else {
                        Token::Exp
                    }
                }
                Some('=') => {
                    self.advance_char();
                    lit.push('=');
                    Token::AssignMul
                }
                _ => Token::Mul,
            },

            '/' => match self.peek_char() {
                Some('=') => {
                    self.advance_char();
                    lit.push('=');
                    Token::AssignDiv
                }
                _ => Token::Div,
            },

            '%' => match self.peek_char() {
                Some('=') => {
                    self.advance_char();
                    lit.push('=');
                    Token::AssignMod
                }
                _ => Token::Mod,
            },

            '!' => match self.peek_char() {
                Some('=') => {
                    self.advance_char();
                    lit.push('=');
                    if self.peek_char() == Some('=') {
                        self.advance_char();
                        lit.push('=');
                        Token::NotEqStrict
                    } else {
                        Token::NotEq
                    }
                }
                _ => Token::Not,
            },

            '<' => match self.peek_char() {
                Some('<') => {
                    self.advance_char();
                    lit.push('<');
                    if self.peek_char() == Some('=') {
                        self.advance_char();
                        lit.push('=');
                        Token::AssignShl
                    } else {
                        Token::Shl
                    }
                }
                Some('=') => {
                    self.advance_char();
                    lit.push('=');
                    Token::Lte
                }
                _ => Token::Lt,
            },

            '>' => match self.peek_char() {
                Some('>') => {
                    self.advance_char();
                    lit.push('>');
                    match self.peek_char() {
                        Some('>') => {
                            self.advance_char();
                            lit.push('>');
                            if self.peek_char() == Some('=') {
                                self.advance_char();
                                lit.push('=');
                                Token::AssignShr
                            } else {
                                Token::Shr
                            }
                        }
                        Some('=') => {
                            self.advance_char();
                            lit.push('=');
                            Token::AssignSar
                        }
                        _ => Token::Sar,
                    }
                }
                Some('=') => {
                    self.advance_char();
                    lit.push('=');
                    Token::Gte
                }
                _ => Token::Gt,
            },

            '&' => match self.peek_char() {
                Some('&') => {
                    self.advance_char();
                    lit.push('&');
                    if self.peek_char() == Some('=') {
                        self.advance_char();
                        lit.push('=');
                        Token::AssignLogicalAnd
                    } else {
                        Token::And
                    }
                }
                Some('=') => {
                    self.advance_char();
                    lit.push('=');
                    Token::AssignAnd
                }
                _ => Token::BitAnd,
            },

            '|' => match self.peek_char() {
                Some('|') => {
                    self.advance_char();
                    lit.push('|');
                    if self.peek_char() == Some('=') {
                        self.advance_char();
                        lit.push('=');
                        Token::AssignLogicalOr
                    } else {
                        Token::Or
                    }
                }
                Some('=') => {
                    self.advance_char();
                    lit.push('=');
                    Token::AssignOr
                }
                _ => Token::BitOr,
            },

            '^' => match self.peek_char() {
                Some('=') => {
                    self.advance_char();
                    lit.push('=');
                    Token::AssignXor
                }
                _ => Token::BitXor,
            },

            _ => Token::Illegal,
        };

        let end = self.current_pos();
        ScannedToken {
            token: tok,
            start,
            end,
            literal: lit,
        }
    }

    /// Scans regular expression body and flags after a leading '/' or '/=' was encountered in primary position.
    pub fn scan_regexp_body_and_flags(&mut self, is_assign_div: bool) -> Result<(String, String), String> {
        let mut pattern = String::new();
        if is_assign_div {
            pattern.push('=');
        }

        let mut in_class = false;
        let mut terminated = false;

        while let Some(c) = self.advance_char() {
            if c == '\\' {
                pattern.push(c);
                if let Some(next) = self.advance_char() {
                    pattern.push(next);
                }
            } else if c == '[' {
                in_class = true;
                pattern.push(c);
            } else if c == ']' && in_class {
                in_class = false;
                pattern.push(c);
            } else if c == '/' && !in_class {
                terminated = true;
                break;
            } else if c == '\n' || c == '\r' {
                return Err("SyntaxError: Unterminated regular expression literal".to_string());
            } else {
                pattern.push(c);
            }
        }

        if !terminated {
            return Err("SyntaxError: Unterminated regular expression literal".to_string());
        }

        let mut flags = String::new();
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphabetic() {
                flags.push(c);
                self.advance_char();
            } else {
                break;
            }
        }

        Ok((pattern, flags))
    }
}
