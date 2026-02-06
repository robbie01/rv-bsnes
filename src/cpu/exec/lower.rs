#![allow(unused_assignments)]

use std::{iter, mem::MaybeUninit};

use anyhow::{bail, ensure};

use crate::{cpu::*, instr::{Instruction as I, *}};

pub type OpFn<H> = unsafe fn(&mut Cpu<H>, &mut H, *const Op<H>) -> anyhow::Result<()>;

#[derive(Debug)]
pub struct Op<H: ?Sized> {
    op_fn: OpFn<H>,
    instr: MaybeUninit<InstructionUnion>
}

impl<H: ?Sized> Clone for Op<H> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<H: ?Sized> Copy for Op<H> {}

#[inline(always)]
fn nte(rounding_mode: RoundingMode) -> anyhow::Result<()> {
    #[cold]
    #[inline(never)]
    fn nte_cold(_: RoundingMode) -> anyhow::Result<()> {
        bail!("not yet implemented: rounding modes other than NTE");
    }

    if rounding_mode != RoundingMode::NearestTieToEven && rounding_mode != RoundingMode::Dynamic {
        become nte_cold(rounding_mode)
    }
    Ok(())
}

#[inline(always)]
unsafe fn dispatch<H: Hypervisor + ?Sized>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
    unsafe { become ((*stream).op_fn)(cpu, h, stream) }
}

unsafe fn end<H: ?Sized>(_: &mut Cpu<H>, _: &mut H, _: *const Op<H>) -> anyhow::Result<()> {
    Ok(())
}

impl<H: Hypervisor + ?Sized> Op<H> {
    pub fn new(instr: Instruction, size: u8) -> anyhow::Result<Self> {
        unsafe fn nop<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn load_int<H: Hypervisor + ?Sized, const SIZE: u32, const WIDTH: LoadWidth>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let LoadInt { dest, width: _, base, offset } = unsafe { (*stream).instr.assume_init().load_int };

            use LoadWidth::*;
            let addr = cpu.read_x(base).wrapping_add_signed(offset.into());

            let v = match WIDTH {
                ByteUnsigned => cpu.load_u8(addr)?.into(),
                Byte => cpu.load_i8(addr)? as i32 as u32,
                HalfUnsigned => cpu.load_u16(addr)?.into(),
                Half => cpu.load_i16(addr)? as i32 as u32,
                Word => cpu.load_u32(addr)?
            };

            unsafe { cpu.write_x_unchecked(dest, v) };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn store_int<H: Hypervisor + ?Sized, const SIZE: u32, const WIDTH: StoreWidth>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let StoreInt { offset, width: _, base, src } = unsafe { (*stream).instr.assume_init().store_int };

            use StoreWidth::*;
            let addr = cpu.read_x(base).wrapping_add_signed(offset.into());

            let v = cpu.read_x(src);

            match WIDTH {
                Byte => cpu.store_u8(addr, v as u8)?,
                Half => cpu.store_u16(addr, v as u16)?,
                Word => cpu.store_u32(addr, v)?
            }
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn load_fp<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
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

        unsafe fn store_fp<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
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

        unsafe fn int<H: Hypervisor + ?Sized, const SIZE: u32, const FUNCT: IntegerFunct>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let Int { dest, funct: _, src1, src2 } = unsafe { (*stream).instr.assume_init().int };

            use IntegerFunct::*;

            let v1 = cpu.read_x(src1);
            let v2 = cpu.read_x(src2);

            let v = match FUNCT {
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

            unsafe { cpu.write_x_unchecked(dest, v) };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_shift_left<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::ImmShift::*;

            let src = cpu.read_x(src);

            let ImmShift(ShiftLeft, n) = funct else { unsafe { std::hint::unreachable_unchecked() } };
            let v = src.unbounded_shl(u8::from(n).into());

            unsafe { cpu.write_x_unchecked(dest, v) };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_shift_right_logical<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::ImmShift::*;

            let src = cpu.read_x(src);

            let ImmShift(ShiftRightLogical, n) = funct else { unsafe { std::hint::unreachable_unchecked() } };
            let v = src.unbounded_shr(u8::from(n).into());

            unsafe { cpu.write_x_unchecked(dest, v) };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_shift_right_arithmetic<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::ImmShift::*;

            let src = cpu.read_x(src);

            let ImmShift(ShiftRightArithmetic, n) = funct else { unsafe { std::hint::unreachable_unchecked() } };
            let v = (src as i32).unbounded_shr(u8::from(n).into()) as u32;

            unsafe { cpu.write_x_unchecked(dest, v) };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_add<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::Imm12::*;

            let src = cpu.read_x(src);

            let Imm12(Add, n) = funct else { unsafe { std::hint::unreachable_unchecked() } };
            let v = (src as i32).wrapping_add(n.into()) as u32;

            unsafe { cpu.write_x_unchecked(dest, v) };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_set_less_than<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::Imm12::*;

            let src = cpu.read_x(src);

            let Imm12(SetLessThan, n) = funct else { unsafe { std::hint::unreachable_unchecked() } };
            let v = ((src as i32) < i32::from(n)) as u32;

            unsafe { cpu.write_x_unchecked(dest, v) };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_set_less_than_unsigned<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::Imm12::*;

            let src = cpu.read_x(src);

            let Imm12(SetLessThanUnsigned, n) = funct else { unsafe { std::hint::unreachable_unchecked() } };
            let v = (src < (i32::from(n) as u32)) as u32;

            unsafe { cpu.write_x_unchecked(dest, v) };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_xor<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::Imm12::*;

            let src = cpu.read_x(src);

            let Imm12(Xor, n) = funct else { unsafe { std::hint::unreachable_unchecked() } };
            let v = src ^ (i32::from(n) as u32);

            unsafe { cpu.write_x_unchecked(dest, v) };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_or<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::Imm12::*;

            let src = cpu.read_x(src);

            let Imm12(Or, n) = funct else { unsafe { std::hint::unreachable_unchecked() } };
            let v = src | (i32::from(n) as u32);

            unsafe { cpu.write_x_unchecked(dest, v) };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn int_immediate_and<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let IntImmediate { dest, funct, src } = unsafe { (*stream).instr.assume_init().int_immediate };

            use IntImmediateFunct::*;
            use crate::instr::Imm12::*;

            let src = cpu.read_x(src);

            let Imm12(And, n) = funct else { unsafe { std::hint::unreachable_unchecked() } };
            let v = src & (i32::from(n) as u32);

            unsafe { cpu.write_x_unchecked(dest, v) };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }
        
        unsafe fn u<H: Hypervisor + ?Sized, const SIZE: u32, const TYPE: UType>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let U { type_: _, dest, imm } = unsafe { (*stream).instr.assume_init().u };

            use UType::*;

            let v = match TYPE {
                LoadUpperImmediate => u32::from(imm) << 12,
                AddUpperImmediateToPc => cpu.pc.wrapping_sub(SIZE).wrapping_add(u32::from(imm) << 12)
            };

            unsafe { cpu.write_x_unchecked(dest, v) };
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }
        
        unsafe fn fp<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
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

        unsafe fn fused<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
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

        unsafe fn amo<H: Hypervisor + ?Sized, const SIZE: u32, const FUNCT: AmoFunct>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let Amo { dest, src1, src2, release: _, acquire: _, funct: _ } = unsafe { (*stream).instr.assume_init().amo };

            use AmoFunct::*;

            match FUNCT {
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
                _ => bail!("not yet implemented: amo {FUNCT:?}")
            }

            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn jump_and_link<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            let JumpAndLink { dest, offset } = unsafe { (*stream).instr.assume_init().jump_and_link };
            
            cpu.write_x(dest, cpu.pc+SIZE);
            cpu.pc = cpu.pc.wrapping_add_signed(offset.into());
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }
        
        unsafe fn jump_and_link_register<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            let JumpAndLinkRegister { dest, base, offset } = unsafe { (*stream).instr.assume_init().jump_and_link_register };
            
            let addr = cpu.read_x(base).wrapping_add_signed(offset.into());
            cpu.write_x(dest, cpu.pc+SIZE);
            cpu.pc = addr;
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn branch<H: Hypervisor + ?Sized, const SIZE: u32, const FUNCT: BranchType>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            let Branch { offset, funct: _, src1, src2 } = unsafe { (*stream).instr.assume_init().branch };

            use BranchType::*;
            
            let v1 = cpu.read_x(src1);
            let v2 = cpu.read_x(src2);

            if match FUNCT {
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

        unsafe fn ebreak<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let _ = unsafe { (*stream).instr.assume_init().system };
            h.ebreak(cpu)?;
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        unsafe fn ecall<H: Hypervisor + ?Sized, const SIZE: u32>(cpu: &mut Cpu<H>, h: &mut H, stream: *const Op<H>) -> anyhow::Result<()> {
            cpu.pc = cpu.pc.wrapping_add(SIZE);
            let _ = unsafe { (*stream).instr.assume_init().system };
            h.ecall(cpu)?;
            unsafe { become dispatch(cpu, h, stream.add(1)) }
        }

        macro_rules! op_fn {
            ($f:ident::<$size:ident>) => {
                match $size {
                    2 => $f::<H, 2>,
                    4 => $f::<H, 4>,
                    _ => unimplemented!()
                }
            };
            ($f:ident::<$size:ident, $funct:path>) => {
                match $size {
                    2 => $f::<H, 2, { $funct }>,
                    4 => $f::<H, 4, { $funct }>,
                    _ => unimplemented!()
                }
            };
            ($f:ident::<$size:ident, $funct:ident>, $($variant:path),+) => {
                match ($size, $funct) {
                    $(
                        (2, $variant) => $f::<H, 2, { $variant }>,
                        (4, $variant) => $f::<H, 4, { $variant }>
                    ),+ ,
                    _ => unimplemented!()
                }
            };
        }

        macro_rules! op_fn_only_4 {
            ($f:ident::<$size:ident>) => {
                match $size {
                    4 => $f::<H, 4>,
                    _ => unimplemented!()
                }
            };
            ($f:ident::<$size:ident, $funct:path>) => {
                match $size {
                    4 => $f::<H, 4, { $funct }>,
                    _ => unimplemented!()
                }
            };
            ($f:ident::<$size:ident, $funct:ident>, $($variant:path),+) => {
                match ($size, $funct) {
                    $(
                        (4, $variant) => $f::<H, 4, { $variant }>
                    ),+ ,
                    _ => unimplemented!()
                }
            };
        }

        let op_fn = match instr {
            I::LoadInt(LoadInt { dest: Register::ZERO, .. }) |
            I::Int(Int { dest: Register::ZERO, .. }) |
            I::IntImmediate(IntImmediate { dest: Register::ZERO, .. }) |
            I::U(U { dest: Register::ZERO, .. }) |
            I::Fence => op_fn!(nop::<size>),

            I::LoadInt(LoadInt { width, .. }) => op_fn!(load_int::<size, width>,
                LoadWidth::Byte,
                LoadWidth::ByteUnsigned,
                LoadWidth::Half,
                LoadWidth::HalfUnsigned,
                LoadWidth::Word
            ),
            I::StoreInt(StoreInt { width, .. }) => op_fn!(store_int::<size, width>,
                StoreWidth::Byte,
                StoreWidth::Half,
                StoreWidth::Word
            ),
            I::LoadFp(_) => op_fn!(load_fp::<size>),
            I::StoreFp(_) => op_fn!(store_fp::<size>),
            I::Int(Int { funct, .. }) => match funct {
                IntegerFunct::Add | IntegerFunct::Xor | IntegerFunct::Or | IntegerFunct::And | IntegerFunct::Subtract => op_fn!(int::<size, funct>,
                    IntegerFunct::Add,
                    IntegerFunct::Xor,
                    IntegerFunct::Or,
                    IntegerFunct::And,
                    IntegerFunct::Subtract
                ),
                _ => op_fn_only_4!(int::<size, funct>,
                    IntegerFunct::ShiftLeft,
                    IntegerFunct::SetLessThan,
                    IntegerFunct::SetLessThanUnsigned,
                    IntegerFunct::ShiftRight,
                    IntegerFunct::ShiftRightArithmetic,
                    IntegerFunct::Multiply,
                    IntegerFunct::MultiplyHalf,
                    IntegerFunct::MultiplyHalfSignedUnsigned,
                    IntegerFunct::MultiplyHalfUnsigned,
                    IntegerFunct::Divide,
                    IntegerFunct::DivideUnsigned,
                    IntegerFunct::Remainder,
                    IntegerFunct::RemainderUnsigned
                )
            },
            I::IntImmediate(IntImmediate { funct, .. }) => {
                use crate::instr::IntImmediateFunct::*;
                use crate::instr::{ImmShift::*, Imm12::*};

                match funct {
                    ImmShift(ShiftLeft, _) => op_fn!(int_immediate_shift_left::<size>),
                    ImmShift(ShiftRightLogical, _) => op_fn!(int_immediate_shift_right_logical::<size>),
                    ImmShift(ShiftRightArithmetic, _) => op_fn!(int_immediate_shift_right_arithmetic::<size>),
                    Imm12(Add, _) => op_fn!(int_immediate_add::<size>),
                    Imm12(SetLessThan, _) => op_fn_only_4!(int_immediate_set_less_than::<size>),
                    Imm12(SetLessThanUnsigned, _) => op_fn_only_4!(int_immediate_set_less_than_unsigned::<size>),
                    Imm12(Xor, _) => op_fn_only_4!(int_immediate_xor::<size>),
                    Imm12(Or, _) => op_fn_only_4!(int_immediate_or::<size>),
                    Imm12(And, _) => op_fn!(int_immediate_and::<size>),
                }
            },
            I::U(U { type_, .. }) => match type_ {
                UType::AddUpperImmediateToPc => op_fn_only_4!(u::<size, UType::AddUpperImmediateToPc>),
                UType::LoadUpperImmediate => op_fn!(u::<size, UType::LoadUpperImmediate>),
            },
            I::Fp(_) => op_fn_only_4!(fp::<size>),
            I::Fused(_) => op_fn_only_4!(fused::<size>),

            // Fun fake atomics for a single-threaded core
            I::Amo(Amo { funct, .. }) => op_fn_only_4!(amo::<size, funct>,
                AmoFunct::Swap,
                AmoFunct::LoadReserved,
                AmoFunct::StoreConditional
            ),

            I::JumpAndLink(_) => op_fn!(jump_and_link::<size>),
            I::JumpAndLinkRegister(_) => op_fn!(jump_and_link_register::<size>),
            I::Branch(Branch { funct, .. }) => match funct {
                BranchType::Equal | BranchType::NotEqual => op_fn!(branch::<size, funct>,
                    BranchType::Equal,
                    BranchType::NotEqual
                ),
                BranchType::LessThan |
                BranchType::GreaterThanOrEqual |
                BranchType::LessThanUnsigned |
                BranchType::GreaterThanOrEqualUnsigned => op_fn_only_4!(branch::<size, funct>,
                    BranchType::LessThan,
                    BranchType::GreaterThanOrEqual,
                    BranchType::LessThanUnsigned,
                    BranchType::GreaterThanOrEqualUnsigned
                )
            },
            
            I::System(System::Ebreak) => op_fn!(ebreak::<size>),
            I::System(System::Ecall) => op_fn_only_4!(ecall::<size>),
            _ => bail!("not yet implemented: {instr:?}")
        };

        Ok(Op { op_fn, instr: MaybeUninit::new(instr.into()) })
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub struct Block<H: ?Sized>([Op<H>]);

impl<H: ?Sized> Block<H> {
    pub fn new(ops: impl IntoIterator<Item = Op<H>>) -> Rc<Self> {
        let stream = ops.into_iter().chain(iter::once(Op {
            op_fn: end,
            instr: MaybeUninit::uninit()
        })).collect::<Rc<[_]>>();

        unsafe { Rc::from_raw(Rc::into_raw(stream) as *const Self) }
    }

    pub fn execute(&self, cpu: &mut Cpu<H>, h: &mut H) -> anyhow::Result<()> where H: Hypervisor {
        unsafe { dispatch(cpu, h, self.0.as_ptr()) }
    }
}
