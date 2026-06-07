mod lower;

use anyhow::bail;
use rand::RngExt;

use crate::instr::{Instruction as I, *};

use super::*;

pub use lower::{Block, Op};

impl<'arena, H: Hypervisor + ?Sized> Interpreter<'arena, H> {
    #[inline(always)]
    fn decode_block(&mut self, mut pc: u32) -> anyhow::Result<&'arena Block<H>> {
        self.block_scratch.clear();

        loop {
            let (instr, size) = if I::next_is_compressed(self.load_u8(pc)?) {
                let raw = I::decode_compressed(self.load_u16(pc)?)?;
                (raw, 2)
            } else {
                let raw = I::decode(self.load_u32(pc)?)?;
                (raw, 4)
            };
            self.block_scratch.push(Op::new(instr, size)?);
            
            match instr {
                I::JumpAndLink(JumpAndLink { dest: _, offset }) => pc = pc.wrapping_add_signed(offset.into()),
                I::JumpAndLinkRegister(_) | I::Branch(_) => break,
                _ => pc += u32::from(size)
            }
        }

        Ok(Block::new(self.arena, &self.block_scratch))
    }

    fn continue_execution(&mut self, h: &mut H) -> anyhow::Result<usize> {
        if self.pc == 0 {
            bail!("tried to execute at address 0 (missing callback?)\nra = {:06X}", self.read_x(Register::RA));
        }

        let hot_key = (self.pc >> 2) as usize & (HOT_SIZE - 1);

        let block = if let Some(block) = self.hot_cache[hot_key].iter().find_map(|i| { let &(pc, block) = i.as_ref()?; (pc == self.pc).then_some(block) }) {
            block
        } else if let Some(block) = self.block_cache.get(&self.pc) {
            if let Some(r) = self.hot_cache[hot_key].iter_mut().find(|i| i.is_none()) {
                *r = Some((self.pc, block));
            } else {
                let way = self.rng.random_range(0..HOT_WAYS);
                self.hot_cache[hot_key][way] = Some((self.pc, block));
            }
            block
        } else {
            let block = self.decode_block(self.pc)?;
            self.block_cache.insert(self.pc, block);
            if let Some(r) = self.hot_cache[hot_key].iter_mut().find(|i| i.is_none()) {
                *r = Some((self.pc, block));
            } else {
                let way = self.rng.random_range(0..HOT_WAYS);
                self.hot_cache[hot_key][way] = Some((self.pc, block));
            }
            block
        };
        
        block.execute(self, h)?;
        Ok(block.len())
    }

    pub fn call_subroutine(&mut self, h: &mut H, sub: u32) -> anyhow::Result<usize> {
        ensure!(self.pc == u32::MAX);
        self.pc = sub;
        self.write_x(Register::RA, u32::MAX); // sentinel
        let mut acc = 0;
        while self.pc != u32::MAX {
            h.before_block(self)?;
            acc += self.continue_execution(h)?;
            h.after_block(self)?;
        }
        Ok(acc)
    }

    pub fn call_subroutine_by_name(&mut self, h: &mut H, sub: &str) -> anyhow::Result<usize> {
        let addr = h.symbol(sub)?;
        self.call_subroutine(h, addr)
    }
}