mod data;

use anyhow::bail;
pub use data::*;

const LOAD: u32   = 0b0000011;
const FENCE: u32  = 0b0001111;
const OP_IMM: u32 = 0b0010011;
const AUIPC: u32  = 0b0010111;
const STORE: u32  = 0b0100011;
const OP: u32     = 0b0110011;
const LUI: u32    = 0b0110111;
const BRANCH: u32 = 0b1100011;
const JAL: u32    = 0b1101111;
const JALR: u32   = 0b1100111;
const SYSTEM: u32 = 0b1110011;

const LOAD_FP: u32 = 0b0000111;
const STORE_FP: u32 = 0b0100111;
const OP_FP: u32 = 0b1010011;
const MADD: u32 = 0b1000011;
const MSUB: u32 = 0b1000111;
const NMSUB: u32 = 0b1001011;
const NMADD: u32 = 0b1001111;

const AMO: u32 = 0b0101111;

const OPCODE_MASK: u32 = 0b1111111;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Load {
    pub dest: Register,
    pub width: LoadWidth,
    pub base: Register,
    pub offset: U12
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpImmediate {
    pub dest: Register,
    pub funct: IntegerOpImmediate,
    pub src: Register
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UType {
    AddUpperImmediateToPc,
    LoadUpperImmediate
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct U {
    pub type_: UType,
    pub dest: Register,
    pub imm: U20
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Store {
    pub offset: U12,
    pub width: StoreWidth,
    pub base: Register,
    pub src: Register
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Op {
    pub dest: Register,
    pub funct: IntegerFunct,
    pub src1: Register,
    pub src2: Register
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Branch {
    pub offset: U12,
    pub funct: BranchType,
    pub src1: Register,
    pub src2: Register
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpAndLink {
    pub dest: Register,
    pub offset: U20
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpAndLinkRegister {
    pub dest: Register,
    pub base: Register,
    pub offset: U12
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Csr {
    pub dest: Register,
    pub funct: CsrFunct,
    pub src: Register,
    pub csr: U12
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Ecall,
    Ebreak,
    Csr(Csr)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadFp {
    pub dest: Register,
    pub width: FpWidth,
    pub base: Register,
    pub offset: U12
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreFp {
    pub offset: U12,
    pub width: FpWidth,
    pub base: Register,
    pub src: Register
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuseType {
    MultiplyAdd,
    MultiplySubtract,
    NegatedMultiplySubtract,
    NegatedMultiplyAdd
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatWidth {
    Single,
    Double
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fused {
    type_: FuseType,
    width: FloatWidth,
    rounding_mode: RoundingMode,
    dest: Register,
    src1: Register,
    src2: Register,
    src3: Register
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpFp {
    rounding_mode: RoundingMode,
    funct: FloatFunct,
    dest: Register,
    src1: Register,
    src2: Register
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
    Load(Load),
    Fence,
    OpImmediate(OpImmediate),
    U(U),
    Store(Store),
    Op(Op),
    Branch(Branch),
    JumpAndLink(JumpAndLink),
    JumpAndLinkRegister(JumpAndLinkRegister),
    System(System),
    LoadFp(LoadFp),
    StoreFp(StoreFp),
    OpFp(OpFp),
    Fused(Fused)
}

impl Instruction {
    pub fn next_is_compressed(next: u8) -> bool {
        next & 0b11 != 0b11
    }

    pub fn decode_compressed(_v: u16) -> anyhow::Result<Self> {
        bail!("compressed instruction")
    }

    pub fn decode(v: u32) -> anyhow::Result<Self> {
        Ok(match v & OPCODE_MASK {
            LOAD => Self::Load(Load {
                dest: Register::new((v >> 7) & 0b11111)?,
                width: LoadWidth::new((v >> 12) & 0b111)?,
                base: Register::new((v >> 15) & 0b11111)?,
                offset: U12::new(v >> 20)?
            }),
            FENCE => Self::Fence,
            OP_IMM => Self::OpImmediate(OpImmediate {
                dest: Register::new((v >> 7) & 0b11111)?,
                funct: IntegerOpImmediate::new((v >> 12) & 0b111, U12::new(v >> 20)?)?,
                src: Register::new((v >> 15) & 0b11111)?
            }),
            AUIPC | LUI => Self::U(U {
                type_: match v & OPCODE_MASK {
                    AUIPC => UType::AddUpperImmediateToPc,
                    LUI => UType::LoadUpperImmediate,
                    _ => unreachable!()
                },
                dest: Register::new((v >> 7) & 0b11111)?,
                imm: U20::new(v >> 12)?
            }),
            STORE => Self::Store(Store {
                offset: U12::new(((v >> 7) & 0b11111) | ((v >> 20) & 0b111111100000))?,
                width: StoreWidth::new((v >> 12) & 0b111)?,
                base: Register::new((v >> 15) & 0b11111)?,
                src: Register::new((v >> 20) & 0b11111)?
            }),
            OP => Self::Op(Op {
                dest: Register::new((v >> 7) & 0b11111)?,
                funct: IntegerFunct::new(((v >> 12) & 0b111) | ((v >> 22) & 0b1111111000))?,
                src1: Register::new((v >> 15) & 0b11111)?,
                src2: Register::new((v >> 20) & 0b11111)?
            }),
            BRANCH => Self::Branch(Branch {
                offset: U12::new(((v >> 7) & 0b11111) | ((v >> 20) & 0b111111100000))?,
                funct: BranchType::new((v >> 12) & 0b111)?,
                src1: Register::new((v >> 15) & 0b11111)?,
                src2: Register::new((v >> 20) & 0b11111)?
            }),
            JAL => Self::JumpAndLink(JumpAndLink {
                dest: Register::new((v >> 7) & 0b11111)?,
                offset: U20::new({
                    let b1_10 = (v >> 21) & 0b1111111111;
                    let b11 = (v >> 20) & 1;
                    let b12_19 = (v >> 12) & 0b11111111;
                    let b20 = v >> 31;
                    
                    b1_10 | (b11 << 10) | (b12_19 << 11) | (b20 << 19)
                })?
            }),
            JALR if v & (0b111 << 12) == 0 => Self::JumpAndLinkRegister(JumpAndLinkRegister {
                dest: Register::new((v >> 7) & 0b11111)?,
                base: Register::new((v >> 15) & 0b11111)?,
                offset: U12::new(v >> 20)?
            }),
            SYSTEM => Self::System(match v >> 7 {
                0 => System::Ecall,
                0b10000000000000 => System::Ebreak,
                _ => match CsrFunct::new(v >> 12 & 0b111) {
                    Ok(funct) => System::Csr(Csr {
                        dest: Register::new((v >> 7) & 0b11111)?,
                        funct,
                        src: Register::new((v >> 15) & 0b11111)?,
                        csr: U12::new(v >> 20)?
                    }),
                    Err(_) => bail!("unknown SYSTEM")
                }
            }),
            LOAD_FP => Self::LoadFp(LoadFp {
                dest: Register::new((v >> 7) & 0b11111)?,
                width: FpWidth::new((v >> 12) & 0b111)?,
                base: Register::new((v >> 15) & 0b11111)?,
                offset: U12::new(v >> 20)?
            }),
            STORE_FP => Self::StoreFp(StoreFp {
                offset: U12::new(((v >> 7) & 0b11111) | ((v >> 20) & 0b111111100000))?,
                width: FpWidth::new((v >> 12) & 0b111)?,
                base: Register::new((v >> 15) & 0b11111)?,
                src: Register::new((v >> 20) & 0b11111)?
            }),
            OP_FP => Self::OpFp(OpFp {
                dest: Register::new((v >> 7) & 0b11111)?,
                funct: FloatFunct::new(v >> 25)?,
                rounding_mode: RoundingMode::new((v >> 12) & 0b111)?,
                src1: Register::new((v >> 15) & 0b11111)?,
                src2: Register::new((v >> 20) & 0b11111)?,
            }),
            MADD | MSUB | NMSUB | NMADD => Self::Fused(Fused {
                type_: match v & OPCODE_MASK {
                    MADD => FuseType::MultiplyAdd,
                    MSUB => FuseType::MultiplySubtract,
                    NMSUB => FuseType::NegatedMultiplySubtract,
                    NMADD => FuseType::NegatedMultiplyAdd,
                    _ => unreachable!()
                },
                width: match (v >> 25) & 0b11 {
                    0 => FloatWidth::Single,
                    1 => FloatWidth::Double,
                    _ => bail!("unknown float width")
                },
                dest: Register::new((v >> 7) & 0b11111)?,
                rounding_mode: RoundingMode::new((v >> 12) & 0b111)?,
                src1: Register::new((v >> 15) & 0b11111)?,
                src2: Register::new((v >> 20) & 0b11111)?,
                src3: Register::new(v >> 27)?
            }),
            AMO => bail!("AMO instruction"),
            i if i > OPCODE_MASK => unreachable!(),
            _ => bail!("unknown opcode")
        })
    }
}