use std::marker::ConstParamTy;
use anyhow::bail;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Register(u8);

impl Register {
    pub const ZERO: Self = Self(0);
    pub const RA: Self = Self(1);
    pub const SP: Self = Self(2);
    pub const GP: Self = Self(3);
    pub const TP: Self = Self(4);
    pub const T0: Self = Self(5);
    pub const T1: Self = Self(6);
    pub const T2: Self = Self(7);
    pub const S0: Self = Self(8);
    pub const S1: Self = Self(9);
    pub const A0: Self = Self(10);
    pub const A1: Self = Self(11);
    pub const A2: Self = Self(12);
    pub const A3: Self = Self(13);
    pub const A4: Self = Self(14);
    pub const A5: Self = Self(15);
    pub const A6: Self = Self(16);
    pub const A7: Self = Self(17);

    pub fn new(v: impl Into<u32>) -> anyhow::Result<Self> {
        let v = v.into();
        if v < 32 {
            Ok(Self(v as u8))
        } else {
            bail!("register out of range")
        }
    }

    pub fn new_rvc(v: u16) -> anyhow::Result<Self> {
        if v < 8 {
            Ok(Self(v as u8 + 8))
        } else {
            bail!("register out of range")
        }
    }
}

impl From<Register> for u8 {
    fn from(value: Register) -> Self {
        value.0
    }
}

impl From<Register> for usize {
    fn from(value: Register) -> Self {
        value.0.into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ConstParamTy)]
#[repr(u8)]
pub enum LoadWidth {
    Byte,
    Half,
    Word,
    ByteUnsigned = 4,
    HalfUnsigned
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ConstParamTy)]
#[repr(u8)]
pub enum StoreWidth {
    Byte,
    Half,
    Word
}

impl LoadWidth {
    pub fn new(v: u32) -> anyhow::Result<Self> {
        use LoadWidth::*;
        Ok(match v {
            0 => Byte,
            1 => Half,
            2 => Word,
            4 => ByteUnsigned,
            5 => HalfUnsigned,
            _ => bail!("load width out of range")
        })
    }
}

impl StoreWidth {
    pub fn new(v: u32) -> anyhow::Result<Self> {
        use StoreWidth::*;
        Ok(match v {
            0 => Byte,
            1 => Half,
            2 => Word,
            _ => bail!("store width out of range")
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FpWidth {
    Word = 2,
    Double
}

impl FpWidth {
    pub fn new(v: u32) -> anyhow::Result<Self> {
        use FpWidth::*;
        Ok(match v {
            2 => Word,
            3 => Double,
            _ => bail!("fp width out of range")
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct U5(u8);

impl U5 {
    pub fn new(v: impl Into<u32>) -> anyhow::Result<Self> {
        let v = v.into();
        if v < (1 << 5) {
            Ok(Self(v as u8))
        } else {
            bail!("u5 out of range")
        }
    }
}

impl From<U5> for u8 {
    fn from(value: U5) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct I12(i16);

impl I12 {
    pub const ZERO: Self = Self(0);

    pub fn new(v: impl Into<u32>) -> anyhow::Result<Self> {
        let v = v.into();
        if v < (1 << 12) {
            Ok(Self((v << 4) as i16 >> 4))
        } else {
            bail!("i12 out of range")
        }
    }

    pub fn new_13(v: u32) -> anyhow::Result<Self> {
        if v < (1 << 13) {
            Ok(Self((v << 3) as i16 >> 3))
        } else {
            bail!("i12 out of range")
        }
    }

    pub fn new_6(v: u16) -> anyhow::Result<Self> {
        if v < (1 << 6) {
            Ok(Self((v << 10) as i16 >> 10))
        } else {
            bail!("i12 out of range")
        }
    }

    pub fn new_9(v: u16) -> anyhow::Result<Self> {
        if v < (1 << 9) {
            Ok(Self((v << 7) as i16 >> 7))
        } else {
            bail!("i12 out of range")
        }
    }

    pub fn new_10(v: u16) -> anyhow::Result<Self> {
        if v < (1 << 10) {
            Ok(Self((v << 6) as i16 >> 6))
        } else {
            bail!("i12 out of range")
        }
    }
}

impl From<I12> for u16 {
    fn from(value: I12) -> Self {
        value.0 as u16 & 0xfff
    }
}

impl From<I12> for i16 {
    fn from(value: I12) -> Self {
        value.0
    }
}

impl From<I12> for i32 {
    fn from(value: I12) -> Self {
        value.0.into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct U20(u32);

impl U20 {
    pub fn new(v: u32) -> anyhow::Result<Self> {
        if v < (1 << 20) {
            Ok(Self(v))
        } else {
            bail!("u20 out of range")
        }
    }

    pub fn new_i6(v: u16) -> anyhow::Result<Self> {
        if v < (1 << 6) {
            Ok(Self((((v as u32) << 26) as i32 >> 26) as u32 & 0xFFFFF))
        } else {
            bail!("u20 out of range")
        }
    }
}

impl From<U20> for u32 {
    fn from(value: U20) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct I20(i32);

impl I20 {
    pub fn new(v: u32) -> anyhow::Result<Self> {
        if v < (1 << 20) {
            let val = (v << 12) as i32 >> 11;
            Ok(Self(val))
        } else {
            bail!("i20 out of range")
        }
    }

    pub fn new_11(v: u16) -> anyhow::Result<Self> {
        if v < (1 << 11) {
            let val = ((v as u32) << 21) as i32 >> 20;
            Ok(Self(val))
        } else {
            bail!("i20 out of range")
        }
    }
}

impl From<I20> for i32 {
    fn from(value: I20) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Imm12 {
    Add,
    SetLessThan = 2,
    SetLessThanUnsigned,
    Xor,
    Or = 6,
    And
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImmShift {
    ShiftLeft,
    ShiftRightLogical,
    ShiftRightArithmetic
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntImmediateFunct {
    Imm12(Imm12, I12),
    ImmShift(ImmShift, U5)
}

impl IntImmediateFunct {
    pub fn new(v: u32, imm: I12) -> anyhow::Result<Self> {
        use self::{
            IntImmediateFunct::*,
            Imm12::*,
            ImmShift::*
        };

        Ok(match v {
            0 => Imm12(Add, imm),
            2 => Imm12(SetLessThan, imm),
            3 => Imm12(SetLessThanUnsigned, imm),
            4 => Imm12(Xor, imm),
            6 => Imm12(Or, imm),
            7 => Imm12(And, imm),

            1 if u16::from(imm) >> 5 == 0 => ImmShift(ShiftLeft, U5::new(u16::from(imm))?),
            5 if u16::from(imm) >> 5 == 0 => ImmShift(ShiftRightLogical, U5::new(u16::from(imm))?),
            5 if u16::from(imm) >> 5 == 0b100000 => ImmShift(ShiftRightArithmetic, U5::new(u16::from(imm) & 0b11111)?),

            _ => bail!("unknown immediate op")
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ConstParamTy)]
#[repr(u8)]
pub enum IntegerFunct {
    Add,
    ShiftLeft,
    SetLessThan,
    SetLessThanUnsigned,
    Xor,
    ShiftRight,
    Or,
    And,
    Subtract,
    ShiftRightArithmetic,

    // RV32M extension
    Multiply,
    MultiplyHalf,
    MultiplyHalfSignedUnsigned,
    MultiplyHalfUnsigned,
    Divide,
    DivideUnsigned,
    Remainder,
    RemainderUnsigned
}

impl IntegerFunct {
    pub fn new(v: u32) -> anyhow::Result<Self> {
        use IntegerFunct::*;
        Ok(match v {
            0 => Add,
            1 => ShiftLeft,
            2 => SetLessThan,
            3 => SetLessThanUnsigned,
            4 => Xor,
            5 => ShiftRight,
            6 => Or,
            7 => And,
            0b100000000 => Subtract,
            0b100000101 => ShiftRightArithmetic,

            8 => Multiply,
            9 => MultiplyHalf,
            10 => MultiplyHalfSignedUnsigned,
            11 => MultiplyHalfUnsigned,
            12 => Divide,
            13 => DivideUnsigned,
            14 => Remainder,
            15 => RemainderUnsigned,

            _ => bail!("unknown op")
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ConstParamTy)]
#[repr(u8)]
pub enum BranchType {
    Equal,
    NotEqual,
    LessThan = 4,
    GreaterThanOrEqual,
    LessThanUnsigned,
    GreaterThanOrEqualUnsigned
}

impl BranchType {
    pub fn new(v: u32) -> anyhow::Result<Self> {
        use BranchType::*;
        Ok(match v {
            0 => Equal,
            1 => NotEqual,
            4 => LessThan,
            5 => GreaterThanOrEqual,
            6 => LessThanUnsigned,
            7 => GreaterThanOrEqualUnsigned,
            _ => bail!("unknown branch type")
        })
    }
}

#[expect(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum InjectSignType {
    AsIs,
    Negated,
    Xor
}

#[expect(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum MinMaxType {
    Minimum,
    Maximum
}

#[expect(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Signedness {
    Signed,
    Unsigned
}

#[expect(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum MoveToXSingleType {
    Move,
    Classify
}

#[expect(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum CompareType {
    LessThanOrEqual,
    LessThan,
    Equal
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FloatFunct {
    AddSingle = 0,
    SubtractSingle = 0b100,
    MultiplySingle = 0b1000,
    DivideSingle = 0b1100,
    SquareRootSingle = 0b101100,
    InjectSignSingle = 0b10000, // rm = 0 (as-is), 1 (negated), or 2 (xor)
    MinMaxSingle = 0b10100, // rm = 0 (min) or 1 (max)
    ConvertToWordSingle = 0b1100000, // rs2 = 0 for signed, 1 for unsigned
    MoveToXSingle = 0b1110000, // rs2 = 0; rm = 0 for move, 1 for classify
    CompareSingle = 0b1010000, // rm = 0 (le), 1 (lt), or 2 (eq)
    ConvertFromWordSingle = 0b1101000, // rs2 = 0 for signed, 1 for unsigned
    MoveFromXSingle = 0b1111000, // rs2 = 0

    // RV32D
    ConvertDoubleToSingle = 0b100000, // rs2 = 1
    ConvertSingleToDouble = 0b100001, // rs2 = 0

    AddDouble = 0b0000001,
    SubtractDouble = 0b0000101,
    MultiplyDouble = 0b0001001,
    DivideDouble = 0b0001101,
    SquareRootDouble = 0b0101101,
    InjectSignDouble = 0b0010001, // rm = 0 (as-is), 1 (negated), or 2 (xor)
    MinMaxDouble = 0b0010101, // rm = 0 (min) or 1 (max)
    CompareDouble = 0b1010001, // rm = 0 (le), 1 (lt), or 2 (eq)
    ClassifyDouble = 0b1110001, // rs2 = 0, rm = 1
    ConvertToWordDouble = 0b1100001, // rs2 = 0 for signed, 1 for unsigned
    ConvertFromWordDouble = 0b1101001 // rs2 = 0 for signed, 1 for unsigned
}

impl FloatFunct {
    pub fn new(v: u32) -> anyhow::Result<Self> {
        use FloatFunct::*;
        Ok(match v {
            0b0000000 => AddSingle,
            0b0000100 => SubtractSingle,
            0b0001000 => MultiplySingle,
            0b0001100 => DivideSingle,
            0b0101100 => SquareRootSingle,
            0b0010000 => InjectSignSingle,
            0b0010100 => MinMaxSingle,
            0b1100000 => ConvertToWordSingle,
            0b1110000 => MoveToXSingle,
            0b1010000 => CompareSingle,
            0b1101000 => ConvertFromWordSingle,
            0b1111000 => MoveFromXSingle,
            0b0100000 => ConvertDoubleToSingle,
            0b0100001 => ConvertSingleToDouble,
            0b0000001 => AddDouble,
            0b0000101 => SubtractDouble,
            0b0001001 => MultiplyDouble,
            0b0001101 => DivideDouble,
            0b0101101 => SquareRootDouble,
            0b0010001 => InjectSignDouble,
            0b0010101 => MinMaxDouble,
            0b1010001 => CompareDouble,
            0b1110001 => ClassifyDouble,
            0b1100001 => ConvertToWordDouble,
            0b1101001 => ConvertFromWordDouble,
            _ => bail!("unknown float funct")
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RoundingMode {
    NearestTieToEven,
    Zero,
    Down,
    Up,
    NearestTieToMaxMagnitude,
    Dynamic = 7
}

impl RoundingMode {
    pub fn new(v: u32) -> anyhow::Result<Self> {
        use RoundingMode::*;
        Ok(match v {
            0 => NearestTieToEven,
            1 => Zero,
            2 => Down,
            3 => Up,
            4 => NearestTieToMaxMagnitude,
            7 => Dynamic,
            _ => bail!("unknown rounding mode")
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CsrFunct {
    ReadWrite = 1,
    ReadAndSetBits,
    ReadAndClearBits,
    ReadWriteImmediate = 5,
    ReadAndSetBitsImmediate,
    ReadAndClearBitsImmediate
}

impl CsrFunct {
    pub fn new(v: u32) -> anyhow::Result<Self> {
        use CsrFunct::*;
        Ok(match v {
            1 => ReadWrite,
            2 => ReadAndSetBits,
            3 => ReadAndClearBits,
            5 => ReadWriteImmediate,
            6 => ReadAndSetBitsImmediate,
            7 => ReadAndClearBitsImmediate,
            _ => bail!("unknown csr funct")
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ConstParamTy)]
#[repr(u8)]
pub enum AmoFunct {
    Add,
    Swap,
    LoadReserved,
    StoreConditional,
    Xor,
    Or = 8,
    And = 12,
    Min = 16,
    Max = 20,
    MinUnsigned = 24,
    MaxUnsigned = 28
}

impl AmoFunct {
    pub fn new(v: u32) -> anyhow::Result<Self> {
        use AmoFunct::*;
        Ok(match v {
            0 => Add,
            1 => Swap,
            2 => LoadReserved,
            3 => StoreConditional,
            4 => Xor,
            8 => Or,
            12 => And,
            16 => Min,
            20 => Max,
            24 => MinUnsigned,
            28 => MaxUnsigned,
            _ => bail!("unknown amo funct")
        })
    }
}