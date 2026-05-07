#![expect(incomplete_features)]
#![feature(adt_const_params)]
#![feature(explicit_tail_calls)]

pub mod instr;
pub mod interpreter;
mod fs;
pub mod cpu;