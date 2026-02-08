mod lower;

use anyhow::bail;

use crate::instr::{Instruction as I, *};

use super::*;

pub use lower::{Block, Op};

impl<H: Hypervisor + ?Sized> Cpu<H> {
    #[inline(always)]
    fn decode_block(&mut self, mut pc: u32) -> anyhow::Result<Rc<Block<H>>> {
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

        Ok(Block::new(self.block_scratch.iter().copied()))
    }

    fn continue_execution(&mut self, h: &mut H) -> anyhow::Result<()> {
        if self.pc == 0 {
            bail!("tried to execute at address 0 (missing callback?)\nra = {:06X}", self.read_x(Register::RA));
        }

        let hot_key = (self.pc >> 2) as usize & (HOT_SIZE - 1);

        let block = if let Some((pc, ref block)) = self.hot_cache[hot_key] && pc == self.pc {
            block.clone()
        } else if let Some(block) = self.block_cache.get(&self.pc) {
            self.hot_cache[hot_key] = Some((self.pc, block.clone()));
            block.clone()
        } else {
            let block = self.decode_block(self.pc)?;
            self.block_cache.insert(self.pc, block.clone());
            self.hot_cache[hot_key] = Some((self.pc, block.clone()));
            block
        };
        
        block.execute(self, h)?;
        Ok(())
    }

    pub fn call_subroutine(&mut self, h: &mut H, sub: u32) -> anyhow::Result<()> {
        ensure!(self.pc == u32::MAX);
        self.pc = sub;
        self.write_x(Register::RA, u32::MAX); // sentinel
        while self.pc != u32::MAX {
            h.before_block(self)?;
            self.continue_execution(h)?;
            h.after_block(self)?;
        }
        Ok(())
    }

    pub fn call_subroutine_by_name(&mut self, h: &mut H, sub: &str) -> anyhow::Result<()> {
        let addr = h.symbol(sub)?;
        self.call_subroutine(h, addr)
    }
}