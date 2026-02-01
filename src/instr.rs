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

const C0_ADDI4SPN: u16 = 0b000;
const C0_FLD: u16 = 0b001;
const C0_LW: u16 = 0b010;
const C0_FLW: u16 = 0b011;
const C0_FSD: u16 = 0b101;
const C0_SW: u16 = 0b110;
const C0_FSW: u16 = 0b111;

const C1_ADDI: u16 = 0b000;
const C1_JAL: u16 = 0b001;
const C1_LI: u16 = 0b010;
/// NOTE: becomes C.ADDI16SP when rd=2
const C1_LUI: u16 = 0b011;
const C1_MANY: u16 = 0b100;
const C1_J: u16 = 0b101;
const C1_BEQZ: u16 = 0b110;
const C1_BNEZ: u16 = 0b111;

const C2_SLLI: u16 = 0b000;
const C2_FLDSP: u16 = 0b001;
const C2_LWSP: u16 = 0b010;
const C2_FLWSP: u16 = 0b011;
const C2_MANY: u16 = 0b100;
const C2_FSDSP: u16 = 0b101;
const C2_SWSP: u16 = 0b110;
const C2_FSWSP: u16 = 0b111;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadInt {
    pub dest: Register,
    pub width: LoadWidth,
    pub base: Register,
    pub offset: U12
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntImmediate {
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
pub struct StoreInt {
    pub offset: U12,
    pub width: StoreWidth,
    pub base: Register,
    pub src: Register
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Int {
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
    pub offset: I20
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
pub struct Fp {
    /// NOTE: restrictions on this depending on funct
    rounding_mode: RoundingMode,
    funct: FloatFunct,
    dest: Register,
    /// NOTE: restrictions on this depending on funct
    src1: Register,
    /// NOTE: restrictions on this depending on funct
    src2: Register
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Amo {
    dest: Register,
    src1: Register,
    /// NOTE: must always be 0 for lr
    src2: Register,
    release: bool,
    acquire: bool,
    funct: AmoFunct
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
    LoadInt(LoadInt),
    Fence,
    IntImmediate(IntImmediate),
    U(U),
    StoreInt(StoreInt),
    Int(Int),
    Branch(Branch),
    JumpAndLink(JumpAndLink),
    JumpAndLinkRegister(JumpAndLinkRegister),
    System(System),
    LoadFp(LoadFp),
    StoreFp(StoreFp),
    OpFp(Fp),
    Fused(Fused),
    Amo(Amo)
}

impl Instruction {
    pub fn next_is_compressed(next: u8) -> bool {
        next & 0b11 != 0b11
    }

    pub fn decode_compressed(v: u16) -> anyhow::Result<Self> {
        Ok(match v & 0b11 {
            0 => match v >> 13 {
                C0_ADDI4SPN => Self::IntImmediate(IntImmediate {
                    dest: Register::new_rvc((v >> 2) & 0b111)?,
                    funct: IntegerOpImmediate::Imm12(
                        Imm12::Add,
                        {
                            let imm = (v >> 5) & 0xff;
                            let nzuimm = ((imm & 1) << 1) | ((imm & 0b10) >> 1) | ((imm & 0b111100) << 2) | ((imm & 0b11000000) >> 4);
                            U12::new((nzuimm * 4).into())?
                        }
                    ),
                    src: Register::new(2)?
                }),
                C0_FLD => bail!("not yet implemented (C)"),
                C0_LW => bail!("not yet implemented (C)"),
                C0_FLW => bail!("not yet implemented (C)"),
                C0_FSD => bail!("not yet implemented (C)"),
                C0_SW => bail!("not yet implemented (C)"),
                C0_FSW => bail!("not yet implemented (C)"),
                0b100 => bail!("unknown opcode"), // reserved
                _ => unreachable!()
            },
            1 => match v >> 13 {
                C1_ADDI => bail!("not yet implemented (C)"),
                C1_JAL => bail!("not yet implemented (C)"),
                C1_LI => bail!("not yet implemented (C)"),
                C1_LUI => bail!("not yet implemented (C)"),
                C1_MANY => bail!("not yet implemented (C)"),
                C1_J => bail!("not yet implemented (C)"),
                C1_BEQZ => bail!("not yet implemented (C)"),
                C1_BNEZ => bail!("not yet implemented (C)"),
                _ => unreachable!()
            },
            2 => match v >> 13 {
                C2_SLLI => bail!("not yet implemented (C)"),
                C2_FLDSP => bail!("not yet implemented (C)"),
                C2_LWSP => bail!("not yet implemented (C)"),
                C2_FLWSP => bail!("not yet implemented (C)"),
                C2_MANY => bail!("not yet implemented (C)"),
                C2_FSDSP => bail!("not yet implemented (C)"),
                C2_SWSP => bail!("not yet implemented (C)"),
                C2_FSWSP => bail!("not yet implemented (C)"),
                _ => unreachable!()
            },
            3 => bail!("not a compressed instruction"),
            _ => unreachable!()
        })
    }

    pub fn decode(v: u32) -> anyhow::Result<Self> {
        Ok(match v & OPCODE_MASK {
            LOAD => Self::LoadInt(LoadInt {
                dest: Register::new((v >> 7) & 0b11111)?,
                width: LoadWidth::new((v >> 12) & 0b111)?,
                base: Register::new((v >> 15) & 0b11111)?,
                offset: U12::new(v >> 20)?
            }),
            FENCE => Self::Fence,
            OP_IMM => Self::IntImmediate(IntImmediate {
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
            STORE => Self::StoreInt(StoreInt {
                offset: U12::new(((v >> 7) & 0b11111) | ((v >> 20) & 0b111111100000))?,
                width: StoreWidth::new((v >> 12) & 0b111)?,
                base: Register::new((v >> 15) & 0b11111)?,
                src: Register::new((v >> 20) & 0b11111)?
            }),
            OP => Self::Int(Int {
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
                offset: I20::new({
                    let imm = v >> 12;
                    ((imm & 0xff) << 11) | ((imm & 0x100) << 2) | ((imm & 0x7fe00) >> 9) | (imm & 0x80000)
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
            OP_FP => Self::OpFp(Fp {
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
            AMO => Self::Amo(Amo {
                dest: Register::new((v >> 7) & 0b11111)?,
                src1: Register::new((v >> 15) & 0b11111)?,
                src2: Register::new((v >> 20) & 0b11111)?,
                release: v & (1 << 25) != 0,
                acquire: v & (1 << 26) != 0,
                funct: AmoFunct::new(v >> 27)?
            }),
            i if i > OPCODE_MASK => unreachable!(),
            _ => bail!("unknown opcode")
        })
    }
}