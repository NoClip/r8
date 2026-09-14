//! Safe Rust reimplementation of Google V8's Irregexp Compiler (`src/regexp/regexp-compiler.h`).
//!
//! Compiles RegExp AST nodes into Irregexp bytecode instructions with jump patching,
//! capture register allocation, and backtrack point orchestration.

use super::ast::{AssertionType, RegExpFlags, RegExpNode};
use super::bytecodes::{RegExpBytecode, RegExpOpcode};
use std::collections::HashMap;

pub const FAIL_TARGET: usize = usize::MAX;

pub struct RegExpCompiler {
    instructions: Vec<RegExpOpcode>,
    flags: RegExpFlags,
    capture_count: usize,
    named_groups: HashMap<String, usize>,
    pattern: String,
}

impl RegExpCompiler {
    pub fn new(
        pattern: String,
        flags: RegExpFlags,
        capture_count: usize,
        named_groups: HashMap<String, usize>,
    ) -> Self {
        Self {
            instructions: Vec::new(),
            flags,
            capture_count,
            named_groups,
            pattern,
        }
    }

    #[inline]
    fn current_pc(&self) -> usize {
        self.instructions.len()
    }

    #[inline]
    fn emit(&mut self, opcode: RegExpOpcode) -> usize {
        let pc = self.instructions.len();
        self.instructions.push(opcode);
        pc
    }

    /// Compiles the AST into an executable `RegExpBytecode`.
    pub fn compile(mut self, root: &RegExpNode) -> Result<RegExpBytecode, String> {
        // Register 0 = match start
        self.emit(RegExpOpcode::SetRegisterCurrentPosition { reg: 0 });

        // Initialize all capture registers to -1
        let num_registers = 2 * (self.capture_count + 1);
        for r in 2..num_registers {
            self.emit(RegExpOpcode::SetRegister { reg: r, value: -1 });
        }

        // Compile root AST node
        self.compile_node(root)?;

        // Register 1 = match end
        self.emit(RegExpOpcode::SetRegisterCurrentPosition { reg: 1 });
        self.emit(RegExpOpcode::Succeed);

        Ok(RegExpBytecode::new(
            self.instructions,
            num_registers,
            self.capture_count,
            self.named_groups,
            self.flags,
            self.pattern,
        ))
    }

    fn compile_node(&mut self, node: &RegExpNode) -> Result<(), String> {
        match node {
            RegExpNode::Empty => Ok(()),

            RegExpNode::Char(c) => {
                self.emit(RegExpOpcode::CheckCharacter {
                    c: *c,
                    case_insensitive: self.flags.ignore_case,
                    on_fail: FAIL_TARGET,
                });
                Ok(())
            }

            RegExpNode::Text(s) => {
                for c in s.chars() {
                    self.emit(RegExpOpcode::CheckCharacter {
                        c,
                        case_insensitive: self.flags.ignore_case,
                        on_fail: FAIL_TARGET,
                    });
                }
                Ok(())
            }

            RegExpNode::CharacterClass { ranges, negated } => {
                self.emit(RegExpOpcode::CheckCharacterClass {
                    ranges: ranges.clone(),
                    negated: *negated,
                    case_insensitive: self.flags.ignore_case,
                    on_fail: FAIL_TARGET,
                });
                Ok(())
            }

            RegExpNode::AnyChar => {
                self.emit(RegExpOpcode::CheckAnyCharacter {
                    dot_all: self.flags.dot_all,
                    on_fail: FAIL_TARGET,
                });
                Ok(())
            }

            RegExpNode::Assertion(assertion) => {
                match assertion {
                    AssertionType::StartOfInput => {
                        self.emit(RegExpOpcode::CheckNotAtStart {
                            multiline: self.flags.multiline,
                            on_fail: FAIL_TARGET,
                        });
                    }
                    AssertionType::EndOfInput => {
                        self.emit(RegExpOpcode::CheckNotAtEnd {
                            multiline: self.flags.multiline,
                            on_fail: FAIL_TARGET,
                        });
                    }
                    AssertionType::WordBoundary => {
                        self.emit(RegExpOpcode::CheckWordBoundary {
                            negated: false,
                            on_fail: FAIL_TARGET,
                        });
                    }
                    AssertionType::NonWordBoundary => {
                        self.emit(RegExpOpcode::CheckWordBoundary {
                            negated: true,
                            on_fail: FAIL_TARGET,
                        });
                    }
                }
                Ok(())
            }

            RegExpNode::Sequence(nodes) => {
                for n in nodes {
                    self.compile_node(n)?;
                }
                Ok(())
            }

            RegExpNode::Disjunction(alternatives) => {
                if alternatives.is_empty() {
                    return Ok(());
                }
                if alternatives.len() == 1 {
                    return self.compile_node(&alternatives[0]);
                }

                let mut end_jumps = Vec::new();

                for (i, alt) in alternatives.iter().enumerate() {
                    let is_last = i == alternatives.len() - 1;

                    if !is_last {
                        let push_idx = self.emit(RegExpOpcode::PushBacktrack { target_pc: 0 });
                        self.compile_node(alt)?;
                        let jump_idx = self.emit(RegExpOpcode::Jump { target: 0 });
                        end_jumps.push(jump_idx);

                        let next_alt_pc = self.current_pc();
                        // Patch target_pc of PushBacktrack
                        if let RegExpOpcode::PushBacktrack { ref mut target_pc } = self.instructions[push_idx] {
                            *target_pc = next_alt_pc;
                        }
                    } else {
                        self.compile_node(alt)?;
                    }
                }

                let end_pc = self.current_pc();
                for jump_idx in end_jumps {
                    if let RegExpOpcode::Jump { ref mut target } = self.instructions[jump_idx] {
                        *target = end_pc;
                    }
                }

                Ok(())
            }

            RegExpNode::Capture { index, body, .. } => {
                let start_reg = 2 * index;
                let end_reg = 2 * index + 1;

                self.emit(RegExpOpcode::SetRegisterCurrentPosition { reg: start_reg });
                self.compile_node(body)?;
                self.emit(RegExpOpcode::SetRegisterCurrentPosition { reg: end_reg });
                Ok(())
            }

            RegExpNode::NonCapturing(body) => self.compile_node(body),

            RegExpNode::BackReference(index) => {
                let start_reg = 2 * index;
                self.emit(RegExpOpcode::CheckBackReference {
                    reg: start_reg,
                    case_insensitive: self.flags.ignore_case,
                    on_fail: FAIL_TARGET,
                });
                Ok(())
            }

            RegExpNode::Quantifier {
                min,
                max,
                greedy,
                body,
            } => self.compile_quantifier(*min, *max, *greedy, body),

            RegExpNode::Lookaround {
                is_positive,
                is_lookbehind,
                body,
            } => {
                self.emit(RegExpOpcode::BeginLookaround {
                    is_positive: *is_positive,
                    is_lookbehind: *is_lookbehind,
                    on_fail: FAIL_TARGET,
                });
                self.compile_node(body)?;
                self.emit(RegExpOpcode::EndLookaround {
                    is_positive: *is_positive,
                });
                Ok(())
            }
        }
    }

    fn compile_quantifier(
        &mut self,
        min: usize,
        max: Option<usize>,
        greedy: bool,
        body: &RegExpNode,
    ) -> Result<(), String> {
        // Step 1: Match mandatory `min` repetitions
        for _ in 0..min {
            self.compile_node(body)?;
        }

        // Step 2: Handle optional repetitions
        match max {
            Some(m) if m == min => {
                // Exactly min repetitions already emitted
                Ok(())
            }
            Some(m) => {
                let extra = m - min;
                if greedy {
                    let mut exit_labels = Vec::new();
                    for _ in 0..extra {
                        let push_idx = self.emit(RegExpOpcode::PushBacktrack { target_pc: 0 });
                        exit_labels.push(push_idx);
                        self.compile_node(body)?;
                    }
                    let end_pc = self.current_pc();
                    for push_idx in exit_labels {
                        if let RegExpOpcode::PushBacktrack { ref mut target_pc } = self.instructions[push_idx] {
                            *target_pc = end_pc;
                        }
                    }
                } else {
                    // Non-greedy / lazy: try fewer first
                    let mut end_labels = Vec::new();
                    for _ in 0..extra {
                        let push_idx = self.emit(RegExpOpcode::PushBacktrack { target_pc: 0 });
                        let jump_idx = self.emit(RegExpOpcode::Jump { target: 0 });
                        end_labels.push(jump_idx);

                        let body_start = self.current_pc();
                        if let RegExpOpcode::PushBacktrack { ref mut target_pc } = self.instructions[push_idx] {
                            *target_pc = body_start;
                        }

                        self.compile_node(body)?;
                    }
                    let end_pc = self.current_pc();
                    for jump_idx in end_labels {
                        if let RegExpOpcode::Jump { ref mut target } = self.instructions[jump_idx] {
                            *target = end_pc;
                        }
                    }
                }
                Ok(())
            }
            None => {
                // Unbounded repetition: * or +
                if greedy {
                    let loop_start = self.current_pc();
                    let push_idx = self.emit(RegExpOpcode::PushBacktrack { target_pc: 0 });
                    self.compile_node(body)?;
                    self.emit(RegExpOpcode::Jump { target: loop_start });

                    let exit_pc = self.current_pc();
                    if let RegExpOpcode::PushBacktrack { ref mut target_pc } = self.instructions[push_idx] {
                        *target_pc = exit_pc;
                    }
                } else {
                    // Lazy unbounded: *? or +?
                    let loop_start = self.current_pc();
                    let push_idx = self.emit(RegExpOpcode::PushBacktrack { target_pc: 0 });
                    let jump_idx = self.emit(RegExpOpcode::Jump { target: 0 });

                    let body_start = self.current_pc();
                    if let RegExpOpcode::PushBacktrack { ref mut target_pc } = self.instructions[push_idx] {
                        *target_pc = body_start;
                    }

                    self.compile_node(body)?;
                    self.emit(RegExpOpcode::Jump { target: loop_start });

                    let exit_pc = self.current_pc();
                    if let RegExpOpcode::Jump { ref mut target } = self.instructions[jump_idx] {
                        *target = exit_pc;
                    }
                }
                Ok(())
            }
        }
    }
}
