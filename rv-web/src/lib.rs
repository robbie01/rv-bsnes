use js_sys::{Array, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use web_sys::DedicatedWorkerGlobalScope;

use bumpalo::Bump;
use include_bytes_aligned::include_bytes_aligned;
use object::{LittleEndian, Object as _, ObjectSymbol as _, read::elf::ElfFile32};

use rv::{cpu::Cpu, interpreter::{Interpreter, linux::LinuxHypervisor}, instr::Register};

trait Context {
    type Ok;

    fn context(self, msg: &str) -> Result<Self::Ok, JsValue>;
}

impl<T> Context for Option<T> {
    type Ok = T;

    fn context(self, msg: &str) -> Result<Self::Ok, JsValue> {
        match self {
            Some(v) => Ok(v),
            None => Err(JsError::new(msg).into())
        }
    }
}

trait ToJsResult {
    type Ok;

    fn js(self) -> Result<Self::Ok, JsValue>;
}

impl<T, E: ToString> ToJsResult for Result<T, E> {
    type Ok = T;

    fn js(self) -> Result<T, JsValue> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(JsError::new(&e.to_string()).into())
        }
    }
}

fn post_log(global: &DedicatedWorkerGlobalScope, log: &JsValue) -> Result<(), JsValue> {
    let msg = Object::create(&JsValue::NULL.into());

    Reflect::set(&msg, &"type".into(), &"log".into())?;
    Reflect::set(&msg, &"msg".into(), log)?;

    global.post_message(&msg)?;

    Ok(())
}

macro_rules! eprintln {
    ($global:expr, $($body:tt)*) => {
        {
            let log = format!($($body)*).into();

            ::web_sys::console::log_1(&log);
            post_log(&$global, &log)
        }
    };
}

const PROGRAM: &[u8] = include_bytes_aligned!(16, "../../bsnes.elf");

#[wasm_bindgen]
pub fn worker_main(game: &[u8]) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let global = js_sys::global().dyn_into::<DedicatedWorkerGlobalScope>().map_err(|_| JsError::new("not in worker"))?;
    let performance = global.performance().ok_or_else(|| JsError::new("no performance"))?;

    let arena = Bump::new();

    let program = ElfFile32::<LittleEndian>::parse(PROGRAM).js()?;
    let mut h = LinuxHypervisor::default();
    let mut cpu = Interpreter::new(&arena);
    cpu.load(&mut h, &program).js()?;

    let init_array_start: u32 = program.symbol_by_name("__init_array_start").context("no __init_array_start")?.address().try_into().js()?;
    let init_array_end: u32 = program.symbol_by_name("__init_array_end").context("no __init_array_end")?.address().try_into().js()?;

    eprintln!(global, "calling initializers...")?;

    for addr in (init_array_start..init_array_end).step_by(4) {
        let ctor = cpu.load_u32(addr).js()?;
        cpu.call_subroutine(&mut h, ctor).js()?;
    }

    let game_addr = h.kmalloc(game.len().try_into().js()?).context("couldn't alloc space for game")?;
    cpu.store_slice(game_addr, game).js()?;

    let gameinfo: Vec<u8> = [
        game_addr,
        game.len().try_into().js()?,
        0,
        0xfffffffe,
        0xfffffffe,
        0xfffffffe,
        0xfffffffe
    ].into_iter().flat_map(u32::to_le_bytes).collect();
    let gameinfo_addr = h.kmalloc(gameinfo.len().try_into().js()?).context("couldn't alloc space for gameinfo")?;
    cpu.store_slice(gameinfo_addr, &gameinfo).js()?;

    let pathinfo: Vec<u8> = [
        0,
        0xfffffffe,
        0,
        0xfffffffe,
        0xfffffffe
    ].into_iter().flat_map(u32::to_le_bytes).collect();
    let pathinfo_addr = h.kmalloc(pathinfo.len().try_into().js()?).context("couldn't alloc space for pathinfo")?;
    cpu.store_slice(pathinfo_addr, &pathinfo).js()?;

    eprintln!(global, "calling jg_init...")?;
    cpu.call_subroutine_by_name(&mut h, "jg_init").js()?;

    eprintln!(global, "calling jg_set_cb_log...")?;
    cpu.write_x(Register::A0, 0xe0000000);
    cpu.call_subroutine_by_name(&mut h, "jg_set_cb_log").js()?;

    eprintln!(global, "calling jg_set_cb_frametime...")?;
    cpu.write_x(Register::A0, 0xe0000004);
    cpu.call_subroutine_by_name(&mut h, "jg_set_cb_frametime").js()?;

    eprintln!(global, "calling jg_set_gameinfo...")?;
    cpu.write_x(Register::A0, gameinfo_addr);
    cpu.call_subroutine_by_name(&mut h, "jg_set_gameinfo").js()?;

    eprintln!(global, "calling jg_set_paths...")?;
    cpu.write_x(Register::A0, pathinfo_addr);
    cpu.call_subroutine_by_name(&mut h, "jg_set_paths").js()?;

    let inputstate = h.kmalloc(16).context("couldn't alloc inputstate")?;
    let buttons = h.kmalloc(12).context("couldn't alloc buttons")?;
    cpu.store_u32(inputstate+4, buttons).js()?;
    eprintln!(global, "calling jg_set_inputstate...")?;
    cpu.write_x(Register::A0, inputstate);
    cpu.write_x(Register::A1, 0);
    cpu.call_subroutine_by_name(&mut h, "jg_set_inputstate").js()?;
    cpu.write_x(Register::A0, inputstate);
    cpu.write_x(Register::A1, 1);
    cpu.call_subroutine_by_name(&mut h, "jg_set_inputstate").js()?;

    let video_addr = h.kmalloc(4 * 253440).context("couldn't alloc vid buf")?;
    eprintln!(global, "calling jg_get_videoinfo...")?;
    cpu.call_subroutine_by_name(&mut h, "jg_get_videoinfo").js()?;
    let vidinfo_addr = cpu.read_x(Register::A0);
    cpu.store_u32(vidinfo_addr+40, video_addr).js()?;

    eprintln!(global, "calling jg_setup_video...")?;
    cpu.call_subroutine_by_name(&mut h, "jg_setup_video").js()?;

    eprintln!(global, "calling jg_game_load...")?;
    cpu.call_subroutine_by_name(&mut h, "jg_game_load").js()?;

    let mut data = Vec::with_capacity(4*512*224);

    for i in 0.. {
        eprintln!(global, "calling jg_exec_frame ({i})...")?;
        let t0 = performance.now();
        let n = cpu.call_subroutine_by_name(&mut h, "jg_exec_frame").js()?;
        let t = (performance.now() - t0) / 1000.;


        let w = cpu.load_u32(vidinfo_addr+0xc).js()?;
        let h = cpu.load_u32(vidinfo_addr+0x10).js()?;
        let p = cpu.load_u32(vidinfo_addr+0x1c).js()?;
        
        data.clear();
        for i in 0..(p*h) {
            let c = cpu.load_u32(video_addr+4*i).js()?;
            data.extend_from_slice(&c.to_be_bytes()[1..]);
            data.push(0xff);
        }

        let buf = Uint8Array::new_from_slice(&data).buffer();
        let msg = Object::create(&JsValue::NULL.into());

        Reflect::set(&msg, &"type".into(), &"frame".into())?;
        Reflect::set(&msg, &"data".into(), &buf)?;
        Reflect::set(&msg, &"width".into(), &w.into())?;
        Reflect::set(&msg, &"height".into(), &h.into())?;
        Reflect::set(&msg, &"pitch".into(), &p.into())?;
        Reflect::set(&msg, &"n_instructions".into(), &n.into())?;
        Reflect::set(&msg, &"time".into(), &t.into())?;

        let transfer = Array::new_typed();
        transfer.push(&buf);

        global.post_message_with_transfer(&msg, &transfer)?;
    }

    Ok(())
}
