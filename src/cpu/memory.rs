use std::{convert::Infallible, fmt::Debug, ops::{Index, IndexMut, Range}};

use anyhow::bail;

fn zeroed_array<const N: usize>() -> Box<[u8; N]> {
    unsafe { Box::new_zeroed().assume_init() }
}

fn zeroed_slice(n: usize) -> Box<[u8]> {
    unsafe { Box::new_zeroed_slice(n).assume_init() }
}

const TRAP: [u8; 4] = [
    0x02, 0x90, // c.ebreak
    0x82, 0x80  // c.ret
];

#[derive(Clone)]
pub struct Memory {
    program: Box<[u8]>, // starts at 0
    fun_area: Box<[u8; 256*1024*1024]> // 0xf0000000-END
}

impl Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("...")
    }
}

fn shift_range(r: &Range<u32>, off: u32) -> Option<Range<u32>> {
    Some(r.start.checked_sub(off)?..r.end.checked_sub(off)?)
}

impl Memory {
    pub fn new(program_size: usize) -> Self {
        Self {
            program: zeroed_slice(program_size),
            fun_area: zeroed_array(),
        }
    }

    fn get(&self, range: Range<u32>) -> Option<&[u8]> {
        if range.start < 0x10000 {
            None
        } else if let Some(o) = shift_range(&range, 0xf0000000) && o.end <= 0xfffffff {
            Some(&self.fun_area[o.start.try_into().ok()?..o.end.try_into().ok()?])
        } else if usize::try_from(range.end).ok()? < self.program.len() {
            Some(&self.program[range.start.try_into().ok()?..range.end.try_into().ok()?])
        } else if range.start >= 0xe0000000 && range.end < 0xf0000000 {
            // Callback
            let maxlen = 4 - (range.start % 4);
            if range.end - range.start > maxlen {
                return None
            }
            let len = range.end - range.start;
            let begin = (range.start % 4) as usize;
            Some(&TRAP[begin..begin + len as usize])
        } else {
            None
        }
    }

    /// Intentionally absent:
    /// - trap range (0xe0000000-0xefffffff)
    /// - zero range (0xfffffff1-0xffffffff)
    fn get_mut(&mut self, range: Range<u32>) -> Option<&mut [u8]> {
        if range.start < 0x10000 {
            None
        } else if let Some(o) = shift_range(&range, 0xf0000000) && o.end <= 0xffffff0 {
            Some(&mut self.fun_area[o.start.try_into().ok()?..o.end.try_into().ok()?])
        } else if usize::try_from(range.end).ok()? < self.program.len() {
            Some(&mut self.program[range.start.try_into().ok()?..range.end.try_into().ok()?])
        } else {
            None
        }
    }
}

impl Index<u32> for Memory {
    type Output = u8;

    fn index(&self, index: u32) -> &Self::Output {
        &self[index..index+1][0]
    }
}

impl IndexMut<u32> for Memory {
    fn index_mut(&mut self, index: u32) -> &mut Self::Output {
        &mut self[index..index+1][0]
    }
}

impl Index<Range<u32>> for Memory {
    type Output = [u8];

    fn index(&self, index: Range<u32>) -> &Self::Output {
        self.get(index).unwrap()
    }
}

impl IndexMut<Range<u32>> for Memory {
    fn index_mut(&mut self, index: Range<u32>) -> &mut Self::Output {
        self.get_mut(index).unwrap()
    }
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

    #[inline(always)]
    pub fn load_u32(&self, addr: u32) -> anyhow::Result<u32> {
        let Some(mem) = self.get(addr..addr+4) else { return self.oob::<false>(addr).map(|v| match v {}) };
        Ok(u32::from_le_bytes(mem.try_into()?))
    }

    #[inline(always)]
    pub fn load_u16(&self, addr: u32) -> anyhow::Result<u16> {
        let Some(mem) = self.get(addr..addr+2) else { return self.oob::<false>(addr).map(|v| match v {}) };
        Ok(u16::from_le_bytes(mem.try_into()?))
    }

    #[inline(always)]
    pub fn load_i16(&self, addr: u32) -> anyhow::Result<i16> {
        self.load_u16(addr).map(|v| v as i16)
    }

    #[inline(always)]
    pub fn load_u8(&self, addr: u32) -> anyhow::Result<u8> {
        let Some(mem) = self.get(addr..addr+1) else { return self.oob::<false>(addr).map(|v| match v {}) };
        Ok(mem[0])
    }

    #[inline(always)]
    pub fn load_i8(&self, addr: u32) -> anyhow::Result<i8> {
        self.load_u8(addr).map(|v| v as i8)
    }

    #[inline(always)]
    pub fn load_f32(&self, addr: u32) -> anyhow::Result<f32> {
        let Some(mem) = self.get(addr..addr+4) else { return self.oob::<false>(addr).map(|v| match v {}) };
        Ok(f32::from_le_bytes(mem.try_into()?))
    }

    #[inline(always)]
    pub fn load_f64(&self, addr: u32) -> anyhow::Result<f64> {
        let Some(mem) = self.get(addr..addr+8) else { return self.oob::<false>(addr).map(|v| match v {}) };
        Ok(f64::from_le_bytes(mem.try_into()?))
    }

    #[inline(always)]
    pub fn store_u32(&mut self, addr: u32, value: u32) -> anyhow::Result<()> {
        let Some(mem) = self.get_mut(addr..addr+4) else { return self.oob::<true>(addr).map(|v| match v {}) };
        mem.copy_from_slice(&value.to_le_bytes());

        Ok(())
    }

    #[inline(always)]
    pub fn store_u16(&mut self, addr: u32, value: u16) -> anyhow::Result<()> {
        let Some(mem) = self.get_mut(addr..addr+2) else { return self.oob::<true>(addr).map(|v| match v {}) };
        mem.copy_from_slice(&value.to_le_bytes());

        Ok(())
    }

    #[inline(always)]
    pub fn store_u8(&mut self, addr: u32, value: u8) -> anyhow::Result<()> {
        let Some(mem) = self.get_mut(addr..addr+1) else { return self.oob::<true>(addr).map(|v| match v {}) };
        mem.copy_from_slice(&[value]);

        Ok(())
    }

    #[inline(always)]
    pub fn store_f32(&mut self, addr: u32, value: f32) -> anyhow::Result<()> {
        let Some(mem) = self.get_mut(addr..addr+4) else { return self.oob::<true>(addr).map(|v| match v {}) };
        mem.copy_from_slice(&value.to_le_bytes());

        Ok(())
    }

    #[inline(always)]
    pub fn store_f64(&mut self, addr: u32, value: f64) -> anyhow::Result<()> {
        let Some(mem) = self.get_mut(addr..addr+8) else { return self.oob::<true>(addr).map(|v| match v {}) };
        mem.copy_from_slice(&value.to_le_bytes());

        Ok(())
    }
}