use std::{fmt::Debug, ops::{Index, IndexMut, Range}};

fn zeroed_array<const N: usize>() -> Box<[u8; N]> {
    unsafe { Box::new_zeroed().assume_init() }
}

fn zeroed_slice(n: usize) -> Box<[u8]> {
    unsafe { Box::new_zeroed_slice(n).assume_init() }
}

#[derive(Clone)]
pub struct Memory {
    program: Box<[u8]>, // starts at 0
    fun_area: Box<[u8; 256*1024*1024]>, // 0xf0000000-END

    mmap_bottom: usize,
    kmalloc_top: usize
}

impl Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("...")
    }
}

pub const BEGINNING_STACK_TOP: u32 = 0xfffffff0;
pub const TP: u32 = 0xff000000;

fn shift_range(r: &Range<usize>, off: usize) -> Option<Range<usize>> {
    Some(r.start.checked_sub(off)?..r.end.checked_sub(off)?)
}

impl Memory {
    pub fn new(program_size: usize) -> Self {
        Self {
            program: zeroed_slice(program_size),
            fun_area: zeroed_array(),

            mmap_bottom: 0xfe000000,
            kmalloc_top: 0xf0000000
        }
    }

    pub fn get(&self, range: Range<usize>) -> Option<&[u8]> {
        if range.start < 0x10000 {
            None
        } else if let Some(o) = shift_range(&range, 0xf0000000) && o.end <= 0xfffffff {
            Some(&self.fun_area[o])
        } else if range.end < self.program.len() {
            Some(&self.program[range])
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, range: Range<usize>) -> Option<&mut [u8]> {
        if range.start < 0x10000 {
            None
        } else if let Some(o) = shift_range(&range, 0xf0000000) && o.end <= 0xfffffff {
            Some(&mut self.fun_area[o])
        } else if range.end < self.program.len() {
            Some(&mut self.program[range])
        } else {
            None
        }
    }

    #[expect(dead_code)]
    pub fn put_u32(&mut self, addr: usize, val: u32) {
        self[addr..addr+4].copy_from_slice(&val.to_le_bytes());
    }

    pub fn mmap_anon(&mut self, size: usize) -> Option<usize> {
        let size = size.next_multiple_of(4096);
        let new_bottom = self.mmap_bottom.checked_sub(size)?;
        if new_bottom < 0xf1000000 {
            None
        } else {
            self.mmap_bottom = new_bottom;
            Some(new_bottom)
        }
    }

    pub fn kmalloc(&mut self, size: usize) -> Option<usize> {
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
}

impl Index<usize> for Memory {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self[index..index+1][0]
    }
}

impl IndexMut<usize> for Memory {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self[index..index+1][0]
    }
}

impl Index<Range<usize>> for Memory {
    type Output = [u8];

    fn index(&self, index: Range<usize>) -> &Self::Output {
        self.get(index).unwrap()
    }
}

impl IndexMut<Range<usize>> for Memory {
    fn index_mut(&mut self, index: Range<usize>) -> &mut Self::Output {
        self.get_mut(index).unwrap()
    }
}