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

    eprintln!("calling static_init...");
    let static_init = program.symbol_by_name("_Z41__static_initialization_and_destruction_0ii.constprop.0").context("no static_init etc")?.address() as u32;
    cpu.call_subroutine(static_init)?;

    let game_addr = cpu.memory.kmalloc(game.len()).context("couldn't alloc space for game")?;
    cpu.memory[game_addr..game_addr+game.len()].copy_from_slice(game);

    let gameinfo: Vec<u8> = [
        game_addr as u32,
        game.len().try_into()?,
        0,
        0xfffffffe,
        0xfffffffe,
        0xfffffffe,
        0xfffffffe
    ].into_iter().flat_map(u32::to_le_bytes).collect();

    let gameinfo_addr = cpu.memory.kmalloc(gameinfo.len()).context("couldn't alloc space for gameinfo")?;

    cpu.memory[gameinfo_addr..gameinfo_addr+gameinfo.len()].copy_from_slice(&gameinfo);

    eprintln!("calling jg_init...");
    let jg_init = program.symbol_by_name("jg_init").context("no jg_init")?.address() as u32;
    cpu.call_subroutine(jg_init)?;

    eprintln!("calling jg_set_gameinfo...");
    cpu.write_x(Register::A0, gameinfo_addr as u32);
    let jg_set_gameinfo = program.symbol_by_name("jg_set_gameinfo").context("no jg_set_gameinfo")?.address() as u32;
    cpu.call_subroutine(jg_set_gameinfo)?;

    eprintln!("calling jg_game_load...");
    let jg_game_load = program.symbol_by_name("jg_game_load").context("no jg_game_load")?.address() as u32;
    cpu.call_subroutine(jg_game_load)?;

    eprintln!("calling jg_deinit...");
    let jg_deinit = program.symbol_by_name("jg_deinit").context("no jg_init")?.address() as u32;
    cpu.call_subroutine(jg_deinit)?;

    Ok(())
}
