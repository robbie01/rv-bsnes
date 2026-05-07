use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, anyhow, bail, ensure};
use object::{Architecture, LittleEndian, Object as _, ObjectSection, ObjectSymbol as _, elf::SHF_ALLOC, read::elf::ElfFile32};
use stable_vec::StableVec;

use crate::{cpu::*, interpreter::memory::PAGE_SIZE, fs::FILES, instr::Register};

#[derive(Debug)]
struct FileDescriptor {
    data: &'static [u8],
    pos: usize
}

#[derive(Debug)]
pub struct LinuxHypervisor<'data> {
    image: Option<&'data ElfFile32<'data, LittleEndian>>,
    fds: StableVec<FileDescriptor>,

    bss: u32,
    brk: u32,

    mmap_bottom: u32,
    kmalloc_top: u32
}

impl<'data> Default for LinuxHypervisor<'data> {
    fn default() -> Self {
        Self {
            image: None,
            fds: StableVec::new(),
            bss: 0,
            brk: 0,

            mmap_bottom: 0xff000000,
            kmalloc_top: 0xf0000000
        }
    }
}

const ERROR_ROUTINES: [u32; 2] = [
    0x6001cc, // std::__throw_logic_error
    // 0x6cb75c, // __cxxabiv1::__cxa_allocate_exception
    0x6cc2a6, // __cxxabiv1::__cxa_throw
];

const TCB_SIZE: u32 = 0x70;

impl<'data> LinuxHypervisor<'data> {
    pub fn kmalloc(&mut self, size: u32) -> Option<u32> {
        let size = size.next_multiple_of(4);
        let new_top = self.kmalloc_top.checked_add(size)?;
        if new_top > 0xf1000000 {
            None
        } else {
            let addr = self.kmalloc_top;
            self.kmalloc_top = new_top;
            Some(addr)
        }
    }

    // __init_libc
    fn init_libc(&mut self, ctx: &mut impl Cpu<H = Self>) -> anyhow::Result<()> {
        let libc: u32 = self.image.as_ref().unwrap().symbol_by_name("__libc").context("no __libc")?.address().try_into()?;

        let auxv_addr = self.kmalloc(8).context("couldn't alloc auxv")?;
        ctx.store_u32(libc.checked_add(8).context("bad __libc")?, auxv_addr)?;

        // page_size
        ctx.store_u32(libc.checked_add(0x1c).context("bad __libc")?, PAGE_SIZE)?;

        let tp = self.kmalloc(TCB_SIZE).context("couldn't allocate tcb")? + TCB_SIZE;
        ctx.write_x(Register::TP, tp);

        let utf8_locale: u32 = self.image.as_ref().unwrap().symbol_by_name("__c_dot_utf8_locale").context("no __c_dot_utf8_locale")?.address().try_into()?;
        ctx.store_u32(tp - 0x18, utf8_locale)?;

        Ok(())
    }
}

impl<'data> LoadableHypervisor<'data> for LinuxHypervisor<'data> {
    type Object = ElfFile32<'data, LittleEndian>;

    fn load<'this, C: Cpu<H = Self> + 'this>(&'this mut self, ctx: &mut C, obj: &'data ElfFile32<'data, LittleEndian>) -> anyhow::Result<()> where 'data: 'this {
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
}

impl<'data> Hypervisor for LinuxHypervisor<'data> {
    fn symbol(&self, sym: &str) -> anyhow::Result<u32> {
        Ok(self.image.as_ref().context("no image")?
            .symbol_by_name(sym).context("sym not found")?
            .address().try_into()?)
    }

    #[inline(always)]
    fn before_block(&mut self, ctx: &mut impl Cpu<H = Self>) -> anyhow::Result<()> {
        if false && ctx.pc().is_some_and(|pc| ERROR_ROUTINES.contains(&pc)) {
            bail!("error routine reached");
        }

        Ok(())
    }

    #[inline(always)]
    fn after_block(&mut self, _ctx: &mut impl Cpu<H = Self>) -> anyhow::Result<()> {
        Ok(())
    }

    #[inline(never)]
    fn ebreak(&mut self, ctx: &mut impl Cpu<H = Self>) -> anyhow::Result<()> {
        match ctx.pc() {
            Some(0xe0000002) => { // jg_cb_log
                let addr = ctx.read_x(Register::A1);
                let mut msg = ctx.load_string(addr)?;
                if msg.contains("%s") {
                    let addr = ctx.read_x(Register::A2);

                    msg = msg.replacen("%s", &ctx.load_string(addr)?, 1);
                }
                eprint!("jg: {msg}");
                Ok(())
            },
            Some(0xe0000006) => { // jg_cb_frametime
                let frametime = ctx.read_f(Register::A0).read_f64();

                eprintln!("frametime = {frametime}");
                Ok(())
            }
            _ => Err(anyhow!("EBREAK reached\npc = {:X?}", ctx.pc()))
        }
    }

    #[inline(never)]
    fn ecall(&mut self, ctx: &mut impl Cpu<H = Self>) -> anyhow::Result<()> {
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
                    let alloc = {
                        let size = length.next_multiple_of(PAGE_SIZE);
                        let new_bottom = self.mmap_bottom.saturating_sub(size);
                        if new_bottom < 0xf1000000 {
                            None
                        } else {
                            self.mmap_bottom = new_bottom;
                            Some(new_bottom)
                        }
                    };
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

                cfg_if::cfg_if! {
                    if #[cfg(all(target_family = "wasm", target_os = "unknown"))] {
                        let time = Duration::ZERO;
                    } else {
                        let time = SystemTime::now().duration_since(UNIX_EPOCH)?;
                    }
                };

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
                println!("\n{:06X?}: ECALL {whence} ({:X})", ctx.pc().map(|pc| pc-4), ctx.read_x(Register::A0));
                ctx.write_x(Register::A0, u32::MAX); // no errno LOL
            }
        }

        Ok(())
    }
}