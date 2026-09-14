//! Safe Rust reimplementation of Google V8's RegExp Abstract Syntax Tree (`src/regexp/regexp-ast.h`).
//!
//! Defines the AST node hierarchy representing parsed ECMAScript regular expressions,
//! along with flag bitfields, character class representations, and assertions.

use std::collections::BTreeSet;
use std::fmt;

/// ECMAScript RegExp flags bitfield.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RegExpFlags {
    pub has_indices: bool,  // 'd'
    pub global: bool,       // 'g'
    pub ignore_case: bool,  // 'i'
    pub multiline: bool,    // 'm'
    pub dot_all: bool,      // 's'
    pub unicode: bool,      // 'u'
    pub unicode_sets: bool, // 'v'
    pub sticky: bool,       // 'y'
}

impl RegExpFlags {
    /// Parses flags string (e.g. "gim"), validating duplicate or invalid flag characters.
    pub fn parse(flags_str: &str) -> Result<Self, String> {
        let mut flags = RegExpFlags::default();
        let mut seen = BTreeSet::new();

        for c in flags_str.chars() {
            if !seen.insert(c) {
                return Err(format!("SyntaxError: Duplicate RegExp flag '{}'", c));
            }
            match c {
                'd' => flags.has_indices = true,
                'g' => flags.global = true,
                'i' => flags.ignore_case = true,
                'm' => flags.multiline = true,
                's' => flags.dot_all = true,
                'u' => flags.unicode = true,
                'v' => flags.unicode_sets = true,
                'y' => flags.sticky = true,
                _ => return Err(format!("SyntaxError: Invalid RegExp flag '{}'", c)),
            }
        }
        if flags.unicode && flags.unicode_sets {
            return Err("SyntaxError: Cannot use both 'u' and 'v' RegExp flags".to_string());
        }
        Ok(flags)
    }

    /// Returns the canonical sorted flag string in ECMAScript standard order: "dgimsuyv" / "dgimsvuy".
    pub fn to_string_canonical(&self) -> String {
        let mut s = String::new();
        if self.has_indices {
            s.push('d');
        }
        if self.global {
            s.push('g');
        }
        if self.ignore_case {
            s.push('i');
        }
        if self.multiline {
            s.push('m');
        }
        if self.dot_all {
            s.push('s');
        }
        if self.unicode {
            s.push('u');
        }
        if self.unicode_sets {
            s.push('v');
        }
        if self.sticky {
            s.push('y');
        }
        s
    }
}

impl fmt::Display for RegExpFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_canonical())
    }
}

/// Boundary and position assertions in regular expressions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AssertionType {
    /// `^` matches beginning of input or line (if multiline)
    StartOfInput,
    /// `$` matches end of input or line (if multiline)
    EndOfInput,
    /// `\b` word boundary
    WordBoundary,
    /// `\B` non-word boundary
    NonWordBoundary,
}

/// Represents an ECMAScript Regular Expression AST node.
#[derive(Clone, Debug, PartialEq)]
pub enum RegExpNode {
    /// Empty string match.
    Empty,
    /// Single literal character.
    Char(char),
    /// String of literal characters.
    Text(String),
    /// Character class `[...]` or predefined class like `\d`, `\w`, `\s`.
    CharacterClass {
        ranges: Vec<(char, char)>,
        negated: bool,
    },
    /// Any character `.` (matches everything if `dot_all`, else all except line terminators).
    AnyChar,
    /// Quantifier: min, max (None = unbounded), greedy flag, and body.
    Quantifier {
        min: usize,
        max: Option<usize>,
        greedy: bool,
        body: Box<RegExpNode>,
    },
    /// Capturing group `(...)` or named capturing group `(?<name>...)`.
    Capture {
        index: usize,
        name: Option<String>,
        body: Box<RegExpNode>,
    },
    /// Non-capturing group `(?:...)`.
    NonCapturing(Box<RegExpNode>),
    /// Numeric backreference to a previous capturing group: `\1`, `\2`, etc.
    BackReference(usize),
    /// Disjunction `a|b|c`.
    Disjunction(Vec<RegExpNode>),
    /// Sequence (concatenation) of nodes: `abc`.
    Sequence(Vec<RegExpNode>),
    /// Zero-width position assertion (`^`, `$`, `\b`, `\B`).
    Assertion(AssertionType),
    /// Lookahead / Lookbehind zero-width assertion: `(?=...)`, `(?!...)`, `(?<=...)`, `(?<!...)`.
    Lookaround {
        is_positive: bool,
        is_lookbehind: bool,
        body: Box<RegExpNode>,
    },
}

impl RegExpNode {
    /// Helper to test if a character matches a set of character class ranges.
    pub fn matches_char_class(ranges: &[(char, char)], negated: bool, case_insensitive: bool, c: char) -> bool {
        let test_char = if case_insensitive {
            c.to_lowercase().next().unwrap_or(c)
        } else {
            c
        };

        let mut matched = false;
        for &(start, end) in ranges {
            let (r_start, r_end) = if case_insensitive {
                (
                    start.to_lowercase().next().unwrap_or(start),
                    end.to_lowercase().next().unwrap_or(end),
                )
            } else {
                (start, end)
            };

            if test_char >= r_start && test_char <= r_end {
                matched = true;
                break;
            }
            if case_insensitive {
                let up = c.to_uppercase().next().unwrap_or(c);
                let up_s = start.to_uppercase().next().unwrap_or(start);
                let up_e = end.to_uppercase().next().unwrap_or(end);
                if up >= up_s && up <= up_e {
                    matched = true;
                    break;
                }
            }
        }

        if negated {
            !matched
        } else {
            matched
        }
    }

    /// Standard digit ranges: `\d`
    pub fn digits_ranges() -> Vec<(char, char)> {
        vec![('0', '9')]
    }

    /// Standard word character ranges: `\w` (`[a-zA-Z0-9_]`)
    pub fn word_ranges() -> Vec<(char, char)> {
        vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')]
    }

    /// Standard whitespace characters: `\s`
    pub fn is_whitespace(c: char) -> bool {
        matches!(
            c,
            ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' | '\u{00a0}' | '\u{feff}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{3000}'
        )
    }

    /// Standard word character check.
    pub fn is_word_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
    }
}
