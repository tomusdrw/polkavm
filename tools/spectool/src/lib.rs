#![allow(clippy::exit)]
#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]
#![allow(clippy::use_debug)]

use polkavm::{Engine, InterruptKind, Module, ModuleConfig, ProgramBlob, Reg};
use polkavm_common::assembler::assemble;
use polkavm_common::program::ProgramParts;

pub struct Testcase {
    pub disassembly: String,
    pub json: TestcaseJson,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Page {
    pub address: u32,
    pub length: u32,
    pub is_writable: bool,
}

#[derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MemoryChunk {
    pub address: u32,
    pub contents: Vec<u8>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestcaseJson {
    pub name: String,
    pub initial_regs: [u32; 13],
    pub initial_pc: u32,
    pub initial_page_map: Vec<Page>,
    pub initial_memory: Vec<MemoryChunk>,
    pub initial_gas: i64,
    pub program: Vec<u8>,
    pub expected_status: String,
    pub expected_regs: Vec<u32>,
    pub expected_pc: u32,
    pub expected_memory: Vec<MemoryChunk>,
    pub expected_gas: i64,
}

fn extract_chunks(base_address: u32, slice: &[u8]) -> Vec<MemoryChunk> {
    let mut output = Vec::new();
    let mut position = 0;
    while let Some(next_position) = slice[position..].iter().position(|&byte| byte != 0).map(|offset| position + offset) {
        position = next_position;
        let length = slice[position..].iter().take_while(|&&byte| byte != 0).count();
        output.push(MemoryChunk {
            address: base_address + position as u32,
            contents: slice[position..position + length].into(),
        });
        position += length;
    }

    output
}

pub fn new_engine() -> Engine {
    let mut config = polkavm::Config::new();
    config.set_backend(Some(polkavm::BackendKind::Interpreter));

    Engine::new(&config).unwrap()
}

pub fn prepare_input(input: &str, engine: &Engine, name: &str) -> Result<Testcase, String> {
    let mut initial_regs = [0; 13];
    let mut initial_gas = 10000;

    let mut input_lines = Vec::new();
    for line in input.lines() {
        if let Some(line) = line.strip_prefix("pre:") {
            let line = line.trim();
            let index = line.find('=').expect("invalid 'pre' directive: no '=' found");
            let lhs = line[..index].trim();
            let rhs = line[index + 1..].trim();
            if lhs == "gas" {
                initial_gas = rhs.parse::<i64>().expect("invalid 'pre' directive: failed to parse rhs");
            } else {
                let lhs = polkavm_common::utils::parse_reg(lhs).expect("invalid 'pre' directive: failed to parse lhs");
                let rhs = polkavm_common::utils::parse_imm(rhs).expect("invalid 'pre' directive: failed to parse rhs");
                initial_regs[lhs as usize] = rhs as u32;
            }
            input_lines.push(""); // Insert dummy line to not mess up the line count.
            continue;
        }

        input_lines.push(line);
    }

    let input = input_lines.join("\n");
    println!("Input: {}", input);
    let blob = match assemble(&input) {
        Ok(blob) => blob,
        Err(error) => {
            let msg = format!("Failed to assemble {name:?}: {error}");
            eprintln!("{}", msg);
            return Err(msg);
        }
    };

    let parts = ProgramParts::from_bytes(blob.into()).unwrap();
    let blob = ProgramBlob::from_parts(parts.clone()).unwrap();

    let mut module_config = ModuleConfig::default();
    module_config.set_strict(true);
    module_config.set_gas_metering(Some(polkavm::GasMeteringKind::Sync));
    module_config.set_step_tracing(true);

    let module = Module::from_blob(&engine, &module_config, blob.clone()).unwrap();
    let mut instance = module.instantiate().unwrap();

    let mut initial_page_map = Vec::new();
    let mut initial_memory = Vec::new();

    if module.memory_map().ro_data_size() > 0 {
        initial_page_map.push(Page {
            address: module.memory_map().ro_data_address(),
            length: module.memory_map().ro_data_size(),
            is_writable: false,
        });

        initial_memory.extend(extract_chunks(module.memory_map().ro_data_address(), blob.ro_data()));
    }

    if module.memory_map().rw_data_size() > 0 {
        initial_page_map.push(Page {
            address: module.memory_map().rw_data_address(),
            length: module.memory_map().rw_data_size(),
            is_writable: true,
        });

        initial_memory.extend(extract_chunks(module.memory_map().rw_data_address(), blob.rw_data()));
    }

    if module.memory_map().stack_size() > 0 {
        initial_page_map.push(Page {
            address: module.memory_map().stack_address_low(),
            length: module.memory_map().stack_size(),
            is_writable: true,
        });
    }

    let initial_pc = blob.exports().find(|export| export.symbol() == "main").unwrap().program_counter();

    #[allow(clippy::map_unwrap_or)]
    let expected_final_pc = blob
        .exports()
        .find(|export| export.symbol() == "expected_exit")
        .map(|export| export.program_counter().0)
        .unwrap_or(blob.code().len() as u32);

    instance.set_gas(initial_gas);
    instance.set_next_program_counter(initial_pc);

    for (reg, value) in Reg::ALL.into_iter().zip(initial_regs) {
        instance.set_reg(reg, value);
    }

    let mut final_pc = initial_pc;
    let expected_status = loop {
        match instance.run().unwrap() {
            InterruptKind::Finished => break "halt",
            InterruptKind::Trap => break "trap",
            InterruptKind::Ecalli(..) => todo!(),
            InterruptKind::NotEnoughGas => break "out-of-gas",
            InterruptKind::Segfault(..) => todo!(),
            InterruptKind::Step => {
                final_pc = instance.program_counter().unwrap();
                continue;
            }
        }
    };

    if expected_status != "halt" {
        final_pc = instance.program_counter().unwrap();
    }

    if final_pc.0 != expected_final_pc {
        let msg = format!("Unexpected final program counter for {name:?}: expected {expected_final_pc}, is {final_pc}");
        eprintln!("{}", msg);
        return Err(msg);
    }

    let mut expected_regs = Vec::new();
    for reg in Reg::ALL {
        let value = instance.reg(reg);
        expected_regs.push(value);
    }

    let mut expected_memory = Vec::new();
    for page in &initial_page_map {
        let memory = instance.read_memory(page.address, page.length).unwrap();
        expected_memory.extend(extract_chunks(page.address, &memory));
    }

    let expected_gas = instance.gas();

    let mut disassembler = polkavm_disassembler::Disassembler::new(&blob, polkavm_disassembler::DisassemblyFormat::Guest).unwrap();
    disassembler.show_raw_bytes(true);
    disassembler.prefer_non_abi_reg_names(true);
    disassembler.prefer_unaliased(true);
    disassembler.emit_header(false);
    disassembler.emit_exports(false);

    let mut disassembly = Vec::new();
    disassembler.disassemble_into(&mut disassembly).unwrap();
    let disassembly = String::from_utf8(disassembly).unwrap();


    Ok(Testcase {
        disassembly,
        json: TestcaseJson {
            name: name.into(),
            initial_regs,
            initial_pc: initial_pc.0,
            initial_page_map,
            initial_memory,
            initial_gas,
            program: parts.code_and_jump_table.to_vec(),
            expected_status: expected_status.to_owned(),
            expected_regs,
            expected_pc: expected_final_pc,
            expected_memory,
            expected_gas,
        },
    })
}
