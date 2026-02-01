use object::{LittleEndian, Object, ObjectSection, ObjectSymbol, read::elf::ElfFile32};

use crate::instr::Instruction;

mod instr;
mod cpu;

fn main() {
    let program = ElfFile32::<LittleEndian>::parse(&include_bytes!("../bsnes.elf")[..]).unwrap();
    let gp = program.symbol_by_name("__global_pointer$").unwrap().address();
    println!("gp = {gp:06x}");
    let text_sect = program.section_by_name(".text").unwrap();
    let text_off = text_sect.address() as usize;
    let text = text_sect.data().unwrap();

    let mut pos = 0;
    while pos < text.len() {
        let (instr, raw, length) = if Instruction::next_is_compressed(text[pos]) {
            let raw = u16::from_le_bytes(text[pos..pos+2].try_into().unwrap());
            (Instruction::decode_compressed(raw), raw.into(), 2)
        } else {
            let raw = u32::from_le_bytes(text[pos..pos+4].try_into().unwrap());
            (Instruction::decode(raw), raw, 4)
        };
        print!("{:06x}: {instr:?}", text_off+pos);
        if instr.is_err() {
            if length == 2 {
                print!(" {{{raw:04X}}}");
            } else {
                print!(" {{{raw:08X}}}");
            }
        }
        println!();
        pos += length;
    }
}
