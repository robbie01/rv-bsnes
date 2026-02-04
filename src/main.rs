use anyhow::Context;
use include_bytes_aligned::include_bytes_aligned;
use object::{LittleEndian, Object, ObjectSymbol, read::elf::ElfFile32};

use crate::{cpu::{Cpu, linux::LinuxHypervisor}, instr::Register};

mod instr;
mod cpu;

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
    cpu.memory[game_addr..game_addr+u32::try_from(game.len())?].copy_from_slice(game);

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
    cpu.memory[gameinfo_addr..gameinfo_addr+u32::try_from(gameinfo.len())?].copy_from_slice(&gameinfo);

    let pathinfo: Vec<u8> = [
        0,
        0xfffffffe,
        0,
        0xfffffffe,
        0xfffffffe
    ].into_iter().flat_map(u32::to_le_bytes).collect();
    let pathinfo_addr = cpu.memory.kmalloc(pathinfo.len().try_into()?).context("couldn't alloc space for pathinfo")?;
    cpu.memory[pathinfo_addr..pathinfo_addr+u32::try_from(pathinfo.len())?].copy_from_slice(&pathinfo);

    eprintln!("calling jg_init...");
    let jg_init = program.symbol_by_name("jg_init").context("no jg_init")?.address() as u32;
    cpu.call_subroutine(jg_init)?;

    eprintln!("calling jg_set_cb_log...");
    cpu.write_x(Register::A0, 0xe0000000);
    let jg_set_cb_log = program.symbol_by_name("jg_set_cb_log").context("no jg_set_cb_log")?.address() as u32;
    cpu.call_subroutine(jg_set_cb_log)?;

    eprintln!("calling jg_set_gameinfo...");
    cpu.write_x(Register::A0, gameinfo_addr);
    let jg_set_gameinfo = program.symbol_by_name("jg_set_gameinfo").context("no jg_set_gameinfo")?.address() as u32;
    cpu.call_subroutine(jg_set_gameinfo)?;

    eprintln!("calling jg_set_paths...");
    cpu.write_x(Register::A0, pathinfo_addr);
    let jg_set_paths = program.symbol_by_name("jg_set_paths").context("no jg_set_paths")?.address() as u32;
    cpu.call_subroutine(jg_set_paths)?;

    eprintln!("calling jg_game_load...");
    let jg_game_load = program.symbol_by_name("jg_game_load").context("no jg_game_load")?.address() as u32;
    cpu.call_subroutine(jg_game_load)?;

    eprintln!("calling jg_deinit...");
    let jg_deinit = program.symbol_by_name("jg_deinit").context("no jg_init")?.address() as u32;
    cpu.call_subroutine(jg_deinit)?;

    Ok(())
}
