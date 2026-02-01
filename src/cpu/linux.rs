use anyhow::{Context as _, anyhow, ensure};
use object::{Architecture, LittleEndian, Object as _, ObjectSection, ObjectSymbol as _, elf::SHF_ALLOC, read::elf::ElfFile32};

use crate::instr::{I12, Instruction, JumpAndLink, JumpAndLinkRegister, Register};

#[derive(Default, Debug)]
pub struct LinuxHypervisor {
    bss: u32,
    brk: u32,
    stack: Vec<u32>
}

impl super::Hypervisor for LinuxHypervisor {
    fn load<'data>(&mut self, ctx: &mut super::Cpu<Self>, obj: &ElfFile32<LittleEndian>) -> anyhow::Result<()> {
        ensure!(obj.architecture() == Architecture::Riscv32);

        let gp = obj.symbol_by_name("__global_pointer$").context("no gp")?.address().try_into()?;
        ctx.write_x(Register::GP, gp);
        
        for section in obj.sections() {
            if section.elf_section_header().sh_flags.get(LittleEndian) & SHF_ALLOC == 0 {
                continue;
            }

            if section.name()? == ".bss" {
                self.bss = section.address().try_into()?;
                self.brk = self.bss + u32::try_from(section.size())?;
                continue;
            }

            let addr = usize::try_from(section.address())?;
            let data = section.data()?;
            ctx.memory[addr..addr+data.len()].copy_from_slice(section.data()?);
        }

        Ok(())
    }

    #[inline(always)]
    fn before_instr(&mut self, ctx: &mut super::Cpu<Self>, instr: Instruction) -> anyhow::Result<()> where Self: Sized {
        match instr {
            Instruction::JumpAndLink(JumpAndLink { dest: Register::RA, .. }) | Instruction::JumpAndLinkRegister(JumpAndLinkRegister { dest: Register::RA, .. }) =>
                { self.stack.push(ctx.pc); },
            Instruction::JumpAndLinkRegister(JumpAndLinkRegister { dest: Register::ZERO, base: Register::RA, offset: I12::ZERO }) =>
                { self.stack.pop(); },
            _ => ()
        }
        Ok(())
    }

    #[inline(always)]
    fn ebreak(&mut self, _ctx: &mut super::Cpu<Self>) -> anyhow::Result<()> where Self: Sized {
        Err(anyhow!("EBREAK reached\nStack trace: {:X?}", self.stack))
    }

    #[inline(always)]
    fn ecall(&mut self, ctx: &mut super::Cpu<Self>) -> anyhow::Result<()> where Self: Sized {
        match ctx.read_x(Register::A7) {
            214 => { // brk
                let req = ctx.read_x(Register::A0);
                if req >= self.bss && req < 0x10000000 {
                    self.brk = req;
                }
                ctx.write_x(Register::A0, self.brk);
            },
            222 => { // mmap
                let _addr = ctx.read_x(Register::A0);
                let length = ctx.read_x(Register::A1);
                let flags = ctx.read_x(Register::A3);
                let _fd = ctx.read_x(Register::A4) as i32;

                println!("{:06X}: mmap(0x{_addr:X}, {length}, _, 0x{flags:X}, {_fd})", ctx.pc-4);

                if flags & 0x20 == 0 { // MAP_ANONYMOUS
                    ctx.write_x(Register::A0, -9i32 as u32); // EBADF
                } else if flags & 0x10 != 0 { // MAP_FIXED
                    ctx.write_x(Register::A0, -12i32 as u32); // ENOMEM
                } else {
                    let alloc = ctx.memory.mmap_anon(length.try_into()?);
                    match alloc {
                        Some(addr) => ctx.write_x(Register::A0, addr.try_into()?),
                        None => ctx.write_x(Register::A0, -12i32 as u32) // ENOMEM
                    }
                }
            },
            135 => { // rt_sigprocmask
                ctx.write_x(Register::A0, 0);
            }
            whence => {
                println!();
                println!("{:X?}", self.stack);
                println!("{:06X}: ECALL {whence} ({:X})", ctx.pc-4, ctx.read_x(Register::A0));
                ctx.write_x(Register::A0, u32::MAX); // no errno LOL
            }
        }

        Ok(())
    }
}