//! Safe Rust reimplementation of Google V8's Irregexp Interpreter (`src/regexp/regexp-interpreter.h`).
//!
//! Provides a safe, backtracking virtual machine executing Irregexp bytecode against
//! subject strings with a heap-allocated backtrack stack to avoid native stack overflow.

use super::ast::RegExpNode;
use super::bytecodes::{RegExpBytecode, RegExpOpcode};
use super::compiler::FAIL_TARGET;
use std::collections::HashMap;

/// A matched regular expression result.
#[derive(Clone, Debug, PartialEq)]
pub struct RegExpMatch {
    pub start: usize,
    pub end: usize,
    pub captures: Vec<Option<String>>,
    pub capture_indices: Vec<Option<(usize, usize)>>,
    pub named_groups: HashMap<String, String>,
}

#[derive(Clone, Debug)]
struct BacktrackEntry {
    pc: usize,
    cp: usize,
    registers: Vec<i32>,
    lookaround_depth: usize,
}

#[derive(Clone, Debug)]
struct LookaroundFrame {
    saved_cp: usize,
    is_positive: bool,
    _is_lookbehind: bool,
}

pub struct RegExpInterpreter<'a> {
    bytecode: &'a RegExpBytecode,
    chars: Vec<char>,
}

impl<'a> RegExpInterpreter<'a> {
    pub fn new(bytecode: &'a RegExpBytecode, subject: &str) -> Self {
        Self {
            bytecode,
            chars: subject.chars().collect(),
        }
    }

    /// Executes the bytecode against the subject string starting from `start_pos`.
    pub fn execute(&self, start_pos: usize) -> Option<RegExpMatch> {
        let subject_len = self.chars.len();
        if start_pos > subject_len {
            return None;
        }

        // If sticky flag 'y' is set, match ONLY at start_pos
        if self.bytecode.flags.sticky {
            return self.match_at(start_pos);
        }

        // If anchored at start '^' and not multiline, match only at 0
        let is_anchored_start = !self.bytecode.flags.multiline
            && matches!(
                self.bytecode.instructions.get(1),
                Some(RegExpOpcode::CheckNotAtStart { .. })
            );

        if is_anchored_start {
            if start_pos == 0 {
                return self.match_at(0);
            } else {
                return None;
            }
        }

        // Search starting from start_pos up to subject_len
        for pos in start_pos..=subject_len {
            if let Some(m) = self.match_at(pos) {
                return Some(m);
            }
        }

        None
    }

    /// Attempts a match starting at exact position `initial_cp`.
    fn match_at(&self, initial_cp: usize) -> Option<RegExpMatch> {
        let mut pc = 0;
        let mut cp = initial_cp;
        let mut registers = vec![-1i32; self.bytecode.num_registers];
        let mut backtrack_stack: Vec<BacktrackEntry> = Vec::new();
        let mut lookaround_stack: Vec<LookaroundFrame> = Vec::new();
        let mut last_push_state: Option<(usize, usize)> = None; // (pc, cp) to avoid zero-width infinite loops

        let instructions = &self.bytecode.instructions;
        let total_instructions = instructions.len();

        while pc < total_instructions {
            let op = &instructions[pc];

            match op {
                RegExpOpcode::CheckCharacter {
                    c,
                    case_insensitive,
                    on_fail,
                } => {
                    let mut matched = false;
                    if cp < self.chars.len() {
                        let sc = self.chars[cp];
                        if *case_insensitive {
                            matched = sc.to_lowercase().collect::<String>()
                                == c.to_lowercase().collect::<String>()
                                || sc.to_uppercase().collect::<String>()
                                    == c.to_uppercase().collect::<String>();
                        } else {
                            matched = sc == *c;
                        }
                    }

                    if matched {
                        cp += 1;
                        pc += 1;
                    } else if !self.fail(
                        *on_fail,
                        &mut pc,
                        &mut cp,
                        &mut registers,
                        &mut backtrack_stack,
                        &mut lookaround_stack,
                    ) {
                        return None;
                    }
                }

                RegExpOpcode::CheckCharacterClass {
                    ranges,
                    negated,
                    case_insensitive,
                    on_fail,
                } => {
                    let mut matched = false;
                    if cp < self.chars.len() {
                        let sc = self.chars[cp];
                        matched = RegExpNode::matches_char_class(
                            ranges,
                            *negated,
                            *case_insensitive,
                            sc,
                        );
                    }

                    if matched {
                        cp += 1;
                        pc += 1;
                    } else if !self.fail(
                        *on_fail,
                        &mut pc,
                        &mut cp,
                        &mut registers,
                        &mut backtrack_stack,
                        &mut lookaround_stack,
                    ) {
                        return None;
                    }
                }

                RegExpOpcode::CheckAnyCharacter { dot_all, on_fail } => {
                    let mut matched = false;
                    if cp < self.chars.len() {
                        let sc = self.chars[cp];
                        if *dot_all {
                            matched = true;
                        } else {
                            matched = sc != '\n' && sc != '\r' && sc != '\u{2028}' && sc != '\u{2029}';
                        }
                    }

                    if matched {
                        cp += 1;
                        pc += 1;
                    } else if !self.fail(
                        *on_fail,
                        &mut pc,
                        &mut cp,
                        &mut registers,
                        &mut backtrack_stack,
                        &mut lookaround_stack,
                    ) {
                        return None;
                    }
                }

                RegExpOpcode::CheckNotAtStart { multiline, on_fail } => {
                    let at_start = cp == 0
                        || (*multiline
                            && cp > 0
                            && (self.chars[cp - 1] == '\n' || self.chars[cp - 1] == '\r'));

                    if at_start {
                        pc += 1;
                    } else if !self.fail(
                        *on_fail,
                        &mut pc,
                        &mut cp,
                        &mut registers,
                        &mut backtrack_stack,
                        &mut lookaround_stack,
                    ) {
                        return None;
                    }
                }

                RegExpOpcode::CheckNotAtEnd { multiline, on_fail } => {
                    let at_end = cp == self.chars.len()
                        || (*multiline
                            && cp < self.chars.len()
                            && (self.chars[cp] == '\n' || self.chars[cp] == '\r'));

                    if at_end {
                        pc += 1;
                    } else if !self.fail(
                        *on_fail,
                        &mut pc,
                        &mut cp,
                        &mut registers,
                        &mut backtrack_stack,
                        &mut lookaround_stack,
                    ) {
                        return None;
                    }
                }

                RegExpOpcode::CheckWordBoundary { negated, on_fail } => {
                    let prev_is_word = if cp > 0 {
                        RegExpNode::is_word_char(self.chars[cp - 1])
                    } else {
                        false
                    };

                    let curr_is_word = if cp < self.chars.len() {
                        RegExpNode::is_word_char(self.chars[cp])
                    } else {
                        false
                    };

                    let is_boundary = prev_is_word != curr_is_word;
                    let matched = if *negated { !is_boundary } else { is_boundary };

                    if matched {
                        pc += 1;
                    } else if !self.fail(
                        *on_fail,
                        &mut pc,
                        &mut cp,
                        &mut registers,
                        &mut backtrack_stack,
                        &mut lookaround_stack,
                    ) {
                        return None;
                    }
                }

                RegExpOpcode::CheckBackReference {
                    reg,
                    case_insensitive,
                    on_fail,
                } => {
                    let start_idx = registers[*reg];
                    let end_idx = registers[*reg + 1];

                    if start_idx == -1 || end_idx == -1 || start_idx > end_idx {
                        // Unmatched group matches empty string
                        pc += 1;
                    } else {
                        let start = start_idx as usize;
                        let end = end_idx as usize;
                        let ref_len = end - start;

                        let mut matched = true;
                        if cp + ref_len <= self.chars.len() {
                            for i in 0..ref_len {
                                let c1 = self.chars[start + i];
                                let c2 = self.chars[cp + i];
                                if *case_insensitive {
                                    if c1.to_lowercase().collect::<String>()
                                        != c2.to_lowercase().collect::<String>()
                                        && c1.to_uppercase().collect::<String>()
                                            != c2.to_uppercase().collect::<String>()
                                    {
                                        matched = false;
                                        break;
                                    }
                                } else if c1 != c2 {
                                    matched = false;
                                    break;
                                }
                            }
                        } else {
                            matched = false;
                        }

                        if matched {
                            cp += ref_len;
                            pc += 1;
                        } else if !self.fail(
                            *on_fail,
                            &mut pc,
                            &mut cp,
                            &mut registers,
                            &mut backtrack_stack,
                            &mut lookaround_stack,
                        ) {
                            return None;
                        }
                    }
                }

                RegExpOpcode::SetRegisterCurrentPosition { reg } => {
                    registers[*reg] = cp as i32;
                    pc += 1;
                }

                RegExpOpcode::SetRegister { reg, value } => {
                    registers[*reg] = *value;
                    pc += 1;
                }

                RegExpOpcode::PushBacktrack { target_pc } => {
                    // Check zero-width loop detection
                    if last_push_state == Some((pc, cp)) {
                        // Avoid infinite loop on empty quantifier repetitions: skip to target
                        pc = *target_pc;
                    } else {
                        last_push_state = Some((pc, cp));
                        backtrack_stack.push(BacktrackEntry {
                            pc: *target_pc,
                            cp,
                            registers: registers.clone(),
                            lookaround_depth: lookaround_stack.len(),
                        });
                        pc += 1;
                    }
                }

                RegExpOpcode::PopBacktrack => {
                    backtrack_stack.pop();
                    pc += 1;
                }

                RegExpOpcode::Jump { target } => {
                    pc = *target;
                }

                RegExpOpcode::BeginLookaround {
                    is_positive,
                    is_lookbehind,
                    on_fail: _,
                } => {
                    let start_cp = if *is_lookbehind {
                        // Lookbehind: we match ending at current cp
                        cp
                    } else {
                        cp
                    };
                    lookaround_stack.push(LookaroundFrame {
                        saved_cp: cp,
                        is_positive: *is_positive,
                        _is_lookbehind: *is_lookbehind,
                    });
                    cp = start_cp;
                    pc += 1;
                }

                RegExpOpcode::EndLookaround { is_positive: _ } => {
                    if let Some(frame) = lookaround_stack.pop() {
                        cp = frame.saved_cp;
                        if !frame.is_positive {
                            // Negative lookaround matched: fail!
                            if !self.fail(
                                FAIL_TARGET,
                                &mut pc,
                                &mut cp,
                                &mut registers,
                                &mut backtrack_stack,
                                &mut lookaround_stack,
                            ) {
                                return None;
                            }
                            continue;
                        }
                    }
                    pc += 1;
                }

                RegExpOpcode::Fail => {
                    if !self.fail(
                        FAIL_TARGET,
                        &mut pc,
                        &mut cp,
                        &mut registers,
                        &mut backtrack_stack,
                        &mut lookaround_stack,
                    ) {
                        return None;
                    }
                }

                RegExpOpcode::Succeed => {
                    let start = registers[0].max(0) as usize;
                    let end = registers[1].max(start as i32) as usize;

                    let mut captures = Vec::new();
                    let mut capture_indices = Vec::new();
                    // captures[0] is the entire match
                    let full_match: String = self.chars[start..end].iter().collect();
                    captures.push(Some(full_match));
                    capture_indices.push(Some((start, end)));

                    // captures 1..=capture_count
                    for i in 1..=self.bytecode.capture_count {
                        let s = registers[2 * i];
                        let e = registers[2 * i + 1];
                        if s >= 0 && e >= s && (e as usize) <= self.chars.len() {
                            let cap_str: String = self.chars[s as usize..e as usize].iter().collect();
                            captures.push(Some(cap_str));
                            capture_indices.push(Some((s as usize, e as usize)));
                        } else {
                            captures.push(None);
                            capture_indices.push(None);
                        }
                    }

                    let mut named_groups = HashMap::new();
                    for (name, &idx) in &self.bytecode.named_groups {
                        if idx < captures.len() {
                            if let Some(ref val) = captures[idx] {
                                named_groups.insert(name.clone(), val.clone());
                            }
                        }
                    }

                    return Some(RegExpMatch {
                        start,
                        end,
                        captures,
                        capture_indices,
                        named_groups,
                    });
                }
            }
        }

        None
    }

    /// Handles instruction failure: either jumps to `on_fail` or pops from the backtrack stack.
    /// Returns `true` if execution can continue, or `false` if match failed.
    fn fail(
        &self,
        on_fail: usize,
        pc: &mut usize,
        cp: &mut usize,
        registers: &mut Vec<i32>,
        backtrack_stack: &mut Vec<BacktrackEntry>,
        lookaround_stack: &mut Vec<LookaroundFrame>,
    ) -> bool {
        // If we are inside a negative lookaround and a failure occurs, the negative lookaround SUCCEEDS!
        if let Some(frame) = lookaround_stack.last() {
            if !frame.is_positive {
                let saved = frame.saved_cp;
                lookaround_stack.pop();
                *cp = saved;
                // Find matching EndLookaround instruction to jump after it
                let mut scan = *pc;
                while scan < self.bytecode.instructions.len() {
                    if matches!(
                        self.bytecode.instructions[scan],
                        RegExpOpcode::EndLookaround { is_positive: false }
                    ) {
                        *pc = scan + 1;
                        return true;
                    }
                    scan += 1;
                }
            }
        }

        if on_fail != FAIL_TARGET {
            *pc = on_fail;
            return true;
        }

        if let Some(entry) = backtrack_stack.pop() {
            *pc = entry.pc;
            *cp = entry.cp;
            *registers = entry.registers;
            lookaround_stack.truncate(entry.lookaround_depth);
            true
        } else {
            false
        }
    }
}
