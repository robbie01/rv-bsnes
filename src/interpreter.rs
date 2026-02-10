mod exec;
mod memory;
pub mod linux;

use std::fmt::Debug;

use anyhow::ensure;
use bumpalo::Bump;
use fnv::FnvHashMap;

use crate::{Cpu, FRegister, Hypervisor, instr::Register};
use memory::Memory;

const HOT_SIZE: usize = 1 << 14;

#[derive(Debug)]
pub struct Interpreter<'arena, H: Hypervisor + ?Sized> {
    arena: &'arena Bump,

    pc: u32,
    pub(self) x: [u32; 32],
    pub(self) f: [FRegister; 32],
    pub(self) memory: Memory,

    pub(self) hot_cache: [Option<(u32, &'arena exec::Block<H>)>; HOT_SIZE],
    pub(self) block_cache: FnvHashMap<u32, &'arena exec::Block<H>>,
    block_scratch: Vec<exec::Op<H>>
}

impl<'arena, H: Hypervisor + ?Sized> Interpreter<'arena, H> {
    pub fn new(arena: &'arena Bump) -> Self {
        let mut this = Self {
            arena,

            pc: u32::MAX,
            x: [0; 32],
            f: [FRegister::write_f64(0.); 32],
            memory: Memory::new(),

            hot_cache: [const { None }; _],
            block_cache: FnvHashMap::default(),
            block_scratch: Vec::new()
        };

        // initialize stack pointer (todo make this better LoL)
        this.write_x(Register::SP, 0xfffffff0);

        this
    }

    #[inline(always)]
    pub unsafe fn write_x_unchecked(&mut self, x: Register, v: u32) {
        debug_assert_ne!(x, Register::ZERO);
        *unsafe { self.x.get_unchecked_mut(usize::from(x)) } = v;
    }

    pub fn load<'data>(&mut self, h: &mut H, elf: &'data H::Object) -> anyhow::Result<()> where H: super::LoadableHypervisor<'data> {
        h.load(self, elf)?;
        Ok(())
    }
}

// Load/store helpers
impl<'arena, H: Hypervisor + ?Sized> crate::Cpu for Interpreter<'arena, H> {
    type H = H;

    #[inline(always)]
    fn pc(&self) -> Option<u32> {
        Some(self.pc)
    }

    #[inline(always)]
    fn read_x(&self, x: Register) -> u32 {
        *unsafe { self.x.get_unchecked(usize::from(x)) }
    }

    #[inline(always)]
    fn write_x(&mut self, x: Register, v: u32) {
        if x != Register::ZERO {
            *unsafe { self.x.get_unchecked_mut(usize::from(x)) } = v;
        }
    }

    #[inline(always)]
    fn read_f(&mut self, f: Register) -> FRegister {
        *unsafe { self.f.get_unchecked(usize::from(f)) }
    }

    #[inline(always)]
    fn write_f(&mut self, f: Register, v: FRegister) {
        *unsafe { self.f.get_unchecked_mut(usize::from(f)) } = v;
    }

    #[inline(always)]
    fn load_u32(&self, addr: u32) -> anyhow::Result<u32> {
        self.memory.load_u32(addr)
    }

    #[inline(always)]
    fn load_u16(&self, addr: u32) -> anyhow::Result<u16> {
        self.memory.load_u16(addr)
    }

    #[inline(always)]
    fn load_i16(&self, addr: u32) -> anyhow::Result<i16> {
        self.memory.load_i16(addr)
    }

    #[inline(always)]
    fn load_u8(&self, addr: u32) -> anyhow::Result<u8> {
        self.memory.load_u8(addr)
    }

    #[inline(always)]
    fn load_i8(&self, addr: u32) -> anyhow::Result<i8> {
        self.memory.load_i8(addr)
    }

    #[inline(always)]
    fn load_f32(&self, addr: u32) -> anyhow::Result<f32> {
        self.memory.load_f32(addr)
    }

    #[inline(always)]
    fn load_f64(&self, addr: u32) -> anyhow::Result<f64> {
        self.memory.load_f64(addr)
    }

    #[inline(always)]
    fn store_u32(&mut self, addr: u32, value: u32) -> anyhow::Result<()> {
        self.memory.store_u32(addr, value)
    }

    #[inline(always)]
    fn store_u16(&mut self, addr: u32, value: u16) -> anyhow::Result<()> {
        self.memory.store_u16(addr, value)
    }

    #[inline(always)]
    fn store_u8(&mut self, addr: u32, value: u8) -> anyhow::Result<()> {
        self.memory.store_u8(addr, value)
    }

    #[inline(always)]
    fn store_f32(&mut self, addr: u32, value: f32) -> anyhow::Result<()> {
        self.memory.store_f32(addr, value)
    }

    #[inline(always)]
    fn store_f64(&mut self, addr: u32, value: f64) -> anyhow::Result<()> {
        self.memory.store_f64(addr, value)
    }
}