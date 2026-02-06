#![allow(unused_assignments)]

use std::{iter, mem::MaybeUninit};

use anyhow::{bail, ensure};

use crate::{cpu::*, instr::{Instruction as I, *}};

pub type OpFn = unsafe fn(&mut Cpu, &mut dyn Hypervisor, *const Op) -> anyhow::Result<()>;

#[derive(Debug, Clone, Copy)]
pub struct Op {
    op_fn: OpFn,
    instr: MaybeUninit<InstructionUnion>
}

fn nte(rounding_mode: RoundingMode) -> anyhow::Result<()> {
    if rounding_mode != RoundingMode::NearestTieToEven && rounding_mode != RoundingMode::Dynamic {
        bail!("not yet implemented: rounding modes other than NTE");
    }
    Ok(())
}

#[inline(always)]
unsafe fn dispatch(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
    unsafe { become ((*stream).op_fn)(cpu, h, stream) }
}

unsafe fn end(_: &mut Cpu, _: &mut dyn Hypervisor, _: *const Op) -> anyhow::Result<()> {
    Ok(())
}

impl Op {
    pub fn new(instr: Instruction, size: u8) -> anyhow::Result<Self> {
        unsafe fn load_int<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let LoadInt { dest, width, base, offset } = unsafe { (*stream).instr.assume_init().load_int };

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
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn store_int<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let StoreInt { offset, width, base, src } = unsafe { (*stream).instr.assume_init().store_int };

            use StoreWidth::*;
            let addr = cpu.read_x(base).wrapping_add_signed(offset.into());

            let v = cpu.read_x(src);

            match width {
                Byte => cpu.store_u8(addr, v as u8)?,
                Half => cpu.store_u16(addr, v as u16)?,
                Word => cpu.store_u32(addr, v)?
            }
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn load_fp<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let LoadFp { dest, width, base, offset } = unsafe { (*stream).instr.assume_init().load_fp };

            use FpWidth::*;
            let addr = cpu.read_x(base).wrapping_add_signed(offset.into());

            let v = match width {
                Word => FRegister::write_f32(cpu.load_f32(addr)?),
                Double => FRegister::write_f64(cpu.load_f64(addr)?),
            };

            cpu.write_f(dest, v);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn store_fp<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let StoreFp { offset, width, base, src } = unsafe { (*stream).instr.assume_init().store_fp };
            
            use FpWidth::*;
            let addr = cpu.read_x(base).wrapping_add_signed(offset.into());

            let v = cpu.read_f(src);

            match width {
                Word => cpu.store_f32(addr, v.read_f32())?,
                Double => cpu.store_f64(addr, v.read_f64())?
            }
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let Int { dest, funct, src1, src2 } = unsafe { (*stream).instr.assume_init().int };

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
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_shift_left<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::ImmShift::*;

            let src = cpu.read_x(src);

            let v = match funct {
                ImmShift(ShiftLeft, n) => src.unbounded_shl(u8::from(n).into()),
                _ => unsafe { std::hint::unreachable_unchecked() }
            };

            cpu.write_x(dest, v);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_shift_right_logical<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::ImmShift::*;

            let src = cpu.read_x(src);

            let v = match funct {
                ImmShift(ShiftRightLogical, n) => src.unbounded_shr(u8::from(n).into()),
                _ => unsafe { std::hint::unreachable_unchecked() }
            };

            cpu.write_x(dest, v);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_shift_right_arithmetic<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::ImmShift::*;

            let src = cpu.read_x(src);

            let v = match funct {
                ImmShift(ShiftRightArithmetic, n) => (src as i32).unbounded_shr(u8::from(n).into()) as u32,
                _ => unsafe { std::hint::unreachable_unchecked() }
            };

            cpu.write_x(dest, v);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_add<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::Imm12::*;

            let src = cpu.read_x(src);

            let v = match funct {
                Imm12(Add, n) => (src as i32).wrapping_add(n.into()) as u32,
                _ => unsafe { std::hint::unreachable_unchecked() }
            };

            cpu.write_x(dest, v);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_set_less_than<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::Imm12::*;

            let src = cpu.read_x(src);

            let v = match funct {
                Imm12(SetLessThan, n) => ((src as i32) < i32::from(n)) as u32,
                _ => unsafe { std::hint::unreachable_unchecked() }
            };

            cpu.write_x(dest, v);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_set_less_than_unsigned<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::Imm12::*;

            let src = cpu.read_x(src);

            let v = match funct {
                Imm12(SetLessThanUnsigned, n) => (src < (i32::from(n) as u32)) as u32,
                _ => unsafe { std::hint::unreachable_unchecked() }
            };

            cpu.write_x(dest, v);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_xor<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::Imm12::*;

            let src = cpu.read_x(src);

            let v = match funct {
                Imm12(Xor, n) => src ^ (i32::from(n) as u32),
                _ => unsafe { std::hint::unreachable_unchecked() }
            };

            cpu.write_x(dest, v);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_or<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::Imm12::*;

            let src = cpu.read_x(src);

            let v = match funct {
                Imm12(Or, n) => src | (i32::from(n) as u32),
                _ => unsafe { std::hint::unreachable_unchecked() }
            };

            cpu.write_x(dest, v);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_and<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::Imm12::*;

            let src = cpu.read_x(src);

            let v = match funct {
                Imm12(And, n) => src & (i32::from(n) as u32),
                _ => unsafe { std::hint::unreachable_unchecked() }
            };

            cpu.write_x(dest, v);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }
        
        unsafe fn u<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let U { type_, dest, imm } = unsafe { (*stream).instr.assume_init().u };

            use UType::*;

            let v = match type_ {
                LoadUpperImmediate => u32::from(imm) << 12,
                AddUpperImmediateToPc => cpu.pc.wrapping_sub(SIZE).wrapping_add(u32::from(imm) << 12)
            };

            cpu.write_x(dest, v);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }
        
        unsafe fn fp<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let Fp { rounding_mode, funct, dest, src1, src2 } = unsafe { (*stream).instr.assume_init().fp };

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
                    unsafe { become dispatch(cpu, h, stream.add(1)) }
                },
                MoveToXSingle => {
                    ensure!(src2 == Register::ZERO);
                    cpu.write_x(dest, match rounding_mode {
                        NearestTieToEven => v1.read_f32().to_bits(),
                        Zero => bail!("not yet implemented: classify"),
                        _ => bail!("baDD")
                    });
                    unsafe { become dispatch(cpu, h, stream.add(1)) }
                },
                CompareSingle => {
                    cpu.write_x(dest, match rounding_mode {
                        NearestTieToEven => v1.read_f32() <= v2.read_f32(),
                        Zero => v1.read_f32() < v2.read_f32(),
                        Down => v1.read_f32() == v2.read_f32(),
                        _ => bail!("bAddd")
                    } as u32);
                    unsafe { become dispatch(cpu, h, stream.add(1)) }
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
                    unsafe { become dispatch(cpu, h, stream.add(1)) }
                },
                CompareDouble => {
                    cpu.write_x(dest, match rounding_mode {
                        NearestTieToEven => v1.read_f64() <= v2.read_f64(),
                        Zero => v1.read_f64() < v2.read_f64(),
                        Down => v1.read_f64() == v2.read_f64(),
                        _ => bail!("bAddd")
                    } as u32);
                    unsafe { become dispatch(cpu, h, stream.add(1)) }
                },
                ClassifyDouble => bail!("not yet implemented: classify double"),
                ConvertFromWordDouble => FRegister::write_f64(match src2 {
                    Register::ZERO => cpu.read_x(src1) as i32 as f64,
                    Register::RA => cpu.read_x(src1) as f64,
                    _ => bail!("bADdD")
                })
            };

            cpu.write_f(dest, v);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn fused<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let Fused { type_, width, rounding_mode, dest, src1, src2, src3 } = unsafe { (*stream).instr.assume_init().fused };

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
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }
        
        unsafe fn fence<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let _ = unsafe { (*stream).instr.assume_init().fence };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn amo<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let Amo { dest, src1, src2, release: _, acquire: _, funct } = unsafe { (*stream).instr.assume_init().amo };

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

            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn jump_and_link<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            let JumpAndLink { dest, offset } = unsafe { (*stream).instr.assume_init().jump_and_link };
            
            cpu.write_x(dest, cpu.pc+SIZE);
            cpu.pc = cpu.pc.wrapping_add_signed(offset.into());
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }
        
        unsafe fn jump_and_link_register<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            let JumpAndLinkRegister { dest, base, offset } = unsafe { (*stream).instr.assume_init().jump_and_link_register };
            
            let addr = cpu.read_x(base).wrapping_add_signed(offset.into());
            cpu.write_x(dest, cpu.pc+SIZE);
            cpu.pc = addr;
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn branch<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            let Branch { offset, funct, src1, src2 } = unsafe { (*stream).instr.assume_init().branch };

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
                cpu.pc = cpu.pc.wrapping_add_signed(offset.into());
            } else {
                cpu.pc = cpu.pc.wrapping_add(SIZE);
            }
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn ebreak<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let _ = unsafe { (*stream).instr.assume_init().system };
            h.ebreak(cpu)?;
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn ecall<const SIZE: u32>(cpu: &mut Cpu, h: &mut dyn Hypervisor, stream: *const Op) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let _ = unsafe { (*stream).instr.assume_init().system };
            h.ecall(cpu)?;
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        macro_rules! op_fn {
            ($f:ident, $size:expr) => {
                match $size {
                    2 => $f::<2>,
                    4 => $f::<4>,
                    _ => unreachable!()
                }
            };
        }

        let op_fn = match instr {
            I::LoadInt(_) => op_fn!(load_int, size),
            I::StoreInt(_) => op_fn!(store_int, size),
            I::LoadFp(_) => op_fn!(load_fp, size),
            I::StoreFp(_) => op_fn!(store_fp, size),
            I::Int(_) => op_fn!(int, size),
            I::IntImmediate(IntImmediate { funct, .. }) => {
                use crate::instr::IntImmediateFunct::*;
                use crate::instr::{ImmShift::*, Imm12::*};

                match funct {
                    ImmShift(ShiftLeft, _) => op_fn!(int_immediate_shift_left, size),
                    ImmShift(ShiftRightLogical, _) => op_fn!(int_immediate_shift_right_logical, size),
                    ImmShift(ShiftRightArithmetic, _) => op_fn!(int_immediate_shift_right_arithmetic, size),
                    Imm12(Add, _) => op_fn!(int_immediate_add, size),
                    Imm12(SetLessThan, _) => op_fn!(int_immediate_set_less_than, size),
                    Imm12(SetLessThanUnsigned, _) => op_fn!(int_immediate_set_less_than_unsigned, size),
                    Imm12(Xor, _) => op_fn!(int_immediate_xor, size),
                    Imm12(Or, _) => op_fn!(int_immediate_or, size),
                    Imm12(And, _) => op_fn!(int_immediate_and, size),
                }
            },
            I::U(_) => op_fn!(u, size),
            I::Fp(_) => op_fn!(fp, size),
            I::Fused(_) => op_fn!(fused, size),

            // Fun fake atomics for a single-threaded core
            I::Fence => op_fn!(fence, size),
            I::Amo(_) => op_fn!(amo, size),

            I::JumpAndLink(_) => op_fn!(jump_and_link, size),
            I::JumpAndLinkRegister(_) => op_fn!(jump_and_link_register, size),
            I::Branch(_) => op_fn!(branch, size),
            
            I::System(System::Ebreak) => op_fn!(ebreak, size),
            I::System(System::Ecall) => op_fn!(ecall, size),
            _ => bail!("not yet implemented: {instr:?}")
        };

        Ok(Op { op_fn, instr: MaybeUninit::new(instr.into()) })
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub struct Block([Op]);

impl Block {
    pub fn new(ops: impl IntoIterator<Item = Op>) -> Rc<Self> {
        let stream = ops.into_iter().chain(iter::once(Op {
            op_fn: end,
            instr: MaybeUninit::uninit()
        })).collect::<Rc<[_]>>();

        unsafe { Rc::from_raw(Rc::into_raw(stream) as *const Self) }
    }

    pub fn execute(&self, cpu: &mut Cpu, h: &mut dyn Hypervisor) -> anyhow::Result<()> {
        unsafe { dispatch(cpu, h, self.0.as_ptr()) }
    }
}
