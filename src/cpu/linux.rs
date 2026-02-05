use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, anyhow, bail, ensure};
use object::{Architecture, LittleEndian, Object as _, ObjectSection, ObjectSymbol as _, elf::SHF_ALLOC, read::elf::ElfFile32};
use stable_vec::ExternStableVec;

use crate::{fs::FILES, instr::{I12, Instruction, JumpAndLink, JumpAndLinkRegister, Register}};

#[derive(Debug)]
struct FileDescriptor {
    data: &'static [u8],
    pos: usize
}

#[derive(Default, Debug)]
pub struct LinuxHypervisor<'data> {
    image: Option<&'data ElfFile32<'data, LittleEndian>>,
    fds: ExternStableVec<FileDescriptor>,

    bss: u32,
    brk: u32,
    stack: Vec<u32>,
    breakpoint_reached: bool
}

// TODO: synthesize __init_libc (https://github.com/kraj/musl/blob/kraj/master/src/env/__libc_start_main.c)
// (this initializes auxv)

const ERROR_ROUTINES: [u32; 2] = [
    0x6001cc, // std::__throw_logic_error
    // 0x6cb75c, // __cxxabiv1::__cxa_allocate_exception
    0x6cc2a6, // __cxxabiv1::__cxa_throw
];

const TCB_SIZE: u32 = 0x70;

impl<'data> LinuxHypervisor<'data> {
    // __init_libc
    fn init_libc(&mut self, ctx: &mut super::Cpu) -> anyhow::Result<()> {
        let image = self.image.as_ref().unwrap();
        let libc: u32 = image.symbol_by_name("__libc").context("no __libc")?.address().try_into()?;

        let auxv_addr = ctx.memory.kmalloc(8).context("couldn't alloc auxv")?;
        ctx.store_u32(libc.checked_add(8).context("bad __libc")?, auxv_addr)?;

        // page_size
        ctx.store_u32(libc.checked_add(0x1c).context("bad __libc")?, 4096)?;

        let tp = ctx.memory.kmalloc(TCB_SIZE).context("couldn't allocate tcb")? + TCB_SIZE;
        ctx.write_x(Register::TP, tp);

        let utf8_locale: u32 = image.symbol_by_name("__c_dot_utf8_locale").context("no __c_dot_utf8_locale")?.address().try_into()?;
        ctx.store_u32(tp - 0x18, utf8_locale)?;

        Ok(())
    }
}

impl<'data> super::Hypervisor<'data> for LinuxHypervisor<'data> {
    type Object = ElfFile32<'data, LittleEndian>;

    fn load<'this>(&'this mut self, ctx: &mut super::Cpu, obj: &'data ElfFile32<'data, LittleEndian>) -> anyhow::Result<()> where 'data: 'this {
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

            let addr = u32::try_from(section.address())?;
            let data = section.data()?;
            ctx.store_slice(addr, data)?;
        }

        self.init_libc(ctx)?;

        Ok(())
    }

    fn symbol(&self, sym: &str) -> anyhow::Result<u32> {
        Ok(self.image.as_ref().context("no image")?
            .symbol_by_name(sym).context("sym not found")?
            .address().try_into()?)
    }

    #[inline(always)]
    fn before_instr(&mut self, ctx: &mut super::Cpu, instr: &Instruction) -> anyhow::Result<()> {
        if ERROR_ROUTINES.contains(&ctx.pc) {
            bail!("error routine reached\nstack: {:X?}", self.stack);
        }

        if false {
            // if ctx.pc == 0x623f6a { // co_swap
            //     println!("context switch!");
            // }
            if self.breakpoint_reached {
                print!("\na0 = {:X}\nstack: {:X?}\n{:X}: {instr:?}", ctx.read_x(Register::A0), self.stack, ctx.pc);
            }

            match instr {
                Instruction::JumpAndLink(JumpAndLink { dest: Register::RA, .. }) | Instruction::JumpAndLinkRegister(JumpAndLinkRegister { dest: Register::RA, .. }) =>
                    { self.stack.push(ctx.pc); },
                Instruction::JumpAndLinkRegister(JumpAndLinkRegister { dest: Register::ZERO, base: Register::RA, offset: I12::ZERO }) =>
                    { self.stack.pop(); },
                _ => ()
            }
        }
        Ok(())
    }

    #[inline(always)]
    fn after_instr(&mut self, ctx: &mut super::Cpu, instr: &Instruction) -> anyhow::Result<()> {
        use crate::instr::{Instruction as I, *};

        if false && self.breakpoint_reached {
            match *instr {
                I::Int(Int { dest, .. }) | I::IntImmediate(IntImmediate { dest, .. }) | I::LoadInt(LoadInt { dest, .. }) | I::U(U { dest, .. }) =>
                    println!(" x{} => {}", u8::from(dest), ctx.read_x(dest)),
                _ => println!()
            }
        }
        Ok(())
    }

    #[inline(always)]
    fn ebreak(&mut self, ctx: &mut super::Cpu) -> anyhow::Result<()> {
        match ctx.pc {
            0xe0000002 => { // jg_cb_log
                let addr = ctx.read_x(Register::A1);
                let mut msg = ctx.load_string(addr)?;
                if msg.contains("%s") {
                    let addr = ctx.read_x(Register::A2);

                    msg = msg.replacen("%s", &ctx.load_string(addr)?, 1);
                }
                eprint!("jg: {msg}");
                Ok(())
            },
            0xe0000006 => { // jg_cb_frametime
                let frametime = ctx.read_f(Register::A0).read_f64();

                eprintln!("frametime = {frametime}");
                Ok(())
            }
            _ => Err(anyhow!("EBREAK reached\npc = {:X}\nStack trace: {:X?}", ctx.pc, self.stack))
        }
    }

    #[inline(always)]
    fn ecall(&mut self, ctx: &mut super::Cpu) -> anyhow::Result<()> {
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
                    let alloc = ctx.memory.mmap_anon(length);
                    match alloc {
                        Some(addr) => addr,
                        None => -12i32 as u32 // ENOMEM
                    }
                };

                // let neg = (-4095..=-1).contains(&(ret as i32));
                // let sign = if neg { "-" } else { "" };

                // if neg { println!("\nstack: {:X?}", self.stack) }
                // println!("{:06X}: mmap(0x{_addr:X}, {length}, 0b{_prot:03b}, 0x{flags:X}, {_fd}) = {sign}0x{:X}", ctx.pc-4, if neg { -(ret as i32) } else { ret as i32 });

                ctx.write_x(Register::A0, ret);
            },
            135 | 215 | 233 => { // rt_sigprocmask | munmap | madvise
                ctx.write_x(Register::A0, 0);
            }
            403 => { // clock_gettime
                let _clockid = ctx.read_x(Register::A0);
                let timespec = ctx.read_x(Register::A1);

                let time = SystemTime::now().duration_since(UNIX_EPOCH)?;

                /*
                 * struct __kernel_timespec {
                 *     __kernel_time64_t tv_sec;
                 *     long long         tv_nsec;
                 * };
                 */
                
                ctx.store_u32(timespec, time.as_secs() as u32)?;
                ctx.store_u32(timespec+4, (time.as_secs() >> 32) as u32)?;
                ctx.store_u32(timespec+8, time.subsec_nanos())?;
                ctx.store_u32(timespec+12, 0)?;
                
                ctx.write_x(Register::A0, 0);
            }
            56 => { // openat
                let dirfd = ctx.read_x(Register::A0) as i32;
                let path = ctx.read_x(Register::A1);
                let path = ctx.load_string(path)?;

                // println!("{:06X}: openat({dirfd}, {path:?}, ...)", ctx.pc-4);

                let ret = if dirfd != -100 {
                    -9i32 as u32 // EBADF
                } else if let Some(data) = FILES.get(&path[..]) {
                    ensure!(self.fds.next_push_index() <= i32::MAX as usize);
                    let fd = self.fds.push(FileDescriptor { data, pos: 0 });
                    fd as u32
                } else {
                    -2i32 as u32 // ENOENT
                };
                ctx.write_x(Register::A0, ret);
            }
            57 => { // close
                let fd = ctx.read_x(Register::A0) as usize;

                let res = if self.fds.remove(fd).is_some() {
                    0
                } else {
                    -9i32 as u32 // EBADF
                };
                ctx.write_x(Register::A0, res);
            }
            63 => { // read
                let fd = ctx.read_x(Register::A0) as usize;
                let buf = ctx.read_x(Register::A1);
                let count = ctx.read_x(Register::A2);

                let res = if let Some(desc) = self.fds.get_mut(fd) {
                    let mut n = 0;
                    for addr in buf..buf+count {
                        let Some(&c) = desc.data.get(desc.pos) else { break };
                        ctx.store_u8(addr, c)?;
                        desc.pos += 1;
                        n += 1;
                    }
                    n
                } else {
                    -9i32 as u32 // EBADF
                };
                ctx.write_x(Register::A0, res);
            }
            whence => {
                println!("\nstack: {:X?}\n{:06X}: ECALL {whence} ({:X})", self.stack, ctx.pc-4, ctx.read_x(Register::A0));
                ctx.write_x(Register::A0, u32::MAX); // no errno LOL
            }
        }

        Ok(())
    }
}