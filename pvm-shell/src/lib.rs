#![allow(non_snake_case)]

use std::sync::Mutex;
use polkavm::{Engine, InterruptKind, Module, ModuleConfig, ProgramBlob, ProgramCounter, RawInstance, Reg};
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
const BYTES_PER_REG: usize = 4;

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
    resetGeneric(program, registers, gas)
}

#[wasm_bindgen]
pub fn resetGeneric(program: Vec<u8>, registers: Vec<u8>, gas: i64) {
    let mut config = polkavm::Config::new();
    config.set_backend(Some(polkavm::BackendKind::Interpreter));

    let engine = Engine::new(&config).unwrap();
    let mut module_config = ModuleConfig::default();
    module_config.set_strict(true);
    module_config.set_gas_metering(Some(polkavm::GasMeteringKind::Sync));
    module_config.set_step_tracing(true);

    let mut parts = ProgramParts::default();
    parts.code_and_jump_table = program.into();
    let blob = ProgramBlob::from_parts(parts).unwrap();

    let module = Module::from_blob(&engine, &module_config, blob).unwrap();
    let mut instance = module.instantiate().unwrap();

    instance.set_gas(gas);
    instance.set_next_program_counter(ProgramCounter(0));

    for (i, reg) in (0..NO_OF_REGISTERS).zip(Reg::ALL) {
        let start_bytes = i * BYTES_PER_REG;
        let mut reg_value = [0u8; BYTES_PER_REG];
        reg_value.copy_from_slice(&registers[start_bytes .. start_bytes + BYTES_PER_REG]);

        instance.set_reg(reg, u32::from_le_bytes(reg_value));
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
                (true, Status::Host)
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
pub fn getProgramCounter() -> u32 {
    with_pvm(|pvm| pvm.program_counter().map(|x| x.0).unwrap_or(0), 0)
}

#[wasm_bindgen]
pub fn setNextProgramCounter(pc: u32) {
    with_pvm(|pvm| pvm.set_next_program_counter(ProgramCounter(pc)), ());
}

#[wasm_bindgen]
pub fn getStatus() -> i8 {
    let status = *STATUS.lock().unwrap();
    if let Status::Ok = status { -1 } else { status as i8 }
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
pub fn getPageDump(index: u32) -> Vec<u8> {
    return vec![index as u8];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_simple_program() {
        let program = vec![
            0,
            0,
            3,
            8,
            135,
            9,
            249
        ];
        let registers = vec![0u8; 13 * 4];
        reset(program, registers, 10_000);
        loop {
            let can_continue = nextStep();
            println!("Status: {:?}, PC: {}", getStatus(), getProgramCounter());
            if !can_continue {
                break;
            }
        }
    }
}
