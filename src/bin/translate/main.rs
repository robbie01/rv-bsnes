mod decode;
mod emit;
use {decode::*, emit::*};

use std::{collections::BTreeMap, fs};

use anyhow::Context;
use object::{LittleEndian, Object, read::elf::ElfFile32};
use wasm_encoder::{ValType::{self, *}, *};

// prologue
fn begin_block() -> Function {
    Function::new([
        (31, I32), // x1-x31
        (32, F64)  // f0-f31
    ])
}

const REGISTER_FILE: [ValType; 63] = [
    I32, I32, I32,
    I32, I32, I32, I32,
    I32, I32, I32, I32,
    I32, I32, I32, I32,
    I32, I32, I32, I32,
    I32, I32, I32, I32,
    I32, I32, I32, I32,
    I32, I32, I32, I32,
    F64, F64, F64, F64,
    F64, F64, F64, F64,
    F64, F64, F64, F64,
    F64, F64, F64, F64,
    F64, F64, F64, F64,
    F64, F64, F64, F64,
    F64, F64, F64, F64,
    F64, F64, F64, F64,
];

fn main() -> anyhow::Result<()> {
    let program = fs::read("bsnes.elf")?;
    let image = ElfFile32::<LittleEndian>::parse(&program)?;
    let text = image.section_by_name(".text").context("no .text")?;
    let blocks = discover_blocks(&text)?;

    let mut module = Module::new();

    {
        let mut type_section = TypeSection::new();
        // 0: basic block
        type_section.ty().function(REGISTER_FILE, REGISTER_FILE);
        module.section(&type_section);
    }

    let n_imports = {
        let mut import_section = ImportSection::new();
        import_section.import("sys", "ebreak", EntityType::Function(0));
        import_section.import("sys", "ecall", EntityType::Function(0));
        module.section(&import_section);
        import_section.len()
    };

    {
        let mut func_section = FunctionSection::new();
        for _ in 0..blocks.len()+1 {
            func_section.function(0);
        }
        module.section(&func_section);
    }
    
    let indices = blocks.keys().enumerate().map(|(idx, &addr)| (addr, idx as u32 + n_imports + 1)).collect::<BTreeMap<_, _>>();

    let first_addr = *blocks.first_key_value().unwrap().0;
    assert_eq!(first_addr, 0x600000);
    let last_addr = *blocks.last_key_value().unwrap().0;

    let table_size = ((last_addr >> 1) - (first_addr >> 1) + 1).into();

    {
        let mut table_section = TableSection::new();
        table_section.table(TableType {
            element_type: RefType::FUNCREF,
            table64: false,
            minimum: table_size,
            maximum: Some(table_size),
            shared: false
        });
        module.section(&table_section);
    }

    {
        let mut memory_section = MemorySection::new();
        memory_section.memory(MemoryType {
            minimum: 0,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None
        });
        module.section(&memory_section);
    }

    {
        let mut elem_section = ElementSection::new();
        let mut elems = vec![2; table_size as usize];
        for (&addr, &func) in &indices {
            let ptr = &mut elems[((addr - first_addr) >> 1) as usize];
            assert_eq!(*ptr, 2);
            *ptr = func;
        }
        elem_section.active(None, &ConstExpr::i32_const(0), Elements::Functions(elems.into()));
        module.section(&elem_section);
    }

    let mut code_section = CodeSection::new();
    {
        let mut func = begin_block();
        func.instructions()
            .unreachable()
            .end();
        code_section.function(&func);
    }
    for Block { body, term } in blocks.into_values() {
        let mut func = begin_block();
        let mut instrs = func.instructions();
        for (pc, instr) in body {
            emit_instruction(&mut instrs, pc, instr);
        }
        emit_epilogue(&mut instrs, &indices, term);
        code_section.function(&func);
    }
    module.section(&code_section);

    let module = module.finish();
    fs::write("bsnes.wasm", module)?;

    Ok(())
}