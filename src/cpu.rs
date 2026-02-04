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

pub trait Hypervisor<'data>: Sized {
    fn load<'this>(&'this mut self, ctx: &mut Cpu<Self>, obj: &'data ElfFile32<'data, LittleEndian>) -> anyhow::Result<()> where 'data: 'this;
    fn before_instr(&mut self, ctx: &mut Cpu<Self>, instr: Instruction) -> anyhow::Result<()>;
    fn after_instr(&mut self, ctx: &mut Cpu<Self>, instr: Instruction) -> anyhow::Result<()>;

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

    pub fn load<'data>(&mut self, elf: &'data ElfFile32<'data, LittleEndian>) -> anyhow::Result<()> where H: Hypervisor<'data> {
        let mut h = self.hypervisor.take().unwrap();
        h.load(self, elf)?;
        self.hypervisor = Some(h);
        Ok(())
    }
}

// Load/store helpers
impl<H> Cpu<H> {
    #[inline(always)]
    pub fn load_u32(&self, addr: u32) -> anyhow::Result<u32> {
        let addr = usize::try_from(addr)?;

        Ok(u32::from_le_bytes(
            self.memory.get(addr..addr+4)
            .with_context(|| format!("oob load @ {addr:06X} (next pc = {:X})", self.pc))?
            .try_into()?
        ))
    }

    #[inline(always)]
    pub fn load_u16(&self, addr: u32) -> anyhow::Result<u16> {
        let addr = usize::try_from(addr)?;

        Ok(u16::from_le_bytes(
            self.memory.get(addr..addr+2)
            .with_context(|| format!("oob load @ {addr:06X} (next pc = {:X})", self.pc))?
            .try_into()?
        ))
    }

    #[inline(always)]
    pub fn load_i16(&self, addr: u32) -> anyhow::Result<i16> {
        let addr = usize::try_from(addr)?;

        Ok(i16::from_le_bytes(
            self.memory.get(addr..addr+2)
            .with_context(|| format!("oob load @ {addr:06X} (next pc = {:X})", self.pc))?
            .try_into()?
        ))
    }

    #[inline(always)]
    pub fn load_u8(&self, addr: u32) -> anyhow::Result<u8> {
        let addr = usize::try_from(addr)?;

        Ok(
            self.memory.get(addr..addr+1)
            .with_context(|| format!("oob load @ {addr:06X} (next pc = {:X})", self.pc))?
            [0]
        )
    }

    #[inline(always)]
    pub fn load_i8(&self, addr: u32) -> anyhow::Result<i8> {
        let addr = usize::try_from(addr)?;

        Ok(
            self.memory.get(addr..addr+1)
            .with_context(|| format!("oob load @ {addr:06X} (next pc = {:X})", self.pc))?
            [0] as i8
        )
    }

    #[inline(always)]
    pub fn load_f32(&self, addr: u32) -> anyhow::Result<f32> {
        let addr = usize::try_from(addr)?;

        Ok(f32::from_le_bytes(
            self.memory.get(addr..addr+4)
            .with_context(|| format!("oob load @ {addr:06X} (next pc = {:X})", self.pc))?
            .try_into()?
        ))
    }

    #[inline(always)]
    pub fn load_f64(&self, addr: u32) -> anyhow::Result<f64> {
        let addr = usize::try_from(addr)?;

        Ok(f64::from_le_bytes(
            self.memory.get(addr..addr+8)
            .with_context(|| format!("oob load @ {addr:06X} (next pc = {:X})", self.pc))?
            .try_into()?
        ))
    }

    #[inline(always)]
    pub fn store_u32(&mut self, addr: u32, value: u32) -> anyhow::Result<()> {
        let addr = usize::try_from(addr)?;

        self.memory.get_mut(addr..addr+4)
            .with_context(|| format!("oob store @ {addr:06X} (next pc = {:X})", self.pc))?
            .copy_from_slice(&value.to_le_bytes());

        Ok(())
    }

    #[inline(always)]
    pub fn store_u16(&mut self, addr: u32, value: u16) -> anyhow::Result<()> {
        let addr = usize::try_from(addr)?;

        self.memory.get_mut(addr..addr+2)
            .with_context(|| format!("oob store @ {addr:06X} (next pc = {:X})", self.pc))?
            .copy_from_slice(&value.to_le_bytes());

        Ok(())
    }

    #[inline(always)]
    pub fn store_u8(&mut self, addr: u32, value: u8) -> anyhow::Result<()> {
        let addr = usize::try_from(addr)?;

        self.memory.get_mut(addr..addr+1)
            .with_context(|| format!("oob store @ {addr:06X} (next pc = {:X})", self.pc))?
            .copy_from_slice(&[value]);

        Ok(())
    }

    #[inline(always)]
    pub fn store_f32(&mut self, addr: u32, value: f32) -> anyhow::Result<()> {
        let addr = usize::try_from(addr)?;

        self.memory.get_mut(addr..addr+4)
            .with_context(|| format!("oob store @ {addr:06X} (next pc = {:X})", self.pc))?
            .copy_from_slice(&value.to_le_bytes());

        Ok(())
    }

    #[inline(always)]
    pub fn store_f64(&mut self, addr: u32, value: f64) -> anyhow::Result<()> {
        let addr = usize::try_from(addr)?;

        self.memory.get_mut(addr..addr+8)
            .with_context(|| format!("oob store @ {addr:06X} (next pc = {:X})", self.pc))?
            .copy_from_slice(&value.to_le_bytes());

        Ok(())
    }
}