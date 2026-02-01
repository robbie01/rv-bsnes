mod exec;
mod memory;
pub mod linux;

use std::fmt::Debug;

use anyhow::{Context, ensure};
use object::{LittleEndian, read::elf::ElfFile32};

use crate::instr::{Instruction, Register};
use memory::Memory;

const CANONICAL_NAN_F32: f32 = f32::from_bits(0x7fc00000);
const CANONICAL_NAN_F64: f64 = f64::from_bits(0x7ff8000000000000);

#[derive(Clone, Copy)]
pub struct FRegister {
    value: f64
}

impl FRegister {
    #[inline(always)]
    pub const fn read_f64(self) -> f64 {
        self.value
    }

    #[inline(always)]
    pub const fn read_f32(self) -> f32 {
        let bits = self.value.to_bits();
        let box_ = (bits >> 32) as u32;
        if box_ == u32::MAX {
            f32::from_bits(bits as u32)
        } else {
            CANONICAL_NAN_F32
        }
    }

    #[inline(always)]
    pub const fn write_f64(value: f64) -> Self {
        Self { value }
    }

    #[inline(always)]
    pub const fn write_f32(value: f32) -> Self {
        Self {
            value: f64::from_bits((u64::MAX << 32) | (value.to_bits() as u64))
        }
    }
}

impl Debug for FRegister {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{{32}} | {}{{64}}", self.read_f32(), self.read_f64())
    }
}

pub trait Hypervisor: Sized {
    fn load(&mut self, ctx: &mut Cpu<Self>, obj: &ElfFile32<LittleEndian>) -> anyhow::Result<()>;
    fn before_instr(&mut self, ctx: &mut Cpu<Self>, instr: Instruction) -> anyhow::Result<()> where Self: Sized;

    fn ebreak(&mut self, ctx: &mut Cpu<Self>) -> anyhow::Result<()>;
    fn ecall(&mut self, ctx: &mut Cpu<Self>) -> anyhow::Result<()>;
}

#[derive(Clone, Debug)]
pub struct Cpu<H> {
    pc: u32,
    pub x: [u32; 31],
    pub f: [FRegister; 32],
    pub memory: Memory,

    hypervisor: Option<H>
}

impl<H> Cpu<H> {
    pub fn new(hypervisor: H) -> Self {
        let mut this = Self {
            pc: u32::MAX,
            x: [0; 31],
            f: [FRegister::write_f64(0.); 32],
            memory: Memory::new(0x10000000),

            hypervisor: Some(hypervisor)
        };

        // initialize stack pointer (todo make this better LoL)
        this.write_x(Register::SP, memory::BEGINNING_STACK_TOP);

        this.write_x(Register::TP, memory::TP);

        this
    }

    #[inline(always)]
    pub fn read_x(&self, x: Register) -> u32 {
        if x == Register::ZERO {
            0
        } else {
            self.x[usize::from(x) - 1]
        }
    }

    #[inline(always)]
    pub fn write_x(&mut self, x: Register, v: u32) {
        if x != Register::ZERO {
            self.x[usize::from(x) - 1] = v;
        }
    }

    #[inline(always)]
    pub fn read_f(&mut self, f: Register) -> FRegister {
        self.f[usize::from(f)]
    }

    #[inline(always)]
    pub fn write_f(&mut self, f: Register, v: FRegister) {
        self.f[usize::from(f)] = v;
    }

    pub fn ingest(&mut self, elf: &ElfFile32<LittleEndian>) -> anyhow::Result<()> where H: Hypervisor {
        let mut h = self.hypervisor.take().unwrap();
        h.load(self, elf)?;
        self.hypervisor = Some(h);
        Ok(())
    }
}