use anyhow::{Context as _, anyhow, ensure};
use object::{Architecture, LittleEndian, Object as _, ObjectSection, ObjectSymbol as _, elf::SHF_ALLOC, read::elf::ElfFile32};

use crate::instr::{I12, Instruction, JumpAndLink, JumpAndLinkRegister, Register};

#[derive(Default, Debug)]
pub struct LinuxHypervisor<'data> {
    image: Option<&'data ElfFile32<'data, LittleEndian>>,

    bss: u32,
    brk: u32,
    stack: Vec<u32>,
    breakpoint_reached: bool
}

// TODO: synthesize __init_libc (https://github.com/kraj/musl/blob/kraj/master/src/env/__libc_start_main.c)
// (this initializes auxv)

impl<'data> LinuxHypervisor<'data> {
    // __init_libc
    fn init_libc(&mut self, ctx: &mut super::Cpu<Self>) -> anyhow::Result<()> {
        let libc: u32 = self.image.as_ref().unwrap().symbol_by_name("__libc").context("no __libc")?.address().try_into()?;

        let auxv_addr = ctx.memory.kmalloc(8).context("couldn't alloc auxv")?;
        ctx.store_u32(libc.checked_add(8).context("bad __libc")?, auxv_addr as u32)?;

        // page_size
        ctx.store_u32(libc.checked_add(0x1c).context("bad __libc")?, 4096)?;

        Ok(())
    }
}

impl<'data> super::Hypervisor<'data> for LinuxHypervisor<'data> {
    fn load<'this>(&'this mut self, ctx: &mut super::Cpu<Self>, obj: &'data ElfFile32<'data, LittleEndian>) -> anyhow::Result<()> where 'data: 'this {
        ensure!(self.image.is_none());
        ensure!(obj.architecture() == Architecture::Riscv32);

        self.image = Some(obj);

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

        self.init_libc(ctx)?;

        Ok(())
    }

    #[inline(always)]
    fn before_instr(&mut self, ctx: &mut super::Cpu<Self>, instr: Instruction) -> anyhow::Result<()> {
        if ctx.pc == 0x60550e {
            self.breakpoint_reached = true;
        }
        if self.breakpoint_reached {
            print!("{:X}: {instr:?}", ctx.pc);
        }

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
    fn after_instr(&mut self, ctx: &mut super::Cpu<Self>, instr: Instruction) -> anyhow::Result<()> {
        use crate::instr::{Instruction as I, *};

        if self.breakpoint_reached {
            match instr {
                I::Int(Int { dest, .. }) | I::IntImmediate(IntImmediate { dest, .. }) | I::LoadInt(LoadInt { dest, .. }) | I::U(U { dest, .. }) =>
                    println!(" x{} => {}", u8::from(dest), ctx.read_x(dest)),
                _ => println!()
            }
        }
        Ok(())
    }

    #[inline(always)]
    fn ebreak(&mut self, _ctx: &mut super::Cpu<Self>) -> anyhow::Result<()> {
        Err(anyhow!("EBREAK reached\nStack trace: {:X?}", self.stack))
    }

    #[inline(always)]
    fn ecall(&mut self, ctx: &mut super::Cpu<Self>) -> anyhow::Result<()> {
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
                let _prot = ctx.read_x(Register::A2);
                let flags = ctx.read_x(Register::A3);
                let _fd = ctx.read_x(Register::A4) as i32;

                let ret = if length == 0 {
                    -22i32 as u32 // EINVAL
                } else if flags & 0x20 == 0 { // MAP_ANONYMOUS
                    -9i32 as u32 // EBADF
                } else if flags & 0x10 != 0 { // MAP_FIXED
                    -12i32 as u32 // ENOMEM
                } else {
                    let alloc = ctx.memory.mmap_anon(length.try_into()?);
                    match alloc {
                        Some(addr) => addr.try_into()?,
                        None => -12i32 as u32 // ENOMEM
                    }
                };

                let neg = (-4095..=-1).contains(&(ret as i32));
                let sign = if neg { "-" } else { "" };

                if neg { println!("\nstack: {:X?}", self.stack) }
                println!("{:06X}: mmap(0x{_addr:X}, {length}, {_prot:b}, 0x{flags:X}, {_fd}) = {sign}0x{:X}", ctx.pc-4, if neg { -(ret as i32) } else { ret as i32 });

                ctx.write_x(Register::A0, ret);
            },
            135 => { // rt_sigprocmask
                ctx.write_x(Register::A0, 0);
            }
            whence => {
                println!("{:06X}: ECALL {whence} ({:X})", ctx.pc-4, ctx.read_x(Register::A0));
                println!("{:X?}", self.stack);
                println!();
                ctx.write_x(Register::A0, u32::MAX); // no errno LOL
            }
        }

        Ok(())
    }
}