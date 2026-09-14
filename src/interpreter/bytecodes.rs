//! Safe Rust reimplementation of Google V8's `src/interpreter/bytecodes.h`.
//!
//! Provides the complete V8 Ignition bytecode instruction set, operand metadata,
//! ShortStar single-byte encodings, jump predicates, and accumulator interaction flags.

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Bytecode {
    // Widening prefixes
    Wide,
    ExtraWide,

    // Loading accumulator
    LdaZero,
    LdaSmi,
    LdaUndefined,
    LdaNull,
    LdaTheHole,
    LdaTrue,
    LdaFalse,
    LdaConstant,
    Ldar,
    LdaNamedProperty,
    LdaKeyedProperty,
    LdaGlobal,

    // Storing accumulator
    Star,
    StaNamedProperty,
    StaKeyedProperty,
    StaGlobal,
    StaInArrayLiteral,
    StaDataPropertyInLiteral,

    // ShortStar variants (single-byte Star r0 .. Star r15)
    Star0,
    Star1,
    Star2,
    Star3,
    Star4,
    Star5,
    Star6,
    Star7,
    Star8,
    Star9,
    Star10,
    Star11,
    Star12,
    Star13,
    Star14,
    Star15,

    // Register moves
    Mov,

    // Arithmetic & Bitwise
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
    BitwiseOr,
    BitwiseXor,
    BitwiseAnd,
    ShiftLeft,
    ShiftRight,
    ShiftRightLogical,
    AddSmi,
    SubSmi,
    MulSmi,
    DivSmi,
    ModSmi,
    ExpSmi,
    BitwiseOrSmi,
    BitwiseXorSmi,
    BitwiseAndSmi,
    ShiftLeftSmi,
    ShiftRightSmi,
    ShiftRightLogicalSmi,

    // Unary conversions
    Inc,
    Dec,
    Negate,
    ToNumber,
    ToNumeric,
    ToObject,
    ToString,
    LogicalNot,
    BitwiseNot,
    TypeOf,

    // Comparisons
    TestEqual,
    TestEqualStrict,
    TestLessThan,
    TestGreaterThan,
    TestLessThanOrEqual,
    TestGreaterThanOrEqual,
    TestReferenceEqual,
    TestNull,
    TestUndefined,
    TestTypeOf,

    // Control Flow & Branches
    Jump,
    JumpConstant,
    JumpIfTrue,
    JumpIfTrueConstant,
    JumpIfFalse,
    JumpIfFalseConstant,
    JumpIfNull,
    JumpIfNotNull,
    JumpIfUndefined,
    JumpIfNotUndefined,
    JumpIfUndefinedOrNull,
    JumpIfJSReceiver,
    JumpLoop,

    // Returns, Errors, Abort
    Return,
    Throw,
    ReThrow,
    Abort,

    // Calls & Constructs
    CallAnyReceiver,
    CallProperty,
    CallProperty0,
    CallProperty1,
    CallProperty2,
    CallUndefinedReceiver,
    CallUndefinedReceiver0,
    CallUndefinedReceiver1,
    CallUndefinedReceiver2,
    CallRuntime,
    Construct,

    // Literals
    CreateEmptyObjectLiteral,
    CreateEmptyArrayLiteral,
    CreateRegExpLiteral,

    Debugger,
    ForInEnumerate,
    Await,
    TestInstanceOf,
    TestIn,
    LdaSuper,
    SpreadInArrayLiteral,
    SpreadInObjectLiteral,
    CallWithSpread,
    SuspendGenerator,
    ResumeGenerator,
    GetIterator,
    IteratorNext,
    IteratorDone,
    IteratorValue,
    DynamicImport,
    DeletePropertySloppy,
    DeleteKeyedPropertySloppy,
    LdaNewTarget,
}

pub const ALL_BYTECODES: [Bytecode; 131] = [
    Bytecode::Wide,
    Bytecode::ExtraWide,
    Bytecode::LdaZero,
    Bytecode::LdaSmi,
    Bytecode::LdaUndefined,
    Bytecode::LdaNull,
    Bytecode::LdaTheHole,
    Bytecode::LdaTrue,
    Bytecode::LdaFalse,
    Bytecode::LdaConstant,
    Bytecode::Ldar,
    Bytecode::LdaNamedProperty,
    Bytecode::LdaKeyedProperty,
    Bytecode::LdaGlobal,
    Bytecode::Star,
    Bytecode::StaNamedProperty,
    Bytecode::StaKeyedProperty,
    Bytecode::StaGlobal,
    Bytecode::StaInArrayLiteral,
    Bytecode::StaDataPropertyInLiteral,
    Bytecode::Star0,
    Bytecode::Star1,
    Bytecode::Star2,
    Bytecode::Star3,
    Bytecode::Star4,
    Bytecode::Star5,
    Bytecode::Star6,
    Bytecode::Star7,
    Bytecode::Star8,
    Bytecode::Star9,
    Bytecode::Star10,
    Bytecode::Star11,
    Bytecode::Star12,
    Bytecode::Star13,
    Bytecode::Star14,
    Bytecode::Star15,
    Bytecode::Mov,
    Bytecode::Add,
    Bytecode::Sub,
    Bytecode::Mul,
    Bytecode::Div,
    Bytecode::Mod,
    Bytecode::Exp,
    Bytecode::BitwiseOr,
    Bytecode::BitwiseXor,
    Bytecode::BitwiseAnd,
    Bytecode::ShiftLeft,
    Bytecode::ShiftRight,
    Bytecode::ShiftRightLogical,
    Bytecode::AddSmi,
    Bytecode::SubSmi,
    Bytecode::MulSmi,
    Bytecode::DivSmi,
    Bytecode::ModSmi,
    Bytecode::ExpSmi,
    Bytecode::BitwiseOrSmi,
    Bytecode::BitwiseXorSmi,
    Bytecode::BitwiseAndSmi,
    Bytecode::ShiftLeftSmi,
    Bytecode::ShiftRightSmi,
    Bytecode::ShiftRightLogicalSmi,
    Bytecode::Inc,
    Bytecode::Dec,
    Bytecode::Negate,
    Bytecode::ToNumber,
    Bytecode::ToNumeric,
    Bytecode::ToObject,
    Bytecode::ToString,
    Bytecode::LogicalNot,
    Bytecode::BitwiseNot,
    Bytecode::TypeOf,
    Bytecode::TestEqual,
    Bytecode::TestEqualStrict,
    Bytecode::TestLessThan,
    Bytecode::TestGreaterThan,
    Bytecode::TestLessThanOrEqual,
    Bytecode::TestGreaterThanOrEqual,
    Bytecode::TestReferenceEqual,
    Bytecode::TestNull,
    Bytecode::TestUndefined,
    Bytecode::TestTypeOf,
    Bytecode::Jump,
    Bytecode::JumpConstant,
    Bytecode::JumpIfTrue,
    Bytecode::JumpIfTrueConstant,
    Bytecode::JumpIfFalse,
    Bytecode::JumpIfFalseConstant,
    Bytecode::JumpIfNull,
    Bytecode::JumpIfNotNull,
    Bytecode::JumpIfUndefined,
    Bytecode::JumpIfNotUndefined,
    Bytecode::JumpIfUndefinedOrNull,
    Bytecode::JumpIfJSReceiver,
    Bytecode::JumpLoop,
    Bytecode::Return,
    Bytecode::Throw,
    Bytecode::ReThrow,
    Bytecode::Abort,
    Bytecode::CallAnyReceiver,
    Bytecode::CallProperty,
    Bytecode::CallProperty0,
    Bytecode::CallProperty1,
    Bytecode::CallProperty2,
    Bytecode::CallUndefinedReceiver,
    Bytecode::CallUndefinedReceiver0,
    Bytecode::CallUndefinedReceiver1,
    Bytecode::CallUndefinedReceiver2,
    Bytecode::CallRuntime,
    Bytecode::Construct,
    Bytecode::CreateEmptyObjectLiteral,
    Bytecode::CreateEmptyArrayLiteral,
    Bytecode::CreateRegExpLiteral,
    Bytecode::Debugger,
    Bytecode::ForInEnumerate,
    Bytecode::Await,
    Bytecode::TestInstanceOf,
    Bytecode::TestIn,
    Bytecode::LdaSuper,
    Bytecode::SpreadInArrayLiteral,
    Bytecode::SpreadInObjectLiteral,
    Bytecode::CallWithSpread,
    Bytecode::SuspendGenerator,
    Bytecode::ResumeGenerator,
    Bytecode::GetIterator,
    Bytecode::IteratorNext,
    Bytecode::IteratorDone,
    Bytecode::IteratorValue,
    Bytecode::DynamicImport,
    Bytecode::DeletePropertySloppy,
    Bytecode::DeleteKeyedPropertySloppy,
    Bytecode::LdaNewTarget,
];

impl Bytecode {
    /// Returns the number of operands expected by this bytecode.
    pub fn number_of_operands(&self) -> usize {
        match self {
            Bytecode::Wide
            | Bytecode::ExtraWide
            | Bytecode::LdaSuper
            | Bytecode::LdaNewTarget
            | Bytecode::LdaZero
            | Bytecode::LdaUndefined
            | Bytecode::LdaNull
            | Bytecode::LdaTheHole
            | Bytecode::LdaTrue
            | Bytecode::LdaFalse
            | Bytecode::Star0
            | Bytecode::Star1
            | Bytecode::Star2
            | Bytecode::Star3
            | Bytecode::Star4
            | Bytecode::Star5
            | Bytecode::Star6
            | Bytecode::Star7
            | Bytecode::Star8
            | Bytecode::Star9
            | Bytecode::Star10
            | Bytecode::Star11
            | Bytecode::Star12
            | Bytecode::Star13
            | Bytecode::Star14
            | Bytecode::Star15
            | Bytecode::Inc
            | Bytecode::Dec
            | Bytecode::Negate
            | Bytecode::ToNumber
            | Bytecode::ToNumeric
            | Bytecode::ToObject
            | Bytecode::ToString
            | Bytecode::LogicalNot
            | Bytecode::BitwiseNot
            | Bytecode::TypeOf
            | Bytecode::TestNull
            | Bytecode::TestUndefined
            | Bytecode::TestTypeOf
            | Bytecode::Return
            | Bytecode::Throw
            | Bytecode::ReThrow
            | Bytecode::CreateEmptyObjectLiteral
            | Bytecode::CreateEmptyArrayLiteral
            | Bytecode::Debugger
            | Bytecode::ForInEnumerate
            | Bytecode::Await
            | Bytecode::SuspendGenerator
            | Bytecode::ResumeGenerator
            | Bytecode::GetIterator
            | Bytecode::DynamicImport => 0,

            Bytecode::LdaSmi
            | Bytecode::LdaConstant
            | Bytecode::Ldar
            | Bytecode::Star
            | Bytecode::Jump
            | Bytecode::JumpConstant
            | Bytecode::JumpIfTrue
            | Bytecode::JumpIfTrueConstant
            | Bytecode::JumpIfFalse
            | Bytecode::JumpIfFalseConstant
            | Bytecode::JumpIfNull
            | Bytecode::JumpIfNotNull
            | Bytecode::JumpIfUndefined
            | Bytecode::JumpIfNotUndefined
            | Bytecode::JumpIfUndefinedOrNull
            | Bytecode::JumpIfJSReceiver
            | Bytecode::JumpLoop
            | Bytecode::SpreadInArrayLiteral
            | Bytecode::SpreadInObjectLiteral
            | Bytecode::IteratorNext
            | Bytecode::IteratorDone
            | Bytecode::IteratorValue
            | Bytecode::Abort => 1,

            Bytecode::Mov
            | Bytecode::Add
            | Bytecode::Sub
            | Bytecode::Mul
            | Bytecode::Div
            | Bytecode::Mod
            | Bytecode::Exp
            | Bytecode::BitwiseOr
            | Bytecode::BitwiseXor
            | Bytecode::BitwiseAnd
            | Bytecode::ShiftLeft
            | Bytecode::ShiftRight
            | Bytecode::ShiftRightLogical
            | Bytecode::AddSmi
            | Bytecode::SubSmi
            | Bytecode::MulSmi
            | Bytecode::DivSmi
            | Bytecode::ModSmi
            | Bytecode::ExpSmi
            | Bytecode::BitwiseOrSmi
            | Bytecode::BitwiseXorSmi
            | Bytecode::BitwiseAndSmi
            | Bytecode::ShiftLeftSmi
            | Bytecode::ShiftRightSmi
            | Bytecode::ShiftRightLogicalSmi
            | Bytecode::TestEqual
            | Bytecode::TestEqualStrict
            | Bytecode::TestLessThan
            | Bytecode::TestGreaterThan
            | Bytecode::TestLessThanOrEqual
            | Bytecode::TestGreaterThanOrEqual
            | Bytecode::TestReferenceEqual
            | Bytecode::LdaNamedProperty
            | Bytecode::LdaGlobal
            | Bytecode::StaGlobal
            | Bytecode::CreateRegExpLiteral
            | Bytecode::TestInstanceOf
            | Bytecode::DeletePropertySloppy
            | Bytecode::DeleteKeyedPropertySloppy
            | Bytecode::TestIn => 2,

            Bytecode::LdaKeyedProperty
            | Bytecode::StaNamedProperty
            | Bytecode::CallUndefinedReceiver
            | Bytecode::CallProperty0
            | Bytecode::CallUndefinedReceiver0
            | Bytecode::CallWithSpread => 3,

            Bytecode::StaKeyedProperty
            | Bytecode::StaInArrayLiteral
            | Bytecode::StaDataPropertyInLiteral
            | Bytecode::CallProperty
            | Bytecode::CallProperty1
            | Bytecode::CallUndefinedReceiver1
            | Bytecode::Construct => 4,

            Bytecode::CallProperty2 | Bytecode::CallUndefinedReceiver2 => 5,

            Bytecode::CallAnyReceiver | Bytecode::CallRuntime => 6,
        }
    }

    /// Tells whether this bytecode is one of the single-byte ShortStar variants (`Star0`..`Star15`).
    pub fn is_short_star(&self) -> bool {
        matches!(
            self,
            Bytecode::Star0
                | Bytecode::Star1
                | Bytecode::Star2
                | Bytecode::Star3
                | Bytecode::Star4
                | Bytecode::Star5
                | Bytecode::Star6
                | Bytecode::Star7
                | Bytecode::Star8
                | Bytecode::Star9
                | Bytecode::Star10
                | Bytecode::Star11
                | Bytecode::Star12
                | Bytecode::Star13
                | Bytecode::Star14
                | Bytecode::Star15
        )
    }

    /// Tells whether this bytecode is an unconditional or conditional jump.
    pub fn is_jump(&self) -> bool {
        matches!(
            self,
            Bytecode::Jump
                | Bytecode::JumpConstant
                | Bytecode::JumpIfTrue
                | Bytecode::JumpIfTrueConstant
                | Bytecode::JumpIfFalse
                | Bytecode::JumpIfFalseConstant
                | Bytecode::JumpIfNull
                | Bytecode::JumpIfNotNull
                | Bytecode::JumpIfUndefined
                | Bytecode::JumpIfNotUndefined
                | Bytecode::JumpIfUndefinedOrNull
                | Bytecode::JumpIfJSReceiver
                | Bytecode::JumpLoop
        )
    }

    /// Tells whether this bytecode is a conditional jump.
    pub fn is_conditional_jump(&self) -> bool {
        self.is_jump() && !matches!(self, Bytecode::Jump | Bytecode::JumpConstant | Bytecode::JumpLoop)
    }

    /// Returns the raw u8 opcode value.
    pub const fn to_byte(&self) -> u8 {
        *self as u8
    }

    /// Reconstructs a Bytecode enum from a raw opcode byte, if valid.
    #[inline(always)]
    pub fn from_byte(code: u8) -> Option<Self> {
        let idx = code as usize;
        if idx < ALL_BYTECODES.len() {
            // SAFETY: idx is verified to be strictly within ALL_BYTECODES table bounds
            Some(unsafe { *ALL_BYTECODES.get_unchecked(idx) })
        } else {
            None
        }
    }

    /// Reconstructs a Bytecode enum from a validated raw opcode byte.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `code < ALL_BYTECODES.len() as u8`.
    #[inline(always)]
    pub unsafe fn from_byte_unchecked(code: u8) -> Self {
        // SAFETY: The caller has verified that code is less than ALL_BYTECODES.len().
        unsafe { *ALL_BYTECODES.get_unchecked(code as usize) }
    }

    /// Converts a ShortStar register index (0..=15) to its corresponding Bytecode (Star0..Star15).
    pub fn from_short_star_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Bytecode::Star0),
            1 => Some(Bytecode::Star1),
            2 => Some(Bytecode::Star2),
            3 => Some(Bytecode::Star3),
            4 => Some(Bytecode::Star4),
            5 => Some(Bytecode::Star5),
            6 => Some(Bytecode::Star6),
            7 => Some(Bytecode::Star7),
            8 => Some(Bytecode::Star8),
            9 => Some(Bytecode::Star9),
            10 => Some(Bytecode::Star10),
            11 => Some(Bytecode::Star11),
            12 => Some(Bytecode::Star12),
            13 => Some(Bytecode::Star13),
            14 => Some(Bytecode::Star14),
            15 => Some(Bytecode::Star15),
            _ => None,
        }
    }

    /// If this bytecode is a ShortStar variant (Star0..Star15), returns the register index 0..=15.
    pub fn short_star_index(&self) -> Option<u8> {
        if self.is_short_star() {
            Some(*self as u8 - Bytecode::Star0 as u8)
        } else {
            None
        }
    }

    /// Returns the canonical mnemonic name for debugging and disassembly.
    pub fn name(&self) -> &'static str {
        match self {
            Bytecode::Wide => "Wide",
            Bytecode::ExtraWide => "ExtraWide",
            Bytecode::LdaZero => "LdaZero",
            Bytecode::LdaSmi => "LdaSmi",
            Bytecode::LdaUndefined => "LdaUndefined",
            Bytecode::LdaNull => "LdaNull",
            Bytecode::LdaTheHole => "LdaTheHole",
            Bytecode::LdaTrue => "LdaTrue",
            Bytecode::LdaFalse => "LdaFalse",
            Bytecode::LdaConstant => "LdaConstant",
            Bytecode::Ldar => "Ldar",
            Bytecode::LdaNamedProperty => "LdaNamedProperty",
            Bytecode::LdaKeyedProperty => "LdaKeyedProperty",
            Bytecode::LdaGlobal => "LdaGlobal",
            Bytecode::Star => "Star",
            Bytecode::StaNamedProperty => "StaNamedProperty",
            Bytecode::StaKeyedProperty => "StaKeyedProperty",
            Bytecode::StaGlobal => "StaGlobal",
            Bytecode::StaInArrayLiteral => "StaInArrayLiteral",
            Bytecode::StaDataPropertyInLiteral => "StaDataPropertyInLiteral",
            Bytecode::Star0 => "Star0",
            Bytecode::Star1 => "Star1",
            Bytecode::Star2 => "Star2",
            Bytecode::Star3 => "Star3",
            Bytecode::Star4 => "Star4",
            Bytecode::Star5 => "Star5",
            Bytecode::Star6 => "Star6",
            Bytecode::Star7 => "Star7",
            Bytecode::Star8 => "Star8",
            Bytecode::Star9 => "Star9",
            Bytecode::Star10 => "Star10",
            Bytecode::Star11 => "Star11",
            Bytecode::Star12 => "Star12",
            Bytecode::Star13 => "Star13",
            Bytecode::Star14 => "Star14",
            Bytecode::Star15 => "Star15",
            Bytecode::Mov => "Mov",
            Bytecode::Add => "Add",
            Bytecode::Sub => "Sub",
            Bytecode::Mul => "Mul",
            Bytecode::Div => "Div",
            Bytecode::Mod => "Mod",
            Bytecode::Exp => "Exp",
            Bytecode::BitwiseOr => "BitwiseOr",
            Bytecode::BitwiseXor => "BitwiseXor",
            Bytecode::BitwiseAnd => "BitwiseAnd",
            Bytecode::ShiftLeft => "ShiftLeft",
            Bytecode::ShiftRight => "ShiftRight",
            Bytecode::ShiftRightLogical => "ShiftRightLogical",
            Bytecode::AddSmi => "AddSmi",
            Bytecode::SubSmi => "SubSmi",
            Bytecode::MulSmi => "MulSmi",
            Bytecode::DivSmi => "DivSmi",
            Bytecode::ModSmi => "ModSmi",
            Bytecode::ExpSmi => "ExpSmi",
            Bytecode::BitwiseOrSmi => "BitwiseOrSmi",
            Bytecode::BitwiseXorSmi => "BitwiseXorSmi",
            Bytecode::BitwiseAndSmi => "BitwiseAndSmi",
            Bytecode::ShiftLeftSmi => "ShiftLeftSmi",
            Bytecode::ShiftRightSmi => "ShiftRightSmi",
            Bytecode::ShiftRightLogicalSmi => "ShiftRightLogicalSmi",
            Bytecode::Inc => "Inc",
            Bytecode::Dec => "Dec",
            Bytecode::Negate => "Negate",
            Bytecode::ToNumber => "ToNumber",
            Bytecode::ToNumeric => "ToNumeric",
            Bytecode::ToObject => "ToObject",
            Bytecode::ToString => "ToString",
            Bytecode::LogicalNot => "LogicalNot",
            Bytecode::BitwiseNot => "BitwiseNot",
            Bytecode::TypeOf => "TypeOf",
            Bytecode::TestEqual => "TestEqual",
            Bytecode::TestEqualStrict => "TestEqualStrict",
            Bytecode::TestLessThan => "TestLessThan",
            Bytecode::TestGreaterThan => "TestGreaterThan",
            Bytecode::TestLessThanOrEqual => "TestLessThanOrEqual",
            Bytecode::TestGreaterThanOrEqual => "TestGreaterThanOrEqual",
            Bytecode::TestReferenceEqual => "TestReferenceEqual",
            Bytecode::TestNull => "TestNull",
            Bytecode::TestUndefined => "TestUndefined",
            Bytecode::TestTypeOf => "TestTypeOf",
            Bytecode::Jump => "Jump",
            Bytecode::JumpConstant => "JumpConstant",
            Bytecode::JumpIfTrue => "JumpIfTrue",
            Bytecode::JumpIfTrueConstant => "JumpIfTrueConstant",
            Bytecode::JumpIfFalse => "JumpIfFalse",
            Bytecode::JumpIfFalseConstant => "JumpIfFalseConstant",
            Bytecode::JumpIfNull => "JumpIfNull",
            Bytecode::JumpIfNotNull => "JumpIfNotNull",
            Bytecode::JumpIfUndefined => "JumpIfUndefined",
            Bytecode::JumpIfNotUndefined => "JumpIfNotUndefined",
            Bytecode::JumpIfUndefinedOrNull => "JumpIfUndefinedOrNull",
            Bytecode::JumpIfJSReceiver => "JumpIfJSReceiver",
            Bytecode::JumpLoop => "JumpLoop",
            Bytecode::Return => "Return",
            Bytecode::Throw => "Throw",
            Bytecode::ReThrow => "ReThrow",
            Bytecode::Abort => "Abort",
            Bytecode::CallAnyReceiver => "CallAnyReceiver",
            Bytecode::CallProperty => "CallProperty",
            Bytecode::CallProperty0 => "CallProperty0",
            Bytecode::CallProperty1 => "CallProperty1",
            Bytecode::CallProperty2 => "CallProperty2",
            Bytecode::CallUndefinedReceiver => "CallUndefinedReceiver",
            Bytecode::CallUndefinedReceiver0 => "CallUndefinedReceiver0",
            Bytecode::CallUndefinedReceiver1 => "CallUndefinedReceiver1",
            Bytecode::CallUndefinedReceiver2 => "CallUndefinedReceiver2",
            Bytecode::CallRuntime => "CallRuntime",
            Bytecode::Construct => "Construct",
            Bytecode::CreateEmptyObjectLiteral => "CreateEmptyObjectLiteral",
            Bytecode::CreateEmptyArrayLiteral => "CreateEmptyArrayLiteral",
            Bytecode::CreateRegExpLiteral => "CreateRegExpLiteral",
            Bytecode::Debugger => "Debugger",
            Bytecode::ForInEnumerate => "ForInEnumerate",
            Bytecode::Await => "Await",
            Bytecode::TestInstanceOf => "TestInstanceOf",
            Bytecode::TestIn => "TestIn",
            Bytecode::LdaSuper => "LdaSuper",
            Bytecode::SpreadInArrayLiteral => "SpreadInArrayLiteral",
            Bytecode::SpreadInObjectLiteral => "SpreadInObjectLiteral",
            Bytecode::CallWithSpread => "CallWithSpread",
            Bytecode::SuspendGenerator => "SuspendGenerator",
            Bytecode::ResumeGenerator => "ResumeGenerator",
            Bytecode::GetIterator => "GetIterator",
            Bytecode::IteratorNext => "IteratorNext",
            Bytecode::IteratorDone => "IteratorDone",
            Bytecode::IteratorValue => "IteratorValue",
            Bytecode::DynamicImport => "DynamicImport",
            Bytecode::DeletePropertySloppy => "DeletePropertySloppy",
            Bytecode::DeleteKeyedPropertySloppy => "DeleteKeyedPropertySloppy",
            Bytecode::LdaNewTarget => "LdaNewTarget",
        }
    }
}

