//! Disassemblers for generated x86_64 and AArch64 machine code.
//!
//! Translates raw binary code buffers into annotated, human-readable assembly listings.

/// Disassembles an x86_64 machine code byte buffer into formatted text.
pub fn disassemble_x64(bytes: &[u8]) -> String {
    let mut output = String::new();
    let mut pc = 0;

    while pc < bytes.len() {
        let inst_start = pc;
        let mut rex_w = false;
        let mut rex_r = false;
        let mut _rex_x = false;
        let mut rex_b = false;

        let mut b = bytes[pc];
        pc += 1;

        // Check REX prefix
        if (b & 0xF0) == 0x40 {
            rex_w = (b & 0x08) != 0;
            rex_r = (b & 0x04) != 0;
            _rex_x = (b & 0x02) != 0;
            rex_b = (b & 0x01) != 0;
            if pc < bytes.len() {
                b = bytes[pc];
                pc += 1;
            }
        }

        let reg_from_code = |code: u8, ext: bool| -> &'static str {
            let idx = (code & 0x7) | if ext { 8 } else { 0 };
            match idx {
                0 => "rax", 1 => "rcx", 2 => "rdx", 3 => "rbx",
                4 => "rsp", 5 => "rbp", 6 => "rsi", 7 => "rdi",
                8 => "r8",  9 => "r9",  10 => "r10", 11 => "r11",
                12 => "r12", 13 => "r13", 14 => "r14", 15 => "r15",
                _ => "???",
            }
        };

        let mnemonic = match b {
            0x50..=0x57 => {
                let reg = reg_from_code(b - 0x50, rex_b);
                format!("push {}", reg)
            }
            0x58..=0x5F => {
                let reg = reg_from_code(b - 0x58, rex_b);
                format!("pop {}", reg)
            }
            0x89 => {
                if pc < bytes.len() {
                    let modrm = bytes[pc];
                    pc += 1;
                    let mod_bits = modrm >> 6;
                    let reg_bits = (modrm >> 3) & 0x7;
                    let rm_bits = modrm & 0x7;
                    let src = reg_from_code(reg_bits, rex_r);
                    if mod_bits == 0b11 {
                        let dst = reg_from_code(rm_bits, rex_b);
                        format!("mov {}, {}", dst, src)
                    } else if mod_bits == 0b10 && pc + 4 <= bytes.len() {
                        let disp = i32::from_le_bytes(bytes[pc..pc+4].try_into().unwrap());
                        pc += 4;
                        let base = reg_from_code(rm_bits, rex_b);
                        format!("mov [{} {:+}], {}", base, disp, src)
                    } else {
                        "mov mem, reg".to_string()
                    }
                } else {
                    "mov ??".to_string()
                }
            }
            0x8B => {
                if pc < bytes.len() {
                    let modrm = bytes[pc];
                    pc += 1;
                    let mod_bits = modrm >> 6;
                    let reg_bits = (modrm >> 3) & 0x7;
                    let rm_bits = modrm & 0x7;
                    let dst = reg_from_code(reg_bits, rex_r);
                    if mod_bits == 0b10 && pc + 4 <= bytes.len() {
                        let disp = i32::from_le_bytes(bytes[pc..pc+4].try_into().unwrap());
                        pc += 4;
                        let base = reg_from_code(rm_bits, rex_b);
                        format!("mov {}, [{} {:+}]", dst, base, disp)
                    } else {
                        format!("mov {}, mem", dst)
                    }
                } else {
                    "mov ??".to_string()
                }
            }
            0xB8..=0xBF => {
                let dst = reg_from_code(b - 0xB8, rex_b);
                if rex_w && pc + 8 <= bytes.len() {
                    let imm = i64::from_le_bytes(bytes[pc..pc+8].try_into().unwrap());
                    pc += 8;
                    format!("movabs {}, 0x{:x}", dst, imm)
                } else if pc + 4 <= bytes.len() {
                    let imm = i32::from_le_bytes(bytes[pc..pc+4].try_into().unwrap());
                    pc += 4;
                    format!("mov {}, {}", dst, imm)
                } else {
                    format!("mov {}, ???", dst)
                }
            }
            0x01 => {
                if pc < bytes.len() {
                    let modrm = bytes[pc];
                    pc += 1;
                    let src = reg_from_code((modrm >> 3) & 7, rex_r);
                    let dst = reg_from_code(modrm & 7, rex_b);
                    format!("add {}, {}", dst, src)
                } else {
                    "add ??".to_string()
                }
            }
            0x29 => {
                if pc < bytes.len() {
                    let modrm = bytes[pc];
                    pc += 1;
                    let src = reg_from_code((modrm >> 3) & 7, rex_r);
                    let dst = reg_from_code(modrm & 7, rex_b);
                    format!("sub {}, {}", dst, src)
                } else {
                    "sub ??".to_string()
                }
            }
            0x0F => {
                if pc < bytes.len() {
                    let b2 = bytes[pc];
                    pc += 1;
                    match b2 {
                        0xAF => {
                            if pc < bytes.len() {
                                let modrm = bytes[pc];
                                pc += 1;
                                let dst = reg_from_code((modrm >> 3) & 7, rex_r);
                                let src = reg_from_code(modrm & 7, rex_b);
                                format!("imul {}, {}", dst, src)
                            } else {
                                "imul ??".to_string()
                            }
                        }
                        0x80..=0x8F => {
                            let cond = match b2 & 0x0F {
                                0x4 => "je", 0x5 => "jne", 0xC => "jl", 0xD => "jge",
                                0xE => "jle", 0xF => "jg", _ => "jcc",
                            };
                            if pc + 4 <= bytes.len() {
                                let rel = i32::from_le_bytes(bytes[pc..pc+4].try_into().unwrap());
                                pc += 4;
                                let target = (pc as isize + rel as isize) as usize;
                                format!("{} 0x{:04x}", cond, target)
                            } else {
                                format!("{} ???", cond)
                            }
                        }
                        0x90..=0x9F => {
                            let cond = match b2 & 0x0F {
                                0x4 => "sete", 0x5 => "setne", 0xC => "setl", 0xD => "setge",
                                0xE => "setle", 0xF => "setg", _ => "setcc",
                            };
                            if pc < bytes.len() {
                                let modrm = bytes[pc];
                                pc += 1;
                                let dst = reg_from_code(modrm & 7, rex_b);
                                format!("{} {}", cond, dst)
                            } else {
                                format!("{} ??", cond)
                            }
                        }
                        0xB6 => {
                            if pc < bytes.len() {
                                let modrm = bytes[pc];
                                pc += 1;
                                let dst = reg_from_code((modrm >> 3) & 7, rex_r);
                                let src = reg_from_code(modrm & 7, rex_b);
                                format!("movzx {}, {}", dst, src)
                            } else {
                                "movzx ??".to_string()
                            }
                        }
                        _ => format!(".byte 0x0F, 0x{:02x}", b2),
                    }
                } else {
                    ".byte 0x0F".to_string()
                }
            }
            0x21 => {
                if pc < bytes.len() {
                    let modrm = bytes[pc];
                    pc += 1;
                    let src = reg_from_code((modrm >> 3) & 7, rex_r);
                    let dst = reg_from_code(modrm & 7, rex_b);
                    format!("and {}, {}", dst, src)
                } else { "and ??".to_string() }
            }
            0x09 => {
                if pc < bytes.len() {
                    let modrm = bytes[pc];
                    pc += 1;
                    let src = reg_from_code((modrm >> 3) & 7, rex_r);
                    let dst = reg_from_code(modrm & 7, rex_b);
                    format!("or {}, {}", dst, src)
                } else { "or ??".to_string() }
            }
            0x31 => {
                if pc < bytes.len() {
                    let modrm = bytes[pc];
                    pc += 1;
                    let src = reg_from_code((modrm >> 3) & 7, rex_r);
                    let dst = reg_from_code(modrm & 7, rex_b);
                    format!("xor {}, {}", dst, src)
                } else { "xor ??".to_string() }
            }
            0x39 => {
                if pc < bytes.len() {
                    let modrm = bytes[pc];
                    pc += 1;
                    let rhs = reg_from_code((modrm >> 3) & 7, rex_r);
                    let lhs = reg_from_code(modrm & 7, rex_b);
                    format!("cmp {}, {}", lhs, rhs)
                } else { "cmp ??".to_string() }
            }
            0x81 => {
                if pc < bytes.len() {
                    let modrm = bytes[pc];
                    pc += 1;
                    let op = (modrm >> 3) & 7;
                    let reg = reg_from_code(modrm & 7, rex_b);
                    if pc + 4 <= bytes.len() {
                        let imm = i32::from_le_bytes(bytes[pc..pc+4].try_into().unwrap());
                        pc += 4;
                        let op_name = match op {
                            0 => "add", 5 => "sub", 7 => "cmp", _ => "alu",
                        };
                        format!("{} {}, {}", op_name, reg, imm)
                    } else { "alu ??".to_string() }
                } else { "alu ??".to_string() }
            }
            0xE9 => {
                if pc + 4 <= bytes.len() {
                    let rel = i32::from_le_bytes(bytes[pc..pc+4].try_into().unwrap());
                    pc += 4;
                    let target = (pc as isize + rel as isize) as usize;
                    format!("jmp 0x{:04x}", target)
                } else { "jmp ???".to_string() }
            }
            0xC3 => "ret".to_string(),
            _ => format!(".byte 0x{:02x}", b),
        };

        let inst_bytes = &bytes[inst_start..pc];
        let mut hex_dump = String::new();
        for ib in inst_bytes {
            hex_dump.push_str(&format!("{:02x} ", ib));
        }

        output.push_str(&format!("{:04x}: {:<18} {}\n", inst_start, hex_dump, mnemonic));
    }

    output
}

/// Disassembles an AArch64 (ARM64) machine code buffer into formatted text.
pub fn disassemble_arm64(bytes: &[u8]) -> String {
    let mut output = String::new();
    let mut pc = 0;

    while pc + 4 <= bytes.len() {
        let insn = u32::from_le_bytes(bytes[pc..pc+4].try_into().unwrap());
        let mnemonic = if insn == 0xD65F03C0 {
            "ret".to_string()
        } else if insn == 0xA9BF7BFD {
            "stp x29, x30, [sp, #-16]!".to_string()
        } else if insn == 0xA8C17BFD {
            "ldp x29, x30, [sp], #16".to_string()
        } else if (insn & 0xFF200000) == 0x8B000000 {
            let rd = insn & 0x1F;
            let rn = (insn >> 5) & 0x1F;
            let rm = (insn >> 16) & 0x1F;
            format!("add x{}, x{}, x{}", rd, rn, rm)
        } else if (insn & 0xFF200000) == 0xCB000000 {
            let rd = insn & 0x1F;
            let rn = (insn >> 5) & 0x1F;
            let rm = (insn >> 16) & 0x1F;
            format!("sub x{}, x{}, x{}", rd, rn, rm)
        } else if (insn & 0xFF800000) == 0x91000000 {
            let rd = insn & 0x1F;
            let rn = (insn >> 5) & 0x1F;
            let imm12 = (insn >> 10) & 0xFFF;
            if rd == 29 && rn == 31 && imm12 == 0 {
                "mov x29, sp".to_string()
            } else if rd == 31 && rn == 29 && imm12 == 0 {
                "mov sp, x29".to_string()
            } else {
                format!("add x{}, x{}, #{}", rd, rn, imm12)
            }
        } else if (insn & 0xFF800000) == 0xD1000000 {
            let rd = insn & 0x1F;
            let rn = (insn >> 5) & 0x1F;
            let imm12 = (insn >> 10) & 0xFFF;
            format!("sub x{}, x{}, #{}", rd, rn, imm12)
        } else if (insn & 0xFFE0FC00) == 0xAA0003E0 {
            let rd = insn & 0x1F;
            let rm = (insn >> 16) & 0x1F;
            format!("mov x{}, x{}", rd, rm)
        } else if (insn & 0xFFE07C00) == 0x9B007C00 {
            let rd = insn & 0x1F;
            let rn = (insn >> 5) & 0x1F;
            let rm = (insn >> 16) & 0x1F;
            format!("mul x{}, x{}, x{}", rd, rn, rm)
        } else if (insn & 0xFFE00C00) == 0x9AC00C00 {
            let rd = insn & 0x1F;
            let rn = (insn >> 5) & 0x1F;
            let rm = (insn >> 16) & 0x1F;
            format!("sdiv x{}, x{}, x{}", rd, rn, rm)
        } else if (insn & 0xFFE0001F) == 0xEB00001F {
            let rn = (insn >> 5) & 0x1F;
            let rm = (insn >> 16) & 0x1F;
            format!("cmp x{}, x{}", rn, rm)
        } else if (insn & 0xFC000000) == 0x14000000 {
            let imm26 = (insn & 0x03FFFFFF) as i32;
            let sign_ext = (imm26 << 6) >> 6;
            let target = (pc as isize + (sign_ext as isize * 4)) as usize;
            format!("b 0x{:04x}", target)
        } else if (insn & 0xFF000010) == 0x54000000 {
            let cond_bits = insn & 0xF;
            let cond = match cond_bits {
                0 => "eq", 1 => "ne", 10 => "ge", 11 => "lt", 12 => "gt", 13 => "le", _ => "cond",
            };
            let imm19 = ((insn >> 5) & 0x7FFFF) as i32;
            let sign_ext = (imm19 << 13) >> 13;
            let target = (pc as isize + (sign_ext as isize * 4)) as usize;
            format!("b.{} 0x{:04x}", cond, target)
        } else {
            format!(".word 0x{:08x}", insn)
        };

        output.push_str(&format!("{:04x}: {:08x}   {}\n", pc, insn, mnemonic));
        pc += 4;
    }

    output
}
