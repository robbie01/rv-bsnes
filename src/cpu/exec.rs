mod lower;

use anyhow::bail;

use crate::instr::{Instruction as I, *};

use super::*;

pub use lower::Op;

impl Cpu {
    #[inline(always)]
    fn decode_block(&self, mut pc: u32) -> anyhow::Result<Rc<[lower::Op]>> {
        let mut block = Vec::new();
        loop {
            let (instr, size) = if I::next_is_compressed(self.load_u8(pc)?) {
                let raw = I::decode_compressed(self.load_u16(pc)?)?;
                (raw, 2)
            } else {
                let raw = I::decode(self.load_u32(pc)?)?;
                (raw, 4)
            };
            block.push(Op::new(instr, size)?);
            
            match instr {
                I::JumpAndLink(JumpAndLink { dest: _, offset }) => pc = pc.wrapping_add_signed(offset.into()),
                I::JumpAndLinkRegister(_) | I::Branch(_) => break,
                _ => pc += u32::from(size)
            }
        }

        Ok(block.into())
    }

    fn continue_execution(&mut self, h: &mut impl Hypervisor) -> anyhow::Result<()> {
        if self.pc == 0 {
            bail!("tried to execute at address 0 (missing callback?)\nra = {:06X}", self.read_x(Register::RA));
        }

        let block = if let Some(block) = self.block_cache.get(&self.pc) {
            block.clone()
        } else {
            let block = self.decode_block(self.pc)?;
            self.block_cache.insert(self.pc, block.clone());
            block
        };
        
        for op in &block[..] {
            op.execute(self, h)?;
        }
        Ok(())
    }

    pub fn call_subroutine(&mut self, h: &mut impl Hypervisor, sub: u32) -> anyhow::Result<()> {
        ensure!(self.pc == u32::MAX);
        self.pc = sub;
        self.write_x(Register::RA, u32::MAX); // sentinel
        while self.pc != u32::MAX {
            self.continue_execution(h)?;
        }
        Ok(())
    }

    pub fn call_subroutine_by_name(&mut self, h: &mut impl Hypervisor, sub: &str) -> anyhow::Result<()> {
        let addr = h.symbol(sub)?;
        self.call_subroutine(h, addr)
    }
}