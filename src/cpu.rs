use crate::instr::Register;

use std::fmt::Debug;

pub trait Hypervisor {
    fn before_block(&mut self, ctx: &mut impl Cpu<H = Self>) -> anyhow::Result<()>;
    fn after_block(&mut self, ctx: &mut impl Cpu<H = Self>) -> anyhow::Result<()>;

    fn symbol(&self, sym: &str) -> anyhow::Result<u32>;

    fn ebreak(&mut self, ctx: &mut impl Cpu<H = Self>) -> anyhow::Result<()>;
    fn ecall(&mut self, ctx: &mut impl Cpu<H = Self>) -> anyhow::Result<()>;
}

pub trait LoadableHypervisor<'data>: Hypervisor {
    type Object: 'data;

    fn load<'this, C: Cpu<H = Self> + 'this>(&'this mut self, ctx: &mut C, obj: &'data Self::Object) -> anyhow::Result<()> where 'data: 'this;
}

#[derive(Clone, Copy)]
pub struct FRegister {
    pub value: f64
}

pub const CANONICAL_NAN_F32: f32 = f32::from_bits(0x7fc00000);

pub const CANONICAL_NAN_F64: f64 = f64::from_bits(0x7ff8000000000000);

pub const BOX_MASK: u64 = 0xffffffff00000000;

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

pub trait Cpu {
    type H: Hypervisor + ?Sized;

    fn pc(&self) -> Option<u32>;

    fn read_x(&self, x: Register) -> u32;
    fn write_x(&mut self, x: Register, v: u32);
    fn read_f(&mut self, f: Register) -> FRegister;
    fn write_f(&mut self, f: Register, v: FRegister);

    fn load_u32(&self, addr: u32) -> anyhow::Result<u32>;
    fn load_u16(&self, addr: u32) -> anyhow::Result<u16>;
    fn load_i16(&self, addr: u32) -> anyhow::Result<i16>;
    fn load_u8(&self, addr: u32) -> anyhow::Result<u8>;
    fn load_i8(&self, addr: u32) -> anyhow::Result<i8>;
    fn load_f32(&self, addr: u32) -> anyhow::Result<f32>;
    fn load_f64(&self, addr: u32) -> anyhow::Result<f64>;

    fn store_u32(&mut self, addr: u32, value: u32) -> anyhow::Result<()>;
    fn store_u16(&mut self, addr: u32, value: u16) -> anyhow::Result<()>;
    fn store_u8(&mut self, addr: u32, value: u8) -> anyhow::Result<()>;
    fn store_f32(&mut self, addr: u32, value: f32) -> anyhow::Result<()>;
    fn store_f64(&mut self, addr: u32, value: f64) -> anyhow::Result<()>;

    fn load_string(&self, addr: u32) -> anyhow::Result<String> {
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

    fn store_slice(&mut self, addr: u32, value: &[u8]) -> anyhow::Result<()> {
        for (a, v) in (addr..).zip(value) {
            self.store_u8(a, *v)?;
        }

        Ok(())
    }
}
