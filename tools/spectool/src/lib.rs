#![allow(clippy::print_stderr)]

use polkavm::{program::ISA64_V1, Engine, InterruptKind, Module, ModuleConfig, ProgramBlob, ProgramCounter, ProgramParts, Reg};
use polkavm_common::assembler::assemble;

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
    pub initial_regs: [u64; 13],
    pub initial_pc: u32,
    pub initial_page_map: Vec<Page>,
    pub initial_memory: Vec<MemoryChunk>,
    pub initial_gas: i64,
    pub program: Vec<u8>,
    pub expected_status: String,
    pub expected_regs: Vec<u64>,
    pub expected_pc: u32,
    pub expected_memory: Vec<MemoryChunk>,
    pub expected_gas: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_page_fault_address: Option<u32>,
}

pub fn new_engine() -> Engine {
    let mut config = polkavm::Config::new();
    config.set_backend(Some(polkavm::BackendKind::Interpreter));

    Engine::new(&config).unwrap()
}

pub fn disassemble(bytecode: Vec<u8>) -> Result<String, String> {
    let mut parts = ProgramParts::default();
    parts.code_and_jump_table = bytecode.into();
    parts.is_64_bit = true;
    let blob = ProgramBlob::from_parts(parts).map_err(to_string)?;

    let mut disassembler =
        polkavm_disassembler::Disassembler::new(&blob, polkavm_disassembler::DisassemblyFormat::Guest).map_err(to_string)?;

    disassembler.show_raw_bytes(false);
    disassembler.prefer_non_abi_reg_names(true);
    disassembler.prefer_unaliased(true);
    disassembler.prefer_offset_jump_targets(false);
    disassembler.emit_header(false);
    disassembler.emit_exports(false);

    let mut disassembly = Vec::new();
    disassembler.disassemble_into(&mut disassembly).map_err(to_string)?;
    let disassembly = String::from_utf8(disassembly).map_err(to_string)?;

    Ok(disassembly)
}

pub fn prepare_input(input: &str, engine: &Engine, name: &str, internal_name: &str, execute: bool) -> Result<Testcase, String> {
    let mut pre = PrePost::default();
    let mut post = PrePost::default();

    let expected_status: Option<String> = None;
    let mut input_lines = Vec::new();
    for line in input.lines() {
        if let Some(line) = line.strip_prefix("pre:") {
            parse_pre_post(line, &mut pre);
            input_lines.push(""); // Insert dummy line to not mess up the line count.
            continue;
        }

        if let Some(line) = line.strip_prefix("post:") {
            parse_pre_post(line, &mut post);
            input_lines.push(""); // Insert dummy line to not mess up the line count.
            continue;
        }

        input_lines.push(line);
    }

    let input = input_lines.join("\n");
    let blob = match assemble(&input) {
        Ok(blob) => blob,
        Err(error) => {
            let msg = format!("Failed to assemble {internal_name}: {error}");
            eprintln!("{}", msg);
            return Err(msg);
        }
    };

    let initial_gas = pre.gas.unwrap_or(10000);
    let initial_regs = pre.regs.map(|value| value.unwrap_or(0));
    assert!(pre.pc.is_none(), "'pre: pc = ...' is currently unsupported");

    let parts = ProgramParts::from_bytes(blob.into()).unwrap();
    let blob = ProgramBlob::from_parts(parts.clone()).unwrap();

    let mut module_config = ModuleConfig::default();
    module_config.set_strict(true);
    module_config.set_gas_metering(Some(polkavm::GasMeteringKind::Sync));
    module_config.set_step_tracing(true);
    module_config.set_dynamic_paging(true);

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

    let expected_final_pc = if let Some(export) = blob.exports().find(|export| export.symbol() == "expected_exit") {
        assert!(
            post.pc.is_none(),
            "'@expected_exit' label and 'post: pc = ...' should not be used together"
        );
        export.program_counter().0
    } else if let Some(ProgramCounterRef::ByLabel { label, instruction_offset }) = post.pc {
        let Some(export) = blob.exports().find(|export| export.symbol().as_bytes() == label.as_bytes()) else {
            panic!("label specified in 'post: pc = ...' is missing: @{label}");
        };

        let instructions: Vec<_> = blob.instructions(ISA64_V1).collect();
        let index = instructions
            .iter()
            .position(|inst| inst.offset == export.program_counter())
            .expect("failed to find label specified in 'post: pc = ...'");
        let instruction = instructions
            .get(index + instruction_offset as usize)
            .expect("invalid 'post: pc = ...': offset goes out of bounds of the basic block");
        instruction.offset.0
    } else if let Some(ProgramCounterRef::Preset(pc)) = post.pc {
        pc.0
    } else {
        blob.code().len() as u32
    };

    instance.set_gas(initial_gas);
    instance.set_next_program_counter(initial_pc);

    for (reg, value) in Reg::ALL.into_iter().zip(initial_regs) {
        instance.set_reg(reg, value);
    }

    if module_config.dynamic_paging() {
        for page in &initial_page_map {
            instance.zero_memory(page.address, page.length).unwrap();
            if !page.is_writable {
                instance.protect_memory(page.address, page.length).unwrap();
            }
        }

        for chunk in &initial_memory {
            instance.write_memory(chunk.address, &chunk.contents).unwrap();
        }
    }

    let mut final_pc = initial_pc;
    let (final_status, page_fault_address) = if execute {
        loop {
            match instance.run().unwrap() {
                InterruptKind::Finished => break ("halt", None),
                InterruptKind::Trap => break ("panic", None),
                InterruptKind::Ecalli(..) => todo!(),
                InterruptKind::NotEnoughGas => break ("out-of-gas", None),
                InterruptKind::Segfault(segfault) => break ("page-fault", Some(segfault.page_address)),
                InterruptKind::Step => {
                    final_pc = instance.program_counter().unwrap();
                    continue;
                }
            }
        }
    } else {
        final_pc.0 = expected_final_pc;
        (expected_status.as_deref().unwrap_or("ok"), None)
    };

    if final_status != "halt" {
        final_pc = instance.program_counter().unwrap();
    }

    if let Some(expected_status) = expected_status.clone() {
        if final_status != expected_status {
            let msg = format!("Unexpected final status for {internal_name}: expected {expected_status}, is {final_status}");
            eprintln!("{}", msg);
            return Err(msg);
        }
    }

    if final_pc.0 != expected_final_pc {
        let msg = format!("Unexpected final program counter for {internal_name}: expected {expected_final_pc}, is {final_pc}");
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

    let mut found_post_check_errors = false;

    if execute {
        for ((final_value, reg), required_value) in expected_regs.iter().zip(Reg::ALL).zip(post.regs.iter()) {
            if let Some(required_value) = required_value {
                if final_value != required_value {
                    eprintln!("{internal_name}: unexpected {reg}: 0x{final_value:x} (expected: 0x{required_value:x})");
                    found_post_check_errors = true;
                }
            }
        }

        if let Some(post_gas) = post.gas {
            if expected_gas != post_gas {
                eprintln!("{internal_name}: unexpected gas: {expected_gas} (expected: {post_gas})");
                found_post_check_errors = true;
            }
        }
    }

    if found_post_check_errors {
        return Err("Found post check errors.".to_string());
    }

    let mut disassembler = polkavm_disassembler::Disassembler::new(&blob, polkavm_disassembler::DisassemblyFormat::Guest).unwrap();
    disassembler.show_raw_bytes(true);
    disassembler.prefer_non_abi_reg_names(true);
    disassembler.prefer_unaliased(true);
    disassembler.prefer_offset_jump_targets(true);
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
            expected_status: final_status.to_owned(),
            expected_regs,
            expected_pc: expected_final_pc,
            expected_memory,
            expected_gas,
            expected_page_fault_address: page_fault_address,
        },
    })
}

fn to_string<E: core::fmt::Debug>(e: E) -> String {
    format!("{:?}", e)
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

enum ProgramCounterRef {
    ByLabel {
        label: String,
        instruction_offset: u32,
    },
    #[allow(dead_code)]
    Preset(ProgramCounter),
}

#[derive(Default)]
struct PrePost {
    gas: Option<i64>,
    regs: [Option<u64>; 13],
    pc: Option<ProgramCounterRef>,
}

fn parse_pre_post(line: &str, output: &mut PrePost) {
    let line = line.trim();
    let index = line.find('=').expect("invalid 'pre' / 'post' directive: no '=' found");
    let lhs = line[..index].trim();
    let rhs = line[index + 1..].trim();
    if lhs == "gas" {
        output.gas = Some(rhs.parse::<i64>().expect("invalid 'pre' / 'post' directive: failed to parse rhs"));
    } else if lhs == "pc" {
        let rhs = rhs
            .strip_prefix('@')
            .expect("invalid 'pre' / 'post' directive: failed to parse 'pc': no '@' found")
            .trim();
        let index = rhs
            .find('[')
            .expect("invalid 'pre' / 'post' directive: failed to parse 'pc': no '[' found");
        let label = &rhs[..index];
        let rhs = &rhs[index + 1..];
        let index = rhs
            .find(']')
            .expect("invalid 'pre' / 'post' directive: failed to parse 'pc': no ']' found");
        let offset = rhs[..index]
            .parse::<u32>()
            .expect("invalid 'pre' / 'post' directive: failed to parse 'pc': invalid offset");
        if !rhs[index + 1..].trim().is_empty() {
            panic!("invalid 'pre' / 'post' directive: failed to parse 'pc': junk after ']'");
        }

        output.pc = Some(ProgramCounterRef::ByLabel {
            label: label.to_owned(),
            instruction_offset: offset,
        });
    } else {
        let lhs = polkavm_common::utils::parse_reg(lhs).expect("invalid 'pre' / 'post' directive: failed to parse lhs");
        let rhs = polkavm_common::utils::parse_immediate(rhs)
            .map(Into::into)
            .expect("invalid 'pre' / 'post' directive: failed to parse rhs");
        output.regs[lhs as usize] = Some(rhs);
    }
}
