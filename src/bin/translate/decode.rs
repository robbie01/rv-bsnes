use std::collections::BTreeMap;

use object::{LittleEndian, ObjectSection as _, read::elf::ElfSection32};
use rv::instr::{Instruction as I, *};

#[derive(Debug, Clone)]
pub struct Block {
    pub body: Vec<(u32, I)>,
    pub term: Termination
}

#[derive(Debug, Clone, Copy)]
pub enum Termination {
    JumpAndLinkRegister { jalr: JumpAndLinkRegister, save_pc: u32 },
    JumpAndLink { dest: Register, pc: u32, save_pc: u32 },
    Branch { src1: Register, src2: Register, funct: BranchType, pc: u32, else_pc: u32 }
}

pub fn discover_blocks(text: &ElfSection32<'_, '_, LittleEndian>) -> anyhow::Result<BTreeMap<u32, Block>> {
    #[derive(Debug, Clone, Default)]
    struct IncompleteBlock {
        body: Vec<(u32, I)>,
        term: Option<Termination>
    }

    let base = text.address() as u32;
    let mut pc = base;
    let data = text.data()?;
    let mut blocks = BTreeMap::new();
    let mut terminated = true;

    while pc - base < data.len() as u32 {
        if terminated {
            blocks.entry(pc).or_default();
        }

        let ptr = (pc - base) as usize;

        let (instr, size) = if I::next_is_compressed(data[ptr]) {
            (I::decode_compressed(u16::from_le_bytes(data[ptr..ptr+2].try_into()?))?, 2)
        } else {
            (I::decode(u32::from_le_bytes(data[ptr..ptr+4].try_into()?))?, 4)
        };

        match instr {
            I::JumpAndLink(JumpAndLink { offset, .. }) => {
                blocks.entry(pc.wrapping_add_signed(offset.into())).or_default();
            },
            I::Branch(Branch { offset, .. }) => {
                blocks.entry(pc.wrapping_add_signed(i16::from(offset).into())).or_default();
            },
            _ => ()
        }

        let current_block = blocks.range_mut(..=pc).last().unwrap();
        terminated = match instr {
            I::JumpAndLink(JumpAndLink { offset, dest }) => {
                let (_, IncompleteBlock { term, .. }) = current_block;
                assert!(term.is_none());
                *term = Some(Termination::JumpAndLink { dest, pc: pc.wrapping_add_signed(offset.into()), save_pc: pc.wrapping_add(size) });
                true
            },
            I::Branch(Branch { offset, funct, src1, src2 }) => {
                let (_, IncompleteBlock { term, .. }) = current_block;
                assert!(term.is_none());
                *term = Some(Termination::Branch { funct, src1, src2, pc: pc.wrapping_add_signed(i16::from(offset).into()), else_pc: u32::MAX });
                true
            },
            I::JumpAndLinkRegister(jalr) => {
                let (_, IncompleteBlock { term, .. }) = current_block;
                assert!(term.is_none());
                *term = Some(Termination::JumpAndLinkRegister { jalr, save_pc: pc.wrapping_add(size) });
                true
            },
            _ => {
                let (_, IncompleteBlock { body, .. }) = current_block;
                body.push((pc, instr));
                false
            }
        };

        pc += size;
    }

    let mut it = blocks.iter_mut();
    let (_, mut prev) = it.next().unwrap();
    for (&pc, next) in it {
        if prev.term.is_none() {
            prev.term = Some(Termination::JumpAndLink { dest: Register::ZERO, pc, save_pc: u32::MAX })
        } else if let Some(Termination::Branch { else_pc, .. }) = &mut prev.term && *else_pc == u32::MAX {
            *else_pc = pc;
        }
        prev = next
    }

    Ok(blocks.into_iter()
        .map(|(pc, IncompleteBlock { body, term })|
            (pc, Block { body, term: term.unwrap() }))
        .collect())
}