//! Safe Rust reimplementation of Google V8's RegExp Parser (`src/regexp/regexp-parser.h`).
//!
//! Implements ECMAScript regular expression parsing into AST representations,
//! supporting character classes, quantifiers, groups, lookaround, escapes, and assertions.

use super::ast::{AssertionType, RegExpFlags, RegExpNode};
use std::collections::HashMap;

pub struct RegExpParser {
    chars: Vec<char>,
    cursor: usize,
    pub flags: RegExpFlags,
    pub capture_count: usize,
    pub named_groups: HashMap<String, usize>,
}

impl RegExpParser {
    pub fn new(pattern: &str, flags: RegExpFlags) -> Self {
        Self {
            chars: pattern.chars().collect(),
            cursor: 0,
            flags,
            capture_count: 0,
            named_groups: HashMap::new(),
        }
    }

    #[inline]
    fn peek(&self) -> Option<char> {
        self.chars.get(self.cursor).copied()
    }

    #[inline]
    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.cursor + offset).copied()
    }

    #[inline]
    fn advance(&mut self) -> Option<char> {
        if self.cursor < self.chars.len() {
            let c = self.chars[self.cursor];
            self.cursor += 1;
            Some(c)
        } else {
            None
        }
    }

    #[inline]
    fn matches(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Parses the entire regex pattern.
    pub fn parse(&mut self) -> Result<RegExpNode, String> {
        let node = self.parse_disjunction()?;
        if self.cursor < self.chars.len() {
            return Err(format!(
                "SyntaxError: Unexpected character '{}' at index {}",
                self.chars[self.cursor], self.cursor
            ));
        }
        Ok(node)
    }

    /// Parses disjunctions: `alt1 | alt2 | alt3`
    fn parse_disjunction(&mut self) -> Result<RegExpNode, String> {
        let mut alternatives = vec![self.parse_sequence()?];

        while self.matches('|') {
            alternatives.push(self.parse_sequence()?);
        }

        if alternatives.len() == 1 {
            Ok(alternatives.pop().unwrap())
        } else {
            Ok(RegExpNode::Disjunction(alternatives))
        }
    }

    /// Parses a sequence of terms (atoms + quantifiers).
    fn parse_sequence(&mut self) -> Result<RegExpNode, String> {
        let mut elements = Vec::new();

        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            let term = self.parse_term()?;
            elements.push(term);
        }

        if elements.is_empty() {
            Ok(RegExpNode::Empty)
        } else if elements.len() == 1 {
            Ok(elements.pop().unwrap())
        } else {
            Ok(RegExpNode::Sequence(elements))
        }
    }

    /// Parses a single term: an atom optionally followed by a quantifier.
    fn parse_term(&mut self) -> Result<RegExpNode, String> {
        let atom = self.parse_atom()?;
        self.parse_quantifier(atom)
    }

    /// Parses an atom (char, class, group, assertion, escape).
    fn parse_atom(&mut self) -> Result<RegExpNode, String> {
        let c = match self.peek() {
            Some(c) => c,
            None => return Ok(RegExpNode::Empty),
        };

        match c {
            '^' => {
                self.advance();
                Ok(RegExpNode::Assertion(AssertionType::StartOfInput))
            }
            '$' => {
                self.advance();
                Ok(RegExpNode::Assertion(AssertionType::EndOfInput))
            }
            '.' => {
                self.advance();
                Ok(RegExpNode::AnyChar)
            }
            '(' => self.parse_group(),
            '[' => self.parse_character_class(),
            '\\' => {
                self.advance(); // consume '\\'
                self.parse_escape()
            }
            '*' | '+' | '?' => {
                Err(format!("SyntaxError: Nothing to repeat at offset {}", self.cursor))
            }
            '{' => {
                // If it looks like a quantifier without atom, check or treat as literal
                if self.looks_like_quantifier() {
                    Err(format!("SyntaxError: Nothing to repeat at offset {}", self.cursor))
                } else {
                    self.advance();
                    Ok(RegExpNode::Char('{'))
                }
            }
            _ => {
                self.advance();
                Ok(RegExpNode::Char(c))
            }
        }
    }

    /// Checks if `{...}` is a valid quantifier.
    fn looks_like_quantifier(&self) -> bool {
        let mut i = 1;
        while let Some(c) = self.peek_at(i) {
            if c.is_ascii_digit() || c == ',' {
                i += 1;
            } else if c == '}' {
                return true;
            } else {
                return false;
            }
        }
        false
    }

    /// Parses groups: `(?:...)`, `(?=...)`, `(?!...)`, `(?<=...)`, `(?<!...)`, `(?<name>...)`, `(...)`
    fn parse_group(&mut self) -> Result<RegExpNode, String> {
        self.advance(); // consume '('

        if self.matches('?') {
            if self.matches(':') {
                // Non-capturing group
                let body = Box::new(self.parse_disjunction()?);
                if !self.matches(')') {
                    return Err("SyntaxError: Unterminated group '(?:'".to_string());
                }
                Ok(RegExpNode::NonCapturing(body))
            } else if self.matches('=') {
                // Positive lookahead
                let body = Box::new(self.parse_disjunction()?);
                if !self.matches(')') {
                    return Err("SyntaxError: Unterminated lookahead '(?='".to_string());
                }
                Ok(RegExpNode::Lookaround {
                    is_positive: true,
                    is_lookbehind: false,
                    body,
                })
            } else if self.matches('!') {
                // Negative lookahead
                let body = Box::new(self.parse_disjunction()?);
                if !self.matches(')') {
                    return Err("SyntaxError: Unterminated lookahead '(?!'".to_string());
                }
                Ok(RegExpNode::Lookaround {
                    is_positive: false,
                    is_lookbehind: false,
                    body,
                })
            } else if self.matches('<') {
                if self.matches('=') {
                    // Positive lookbehind
                    let body = Box::new(self.parse_disjunction()?);
                    if !self.matches(')') {
                        return Err("SyntaxError: Unterminated lookbehind '(?<='".to_string());
                    }
                    Ok(RegExpNode::Lookaround {
                        is_positive: true,
                        is_lookbehind: true,
                        body,
                    })
                } else if self.matches('!') {
                    // Negative lookbehind
                    let body = Box::new(self.parse_disjunction()?);
                    if !self.matches(')') {
                        return Err("SyntaxError: Unterminated lookbehind '(?<!'".to_string());
                    }
                    Ok(RegExpNode::Lookaround {
                        is_positive: false,
                        is_lookbehind: true,
                        body,
                    })
                } else {
                    // Named capture group: `(?<name>...)`
                    let mut name = String::new();
                    while let Some(c) = self.peek() {
                        if c == '>' {
                            self.advance();
                            break;
                        }
                        name.push(c);
                        self.advance();
                    }
                    if name.is_empty() {
                        return Err("SyntaxError: Empty group name in '(?<>'".to_string());
                    }
                    self.capture_count += 1;
                    let index = self.capture_count;
                    self.named_groups.insert(name.clone(), index);

                    let body = Box::new(self.parse_disjunction()?);
                    if !self.matches(')') {
                        return Err(format!("SyntaxError: Unterminated group '(?<{}>'", name));
                    }
                    Ok(RegExpNode::Capture {
                        index,
                        name: Some(name),
                        body,
                    })
                }
            } else {
                Err("SyntaxError: Invalid group '?...'".to_string())
            }
        } else {
            // Normal capturing group
            self.capture_count += 1;
            let index = self.capture_count;
            let body = Box::new(self.parse_disjunction()?);
            if !self.matches(')') {
                return Err("SyntaxError: Unterminated capturing group '('".to_string());
            }
            Ok(RegExpNode::Capture {
                index,
                name: None,
                body,
            })
        }
    }

    /// Parses character class `[...]` or `[^...]`
    fn parse_character_class(&mut self) -> Result<RegExpNode, String> {
        self.advance(); // consume '['

        let negated = self.matches('^');
        let mut ranges = Vec::new();
        let mut first = true;

        while let Some(c) = self.peek() {
            if c == ']' && !first {
                self.advance();
                return Ok(RegExpNode::CharacterClass { ranges, negated });
            }
            first = false;

            let start_char = if c == '\\' {
                self.advance();
                self.parse_class_escape(&mut ranges)?
            } else {
                self.advance();
                Some(c)
            };

            if let Some(sc) = start_char {
                if self.peek() == Some('-') && self.peek_at(1) != Some(']') {
                    self.advance(); // consume '-'
                    let end_char = if self.peek() == Some('\\') {
                        self.advance();
                        self.parse_class_escape(&mut ranges)?
                    } else {
                        self.advance()
                    };

                    if let Some(ec) = end_char {
                        if sc > ec {
                            return Err(format!(
                                "SyntaxError: Range out of order in character class: '{}-{}'",
                                sc, ec
                            ));
                        }
                        ranges.push((sc, ec));
                    } else {
                        ranges.push((sc, sc));
                        ranges.push(('-', '-'));
                    }
                } else {
                    ranges.push((sc, sc));
                }
            }
        }

        Err("SyntaxError: Unterminated character class '['".to_string())
    }

    /// Parses escapes inside character classes.
    fn parse_class_escape(&mut self, ranges: &mut Vec<(char, char)>) -> Result<Option<char>, String> {
        let c = match self.advance() {
            Some(c) => c,
            None => return Err("SyntaxError: Incomplete escape in character class".to_string()),
        };

        match c {
            'd' => {
                ranges.extend(RegExpNode::digits_ranges());
                Ok(None)
            }
            'D' => {
                // Inverted digits: 0..='0'-1, '9'+1..=MAX
                ranges.push(('\0', '/'));
                ranges.push((':', '\u{10ffff}'));
                Ok(None)
            }
            'w' => {
                ranges.extend(RegExpNode::word_ranges());
                Ok(None)
            }
            'W' => {
                ranges.push(('\0', '0' as u32 as u8 as char)); // non-word ranges
                ranges.push(('\0', '/'));
                ranges.push((':', '@'));
                ranges.push(('[', '^'));
                ranges.push(('`', '`'));
                ranges.push(('{', '\u{10ffff}'));
                Ok(None)
            }
            's' => {
                ranges.push((' ', ' '));
                ranges.push(('\t', '\r'));
                ranges.push(('\u{00a0}', '\u{00a0}'));
                ranges.push(('\u{feff}', '\u{feff}'));
                Ok(None)
            }
            't' => Ok(Some('\t')),
            'r' => Ok(Some('\r')),
            'n' => Ok(Some('\n')),
            'v' => Ok(Some('\x0b')),
            'f' => Ok(Some('\x0c')),
            '0' => Ok(Some('\0')),
            'b' => Ok(Some('\x08')), // Backspace inside character class
            'x' => self.parse_hex_escape(2).map(Some),
            'u' => self.parse_unicode_escape().map(Some),
            _ => Ok(Some(c)),
        }
    }

    /// Parses escapes in normal regex context.
    fn parse_escape(&mut self) -> Result<RegExpNode, String> {
        let c = match self.advance() {
            Some(c) => c,
            None => return Err("SyntaxError: Incomplete escape sequence '\\'".to_string()),
        };

        match c {
            'd' => Ok(RegExpNode::CharacterClass {
                ranges: RegExpNode::digits_ranges(),
                negated: false,
            }),
            'D' => Ok(RegExpNode::CharacterClass {
                ranges: RegExpNode::digits_ranges(),
                negated: true,
            }),
            'w' => Ok(RegExpNode::CharacterClass {
                ranges: RegExpNode::word_ranges(),
                negated: false,
            }),
            'W' => Ok(RegExpNode::CharacterClass {
                ranges: RegExpNode::word_ranges(),
                negated: true,
            }),
            's' => Ok(RegExpNode::CharacterClass {
                ranges: vec![
                    (' ', ' '),
                    ('\t', '\r'),
                    ('\u{00a0}', '\u{00a0}'),
                    ('\u{feff}', '\u{feff}'),
                ],
                negated: false,
            }),
            'S' => Ok(RegExpNode::CharacterClass {
                ranges: vec![
                    (' ', ' '),
                    ('\t', '\r'),
                    ('\u{00a0}', '\u{00a0}'),
                    ('\u{feff}', '\u{feff}'),
                ],
                negated: true,
            }),
            'b' => Ok(RegExpNode::Assertion(AssertionType::WordBoundary)),
            'B' => Ok(RegExpNode::Assertion(AssertionType::NonWordBoundary)),
            't' => Ok(RegExpNode::Char('\t')),
            'r' => Ok(RegExpNode::Char('\r')),
            'n' => Ok(RegExpNode::Char('\n')),
            'v' => Ok(RegExpNode::Char('\x0b')),
            'f' => Ok(RegExpNode::Char('\x0c')),
            '0' => Ok(RegExpNode::Char('\0')),
            '1'..='9' => {
                let mut num = (c as u32 - '0' as u32) as usize;
                while let Some(next_c) = self.peek() {
                    if next_c.is_ascii_digit() {
                        num = num * 10 + (next_c as u32 - '0' as u32) as usize;
                        self.advance();
                    } else {
                        break;
                    }
                }
                Ok(RegExpNode::BackReference(num))
            }
            'x' => {
                let code = self.parse_hex_escape(2)?;
                Ok(RegExpNode::Char(code))
            }
            'u' => {
                let code = self.parse_unicode_escape()?;
                Ok(RegExpNode::Char(code))
            }
            _ => Ok(RegExpNode::Char(c)),
        }
    }

    /// Parses hexadecimal escape `\xHH`.
    fn parse_hex_escape(&mut self, len: usize) -> Result<char, String> {
        let mut hex = String::new();
        for _ in 0..len {
            if let Some(c) = self.advance() {
                if c.is_ascii_hexdigit() {
                    hex.push(c);
                } else {
                    return Err(format!("SyntaxError: Invalid hex digit '{}'", c));
                }
            } else {
                return Err("SyntaxError: Unterminated hex escape".to_string());
            }
        }
        let code = u32::from_str_radix(&hex, 16).map_err(|e| e.to_string())?;
        char::from_u32(code).ok_or_else(|| "SyntaxError: Invalid Unicode codepoint".to_string())
    }

    /// Parses Unicode escape `\uHHHH` or `\u{H...}`.
    fn parse_unicode_escape(&mut self) -> Result<char, String> {
        if self.peek() == Some('{') {
            self.advance();
            let mut hex = String::new();
            while let Some(c) = self.peek() {
                if c == '}' {
                    self.advance();
                    break;
                }
                if c.is_ascii_hexdigit() {
                    hex.push(c);
                    self.advance();
                } else {
                    return Err(format!("SyntaxError: Invalid hex in unicode escape: '{}'", c));
                }
            }
            let code = u32::from_str_radix(&hex, 16).map_err(|e| e.to_string())?;
            char::from_u32(code).ok_or_else(|| "SyntaxError: Invalid Unicode codepoint".to_string())
        } else {
            self.parse_hex_escape(4)
        }
    }

    /// Parses quantifiers after an atom: `*`, `+`, `?`, `{m,n}`, and handles lazy `?`.
    fn parse_quantifier(&mut self, atom: RegExpNode) -> Result<RegExpNode, String> {
        let (min, max) = match self.peek() {
            Some('*') => {
                self.advance();
                (0, None)
            }
            Some('+') => {
                self.advance();
                (1, None)
            }
            Some('?') => {
                self.advance();
                (0, Some(1))
            }
            Some('{') => {
                if self.looks_like_quantifier() {
                    self.advance(); // consume '{'
                    let mut min_str = String::new();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() {
                            min_str.push(c);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    let min: usize = min_str.parse().map_err(|_| "SyntaxError: Invalid number in quantifier".to_string())?;

                    let max = if self.matches(',') {
                        let mut max_str = String::new();
                        while let Some(c) = self.peek() {
                            if c.is_ascii_digit() {
                                max_str.push(c);
                                self.advance();
                            } else {
                                break;
                            }
                        }
                        if max_str.is_empty() {
                            None
                        } else {
                            let m: usize = max_str.parse().map_err(|_| "SyntaxError: Invalid number in quantifier".to_string())?;
                            if m < min {
                                return Err("SyntaxError: numbers out of order in {} quantifier".to_string());
                            }
                            Some(m)
                        }
                    } else {
                        Some(min)
                    };

                    if !self.matches('}') {
                        return Err("SyntaxError: Unterminated quantifier '{'".to_string());
                    }

                    (min, max)
                } else {
                    return Ok(atom);
                }
            }
            _ => return Ok(atom),
        };

        // Check for non-greedy (lazy) flag: `?`
        let greedy = !self.matches('?');

        Ok(RegExpNode::Quantifier {
            min,
            max,
            greedy,
            body: Box::new(atom),
        })
    }
}
