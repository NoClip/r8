//! Target-specific and abstract CPU registers and calling conventions.

/// x86_64 General-Purpose 64-bit Registers.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum X64Register {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8  = 8,
    R9  = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

impl X64Register {
    /// Returns the 3-bit hardware encoding (0..=7).
    #[inline]
    pub const fn code(self) -> u8 {
        (self as u8) & 0x7
    }

    /// Returns true if the register requires an extended REX prefix bit (R8..R15).
    #[inline]
    pub const fn is_extended(self) -> bool {
        (self as u8) >= 8
    }

    /// Returns the canonical mnemonic name.
    pub const fn name(self) -> &'static str {
        match self {
            X64Register::Rax => "rax",
            X64Register::Rcx => "rcx",
            X64Register::Rdx => "rdx",
            X64Register::Rbx => "rbx",
            X64Register::Rsp => "rsp",
            X64Register::Rbp => "rbp",
            X64Register::Rsi => "rsi",
            X64Register::Rdi => "rdi",
            X64Register::R8  => "r8",
            X64Register::R9  => "r9",
            X64Register::R10 => "r10",
            X64Register::R11 => "r11",
            X64Register::R12 => "r12",
            X64Register::R13 => "r13",
            X64Register::R14 => "r14",
            X64Register::R15 => "r15",
        }
    }
}

/// AArch64 (ARM64) 64-bit General-Purpose Registers.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Arm64Register {
    X0, X1, X2, X3, X4, X5, X6, X7,
    X8, X9, X10, X11, X12, X13, X14, X15,
    X16, X17, X18, X19, X20, X21, X22, X23,
    X24, X25, X26, X27, X28, X29, X30,
    Sp,
    Xzr,
}

impl Arm64Register {
    /// Returns the 5-bit register encoding (0..=31).
    pub const fn code(self) -> u8 {
        match self {
            Arm64Register::X0  => 0,  Arm64Register::X1  => 1,
            Arm64Register::X2  => 2,  Arm64Register::X3  => 3,
            Arm64Register::X4  => 4,  Arm64Register::X5  => 5,
            Arm64Register::X6  => 6,  Arm64Register::X7  => 7,
            Arm64Register::X8  => 8,  Arm64Register::X9  => 9,
            Arm64Register::X10 => 10, Arm64Register::X11 => 11,
            Arm64Register::X12 => 12, Arm64Register::X13 => 13,
            Arm64Register::X14 => 14, Arm64Register::X15 => 15,
            Arm64Register::X16 => 16, Arm64Register::X17 => 17,
            Arm64Register::X18 => 18, Arm64Register::X19 => 19,
            Arm64Register::X20 => 20, Arm64Register::X21 => 21,
            Arm64Register::X22 => 22, Arm64Register::X23 => 23,
            Arm64Register::X24 => 24, Arm64Register::X25 => 25,
            Arm64Register::X26 => 26, Arm64Register::X27 => 27,
            Arm64Register::X28 => 28, Arm64Register::X29 => 29,
            Arm64Register::X30 => 30,
            Arm64Register::Sp  => 31,
            Arm64Register::Xzr => 31,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Arm64Register::X0  => "x0",  Arm64Register::X1  => "x1",
            Arm64Register::X2  => "x2",  Arm64Register::X3  => "x3",
            Arm64Register::X4  => "x4",  Arm64Register::X5  => "x5",
            Arm64Register::X6  => "x6",  Arm64Register::X7  => "x7",
            Arm64Register::X8  => "x8",  Arm64Register::X9  => "x9",
            Arm64Register::X10 => "x10", Arm64Register::X11 => "x11",
            Arm64Register::X12 => "x12", Arm64Register::X13 => "x13",
            Arm64Register::X14 => "x14", Arm64Register::X15 => "x15",
            Arm64Register::X16 => "x16", Arm64Register::X17 => "x17",
            Arm64Register::X18 => "x18", Arm64Register::X19 => "x19",
            Arm64Register::X20 => "x20", Arm64Register::X21 => "x21",
            Arm64Register::X22 => "x22", Arm64Register::X23 => "x23",
            Arm64Register::X24 => "x24", Arm64Register::X25 => "x25",
            Arm64Register::X26 => "x26", Arm64Register::X27 => "x27",
            Arm64Register::X28 => "x28", Arm64Register::X29 => "x29",
            Arm64Register::X30 => "x30",
            Arm64Register::Sp  => "sp",
            Arm64Register::Xzr => "xzr",
        }
    }
}

/// Standard ABI calling conventions.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CallingConvention {
    /// Microsoft x64 Calling Convention: rcx, rdx, r8, r9 (32-byte shadow store).
    WindowsX64,
    /// System V AMD64 ABI (Linux, macOS): rdi, rsi, rdx, rcx, r8, r9.
    SystemVAmd64,
    /// ARM64 AAPCS64: x0..x7.
    Aapcs64,
}

impl CallingConvention {
    /// Returns the default calling convention for the current host compilation target.
    pub fn host_default() -> Self {
        #[cfg(target_os = "windows")]
        {
            CallingConvention::WindowsX64
        }
        #[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
        {
            CallingConvention::SystemVAmd64
        }
        #[cfg(target_arch = "aarch64")]
        {
            CallingConvention::Aapcs64
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            CallingConvention::WindowsX64
        }
    }

    /// Returns parameter registers for x86_64.
    pub fn x64_parameter_registers(self) -> &'static [X64Register] {
        match self {
            CallingConvention::WindowsX64 => &[
                X64Register::Rcx,
                X64Register::Rdx,
                X64Register::R8,
                X64Register::R9,
            ],
            CallingConvention::SystemVAmd64 => &[
                X64Register::Rdi,
                X64Register::Rsi,
                X64Register::Rdx,
                X64Register::Rcx,
                X64Register::R8,
                X64Register::R9,
            ],
            CallingConvention::Aapcs64 => &[],
        }
    }

    /// Returns parameter registers for ARM64.
    pub fn arm64_parameter_registers(self) -> &'static [Arm64Register] {
        &[
            Arm64Register::X0,
            Arm64Register::X1,
            Arm64Register::X2,
            Arm64Register::X3,
            Arm64Register::X4,
            Arm64Register::X5,
            Arm64Register::X6,
            Arm64Register::X7,
        ]
    }
}
