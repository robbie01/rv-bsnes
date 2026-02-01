use crate::instr::Instruction;

mod instr;
mod cpu;

fn main() {
    let program = include_bytes!("/Users/robbie/Downloads/bsnes.bin");
    let mut pos = 0;
    while pos < program.len() {
        let (instr, raw, length) = if Instruction::next_is_compressed(program[pos]) {
            let raw = u16::from_le_bytes(program[pos..pos+2].try_into().unwrap());
            (Instruction::decode_compressed(raw), raw.into(), 2)
        } else {
            let raw = u32::from_le_bytes(program[pos..pos+4].try_into().unwrap());
            (Instruction::decode(raw), raw, 4)
        };
        print!("{pos:08X}: {instr:?}");
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
