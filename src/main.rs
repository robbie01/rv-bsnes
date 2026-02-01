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
    cpu.ingest(&program)?;

    assert_eq!(game.len(), 0x100000);
    cpu.memory[0xf0000000..0xf0000000+game.len()].copy_from_slice(game);

    let gameinfo: Vec<u8> = [
        0xf0000000,
        game.len().try_into()?,
        0,
        0xfffffffe,
        0xfffffffe,
        0xfffffffe,
        0xfffffffe
    ].into_iter().flat_map(u32::to_le_bytes).collect();

    cpu.memory[0xf0100000..0xf0100000+gameinfo.len()].copy_from_slice(&gameinfo);

    let jg_init = program.symbol_by_name("jg_init").context("no jg_init")?.address() as u32;
    cpu.call_subroutine(jg_init)?;

    println!("after jg_init: {cpu:#X?}");

    cpu.write_x(Register::A0, 0xf0100000);
    let jg_set_gameinfo = program.symbol_by_name("jg_set_gameinfo").context("no jg_set_gameinfo")?.address() as u32;
    cpu.call_subroutine(jg_set_gameinfo)?;

    println!("after jg_set_gameinfo: {cpu:#X?}");

    let jg_game_load = program.symbol_by_name("jg_game_load").context("no jg_game_load")?.address() as u32;
    cpu.call_subroutine(jg_game_load)?;

    println!("after jg_game_load: {cpu:#X?}");

    let jg_deinit = program.symbol_by_name("jg_deinit").context("no jg_init")?.address() as u32;
    cpu.call_subroutine(jg_deinit)?;

    println!("after jg_deinit: {cpu:#X?}");

    Ok(())
}
