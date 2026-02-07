use std::{fmt::Debug, ops::{Index, IndexMut, Range}};

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

pub const BEGINNING_STACK_TOP: u32 = 0xfffffff0;

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

    pub fn get(&self, range: Range<u32>) -> Option<&[u8]> {
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
    pub fn get_mut(&mut self, range: Range<u32>) -> Option<&mut [u8]> {
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