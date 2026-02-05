use std::{fs::File, io::{BufWriter, Write}};

use anyhow::Context;
use include_bytes_aligned::include_bytes_aligned;
use object::{LittleEndian, Object, ObjectSymbol, read::elf::ElfFile32};

use crate::{cpu::{Cpu, linux::LinuxHypervisor}, instr::Register};

mod instr;
mod cpu;
mod fs;

fn main() -> anyhow::Result<()> {
    let game = include_bytes!("../lttp.sfc");

    let program = ElfFile32::<LittleEndian>::parse(&include_bytes_aligned!(16, "../bsnes.elf")[..])?;
    let mut cpu = Cpu::new(LinuxHypervisor::default());
    cpu.load(&program)?;

    let init_array_start: u32 = program.symbol_by_name("__init_array_start").context("no __init_array_start")?.address().try_into()?;
    let init_array_end: u32 = program.symbol_by_name("__init_array_end").context("no __init_array_end")?.address().try_into()?;

    eprintln!("calling initializers...");

    for addr in (init_array_start..init_array_end).step_by(4) {
        let ctor = cpu.load_u32(addr)?;
        cpu.call_subroutine(ctor)?;
    }

    let game_addr = cpu.memory.kmalloc(game.len().try_into()?).context("couldn't alloc space for game")?;
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
    let gameinfo_addr = cpu.memory.kmalloc(gameinfo.len().try_into()?).context("couldn't alloc space for gameinfo")?;
    cpu.store_slice(gameinfo_addr, &gameinfo)?;

    let pathinfo: Vec<u8> = [
        0,
        0xfffffffe,
        0,
        0xfffffffe,
        0xfffffffe
    ].into_iter().flat_map(u32::to_le_bytes).collect();
    let pathinfo_addr = cpu.memory.kmalloc(pathinfo.len().try_into()?).context("couldn't alloc space for pathinfo")?;
    cpu.store_slice(pathinfo_addr, &pathinfo)?;

    eprintln!("calling jg_init...");
    cpu.call_subroutine_by_name("jg_init")?;

    eprintln!("calling jg_set_cb_log...");
    cpu.write_x(Register::A0, 0xe0000000);
    cpu.call_subroutine_by_name("jg_set_cb_log")?;

    eprintln!("calling jg_set_cb_frametime...");
    cpu.write_x(Register::A0, 0xe0000004);
    cpu.call_subroutine_by_name("jg_set_cb_frametime")?;

    eprintln!("calling jg_set_gameinfo...");
    cpu.write_x(Register::A0, gameinfo_addr);
    cpu.call_subroutine_by_name("jg_set_gameinfo")?;

    eprintln!("calling jg_set_paths...");
    cpu.write_x(Register::A0, pathinfo_addr);
    cpu.call_subroutine_by_name("jg_set_paths")?;

    let inputstate = cpu.memory.kmalloc(16).context("couldn't alloc inputstate")?;
    let buttons = cpu.memory.kmalloc(12).context("couldn't alloc buttons")?;
    cpu.store_u32(inputstate+4, buttons)?;
    eprintln!("calling jg_set_inputstate...");
    cpu.write_x(Register::A0, inputstate);
    cpu.write_x(Register::A1, 0);
    cpu.call_subroutine_by_name("jg_set_inputstate")?;
    cpu.write_x(Register::A0, inputstate);
    cpu.write_x(Register::A1, 1);
    cpu.call_subroutine_by_name("jg_set_inputstate")?;

    let video_addr = cpu.memory.kmalloc(4 * 253440).context("couldn't alloc vid buf")?;
    eprintln!("calling jg_get_videoinfo...");
    cpu.call_subroutine_by_name("jg_get_videoinfo")?;
    let vidinfo_addr = cpu.read_x(Register::A0);
    cpu.store_u32(vidinfo_addr+40, video_addr)?;

    eprintln!("calling jg_setup_video...");
    cpu.call_subroutine_by_name("jg_setup_video")?;

    eprintln!("calling jg_game_load...");
    cpu.call_subroutine_by_name("jg_game_load")?;

    // for i in 0..300 {
    //     eprintln!("calling jg_exec_frame ({i})...");
    //     cpu.call_subroutine_by_name("jg_exec_frame")?;

    //     if i > 80 {
    //         let mut f = BufWriter::new(File::create(format!("frames/{i:03}.ppm"))?);
    //         writeln!(f, "P6\n480 256\n255\n")?;
    //         for i in 0..(480*256) {
    //             let c = [cpu.load_u8(video_addr+4*i+2)?, cpu.load_u8(video_addr+4*i+1)?, cpu.load_u8(video_addr+4*i)?];
    //             f.write_all(&c)?;
    //         }
    //         f.flush()?;
    //     }
    // }

    Ok(())
}
