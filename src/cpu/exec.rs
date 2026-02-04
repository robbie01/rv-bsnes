use anyhow::bail;

use crate::instr::{Instruction as I, *};

use super::*;

fn nte(rounding_mode: RoundingMode) {
    if rounding_mode != RoundingMode::NearestTieToEven && rounding_mode != RoundingMode::Dynamic {
        todo!("rounding modes other than NTE");
    }
}

impl<'data, H: Hypervisor<'data> + Debug> Cpu<H> {
    fn execute_one(&mut self) -> anyhow::Result<bool> {
        let pc = self.pc;

        if pc == 0 {
            bail!("tried to execute at address 0 (missing callback?)");
        }

        let (instr, size) = if I::next_is_compressed(self.memory[pc]) {
            let raw = I::decode_compressed(u16::from_le_bytes(self.memory[pc..pc+2].try_into()?))?;
            (raw, 2)
        } else {
            let raw = I::decode(u32::from_le_bytes(self.memory[pc..pc+4].try_into()?))?;
            (raw, 4)
        };

        let mut h = self.hypervisor.take().unwrap();
        h.before_instr(self, instr)?;
        self.hypervisor = Some(h);
        
        self.pc += size;

        let res = match instr {
            I::LoadInt(LoadInt { dest, width, base, offset }) => {
                use LoadWidth::*;
                let addr = self.read_x(base).wrapping_add_signed(offset.into());

                let v = match width {
                    ByteUnsigned => self.load_u8(addr)?.into(),
                    Byte => self.load_i8(addr)? as i32 as u32,
                    HalfUnsigned => self.load_u16(addr)?.into(),
                    Half => self.load_i16(addr)? as i32 as u32,
                    Word => self.load_u32(addr)?
                };

                self.write_x(dest, v);
                Ok(true)
            },
            I::StoreInt(StoreInt { offset, width, base, src }) => {
                use StoreWidth::*;
                let addr = self.read_x(base).wrapping_add_signed(offset.into());

                let v = self.read_x(src);

                match width {
                    Byte => self.store_u8(addr, v as u8)?,
                    Half => self.store_u16(addr, v as u16)?,
                    Word => self.store_u32(addr, v)?
                }
                Ok(true)
            },
            I::LoadFp(LoadFp { dest, width, base, offset }) => {
                use FpWidth::*;
                let addr = self.read_x(base).wrapping_add_signed(offset.into());

                let v = match width {
                    Word => FRegister::write_f32(self.load_f32(addr)?),
                    Double => FRegister::write_f64(self.load_f64(addr)?),
                };

                self.write_f(dest, v);
                Ok(true)
            },
            I::StoreFp(StoreFp { offset, width, base, src }) => {
                use FpWidth::*;
                let addr = self.read_x(base).wrapping_add_signed(offset.into());

                let v = self.read_f(src);

                match width {
                    Word => self.store_f32(addr, v.read_f32())?,
                    Double => self.store_f64(addr, v.read_f64())?
                }
                Ok(true)
            },
            I::Int(Int { dest, funct, src1, src2 }) => {
                use IntegerFunct::*;

                let v1 = self.read_x(src1);
                let v2 = self.read_x(src2);

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

                self.write_x(dest, v);
                Ok(true)
            },
            I::IntImmediate(IntImmediate { dest, funct, src }) => {
                use IntImmediateFunct::*;
                use crate::instr::{ImmShift::*, Imm12::*};

                let src = self.read_x(src);

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

                self.write_x(dest, v);
                Ok(true)
            },
            I::U(U { type_, dest, imm }) => {
                use UType::*;

                let v = match type_ {
                    LoadUpperImmediate => u32::from(imm) << 12,
                    AddUpperImmediateToPc => pc.wrapping_add(u32::from(imm) << 12)
                };

                self.write_x(dest, v);
                Ok(true)
            },
            I::Fp(Fp { rounding_mode, funct, dest, src1, src2 }) => {
                use RoundingMode::*;

                let v1 = self.read_f(src1);
                let v2 = self.read_f(src2);

                use FloatFunct::*;
                let v = match funct {
                    AddSingle => { nte(rounding_mode); FRegister::write_f32(v1.read_f32() + v2.read_f32()) },
                    SubtractSingle => { nte(rounding_mode); FRegister::write_f32(v1.read_f32() - v2.read_f32()) },
                    MultiplySingle => { nte(rounding_mode); FRegister::write_f32(v1.read_f32() * v2.read_f32()) },
                    DivideSingle => { nte(rounding_mode); FRegister::write_f32(v1.read_f32() / v2.read_f32()) },
                    SquareRootSingle => {
                        nte(rounding_mode);
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
                        self.write_x(dest, match src2 {
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
                        return Ok(true)
                    },
                    MoveToXSingle => {
                        ensure!(src2 == Register::ZERO);
                        self.write_x(dest, match rounding_mode {
                            NearestTieToEven => v1.read_f32().to_bits(),
                            Zero => todo!("classify"),
                            _ => bail!("baDD")
                        });
                        return Ok(true)
                    },
                    CompareSingle => {
                        self.write_x(dest, match rounding_mode {
                            NearestTieToEven => v1.read_f32() <= v2.read_f32(),
                            Zero => v1.read_f32() < v2.read_f32(),
                            Down => v1.read_f32() == v2.read_f32(),
                            _ => bail!("bAddd")
                        } as u32);
                        return Ok(true)
                    },
                    ConvertFromWordSingle => FRegister::write_f32(match src2 {
                        Register::ZERO => self.read_x(src1) as i32 as f32,
                        Register::RA => self.read_x(src1) as f32,
                        _ => bail!("bADdD")
                    }),
                    MoveFromXSingle => {
                        ensure!(src2 == Register::ZERO);
                        FRegister::write_f32(f32::from_bits(self.read_x(src1)))
                    },

                    ConvertDoubleToSingle => {
                        ensure!(src2 == Register::RA);
                        nte(rounding_mode);
                        FRegister::write_f32(v1.read_f64() as f32)
                    },
                    ConvertSingleToDouble => {
                        ensure!(src2 == Register::ZERO);
                        FRegister::write_f64(v1.read_f32() as f64)
                    },

                    AddDouble => { nte(rounding_mode); FRegister::write_f64(v1.read_f64() + v2.read_f64()) },
                    SubtractDouble => { nte(rounding_mode); FRegister::write_f64(v1.read_f64() - v2.read_f64()) },
                    MultiplyDouble => { nte(rounding_mode); FRegister::write_f64(v1.read_f64() * v2.read_f64()) },
                    DivideDouble => { nte(rounding_mode); FRegister::write_f64(v1.read_f64() / v2.read_f64()) },
                    SquareRootDouble => {
                        nte(rounding_mode);
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
                            v1.read_f64().max(-v2.read_f64()),
                        NearestTieToEven | Zero if v1.read_f64().is_nan() =>
                            v2.read_f64(),
                        NearestTieToEven | Zero if v2.read_f64().is_nan() =>
                            v1.read_f64(),
                        NearestTieToEven | Zero if v1.read_f64().is_nan() && v2.read_f64().is_nan() =>
                            CANONICAL_NAN_F64,
                        _ => bail!("badddd")
                    }),
                    ConvertToWordDouble => {
                        self.write_x(dest, match src2 {
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
                        return Ok(true)
                    },
                    CompareDouble => {
                        self.write_x(dest, match rounding_mode {
                            NearestTieToEven => v1.read_f64() <= v2.read_f64(),
                            Zero => v1.read_f64() < v2.read_f64(),
                            Down => v1.read_f64() == v2.read_f64(),
                            _ => bail!("bAddd")
                        } as u32);
                        return Ok(true)
                    },
                    ClassifyDouble => todo!("classify double"),
                    ConvertFromWordDouble => FRegister::write_f64(match src2 {
                        Register::ZERO => self.read_x(src1) as i32 as f64,
                        Register::RA => self.read_x(src1) as f64,
                        _ => bail!("bADdD")
                    })
                };

                self.write_f(dest, v);
                Ok(true)
            },
            I::Fused(Fused { type_, width, rounding_mode, dest, src1, src2, src3 }) => {
                use FloatWidth::*;
                use FuseType::*;

                nte(rounding_mode);

                let v1 = self.read_f(src1);
                let v2 = self.read_f(src2);
                let v3 = self.read_f(src3);

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

                self.write_f(dest, v);
                Ok(true)
            },

            // Fun fake atomics for a single-threaded core
            I::Fence => Ok(true),
            I::Amo(Amo { dest, src1, src2, release: _, acquire: _, funct }) => {
                use AmoFunct::*;

                match funct {
                    LoadReserved => {
                        ensure!(src2 == Register::ZERO);
                        let addr = self.read_x(src1);
                        let v = u32::from_le_bytes(self.memory[addr..addr+4].try_into()?);

                        self.write_x(dest, v);
                        Ok(true)
                    },
                    StoreConditional => {
                        let addr = self.read_x(src1);
                        let v = self.read_x(src2);

                        self.memory[addr..addr+4].copy_from_slice(&v.to_le_bytes());
                        self.write_x(dest, 0);
                        Ok(true)
                    },
                    Swap => {
                        let addr = self.read_x(src1);
                        let old = self.load_u32(addr)?;
                        self.store_u32(addr, self.read_x(src2))?;
                        self.write_x(dest, old);
                        Ok(true)
                    }
                    _ => todo!("amo {funct:?}")
                }
            }

            I::JumpAndLink(JumpAndLink { dest, offset }) => {
                self.write_x(dest, self.pc);
                self.pc = pc.wrapping_add_signed(offset.into());
                Ok(false)
            },
            I::JumpAndLinkRegister(JumpAndLinkRegister { dest, base, offset }) => {
                let addr = self.read_x(base).wrapping_add_signed(offset.into());
                self.write_x(dest, self.pc);
                self.pc = addr;
                Ok(false)
            },
            I::Branch(Branch { offset, funct, src1, src2 }) => {
                use BranchType::*;
                
                let v1 = self.read_x(src1);
                let v2 = self.read_x(src2);

                if match funct {
                    Equal => v1 == v2,
                    NotEqual => v1 != v2,
                    LessThan => (v1 as i32) < (v2 as i32),
                    GreaterThanOrEqual => (v1 as i32) >= (v2 as i32),
                    LessThanUnsigned => v1 < v2,
                    GreaterThanOrEqualUnsigned => v1 >= v2
                } {
                    self.pc = pc.wrapping_add_signed(offset.into());
                }
                Ok(false)
            },
            
            I::System(System::Ebreak) => {
                let mut h = self.hypervisor.take().unwrap();
                h.ebreak(self)?;
                self.hypervisor = Some(h);
                Ok(true)
            },
            I::System(System::Ecall) => {
                let mut h = self.hypervisor.take().unwrap();
                h.ecall(self)?;
                self.hypervisor = Some(h);
                Ok(true)
            },
            _ => todo!("{instr:?}")
        };

        if res.is_ok() {
            let mut h = self.hypervisor.take().unwrap();
            h.after_instr(self, instr)?;
            self.hypervisor = Some(h);
        }

        res
    }

    fn continue_execution(&mut self) -> anyhow::Result<()> {
        while self.execute_one()? {}
        Ok(())
    }

    pub fn call_subroutine(&mut self, sub: u32) -> anyhow::Result<()> {
        ensure!(self.pc == u32::MAX);
        self.pc = sub;
        self.write_x(Register::RA, u32::MAX); // sentinel
        while self.pc != u32::MAX {
            self.continue_execution()?;
        }
        Ok(())
    }
}