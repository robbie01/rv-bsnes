#![feature(adt_const_params)]
#![feature(explicit_tail_calls)]
#![expect(incomplete_features)]

use std::{fmt::Debug, time::Instant};

use anyhow::Context;
use bumpalo::Bump;
use include_bytes_aligned::include_bytes_aligned;
use object::{LittleEndian, Object, ObjectSymbol, read::elf::ElfFile32};

use crate::{interpreter::{Interpreter, linux::LinuxHypervisor}, instr::Register};

use rv::instr;
mod interpreter;
mod fs;


trait Hypervisor {
    fn before_block(&mut self, ctx: &mut impl crate::Cpu<H = Self>) -> anyhow::Result<()>;
    fn after_block(&mut self, ctx: &mut impl crate::Cpu<H = Self>) -> anyhow::Result<()>;

    fn symbol(&self, sym: &str) -> anyhow::Result<u32>;

    fn ebreak(&mut self, ctx: &mut impl crate::Cpu<H = Self>) -> anyhow::Result<()>;
    fn ecall(&mut self, ctx: &mut impl crate::Cpu<H = Self>) -> anyhow::Result<()>;
}

trait LoadableHypervisor<'data>: Hypervisor {
    type Object: 'data;

    fn load<'this, C: crate::Cpu<H = Self> + 'this>(&'this mut self, ctx: &mut C, obj: &'data Self::Object) -> anyhow::Result<()> where 'data: 'this;
}

#[derive(Clone, Copy)]
pub struct FRegister {
    value: f64
}

const CANONICAL_NAN_F32: f32 = f32::from_bits(0x7fc00000);
const CANONICAL_NAN_F64: f64 = f64::from_bits(0x7ff8000000000000);

const BOX_MASK: u64 = 0xffffffff00000000;

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

trait Cpu {
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

fn main() -> anyhow::Result<()> {
    let game = include_bytes!("../lttp.sfc");
    let arena = Bump::new();

    let program = ElfFile32::<LittleEndian>::parse(&include_bytes_aligned!(16, "../bsnes.elf")[..])?;
    let mut h = LinuxHypervisor::default();
    let mut cpu = Interpreter::new(&arena);
    cpu.load(&mut h, &program)?;

    let init_array_start: u32 = program.symbol_by_name("__init_array_start").context("no __init_array_start")?.address().try_into()?;
    let init_array_end: u32 = program.symbol_by_name("__init_array_end").context("no __init_array_end")?.address().try_into()?;

    eprintln!("calling initializers...");

    for addr in (init_array_start..init_array_end).step_by(4) {
        let ctor = cpu.load_u32(addr)?;
        cpu.call_subroutine(&mut h, ctor)?;
    }

    let game_addr = h.kmalloc(game.len().try_into()?).context("couldn't alloc space for game")?;
    cpu.store_slice(game_addr, game)?;

    let gameinfo: Vec<u8> = [
        game_addr,
        game.len().try_into()?,
        0,
        0xfffffffe,
        0xfffffffe,
        0xfffffffe,
        0xfffffffe
    ].into_iter().flat_map(u32::to_le_bytes).collect();
    let gameinfo_addr = h.kmalloc(gameinfo.len().try_into()?).context("couldn't alloc space for gameinfo")?;
    cpu.store_slice(gameinfo_addr, &gameinfo)?;

    let pathinfo: Vec<u8> = [
        0,
        0xfffffffe,
        0,
        0xfffffffe,
        0xfffffffe
    ].into_iter().flat_map(u32::to_le_bytes).collect();
    let pathinfo_addr = h.kmalloc(pathinfo.len().try_into()?).context("couldn't alloc space for pathinfo")?;
    cpu.store_slice(pathinfo_addr, &pathinfo)?;

    eprintln!("calling jg_init...");
    cpu.call_subroutine_by_name(&mut h, "jg_init")?;

    eprintln!("calling jg_set_cb_log...");
    cpu.write_x(Register::A0, 0xe0000000);
    cpu.call_subroutine_by_name(&mut h, "jg_set_cb_log")?;

    eprintln!("calling jg_set_cb_frametime...");
    cpu.write_x(Register::A0, 0xe0000004);
    cpu.call_subroutine_by_name(&mut h, "jg_set_cb_frametime")?;

    eprintln!("calling jg_set_gameinfo...");
    cpu.write_x(Register::A0, gameinfo_addr);
    cpu.call_subroutine_by_name(&mut h, "jg_set_gameinfo")?;

    eprintln!("calling jg_set_paths...");
    cpu.write_x(Register::A0, pathinfo_addr);
    cpu.call_subroutine_by_name(&mut h, "jg_set_paths")?;

    let inputstate = h.kmalloc(16).context("couldn't alloc inputstate")?;
    let buttons = h.kmalloc(12).context("couldn't alloc buttons")?;
    cpu.store_u32(inputstate+4, buttons)?;
    eprintln!("calling jg_set_inputstate...");
    cpu.write_x(Register::A0, inputstate);
    cpu.write_x(Register::A1, 0);
    cpu.call_subroutine_by_name(&mut h, "jg_set_inputstate")?;
    cpu.write_x(Register::A0, inputstate);
    cpu.write_x(Register::A1, 1);
    cpu.call_subroutine_by_name(&mut h, "jg_set_inputstate")?;

    let video_addr = h.kmalloc(4 * 253440).context("couldn't alloc vid buf")?;
    eprintln!("calling jg_get_videoinfo...");
    cpu.call_subroutine_by_name(&mut h, "jg_get_videoinfo")?;
    let vidinfo_addr = cpu.read_x(Register::A0);
    cpu.store_u32(vidinfo_addr+40, video_addr)?;

    eprintln!("calling jg_setup_video...");
    cpu.call_subroutine_by_name(&mut h, "jg_setup_video")?;

    eprintln!("calling jg_game_load...");
    cpu.call_subroutine_by_name(&mut h, "jg_game_load")?;

    let nframes = 200;
    let mut frametime = 0.;

    for i in 0..nframes {
        eprintln!("calling jg_exec_frame ({i})...");
        let t1 = Instant::now();
        cpu.call_subroutine_by_name(&mut h, "jg_exec_frame")?;
        let t = Instant::now() - t1;
        frametime += t.as_secs_f64();

        // if i >= 300 {
        //     let _w = cpu.load_u32(vidinfo_addr+0xc)?;
        //     let h = cpu.load_u32(vidinfo_addr+0x10)?;
        //     let p = cpu.load_u32(vidinfo_addr+0x1c)?;
        //     let mut f = BufWriter::new(File::create(format!("frames/{i:03}.ppm"))?);
        //     write!(f, "P6\n{p} {h}\n255\n")?;
        //     for i in 0..(p*h) {
        //         let c = cpu.load_u32(video_addr+4*i)?;
        //         f.write_all(&c.to_be_bytes()[1..])?;
        //     }
        //     f.flush()?;
        // }
    }

    println!("avg s per frame: {}", frametime / nframes as f64);

    Ok(())
}
