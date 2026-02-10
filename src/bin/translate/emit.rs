use std::collections::BTreeMap;

use rv::instr::{Instruction as I, *};
use wasm_encoder::{ValType::*, *};

use crate::Termination;

fn emit_read_x<'a, 'b>(instrs: &'a mut InstructionSink<'b>, r: Register) -> &'a mut InstructionSink<'b> {
    match r {
        Register::ZERO => instrs.i32_const(0),
        _ => instrs.local_get(x(r))
    }
}

pub fn emit_epilogue(instrs: &mut InstructionSink<'_>, indices: &BTreeMap<u32, u32>, term: Termination) {
    match term {
        Termination::JumpAndLink { dest, pc, save_pc } => {
            if dest != Register::ZERO {
                instrs.i32_const(save_pc as i32);
                instrs.local_set(x(dest));
            }

            for i in 0..63 {
                instrs.local_get(i);
            }

            instrs.return_call(*indices.get(&pc).unwrap());
        },
        Termination::Branch { src1, src2, funct, pc, else_pc } => {
            for i in 0..63 {
                instrs.local_get(i);
            }

            emit_read_x(instrs, src1);
            emit_read_x(instrs, src2);

            match funct {
                BranchType::Equal => instrs.i32_eq(),
                BranchType::NotEqual => instrs.i32_ne(),
                BranchType::LessThan => instrs.i32_lt_s(),
                BranchType::LessThanUnsigned => instrs.i32_lt_u(),
                BranchType::GreaterThanOrEqual => instrs.i32_ge_s(),
                BranchType::GreaterThanOrEqualUnsigned => instrs.i32_ge_u()
            };

            instrs.if_(BlockType::FunctionType(0));
            instrs
                .return_call(*indices.get(&pc).unwrap())
                .else_();
            instrs
                .return_call(*indices.get(&else_pc).unwrap())
                .end();
        },
        Termination::JumpAndLinkRegister { jalr: JumpAndLinkRegister { dest, base, offset }, save_pc } => {
            if dest != Register::ZERO {
                instrs.i32_const(save_pc as i32);
                instrs.local_set(x(dest));
            }

            for i in 0..63 {
                instrs.local_get(i);
            }

            match base {
                Register::ZERO => instrs.i32_const(0),
                r => instrs.local_get(u32::from(u8::from(r)) - 1)
            };

            instrs
                .i32_const(i16::from(offset).into())
                .i32_add();

            match base {
                Register::ZERO => instrs.i32_const(0),
                r => instrs.local_get(u32::from(u8::from(r)) - 1)
            };

            instrs
                .i32_const(i16::from(offset).into())
                .i32_add();

            let start_addr = *indices.first_key_value().unwrap().0;
            instrs
                .i32_const(0xe0000000u32 as i32)
                .i32_ge_u()
                .if_(BlockType::FunctionType(2))
                .local_set(x(Register::A7))
                .return_()
                .else_()
                .i32_const(start_addr as i32)
                .i32_sub()
                .i32_const(1)
                .i32_shr_u()
                .return_call_indirect(0, 0)
                .end();
        },
        Termination::ReturnToSender => {
            for i in 0..63 {
                instrs.local_get(i);
            }
            instrs.return_();
        }
    }
    instrs.end();
}

fn x(r: Register) -> u32 {
    u32::from(u8::from(r)) - 1
}

fn f(r: Register) -> u32 {
    u32::from(u8::from(r)) + 31
}

const MEMORY: MemArg = MemArg {
    offset: 0,
    align: 0,
    memory_index: 0
};

trait InstructionSinkExt {
    fn f64_box_f32(&mut self) -> &mut Self;
    fn f32_unbox_f64(&mut self, src: Register) -> &mut Self;
}

impl<'a> InstructionSinkExt for InstructionSink<'a> {
    fn f64_box_f32(&mut self) -> &mut Self {
        self
            .i32_reinterpret_f32()
            .i64_extend_i32_u()
            .i64_const(0xFFFFFFFF00000000u64 as i64)
            .i64_or()
            .f64_reinterpret_i64()
    }

    fn f32_unbox_f64(&mut self, src: Register) -> &mut Self {
        self
            .i64_reinterpret_f64()
            .i64_const(32)
            .i64_shr_s()
            .i64_const(-1)
            .i64_eq()
            .if_(BlockType::Result(F32))
            .local_get(f(src))
            .i64_reinterpret_f64()
            .i32_wrap_i64()
            .f32_reinterpret_i32()
            .else_()
            .f32_const(Ieee32::new(0x7fc00000))
            .end()
    }
}

pub fn emit_instruction(instrs: &mut InstructionSink<'_>, pc: u32, instr: I) {
    use FloatFunct::*;

    match instr {
        I::Int(Int { dest, funct: IntegerFunct::Divide, src1, src2 }) => {
            if dest != Register::ZERO {
                emit_read_x(instrs, src2);
                instrs
                    .i32_eqz()
                    .if_(BlockType::Result(I32))
                    .i32_const(-1)
                    .else_();
                emit_read_x(instrs, src1);
                emit_read_x(instrs, src2);
                instrs
                    .i32_div_s()
                    .end()
                    .local_set(x(dest));
            }
        },
        I::Int(Int { dest, funct: IntegerFunct::DivideUnsigned, src1, src2 }) => {
            if dest != Register::ZERO {
                emit_read_x(instrs, src2);
                instrs
                    .i32_eqz()
                    .if_(BlockType::Result(I32))
                    .i32_const(-1)
                    .else_();
                emit_read_x(instrs, src1);
                emit_read_x(instrs, src2);
                instrs
                    .i32_div_u()
                    .end()
                    .local_set(x(dest));
            }
        },
        I::Int(Int { dest, funct: IntegerFunct::Remainder, src1, src2 }) => {
            if dest != Register::ZERO {
                emit_read_x(instrs, src2);
                instrs
                    .i32_eqz()
                    .if_(BlockType::Result(I32));
                emit_read_x(instrs, src1);
                instrs.else_();
                emit_read_x(instrs, src1);
                emit_read_x(instrs, src2);
                instrs
                    .i32_rem_s()
                    .end()
                    .local_set(x(dest));
            }
        },
        I::Int(Int { dest, funct: IntegerFunct::RemainderUnsigned, src1, src2 }) => {
            if dest != Register::ZERO {
                emit_read_x(instrs, src2);
                instrs
                    .i32_eqz()
                    .if_(BlockType::Result(I32));
                emit_read_x(instrs, src1);
                instrs.else_();
                emit_read_x(instrs, src1);
                emit_read_x(instrs, src2);
                instrs
                    .i32_rem_u()
                    .end()
                    .local_set(x(dest));
            }
        },
        I::Int(Int { dest, funct: IntegerFunct::MultiplyHalf, src1, src2 }) => {
            if dest != Register::ZERO {
                emit_read_x(instrs, src1);
                instrs.i64_extend_i32_s();
                emit_read_x(instrs, src2);
                instrs
                    .i64_extend_i32_s()
                    .i64_mul()
                    .i64_const(32)
                    .i64_shr_u()
                    .i32_wrap_i64()
                    .local_set(x(dest));
            }
        },
        I::Int(Int { dest, funct: IntegerFunct::MultiplyHalfUnsigned, src1, src2 }) => {
            if dest != Register::ZERO {
                emit_read_x(instrs, src1);
                instrs.i64_extend_i32_u();
                emit_read_x(instrs, src2);
                instrs
                    .i64_extend_i32_u()
                    .i64_mul()
                    .i64_const(32)
                    .i64_shr_u()
                    .i32_wrap_i64()
                    .local_set(x(dest));
            }
        },
        I::Int(Int { dest, funct: IntegerFunct::MultiplyHalfSignedUnsigned, src1, src2 }) => {
            if dest != Register::ZERO {
                emit_read_x(instrs, src1);
                instrs.i64_extend_i32_s();
                emit_read_x(instrs, src2);
                instrs
                    .i64_extend_i32_u()
                    .i64_mul()
                    .i64_const(32)
                    .i64_shr_u()
                    .i32_wrap_i64()
                    .local_set(x(dest));
            }
        },
        I::Int(Int { dest, funct, src1, src2 }) => {
            if dest != Register::ZERO {
                emit_read_x(instrs, src1);
                emit_read_x(instrs, src2);
                use IntegerFunct::*;
                match funct {
                    Add => instrs.i32_add(),
                    ShiftLeft => instrs.i32_shl(),
                    SetLessThan => instrs.i32_lt_s(),
                    SetLessThanUnsigned => instrs.i32_lt_u(),
                    Xor => instrs.i32_xor(),
                    ShiftRight => instrs.i32_shr_u(),
                    Or => instrs.i32_or(),
                    And => instrs.i32_and(),
                    Subtract => instrs.i32_sub(),
                    ShiftRightArithmetic => instrs.i32_shr_s(),
                    Multiply => instrs.i32_mul(),

                    MultiplyHalf | MultiplyHalfSignedUnsigned | MultiplyHalfUnsigned | Divide | DivideUnsigned | Remainder | RemainderUnsigned => unreachable!()
                };
                instrs.local_set(x(dest));
            }
        },
        I::IntImmediate(IntImmediate { dest, funct, src }) => {
            if dest != Register::ZERO {
                emit_read_x(instrs, src);
                match funct {
                    IntImmediateFunct::Imm12(funct, imm) => {
                        instrs.i32_const(i16::from(imm).into());
                        use Imm12::*;
                        match funct {
                            Add => instrs.i32_add(),
                            SetLessThan => instrs.i32_lt_s(),
                            SetLessThanUnsigned => instrs.i32_lt_u(),
                            Xor => instrs.i32_xor(),
                            Or => instrs.i32_or(),
                            And => instrs.i32_and()
                        };
                    },
                    IntImmediateFunct::ImmShift(funct, imm) => {
                        instrs.i32_const(u8::from(imm).into());
                        use ImmShift::*;
                        match funct {
                            ShiftLeft => instrs.i32_shl(),
                            ShiftRightLogical => instrs.i32_shr_u(),
                            ShiftRightArithmetic => instrs.i32_shr_s()
                        };
                    }
                }
                instrs.local_set(x(dest));
            }
        },
        I::U(U { type_, dest, imm }) => {
            if dest != Register::ZERO {
                let imm = u32::from(imm) << 12;

                match type_ {
                    UType::AddUpperImmediateToPc => instrs.i32_const(pc.wrapping_add(imm) as i32),
                    UType::LoadUpperImmediate => instrs.i32_const(imm as i32)
                };
                instrs.local_set(x(dest));
            }
        },
        I::LoadInt(LoadInt { dest, width, base, offset }) => {
            if dest != Register::ZERO {
                emit_read_x(instrs, base);
                instrs
                    .i32_const(i16::from(offset).into())
                    .i32_add();
                use LoadWidth::*;
                match width {
                    Byte => instrs.i32_load8_s(MEMORY),
                    ByteUnsigned => instrs.i32_load8_u(MEMORY),
                    Half => instrs.i32_load16_s(MEMORY),
                    HalfUnsigned => instrs.i32_load16_u(MEMORY),
                    Word => instrs.i32_load(MEMORY)
                };
                instrs.local_set(x(dest));
            }
        },
        I::StoreInt(StoreInt { offset, width, base, src }) => {
            emit_read_x(instrs, base);
            instrs
                .i32_const(i16::from(offset).into())
                .i32_add();
            emit_read_x(instrs, src);
            use StoreWidth::*;
            match width {
                Byte => instrs.i32_store8(MEMORY),
                Half => instrs.i32_store16(MEMORY),
                Word => instrs.i32_store(MEMORY)
            };
        },
        I::Fence => (),
        I::System(sys) => {
            for i in 0..63 {
                instrs.local_get(i);
            }
            match sys {
                System::Ebreak => instrs.call(0),
                System::Ecall => instrs.call(1),
                System::Csr(_csr) => {
                    eprintln!("warning: csrs are not implemented @{pc:X} {_csr:?}");
                    instrs.unreachable()
                }
            };
            for i in (0..63).rev() {
                instrs.local_set(i);
            }
        },
        I::LoadFp(LoadFp { dest, width, base, offset }) => {
            emit_read_x(instrs, base);
            instrs
                .i32_const(i16::from(offset).into())
                .i32_add();

            match width {
                FpWidth::Double => { instrs.f64_load(MEMORY); }
                FpWidth::Word => {
                    instrs
                        .i32_load(MEMORY)
                        .i64_extend_i32_u()
                        .i64_const(0xFFFFFFFF00000000u64 as i64)
                        .i64_or()
                        .f64_reinterpret_i64();
                }
            }

            instrs.local_set(f(dest));
        },
        I::StoreFp(StoreFp { offset, width, base, src }) => {
            emit_read_x(instrs, base);
            instrs
                .i32_const(i16::from(offset).into())
                .i32_add()
                .local_get(f(src));

            match width {
                FpWidth::Double => instrs.f64_store(MEMORY),
                FpWidth::Word => instrs
                    .f32_unbox_f64(src)
                    .f32_store(MEMORY)
            };
        },
        I::Fp(Fp { rounding_mode, funct: funct@(AddDouble | SubtractDouble | MultiplyDouble | DivideDouble | InjectSignDouble | MinMaxDouble), dest, src1, src2 }) => {
            instrs
                .local_get(f(src1))
                .local_get(f(src2));
            
            match funct {
                AddDouble => instrs.f64_add(),
                SubtractDouble => instrs.f64_sub(),
                MultiplyDouble => instrs.f64_mul(),
                DivideDouble => instrs.f64_div(),
                InjectSignDouble => match rounding_mode {
                    RoundingMode::NearestTieToEven => instrs.f64_copysign(),
                    RoundingMode::Zero => instrs.f64_neg().f64_copysign(),
                    RoundingMode::Down => if src1 == src2 {
                        instrs.drop().f64_abs()
                    } else {
                        todo!("FSGNJX")
                    },
                    _ => unimplemented!()
                },
                MinMaxDouble => match rounding_mode {
                    RoundingMode::NearestTieToEven => instrs.f64_min(),
                    RoundingMode::Zero => instrs.f64_max(),
                    _ => unimplemented!()
                },
                _ => unreachable!()
            };

            instrs.local_set(f(dest));
        },
        I::Fp(Fp { rounding_mode: _, funct: funct@(SquareRootDouble | SquareRootSingle), dest, src1, src2 }) => {
            if src2 != Register::ZERO {
                unimplemented!()
            }

            instrs.local_get(f(src1));
            
            match funct {
                SquareRootDouble => instrs.f64_sqrt(),
                SquareRootSingle => todo!(),
                _ => unreachable!()
            };

            instrs.local_set(f(dest));
        },
        I::Fp(Fp { rounding_mode, funct: funct@(AddSingle | SubtractSingle | MultiplySingle | DivideSingle | InjectSignSingle | MinMaxSingle), dest, src1, src2 }) => {
            instrs
                .local_get(f(src1))
                .f32_unbox_f64(src1)
                .local_get(f(src2))
                .f32_unbox_f64(src2);
            
            match funct {
                AddSingle => instrs.f32_add(),
                SubtractSingle => instrs.f32_sub(),
                MultiplySingle => instrs.f32_mul(),
                DivideSingle => instrs.f32_div(),
                InjectSignSingle => match rounding_mode {
                    RoundingMode::NearestTieToEven => instrs.f32_copysign(),
                    RoundingMode::Zero => instrs.f32_neg().f32_copysign(),
                    RoundingMode::Down => if src1 == src2 {
                        instrs.drop().f32_abs()
                    } else {
                        todo!("FSGNJX")
                    },
                    _ => unimplemented!()
                },
                MinMaxSingle => match rounding_mode {
                    RoundingMode::NearestTieToEven => instrs.f32_min(),
                    RoundingMode::Zero => instrs.f32_max(),
                    _ => unimplemented!()
                },
                _ => unreachable!()
            };

            instrs
                .f64_box_f32()
                .local_set(f(dest));
        },
        I::Fp(Fp { rounding_mode: _, funct: funct@(ConvertFromWordSingle | ConvertFromWordDouble | MoveFromXSingle), dest, src1, src2 }) => {
            // TODO: investigate rounding mode consequences

            emit_read_x(instrs, src1);

            match funct {
                ConvertFromWordSingle => match src2 {
                    Register::ZERO => instrs.f32_convert_i32_s(),
                    Register::RA => instrs.f32_convert_i32_u(),
                    _ => unimplemented!()
                }.f64_box_f32(),
                ConvertFromWordDouble => match src2 {
                    Register::ZERO => instrs.f64_convert_i32_s(),
                    Register::RA => instrs.f64_convert_i32_u(),
                    _ => unimplemented!()
                },
                MoveFromXSingle => {
                    if src2 != Register::ZERO {
                        unimplemented!()
                    }

                    instrs
                        .f32_reinterpret_i32()
                        .f64_box_f32()
                }
                _ => unreachable!()
            };

            instrs.local_set(f(dest));
        },
        I::Fp(Fp { rounding_mode, funct: funct@(CompareSingle | CompareDouble), dest, src1, src2 }) => {
            if dest != Register::ZERO {
                match funct {
                    CompareDouble => {
                        instrs
                            .local_get(f(src1))
                            .local_get(f(src2));

                        match rounding_mode {
                            RoundingMode::NearestTieToEven => instrs.f64_eq(),
                            RoundingMode::Zero => instrs.f64_lt(),
                            RoundingMode::Down => instrs.f64_le(),
                            _ => unimplemented!()
                        }
                    },
                    CompareSingle => {
                        instrs
                            .local_get(f(src1))
                            .f32_unbox_f64(src1)
                            .local_get(f(src2))
                            .f32_unbox_f64(src2);

                        match rounding_mode {
                            RoundingMode::NearestTieToEven => instrs.f32_eq(),
                            RoundingMode::Zero => instrs.f32_lt(),
                            RoundingMode::Down => instrs.f32_le(),
                            _ => unimplemented!()
                        }
                    },
                    _ => unreachable!()
                };

                instrs.local_set(x(dest));
            }
        },
        I::Fp(Fp { rounding_mode, funct: funct@(ConvertToWordSingle | ConvertToWordDouble | MoveToXSingle), dest, src1, src2 }) => {
            if dest != Register::ZERO {
                instrs.local_get(f(src1));

                match funct {
                    ConvertToWordDouble => match src2 {
                        Register::ZERO => instrs.i32_trunc_sat_f64_s(),
                        Register::RA => instrs.i32_trunc_sat_f64_u(),
                        _ => unimplemented!()
                    },
                    ConvertToWordSingle => {
                        instrs.f32_unbox_f64(src1);

                        match src2 {
                            Register::ZERO => instrs.i32_trunc_sat_f32_s(),
                            Register::RA => instrs.i32_trunc_sat_f32_u(),
                            _ => unimplemented!()
                        }
                    },
                    MoveToXSingle => {
                        if src2 != Register::ZERO {
                            unimplemented!()
                        }

                        match rounding_mode {
                            RoundingMode::NearestTieToEven => instrs
                                .f32_unbox_f64(src1)
                                .i32_reinterpret_f32(),
                            RoundingMode::Zero => todo!(),
                            _ => unimplemented!()
                        }
                    },
                    _ => unreachable!()
                };

                instrs.local_set(x(dest));
            }
        },
        I::Fp(Fp { rounding_mode: _, funct: ConvertDoubleToSingle, dest, src1, src2 }) => {
            if src2 != Register::RA {
                unimplemented!()
            }
            
            instrs
                .local_get(f(src1))
                .f32_demote_f64()
                .f64_box_f32()
                .local_set(f(dest));
        },
        I::Fp(Fp { rounding_mode: _, funct: ConvertSingleToDouble, dest, src1, src2 }) => {
            if src2 != Register::ZERO {
                unimplemented!()
            }
            
            instrs
                .local_get(f(src1))
                .f32_unbox_f64(src1)
                .f64_promote_f32()
                .local_set(f(dest));
        },
        I::Fused(Fused { type_, width, rounding_mode: _, dest, src1, src2, src3 }) => {
            match width {
                FloatWidth::Double => {
                    instrs
                        .local_get(f(src1))
                        .local_get(f(src2))
                        .f64_mul();

                    if type_ == FuseType::NegatedMultiplySubtract || type_ == FuseType::NegatedMultiplyAdd {
                        instrs.f64_neg();
                    }

                    instrs.local_get(f(src3));
                    
                    match type_ {
                        FuseType::MultiplyAdd | FuseType::NegatedMultiplySubtract => instrs.f64_add(),
                        FuseType::MultiplySubtract | FuseType::NegatedMultiplyAdd => instrs.f64_sub()
                    };

                    instrs.local_set(f(dest));
                },
                FloatWidth::Single => todo!()
            }
        },
        I::Amo(Amo { dest, src1, src2, release: _, acquire: _, funct: AmoFunct::LoadReserved }) => {
            if src2 != Register::ZERO {
                unimplemented!()
            }

            if dest != Register::ZERO {
                emit_read_x(instrs, src1);
                instrs
                    .i32_load(MEMORY)
                    .local_set(x(dest));
            }
        },
        I::Amo(Amo { dest, src1, src2, release: _, acquire: _, funct: AmoFunct::StoreConditional }) => {
            emit_read_x(instrs, src1);
            emit_read_x(instrs, src2);
            instrs.i32_store(MEMORY);
            if dest != Register::ZERO {
                instrs
                    .i32_const(0)
                    .local_set(x(dest));
            }
        },
        I::Amo(Amo { dest, src1, src2, release: _, acquire: _, funct: AmoFunct::Swap }) => {
            if dest != Register::ZERO {
                emit_read_x(instrs, src1);
                instrs.i32_load(MEMORY);
            }

            emit_read_x(instrs, src1);
            emit_read_x(instrs, src2);
            instrs.i32_store(MEMORY);

            if dest != Register::ZERO {
                instrs.local_set(x(dest));
            }
        },
        I::Amo(Amo { dest, src1, src2, release: _, acquire: _, funct }) => {
            if dest != Register::ZERO {
                emit_read_x(instrs, src1);
                instrs.i32_load(MEMORY);
            }
            emit_read_x(instrs, src1);
            emit_read_x(instrs, src1);
            instrs.i32_load(MEMORY);
            emit_read_x(instrs, src2);

            use AmoFunct::*;
            match funct {
                Add => instrs.i32_add(),
                And => instrs.i32_and(),
                Or => instrs.i32_or(),
                Xor => instrs.i32_xor(),
                Max | MaxUnsigned | Min | MinUnsigned | Swap => todo!(),
                LoadReserved | StoreConditional => unreachable!()
            };

            instrs.i32_store(MEMORY);
            if dest != Register::ZERO {
                instrs.local_set(x(dest));
            }
        },
        _ => todo!("{instr:?}")
    }
}