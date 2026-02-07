mod exec;
mod memory;
pub mod linux;

use std::{fmt::Debug, rc::Rc};

use anyhow::ensure;
use fnv::FnvHashMap;

use crate::instr::Register;
use memory::Memory;

const CANONICAL_NAN_F32: f32 = f32::from_bits(0x7fc00000);
const CANONICAL_NAN_F64: f64 = f64::from_bits(0x7ff8000000000000);

const BOX_MASK: u64 = 0xffffffff00000000;

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
        if bits & BOX_MASK == BOX_MASK {
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
            value: f64::from_bits(BOX_MASK | value.to_bits() as u64)
        }
    }
}

impl Debug for FRegister {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{{32}} | {}{{64}}", self.read_f32(), self.read_f64())
    }
}

pub trait Hypervisor {
    fn before_block(&mut self, ctx: &mut Cpu<Self>) -> anyhow::Result<()>;
    fn after_block(&mut self, ctx: &mut Cpu<Self>) -> anyhow::Result<()>;

    fn symbol(&self, sym: &str) -> anyhow::Result<u32>;

    fn ebreak(&mut self, ctx: &mut Cpu<Self>) -> anyhow::Result<()>;
    fn ecall(&mut self, ctx: &mut Cpu<Self>) -> anyhow::Result<()>;
}

pub trait LoadableHypervisor<'data>: Hypervisor {
    type Object: 'data;

    fn load<'this>(&'this mut self, ctx: &mut Cpu<Self>, obj: &'data Self::Object) -> anyhow::Result<()> where 'data: 'this;
}

const HOT_SIZE: usize = 1 << 14;

#[derive(Clone, Debug)]
pub struct Cpu<H: ?Sized> {
    pc: u32,
    pub x: [u32; 32],
    pub f: [FRegister; 32],
    pub memory: Memory,

    pub hot_cache: [Option<(u32, Rc<exec::Block<H>>)>; HOT_SIZE],
    pub block_cache: FnvHashMap<u32, Rc<exec::Block<H>>>,
    block_scratch: Vec<exec::Op<H>>
}

impl<H: ?Sized> Cpu<H> {
    pub fn new() -> Self {
        let mut this = Self {
            pc: u32::MAX,
            x: [0; 32],
            f: [FRegister::write_f64(0.); 32],
            memory: Memory::new(0x10000000),

            hot_cache: [const { None }; _],
            block_cache: FnvHashMap::default(),
            block_scratch: Vec::new()
        };

        // initialize stack pointer (todo make this better LoL)
        this.write_x(Register::SP, 0xfffffff0);

        this
    }

    #[inline(always)]
    pub fn read_x(&self, x: Register) -> u32 {
        *unsafe { self.x.get_unchecked(usize::from(x)) }
    }

    #[inline(always)]
    pub fn write_x(&mut self, x: Register, v: u32) {
        if x != Register::ZERO {
            *unsafe { self.x.get_unchecked_mut(usize::from(x)) } = v;
        }
    }

    #[inline(always)]
    pub unsafe fn write_x_unchecked(&mut self, x: Register, v: u32) {
        debug_assert_ne!(x, Register::ZERO);
        *unsafe { self.x.get_unchecked_mut(usize::from(x)) } = v;
    }

    #[inline(always)]
    pub fn read_f(&mut self, f: Register) -> FRegister {
        *unsafe { self.f.get_unchecked(usize::from(f)) }
    }

    #[inline(always)]
    pub fn write_f(&mut self, f: Register, v: FRegister) {
        *unsafe { self.f.get_unchecked_mut(usize::from(f)) } = v;
    }

    pub fn load<'data>(&mut self, h: &mut H, elf: &'data H::Object) -> anyhow::Result<()> where H: LoadableHypervisor<'data> {
        h.load(self, elf)?;
        Ok(())
    }
}

// Load/store helpers
impl<H: ?Sized> Cpu<H> {
    #[inline(always)]
    pub fn load_u32(&self, addr: u32) -> anyhow::Result<u32> {
        self.memory.load_u32(addr)
    }

    #[inline(always)]
    pub fn load_u16(&self, addr: u32) -> anyhow::Result<u16> {
        self.memory.load_u16(addr)
    }

    #[inline(always)]
    pub fn load_i16(&self, addr: u32) -> anyhow::Result<i16> {
        self.memory.load_i16(addr)
    }

    #[inline(always)]
    pub fn load_u8(&self, addr: u32) -> anyhow::Result<u8> {
        self.memory.load_u8(addr)
    }

    #[inline(always)]
    pub fn load_i8(&self, addr: u32) -> anyhow::Result<i8> {
        self.memory.load_i8(addr)
    }

    #[inline(always)]
    pub fn load_f32(&self, addr: u32) -> anyhow::Result<f32> {
        self.memory.load_f32(addr)
    }

    #[inline(always)]
    pub fn load_f64(&self, addr: u32) -> anyhow::Result<f64> {
        self.memory.load_f64(addr)
    }

    #[inline(always)]
    pub fn store_u32(&mut self, addr: u32, value: u32) -> anyhow::Result<()> {
        self.memory.store_u32(addr, value)
    }

    #[inline(always)]
    pub fn store_u16(&mut self, addr: u32, value: u16) -> anyhow::Result<()> {
        self.memory.store_u16(addr, value)
    }

    #[inline(always)]
    pub fn store_u8(&mut self, addr: u32, value: u8) -> anyhow::Result<()> {
        self.memory.store_u8(addr, value)
    }

    #[inline(always)]
    pub fn store_f32(&mut self, addr: u32, value: f32) -> anyhow::Result<()> {
        self.memory.store_f32(addr, value)
    }

    #[inline(always)]
    pub fn store_f64(&mut self, addr: u32, value: f64) -> anyhow::Result<()> {
        self.memory.store_f64(addr, value)
    }

    pub fn load_string(&self, addr: u32) -> anyhow::Result<String> {
        let mut s = Vec::new();
        for i in addr.. {
            let c = self.load_u8(i)?;
            if c == 0 {
                break;
            }
            s.push(c);
        }
        Ok(String::try_from(s)?)
    }

    pub fn store_slice(&mut self, addr: u32, value: &[u8]) -> anyhow::Result<()> {
        for (a, v) in (addr..).zip(value) {
            self.store_u8(a, *v)?;
        }

        Ok(())
    }
}