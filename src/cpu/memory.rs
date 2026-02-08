use std::{cell::UnsafeCell, convert::Infallible, fmt::Debug, mem::MaybeUninit, rc::Rc};

use anyhow::{bail, ensure};

const TRAP: [u8; 4] = [
    0x02, 0x90, // c.ebreak
    0x82, 0x80  // c.ret
];

struct TlbEntry {
    buf: Rc<UnsafeCell<[u8]>>,
    offset: usize
}

pub struct Memory {
    tlb: Box<[Option<TlbEntry>]> // 0x10000
}

impl Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("...")
    }
}

fn zeroed_buffer(len: usize) -> Rc<UnsafeCell<[u8]>> {
    let buf = Rc::<[u8]>::new_uninit_slice(len);
    unsafe { Rc::from_raw(Rc::into_raw(buf) as *const UnsafeCell<[u8]>) }
}

impl Memory {
    pub fn new() -> Self {
        let mut tlb = Box::new_uninit_slice(0x100000);
        tlb.fill_with(|| MaybeUninit::new(None));
        let tlb = unsafe { tlb.assume_init() };
        
        let mut this = Self {
            tlb
        };

        this.mount(zeroed_buffer(0x10000000 - 0x10000), 0x10000).unwrap();
        this.mount(zeroed_buffer(0x10000000), 0xf0000000).unwrap();

        let mut trap_buf = zeroed_buffer(0x1000);
        {
            let (chunks, []) = Rc::get_mut(&mut trap_buf).unwrap().get_mut().as_chunks_mut() else { unreachable!() };
            chunks.fill(TRAP);
        }
        this.mount(trap_buf, 0xe0000000).unwrap();

        this
    }

    fn mount(&mut self, buf: Rc<UnsafeCell<[u8]>>, addr: u32) -> anyhow::Result<()> {
        ensure!(buf.get().len() & 0xfff == 0);
        ensure!(addr & 0xfff == 0);
        let page1 = (addr >> 12) as usize;
        let npages = buf.get().len() >> 12;
        ensure!((npages + page1 as usize) <= 0x100000);

        for (i, ent) in self.tlb[page1..page1+npages].iter_mut().enumerate() {
            ensure!(ent.is_none());

            *ent = Some(TlbEntry {
                buf: buf.clone(),
                offset: i * 0x1000
            });
        }
        Ok(())
    }

    fn translate(&self, addr: u32) -> Option<*mut u8> {
        let off = (addr & 0xfff) as usize;
        let page = (addr >> 12) as usize;
        let entry = self.tlb[page].as_ref()?;
        Some(unsafe { (entry.buf.get() as *mut u8).add(entry.offset + off) })
    }
}

macro_rules! impl_load {
    ($($type:ty),+) => {
        ::paste::paste! {
            $(
                #[inline(always)]
                pub fn [<load_ $type>](&self, addr: u32) -> anyhow::Result<$type> {
                    #[cold]
                    #[inline(never)]
                    fn [<load_ $type _slow>](this: &Memory, addr: u32) -> anyhow::Result<$type> {
                        let mut buf = [0; ::std::mem::size_of::<$type>()];
                        for (i, v) in (addr..).zip(&mut buf) {
                            *v = this.load_u8(i)?;
                        }
                        Ok($type::from_le_bytes(buf))
                    }

                    if addr & 0xfff < (0x1000 - ::std::mem::size_of::<$type>() as u32 + 1) && let Some(pa) = self.translate(addr) {
                        return Ok(unsafe { (pa as *const $type).read_unaligned() })
                    }

                    become [<load_ $type _slow>](&self, addr)
                }
            )+
        }
    };
}

macro_rules! impl_store {
    ($($type:ty),+) => {
        ::paste::paste! {
            $(
                #[inline(always)]
                pub fn [<store_ $type>](&mut self, addr: u32, value: $type) -> anyhow::Result<()> {
                    #[cold]
                    #[inline(never)]
                    fn [<store_ $type _slow>](this: &mut Memory, addr: u32, value: $type) -> anyhow::Result<()> {
                        for (i, v) in (addr..).zip(value.to_le_bytes()) {
                            this.store_u8(i, v)?
                        }

                        Ok(())
                    }

                    if addr & 0xfff < (0x1000 - std::mem::size_of::<$type>() as u32 + 1) && let Some(pa) = self.translate(addr) {
                        unsafe { (pa as *mut $type).write_unaligned(value) }
                        return Ok(())
                    }

                    become [<store_ $type _slow>](self, addr, value)
                }
            )+
        }
    };
}

impl Memory {
    #[cold]
    #[inline(never)]
    fn oob<const STORE: bool>(&self, addr: u32) -> anyhow::Result<Infallible> {
        if STORE {
            bail!("oob store @ {addr:06X}")
        } else {
            bail!("oob load @ {addr:06X}")
        }
    }

    impl_load! { u32, u16, i16, f64, f32 }
    impl_store! { u32, u16, f64, f32 }

    #[inline(always)]
    pub fn load_u8(&self, addr: u32) -> anyhow::Result<u8> {
        #[cold]
        #[inline(never)]
        fn load_u8_slow(this: &Memory, addr: u32) -> anyhow::Result<u8> {
            this.oob::<false>(addr).map(|v| match v {})
        }

        if let Some(pa) = self.translate(addr) {
            return Ok(unsafe { pa.read_unaligned() })
        }

        become load_u8_slow(&self, addr)
    }

    #[inline(always)]
    pub fn load_i8(&self, addr: u32) -> anyhow::Result<i8> {
        #[cold]
        #[inline(never)]
        fn load_i8_slow(this: &Memory, addr: u32) -> anyhow::Result<i8> {
            this.oob::<false>(addr).map(|v| match v {})
        }

        if let Some(pa) = self.translate(addr) {
            return Ok(unsafe { (pa as *const i8).read() })
        }

        become load_i8_slow(&self, addr)
    }

    #[inline(always)]
    pub fn store_u8(&mut self, addr: u32, value: u8) -> anyhow::Result<()> {
        #[cold]
        #[inline(never)]
        fn store_u8_slow(this: &mut Memory, addr: u32, _value: u8) -> anyhow::Result<()> {
            this.oob::<true>(addr).map(|v| match v {})
        }

        if let Some(pa) = self.translate(addr) {
            unsafe { pa.write_unaligned(value) }
            return Ok(())
        }

        become store_u8_slow(self, addr, value)
    }
}