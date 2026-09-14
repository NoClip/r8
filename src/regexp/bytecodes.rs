//! Safe Rust reimplementation of Google V8's Irregexp Bytecodes (`src/regexp/regexp-bytecodes.h`).
//!
//! Provides the specialized instruction set for the Irregexp backtracking VM,
//! opcode definitions, and bytecode representation.

use super::ast::RegExpFlags;
use std::collections::HashMap;

/// Irregexp VM instruction opcodes.
#[derive(Clone, Debug, PartialEq)]
pub enum RegExpOpcode {
    /// Matches a single character, advances cursor by 1 on match; jumps to `on_fail` otherwise.
    CheckCharacter {
        c: char,
        case_insensitive: bool,
        on_fail: usize,
    },

    /// Matches a character class, advances cursor by 1 on match; jumps to `on_fail` otherwise.
    CheckCharacterClass {
        ranges: Vec<(char, char)>,
        negated: bool,
        case_insensitive: bool,
        on_fail: usize,
    },

    /// Matches any character (`.`), advances cursor by 1 on match; jumps to `on_fail` otherwise.
    CheckAnyCharacter {
        dot_all: bool,
        on_fail: usize,
    },

    /// Assertion `^`: checks start of subject or newline in multiline mode.
    CheckNotAtStart {
        multiline: bool,
        on_fail: usize,
    },

    /// Assertion `$`: checks end of subject or newline in multiline mode.
    CheckNotAtEnd {
        multiline: bool,
        on_fail: usize,
    },

    /// Assertion `\b` or `\B`: word boundary test.
    CheckWordBoundary {
        negated: bool,
        on_fail: usize,
    },

    /// Backreference check: compares current chars with captured group at `reg` and `reg + 1`.
    CheckBackReference {
        reg: usize,
        case_insensitive: bool,
        on_fail: usize,
    },

    /// Sets `registers[reg] = current_position`.
    SetRegisterCurrentPosition {
        reg: usize,
    },

    /// Sets `registers[reg] = value`.
    SetRegister {
        reg: usize,
        value: i32,
    },

    /// Pushes a backtrack point to resume at `target_pc` if subsequent instructions fail.
    PushBacktrack {
        target_pc: usize,
    },

    /// Discards the top backtrack point.
    PopBacktrack,

    /// Unconditional jump to target instruction index.
    Jump {
        target: usize,
    },

    /// Saves cursor position and initiates lookaround assertion.
    BeginLookaround {
        is_positive: bool,
        is_lookbehind: bool,
        on_fail: usize,
    },

    /// Completes lookaround assertion, restoring original position.
    EndLookaround {
        is_positive: bool,
    },

    /// Explicit failure: triggers backtrack stack pop or match failure.
    Fail,

    /// Successful match completion.
    Succeed,
}

/// A compiled Irregexp program ready for execution by the Irregexp Interpreter.
#[derive(Clone, Debug, PartialEq)]
pub struct RegExpBytecode {
    pub instructions: Vec<RegExpOpcode>,
    pub num_registers: usize,
    pub capture_count: usize,
    pub named_groups: HashMap<String, usize>,
    pub flags: RegExpFlags,
    pub pattern: String,
}

impl RegExpBytecode {
    /// Creates a new bytecode container.
    pub fn new(
        instructions: Vec<RegExpOpcode>,
        num_registers: usize,
        capture_count: usize,
        named_groups: HashMap<String, usize>,
        flags: RegExpFlags,
        pattern: String,
    ) -> Self {
        Self {
            instructions,
            num_registers,
            capture_count,
            named_groups,
            flags,
            pattern,
        }
    }
}
