#![allow(non_snake_case)]

use std::sync::Mutex;
use polkavm::{ArcBytes, Engine, InterruptKind, Module, ModuleConfig, ProgramBlob, ProgramCounter, RawInstance, Reg};
use polkavm_common::program::ProgramParts;
use wasm_bindgen::prelude::wasm_bindgen;

#[repr(C)]
#[wasm_bindgen]
#[derive(Copy, Clone, Debug)]
pub enum Status {
    Ok = 255,
    Halt = 0,
    Panic = 1,
    Fault = 2,
    Host = 3,
    OutOfGas = 4,
}

static PVM: Mutex<Option<RawInstance>> = Mutex::new(None);
static STATUS: Mutex<Status> = Mutex::new(Status::Ok);
static EXIT_ARG: Mutex<u32> = Mutex::new(0);

const NO_OF_REGISTERS: usize = 13;
const BYTES_PER_REG: usize = 8;

const PAGE_SIZE: usize = 4_096;

fn with_pvm<F, R>(f: F, default: R) -> R where F: FnMut(&mut RawInstance) -> R {
    let pvm_l = PVM.lock();
    if let Ok(mut pvm_l) = pvm_l {
        pvm_l.as_mut().map(f).unwrap_or(default)
    } else {
        default
    }
}

#[deprecated = "Use setGasLeft / setNextProgramCounter instead."]
#[wasm_bindgen]
pub fn resume(pc: u32, gas: i64) {
    with_pvm(|pvm| {
        pvm.set_gas(gas);
        pvm.set_next_program_counter(ProgramCounter(pc));
    }, ());
}

#[deprecated = "Use resetGeneric instead"]
#[wasm_bindgen]
pub fn reset(program: Vec<u8>, registers: Vec<u8>, gas: i64) {
    resetGeneric(
        program,
        registers,
        gas,
    )
}

#[wasm_bindgen]
pub fn resetGeneric(
    program: Vec<u8>,
    registers: Vec<u8>,
    gas: i64,
) {
    resetGenericWithMemory(program, registers, vec![], vec![], gas);
}

#[wasm_bindgen]
pub fn resetGenericWithMemory(
    program: Vec<u8>,
    registers: Vec<u8>,
    page_map: Vec<u8>,
    chunks: Vec<u8>,
    gas: i64,
) {
    let mut config = polkavm::Config::new();
    config.set_backend(Some(polkavm::BackendKind::Interpreter));

    let engine = Engine::new(&config).unwrap();
    let mut module_config = ModuleConfig::default();
    module_config.set_strict(true);
    module_config.set_gas_metering(Some(polkavm::GasMeteringKind::Sync));
    module_config.set_step_tracing(true);

    let mut parts = ProgramParts::default();
    parts.is_64_bit = true;
    parts.code_and_jump_table = program.into();
    setup_memory(&mut parts, page_map, chunks);
    let blob = ProgramBlob::from_parts(parts).unwrap();

    let module = Module::from_blob(&engine, &module_config, blob).unwrap();
    let mut instance = module.instantiate().unwrap();

    instance.set_gas(gas);
    instance.set_next_program_counter(ProgramCounter(0));

    for (i, reg) in (0..NO_OF_REGISTERS).zip(Reg::ALL) {
        let start_bytes = i * BYTES_PER_REG;
        let reg_value = read_u64(&registers, start_bytes);
        instance.set_reg(reg, reg_value);
    }

    *PVM.lock().unwrap() = Some(instance);
    nextStep();
}

#[wasm_bindgen]
pub fn nextStep() -> bool {
    let (can_continue, status) = with_pvm(|pvm| {
        match pvm.run() {
            Ok(InterruptKind::Finished) => {
                (false, Status::Halt)
            },
            Ok(InterruptKind::Trap) => {
                (false, Status::Panic)
            },
            Ok(InterruptKind::Ecalli(call)) => {
                *EXIT_ARG.lock().unwrap() = call;
                (false, Status::Host)
            },
            Ok(InterruptKind::Segfault(page)) => {
                *EXIT_ARG.lock().unwrap() = page.page_address;
                (false, Status::Fault)
            },
            Ok(InterruptKind::NotEnoughGas) => {
                (false, Status::OutOfGas)
            },
            Ok(InterruptKind::Step) => {
                (true, Status::Ok)
            },
            Err(e) => {
                eprintln!("Error: {:?}", e);
                (false, Status::Panic)
            },
        }
    }, (false, Status::Panic));
    *STATUS.lock().unwrap() = status;
    can_continue
}

#[wasm_bindgen]
pub fn nSteps(steps: u32) -> bool {
    for _ in 0..steps {
        if !nextStep() {
            return false;
        }
    }
    return true;
}

#[wasm_bindgen]
pub fn getProgramCounter() -> u32 {
    with_pvm(|pvm| pvm.program_counter().map(|x| x.0).unwrap_or(0), 0)
}

#[wasm_bindgen]
pub fn setNextProgramCounter(pc: u32) {
    with_pvm(|pvm| pvm.set_next_program_counter(ProgramCounter(pc)), ());
}

#[wasm_bindgen]
pub fn getStatus() -> u8 {
    let status = *STATUS.lock().unwrap();
    status as u8
}

#[wasm_bindgen]
pub fn getExitArg() -> u32 {
    *EXIT_ARG.lock().unwrap()
}

#[wasm_bindgen]
pub fn getGasLeft() -> i64 {
    with_pvm(|pvm| pvm.gas(), 0)
}

#[wasm_bindgen]
pub fn setGasLeft(gas: i64) {
    with_pvm(|pvm| pvm.set_gas(gas), ());
}

#[wasm_bindgen]
pub fn getRegisters() -> Vec<u8> {
    let mut registers = vec![0u8; NO_OF_REGISTERS * BYTES_PER_REG];
    with_pvm(|pvm| {
        for (i, reg) in (0..NO_OF_REGISTERS).zip(Reg::ALL) {
            let start_byte = i * BYTES_PER_REG;
            let val_le_bytes = pvm.reg(reg).to_le_bytes();
            registers[start_byte..start_byte +BYTES_PER_REG].copy_from_slice(&val_le_bytes);
        }
    }, ());

    registers
}

#[wasm_bindgen]
pub fn setRegisters(registers: Vec<u8>) {
    with_pvm(|pvm| {
        for (i, reg) in (0..NO_OF_REGISTERS).zip(Reg::ALL) {
            let start_bytes = i * BYTES_PER_REG;
            let reg_value = read_u64(&registers, start_bytes);
            pvm.set_reg(reg, reg_value);
        }
    }, ());
}

#[wasm_bindgen]
pub fn getPageDump(index: u32) -> Vec<u8> {
    with_pvm(|pvm| {
        let address = index * PAGE_SIZE as u32;
        let page = pvm
            .read_memory(address, PAGE_SIZE as u32)
            .unwrap_or_else(|_| vec![0; PAGE_SIZE]);
        page
    }, vec![0; PAGE_SIZE])
}

#[wasm_bindgen]
pub fn setMemory(address: u32, data: Vec<u8>) {
    with_pvm(|pvm| {
        let _ = pvm.write_memory(address, &data);
    }, ());
}

pub fn setup_memory(
    parts: &mut ProgramParts,
    page_map: Vec<u8>,
    chunks: Vec<u8>,
) {
    let pages = read_pages(page_map);
    let chunks = read_chunks(chunks);

    let mut ro_start = None;
    let mut rw_start = None;
    let mut stack_start = None;

    for page in pages {
        if page.is_writable {
            if rw_start.is_some() {
                if stack_start.is_some() { panic!("Can't set STACK/RW memory twice"); }
                parts.stack_size = page.length;
                stack_start = Some(page.address);
            } else {
                parts.rw_data_size = page.length;
                rw_start = Some(page.address);
            }
        } else {
            if ro_start.is_some() { panic!("Can't set RO memory twice"); }
            parts.ro_data_size = page.length;
            ro_start = Some(page.address);
        }
    }

    let mut ro_data = vec![0; parts.ro_data_size as usize];
    let mut rw_data = vec![0; parts.rw_data_size as usize];

    let copy_chunk = |chunk: &Chunk, start, size, into: &mut Vec<u8>| {
        if let Some(start) = start {
            if chunk.address >= start {
                let rel_address = chunk.address - start;
                if rel_address < size {
                    let rel_address = rel_address as usize;
                    let rel_end = rel_address + chunk.data.len();
                    into[rel_address .. rel_end].copy_from_slice(&chunk.data);
                    return true;
                }
            }
        }
        false
    };

    if let Some(ro_start) = ro_start {
        if ro_start != 0x10000 {
            panic!("Unsupported address of RO data.");
        }
    }

    if let Some(rw_start) = rw_start {
        if rw_start != 0x20000 {
            panic!("Unsupported address of RW data.");
        }
    }

    for chunk in chunks {
        let is_in_ro = copy_chunk(&chunk, ro_start, parts.ro_data_size, &mut ro_data);
        let is_in_rw = copy_chunk(&chunk, rw_start, parts.rw_data_size, &mut rw_data);
        if !is_in_ro && !is_in_rw {
            panic!("Invalid chunk!");
        }
    }

    parts.ro_data = ArcBytes::from(ro_data);
    parts.rw_data = ArcBytes::from(rw_data);

}

fn read_u32(source: &[u8], index: usize) -> u32 {
    let mut val = [0u8; 4];
    val.copy_from_slice(&source[index .. index + 4]);
    u32::from_le_bytes(val)
}

fn read_u64(source: &[u8], index: usize) -> u64 {
    let mut val = [0u8; 8];
    val.copy_from_slice(&source[index .. index + 8]);
    u64::from_le_bytes(val)
}

/// Page Map is defined in JAM codec lingo as: `sequence(tuple(u32, u32, bool))`
fn read_pages(page_map: Vec<u8>) -> Vec<Page> {
    let mut pages = vec![];
    let mut index = 0;
    while index < page_map.len() {
        let address = read_u32(&page_map, index);
        index += 4;
        let length = read_u32(&page_map, index);
        index += 4;
        let is_writable = page_map[index] > 0;
        index += 1;
        pages.push(Page {
            address, length, is_writable
        });
    }
    pages
}

/// Chunks is defined in JAM codec lingo as: `sequence(tuple(u32, u32, bytes))`
fn read_chunks(chunks: Vec<u8>) -> Vec<Chunk> {
    let mut res = vec![];
    let mut index = 0;
    while index < chunks.len() {
        let address = read_u32(&chunks, index);
        index += 4;
        let length = read_u32(&chunks, index) as usize;
        index += 4;
        let data = chunks[index .. index + length].to_vec();
        res.push(Chunk {
            address,
            data,
        });
        index += length;
    }
    res
}

struct Page {
    address: u32,
    length: u32,
    is_writable: bool,
}

struct Chunk {
    address: u32,
    data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // we need to run the tests serially instead of parallel.
    static TEST: Mutex<()> = Mutex::new(());

    #[test]
    fn run_simple_program() {
        let _lock = TEST.lock();
        let program = FIB.to_vec();
        let mut registers = vec![0u8; 13 * 8];
        registers[7 * 8] = 9;
        resetGeneric(program, registers, 10_000);
        loop {
            assert_eq!(getStatus(), 255);
            let can_continue = nextStep();
            println!("Status: {:?}, PC: {}", getStatus(), getProgramCounter());
            if !can_continue {
                break;
            }
        }
        assert_eq!(getStatus(), 1);
        assert_eq!(getProgramCounter(), 31);
    }

    #[test]
    fn should_change_pc_after_first_step() {
        let _lock = TEST.lock();
        let program = FIB.to_vec();
        let mut registers = vec![0u8; 13 * 8];
        registers[7 * 8] = 9;
        resetGeneric(program, registers, 10_000);
        assert_eq!(getProgramCounter(), 0);
        assert_eq!(getStatus(), 255);
        nextStep();
        assert_eq!(getProgramCounter(), 3);
        assert_eq!(getStatus(), 255);
    }

    #[test]
    fn should_run_gol() {
        let _lock = TEST.lock();
        let program = GOL.to_vec();
        let mut registers = vec![0u8; 13 * 8];
        let page_map = vec![0,0,2,0,0,16,0,0,1];
        resetGenericWithMemory(
            program,
            registers,
            page_map,
            vec![],
            10_000
        );
        loop {
            let can_continue = nextStep();
            println!("Status: {:?}, PC: {}", getStatus(), getProgramCounter());
            if !can_continue {
                break;
            }
        }
        assert_eq!(getStatus(), 1);
        assert_eq!(getProgramCounter(), 323);
    }

    const FIB: &[u8] = &[
        0, 0, 33, 51, 8, 1, 51, 9, 1, 40, 3, 0, 149, 119, 255, 81, 7, 12, 100, 138, 200, 152, 8, 100, 169, 40, 243, 100, 135, 51, 8, 51, 9, 1, 50, 0, 73, 147, 82, 213, 0
    ];
    const GOL: &[u8] = &[
    0,
    0,
    129,
    83,
    30,
    3,
    3,
    0,
    2,
    255,
    0,
    30,
    3,
    11,
    0,
    2,
    255,
    0,
    30,
    3,
    19,
    0,
    2,
    255,
    0,
    30,
    3,
    18,
    0,
    2,
    255,
    0,
    30,
    3,
    9,
    0,
    2,
    255,
    0,
    40,
    22,
    1,
    51,
    1,
    255,
    1,
    139,
    17,
    1,
    81,
    17,
    8,
    12,
    1,
    51,
    2,
    255,
    1,
    139,
    34,
    1,
    81,
    18,
    8,
    241,
    140,
    19,
    8,
    139,
    51,
    0,
    0,
    2,
    180,
    35,
    3,
    40,
    47,
    139,
    51,
    128,
    0,
    114,
    52,
    122,
    68,
    1,
    82,
    20,
    1,
    14,
    83,
    21,
    2,
    25,
    86,
    21,
    3,
    21,
    40,
    8,
    81,
    21,
    3,
    6,
    40,
    11,
    139,
    51,
    128,
    70,
    3,
    255,
    0,
    40,
    200,
    139,
    51,
    128,
    70,
    3,
    40,
    193,
    51,
    5,
    100,
    52,
    51,
    8,
    64,
    139,
    68,
    255,
    185,
    132,
    7,
    139,
    119,
    0,
    0,
    2,
    139,
    119,
    128,
    0,
    114,
    118,
    122,
    102,
    1,
    180,
    101,
    5,
    139,
    68,
    2,
    185,
    132,
    7,
    139,
    119,
    0,
    0,
    2,
    139,
    119,
    128,
    0,
    114,
    118,
    122,
    102,
    1,
    180,
    101,
    5,
    139,
    68,
    247,
    185,
    132,
    7,
    139,
    119,
    0,
    0,
    2,
    139,
    119,
    128,
    0,
    114,
    118,
    122,
    102,
    1,
    180,
    101,
    5,
    139,
    68,
    16,
    185,
    132,
    7,
    139,
    119,
    0,
    0,
    2,
    139,
    119,
    128,
    0,
    114,
    118,
    122,
    102,
    1,
    180,
    101,
    5,
    139,
    68,
    1,
    185,
    132,
    7,
    139,
    119,
    0,
    0,
    2,
    139,
    119,
    128,
    0,
    114,
    118,
    122,
    102,
    1,
    180,
    101,
    5,
    139,
    68,
    254,
    185,
    132,
    7,
    139,
    119,
    0,
    0,
    2,
    139,
    119,
    128,
    0,
    114,
    118,
    122,
    102,
    1,
    180,
    101,
    5,
    139,
    68,
    240,
    185,
    132,
    7,
    139,
    119,
    0,
    0,
    2,
    139,
    119,
    128,
    0,
    114,
    118,
    122,
    102,
    1,
    180,
    101,
    5,
    139,
    68,
    2,
    185,
    132,
    7,
    139,
    119,
    0,
    0,
    2,
    139,
    119,
    128,
    0,
    114,
    118,
    122,
    102,
    1,
    180,
    101,
    5,
    40,
    20,
    255,
    51,
    1,
    0,
    0,
    2,
    1,
    139,
    19,
    128,
    0,
    118,
    18,
    112,
    50,
    139,
    17,
    4,
    81,
    49,
    100,
    0,
    2,
    220,
    254,
    40,
    238,
    129,
    64,
    32,
    16,
    72,
    38,
    100,
    34,
    33,
    69,
    137,
    136,
    162,
    68,
    169,
    74,
    18,
    162,
    36,
    9,
    81,
    146,
    132,
    40,
    73,
    66,
    148,
    36,
    33,
    74,
    146,
    16,
    37,
    73,
    136,
    146,
    36,
    68,
    73,
    194,
    168,
    4,
    2
  ];


}
