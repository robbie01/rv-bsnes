use anyhow::{bail, ensure};

use crate::{cpu::*, instr::{Instruction as I, *}};

pub type OpFn = fn(&mut Cpu, &mut dyn Hypervisor, &Op) -> anyhow::Result<()>;

#[derive(Debug, Clone, Copy)]
pub struct Op {
    op_fn: OpFn,
    size: u8,
    instr: Instruction
}

fn nte(rounding_mode: RoundingMode) -> anyhow::Result<()> {
    if rounding_mode != RoundingMode::NearestTieToEven && rounding_mode != RoundingMode::Dynamic {
        bail!("not yet implemented: rounding modes other than NTE");
    }
    Ok(())
}

impl Op {
    pub fn new(instr: Instruction, size: u8) -> anyhow::Result<Self> {
        let op_fn = match instr {
            I::LoadInt(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { ref instr, .. }: &Op| {
                let I::LoadInt(LoadInt { dest, width, base, offset }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };

                use LoadWidth::*;
                let addr = cpu.read_x(base).wrapping_add_signed(offset.into());

                let v = match width {
                    ByteUnsigned => cpu.load_u8(addr)?.into(),
                    Byte => cpu.load_i8(addr)? as i32 as u32,
                    HalfUnsigned => cpu.load_u16(addr)?.into(),
                    Half => cpu.load_i16(addr)? as i32 as u32,
                    Word => cpu.load_u32(addr)?
                };

                cpu.write_x(dest, v);
                Ok(())
            },
            I::StoreInt(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { ref instr, .. }: &Op| {
                let I::StoreInt(StoreInt { offset, width, base, src }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };

                use StoreWidth::*;
                let addr = cpu.read_x(base).wrapping_add_signed(offset.into());

                let v = cpu.read_x(src);

                match width {
                    Byte => cpu.store_u8(addr, v as u8)?,
                    Half => cpu.store_u16(addr, v as u16)?,
                    Word => cpu.store_u32(addr, v)?
                }
                Ok(())
            },
            I::LoadFp(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { ref instr, .. }: &Op| {
                let I::LoadFp(LoadFp { dest, width, base, offset }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };

                use FpWidth::*;
                let addr = cpu.read_x(base).wrapping_add_signed(offset.into());

                let v = match width {
                    Word => FRegister::write_f32(cpu.load_f32(addr)?),
                    Double => FRegister::write_f64(cpu.load_f64(addr)?),
                };

                cpu.write_f(dest, v);
                Ok(())
            },
            I::StoreFp(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { ref instr, .. }: &Op| {
                let I::StoreFp(StoreFp { offset, width, base, src }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };
                
                use FpWidth::*;
                let addr = cpu.read_x(base).wrapping_add_signed(offset.into());

                let v = cpu.read_f(src);

                match width {
                    Word => cpu.store_f32(addr, v.read_f32())?,
                    Double => cpu.store_f64(addr, v.read_f64())?
                }
                Ok(())
            },
            I::Int(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { ref instr, .. }: &Op| {
                let I::Int(Int { dest, funct, src1, src2 }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };

                use IntegerFunct::*;

                let v1 = cpu.read_x(src1);
                let v2 = cpu.read_x(src2);

                let v = match funct {
                    Add => v1.wrapping_add(v2),
                    ShiftLeft => v1.unbounded_shl(v2),
                    SetLessThan => ((v1 as i32) < (v2 as i32)) as u32,
                    SetLessThanUnsigned => (v1 < v2) as u32,
                    Xor => v1 ^ v2,
                    ShiftRight => v1.unbounded_shr(v2),
                    Or => v1 | v2,
                    And => v1 & v2,
                    Subtract => v1.wrapping_sub(v2),
                    ShiftRightArithmetic => (v1 as i32).unbounded_shr(v2) as u32,
                    Multiply => v1.wrapping_mul(v2),
                    MultiplyHalf => (((v1 as i32 as i64) * (v2 as i32 as i64)) >> 32) as u32,
                    MultiplyHalfUnsigned => ((u64::from(v1) * u64::from(v2)) >> 32) as u32,
                    MultiplyHalfSignedUnsigned => (((v1 as i32 as i64) * (v2 as i64)) >> 32) as u32,
                    Divide if v2 == 0 => u32::MAX,
                    Divide => (v1 as i32).wrapping_div(v2 as i32) as u32,
                    DivideUnsigned if v2 == 0 => u32::MAX,
                    DivideUnsigned => v1.wrapping_div(v2),
                    Remainder if v2 == 0 => v1,
                    Remainder => ((v1 as i32) % (v2 as i32)) as u32,
                    RemainderUnsigned if v2 == 0 => v1,
                    RemainderUnsigned => v1 % v2
                };

                cpu.write_x(dest, v);
                Ok(())
            },
            I::IntImmediate(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { ref instr, .. }: &Op| {
                let I::IntImmediate(IntImmediate { dest, funct, src }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };

                use IntImmediateFunct::*;
                use crate::instr::{ImmShift::*, Imm12::*};

                let src = cpu.read_x(src);

                let v = match funct {
                    ImmShift(ShiftLeft, n) => src.unbounded_shl(u8::from(n).into()),
                    ImmShift(ShiftRightLogical, n) => src.unbounded_shr(u8::from(n).into()),
                    ImmShift(ShiftRightArithmetic, n) => (src as i32).unbounded_shr(u8::from(n).into()) as u32,
                    Imm12(Add, n) => (src as i32).wrapping_add(n.into()) as u32,
                    Imm12(SetLessThan, n) => ((src as i32) < i32::from(n)) as u32,
                    Imm12(SetLessThanUnsigned, n) => (src < (i32::from(n) as u32)) as u32,
                    Imm12(Xor, n) => src ^ (i32::from(n) as u32),
                    Imm12(Or, n) => src | (i32::from(n) as u32),
                    Imm12(And, n) => src & (i32::from(n) as u32)
                };

                cpu.write_x(dest, v);
                Ok(())
            },
            I::U(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { size, ref instr, .. }: &Op| {
                let I::U(U { type_, dest, imm }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };

                use UType::*;

                let v = match type_ {
                    LoadUpperImmediate => u32::from(imm) << 12,
                    AddUpperImmediateToPc => (cpu.pc - u32::from(size)).wrapping_add(u32::from(imm) << 12)
                };

                cpu.write_x(dest, v);
                Ok(())
            },
            I::Fp(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { ref instr, .. }: &Op| {
                let I::Fp(Fp { rounding_mode, funct, dest, src1, src2 }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };

                use RoundingMode::*;

                let v1 = cpu.read_f(src1);
                let v2 = cpu.read_f(src2);

                use FloatFunct::*;
                let v = match funct {
                    AddSingle => { nte(rounding_mode)?; FRegister::write_f32(v1.read_f32() + v2.read_f32()) },
                    SubtractSingle => { nte(rounding_mode)?; FRegister::write_f32(v1.read_f32() - v2.read_f32()) },
                    MultiplySingle => { nte(rounding_mode)?; FRegister::write_f32(v1.read_f32() * v2.read_f32()) },
                    DivideSingle => { nte(rounding_mode)?; FRegister::write_f32(v1.read_f32() / v2.read_f32()) },
                    SquareRootSingle => {
                        nte(rounding_mode)?;
                        ensure!(src2 == Register::ZERO);
                        FRegister::write_f32(v1.read_f32().sqrt())
                    },
                    InjectSignSingle => FRegister::write_f32(match rounding_mode {
                        NearestTieToEven => v1.read_f32().copysign(v2.read_f32()),
                        Zero => v1.read_f32().copysign(-v2.read_f32()),
                        Down => v1.read_f32().copysign(
                            if v1.read_f32().is_sign_positive() != v2.read_f32().is_sign_positive() {
                                -1.0
                            } else {
                                1.0
                            }
                        ),
                        _ => bail!("baddd")
                    }),
                    MinMaxSingle => FRegister::write_f32(match rounding_mode {
                        NearestTieToEven if !v1.read_f32().is_nan() && !v2.read_f32().is_nan() =>
                            v1.read_f32().min(v2.read_f32()),
                        Zero if !v1.read_f32().is_nan() && !v2.read_f32().is_nan() =>
                            v1.read_f32().max(v2.read_f32()),
                        NearestTieToEven | Zero if v1.read_f32().is_nan() =>
                            v2.read_f32(),
                        NearestTieToEven | Zero if v2.read_f32().is_nan() =>
                            v1.read_f32(),
                        NearestTieToEven | Zero if v1.read_f32().is_nan() && v2.read_f32().is_nan() =>
                            CANONICAL_NAN_F32,
                        _ => bail!("badddd")
                    }),
                    ConvertToWordSingle => {
                        cpu.write_x(dest, match src2 {
                            Register::ZERO => match rounding_mode {
                                NearestTieToEven | Dynamic => v1.read_f32().round_ties_even() as i32 as u32,
                                Zero => v1.read_f32() as i32 as u32,
                                NearestTieToMaxMagnitude => v1.read_f32().round() as i32 as u32,
                                Down => v1.read_f32().floor() as i32 as u32,
                                Up => v1.read_f32().ceil() as i32 as u32
                            },
                            Register::RA => match rounding_mode {
                                NearestTieToEven | Dynamic => v1.read_f32().round_ties_even() as u32,
                                Zero => v1.read_f32() as u32,
                                NearestTieToMaxMagnitude => v1.read_f32().round() as u32,
                                Down => v1.read_f32().floor() as u32,
                                Up => v1.read_f32().ceil() as u32
                            },
                            _ => bail!("BADddd")
                        });
                        return Ok(())
                    },
                    MoveToXSingle => {
                        ensure!(src2 == Register::ZERO);
                        cpu.write_x(dest, match rounding_mode {
                            NearestTieToEven => v1.read_f32().to_bits(),
                            Zero => bail!("not yet implemented: classify"),
                            _ => bail!("baDD")
                        });
                        return Ok(())
                    },
                    CompareSingle => {
                        cpu.write_x(dest, match rounding_mode {
                            NearestTieToEven => v1.read_f32() <= v2.read_f32(),
                            Zero => v1.read_f32() < v2.read_f32(),
                            Down => v1.read_f32() == v2.read_f32(),
                            _ => bail!("bAddd")
                        } as u32);
                        return Ok(())
                    },
                    ConvertFromWordSingle => FRegister::write_f32(match src2 {
                        Register::ZERO => cpu.read_x(src1) as i32 as f32,
                        Register::RA => cpu.read_x(src1) as f32,
                        _ => bail!("bADdD")
                    }),
                    MoveFromXSingle => {
                        ensure!(src2 == Register::ZERO);
                        FRegister::write_f32(f32::from_bits(cpu.read_x(src1)))
                    },

                    ConvertDoubleToSingle => {
                        ensure!(src2 == Register::RA);
                        nte(rounding_mode)?;
                        FRegister::write_f32(v1.read_f64() as f32)
                    },
                    ConvertSingleToDouble => {
                        ensure!(src2 == Register::ZERO);
                        FRegister::write_f64(v1.read_f32() as f64)
                    },

                    AddDouble => { nte(rounding_mode)?; FRegister::write_f64(v1.read_f64() + v2.read_f64()) },
                    SubtractDouble => { nte(rounding_mode)?; FRegister::write_f64(v1.read_f64() - v2.read_f64()) },
                    MultiplyDouble => { nte(rounding_mode)?; FRegister::write_f64(v1.read_f64() * v2.read_f64()) },
                    DivideDouble => { nte(rounding_mode)?; FRegister::write_f64(v1.read_f64() / v2.read_f64()) },
                    SquareRootDouble => {
                        nte(rounding_mode)?;
                        ensure!(src2 == Register::ZERO);
                        FRegister::write_f64(v1.read_f64().sqrt())
                    },
                    InjectSignDouble => FRegister::write_f64(match rounding_mode {
                        NearestTieToEven => v1.read_f64().copysign(v2.read_f64()),
                        Zero => v1.read_f64().copysign(-v2.read_f64()),
                        Down => v1.read_f64().copysign(
                            if v1.read_f64().is_sign_positive() != v2.read_f64().is_sign_positive() {
                                -1.0
                            } else {
                                1.0
                            }
                        ),
                        _ => bail!("baddd")
                    }),
                    MinMaxDouble => FRegister::write_f64(match rounding_mode {
                        NearestTieToEven if !v1.read_f64().is_nan() && !v2.read_f64().is_nan() =>
                            v1.read_f64().min(v2.read_f64()),
                        Zero if !v1.read_f64().is_nan() && !v2.read_f64().is_nan() =>
                            v1.read_f64().max(v2.read_f64()),
                        NearestTieToEven | Zero if v1.read_f64().is_nan() =>
                            v2.read_f64(),
                        NearestTieToEven | Zero if v2.read_f64().is_nan() =>
                            v1.read_f64(),
                        NearestTieToEven | Zero if v1.read_f64().is_nan() && v2.read_f64().is_nan() =>
                            CANONICAL_NAN_F64,
                        _ => bail!("badddd")
                    }),
                    ConvertToWordDouble => {
                        cpu.write_x(dest, match src2 {
                            Register::ZERO => match rounding_mode {
                                NearestTieToEven | Dynamic => v1.read_f64().round_ties_even() as i32 as u32,
                                Zero => v1.read_f64() as i32 as u32,
                                NearestTieToMaxMagnitude => v1.read_f64().round() as i32 as u32,
                                Down => v1.read_f64().floor() as i32 as u32,
                                Up => v1.read_f64().ceil() as i32 as u32
                            },
                            Register::RA => match rounding_mode {
                                NearestTieToEven | Dynamic => v1.read_f64().round_ties_even() as u32,
                                Zero => v1.read_f64() as u32,
                                NearestTieToMaxMagnitude => v1.read_f64().round() as u32,
                                Down => v1.read_f64().floor() as u32,
                                Up => v1.read_f64().ceil() as u32
                            },
                            _ => bail!("BADddd")
                        });
                        return Ok(())
                    },
                    CompareDouble => {
                        cpu.write_x(dest, match rounding_mode {
                            NearestTieToEven => v1.read_f64() <= v2.read_f64(),
                            Zero => v1.read_f64() < v2.read_f64(),
                            Down => v1.read_f64() == v2.read_f64(),
                            _ => bail!("bAddd")
                        } as u32);
                        return Ok(())
                    },
                    ClassifyDouble => bail!("not yet implemented: classify double"),
                    ConvertFromWordDouble => FRegister::write_f64(match src2 {
                        Register::ZERO => cpu.read_x(src1) as i32 as f64,
                        Register::RA => cpu.read_x(src1) as f64,
                        _ => bail!("bADdD")
                    })
                };

                cpu.write_f(dest, v);
                Ok(())
            },
            I::Fused(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { ref instr, .. }: &Op| {
                let I::Fused(Fused { type_, width, rounding_mode, dest, src1, src2, src3 }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };

                use FloatWidth::*;
                use FuseType::*;

                nte(rounding_mode)?;

                let v1 = cpu.read_f(src1);
                let v2 = cpu.read_f(src2);
                let v3 = cpu.read_f(src3);

                let v = match width {
                    Single => FRegister::write_f32(match type_ {
                        MultiplyAdd => v1.read_f32().mul_add(v2.read_f32(), v3.read_f32()),
                        MultiplySubtract => v1.read_f32().mul_add(v2.read_f32(), -v3.read_f32()),
                        NegatedMultiplySubtract => -(v1.read_f32() * v2.read_f32()) + v3.read_f32(),
                        NegatedMultiplyAdd => -(v1.read_f32() * v2.read_f32()) - v3.read_f32()
                    }),
                    Double => FRegister::write_f64(match type_ {
                        MultiplyAdd => v1.read_f64().mul_add(v2.read_f64(), v3.read_f64()),
                        MultiplySubtract => v1.read_f64().mul_add(v2.read_f64(), -v3.read_f64()),
                        NegatedMultiplySubtract => -(v1.read_f64() * v2.read_f64()) + v3.read_f64(),
                        NegatedMultiplyAdd => -(v1.read_f64() * v2.read_f64()) - v3.read_f64()
                    })
                };

                cpu.write_f(dest, v);
                Ok(())
            },

            // Fun fake atomics for a single-threaded core
            I::Fence => |_cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { .. }: &Op| {
                Ok(())
            },
            I::Amo(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { ref instr, .. }: &Op| {
                let I::Amo(Amo { dest, src1, src2, release: _, acquire: _, funct }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };

                use AmoFunct::*;

                match funct {
                    LoadReserved => {
                        ensure!(src2 == Register::ZERO);
                        let addr = cpu.read_x(src1);
                        let v = cpu.load_u32(addr)?;

                        cpu.write_x(dest, v);
                    },
                    StoreConditional => {
                        let addr = cpu.read_x(src1);
                        let v = cpu.read_x(src2);

                        cpu.store_u32(addr, v)?;
                        cpu.write_x(dest, 0);
                    },
                    Swap => {
                        let addr = cpu.read_x(src1);
                        let old = cpu.load_u32(addr)?;
                        cpu.store_u32(addr, cpu.read_x(src2))?;
                        cpu.write_x(dest, old);
                    }
                    _ => bail!("not yet implemented: amo {funct:?}")
                }

                Ok(())
            },

            I::JumpAndLink(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { size, ref instr, .. }: &Op| {
                let I::JumpAndLink(JumpAndLink { dest, offset }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };
                
                cpu.write_x(dest, cpu.pc);
                cpu.pc = (cpu.pc - u32::from(size)).wrapping_add_signed(offset.into());
                Ok(())
            },
            I::JumpAndLinkRegister(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { ref instr, .. }: &Op| {
                let I::JumpAndLinkRegister(JumpAndLinkRegister { dest, base, offset }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };
                
                let addr = cpu.read_x(base).wrapping_add_signed(offset.into());
                cpu.write_x(dest, cpu.pc);
                cpu.pc = addr;
                Ok(())
            },
            I::Branch(_) => |cpu: &mut Cpu, _h: &mut dyn Hypervisor, &Op { size, ref instr, .. }: &Op| {
                let I::Branch(Branch { offset, funct, src1, src2 }) = *instr else { unsafe { std::hint::unreachable_unchecked() } };

                use BranchType::*;
                
                let v1 = cpu.read_x(src1);
                let v2 = cpu.read_x(src2);

                if match funct {
                    Equal => v1 == v2,
                    NotEqual => v1 != v2,
                    LessThan => (v1 as i32) < (v2 as i32),
                    GreaterThanOrEqual => (v1 as i32) >= (v2 as i32),
                    LessThanUnsigned => v1 < v2,
                    GreaterThanOrEqualUnsigned => v1 >= v2
                } {
                    cpu.pc = (cpu.pc - u32::from(size)).wrapping_add_signed(offset.into());
                }
                Ok(())
            },
            
            I::System(System::Ebreak) => |cpu: &mut Cpu, h: &mut dyn Hypervisor, &Op { .. }: &Op| {
                h.ebreak(cpu)?;
                Ok(())
            },
            I::System(System::Ecall) => |cpu: &mut Cpu, h: &mut dyn Hypervisor, &Op { .. }: &Op| {
                h.ecall(cpu)?;
                Ok(())
            },
            _ => bail!("not yet implemented: {instr:?}")
        };

        Ok(Op { op_fn, size, instr })
    }

    #[inline(always)]
    pub fn execute(&self, cpu: &mut Cpu, h: &mut dyn Hypervisor) -> anyhow::Result<()> {
        cpu.pc += u32::from(self.size);
        (self.op_fn)(cpu, h, self)
    }
}