#![allow(clippy::exit)]
#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]
#![allow(clippy::use_debug)]

use clap::Parser;
use core::fmt::Write;
use polkavm::{Engine, Reg};
use spectool::{prepare_input, Testcase};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[clap(version)]
enum Args {
    Prepare {
        /// The input file.
        input: PathBuf,
    },
    Generate,
    Test,
}

fn main() {
    env_logger::init();

    let args = Args::parse();
    match args {
        Args::Prepare { input } => main_prepare(input),
        Args::Generate => main_generate(),
        Args::Test => main_test(),
    }
}

fn main_generate() {
    let mut tests = Vec::new();

    let mut config = polkavm::Config::new();
    config.set_backend(Some(polkavm::BackendKind::Interpreter));

    let engine = Engine::new(&config).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec");
    let mut found_errors = false;
    for entry in std::fs::read_dir(root.join("src")).unwrap() {
        let path = entry.unwrap().path();
        let test_case = prepare_file(&engine, &path);
        if let Ok(test_case) = test_case {
            tests.push(test_case);
        } else {
            found_errors = true;
        }
    }

    tests.sort_by_key(|test| test.json.name.clone());

    let output_programs_root = root.join("output").join("programs");
    std::fs::create_dir_all(&output_programs_root).unwrap();

    let mut index_md = String::new();
    writeln!(&mut index_md, "# Testcases\n").unwrap();
    writeln!(&mut index_md, "This file contains a human-readable index of all of the testcases,").unwrap();
    writeln!(&mut index_md, "along with their disassemblies and other relevant information.\n\n").unwrap();

    for test in tests {
        let payload = serde_json::to_string_pretty(&test.json).unwrap();
        let output_path = output_programs_root.join(format!("{}.json", test.json.name));
        if !std::fs::read(&output_path)
            .map(|old_payload| old_payload == payload.as_bytes())
            .unwrap_or(false)
        {
            println!("Generating {output_path:?}...");
            std::fs::write(output_path, payload).unwrap();
        }

        writeln!(&mut index_md, "## {}\n", test.json.name).unwrap();

        if !test.json.initial_page_map.is_empty() {
            writeln!(&mut index_md, "Initial page map:").unwrap();
            for page in &test.json.initial_page_map {
                let access = if page.is_writable { "RW" } else { "RO" };

                writeln!(
                    &mut index_md,
                    "   * {access}: 0x{:x}-0x{:x} (0x{:x} bytes)",
                    page.address,
                    page.address + page.length,
                    page.length
                )
                .unwrap();
            }

            writeln!(&mut index_md).unwrap();
        }

        if !test.json.initial_memory.is_empty() {
            writeln!(&mut index_md, "Initial non-zero memory chunks:").unwrap();
            for chunk in &test.json.initial_memory {
                let contents: Vec<_> = chunk.contents.iter().map(|byte| format!("0x{:02x}", byte)).collect();
                let contents = contents.join(", ");
                writeln!(
                    &mut index_md,
                    "   * 0x{:x}-0x{:x} (0x{:x} bytes) = [{}]",
                    chunk.address,
                    chunk.address + chunk.contents.len() as u32,
                    chunk.contents.len(),
                    contents
                )
                .unwrap();
            }

            writeln!(&mut index_md).unwrap();
        }

        if test.json.initial_regs.iter().any(|value| *value != 0) {
            writeln!(&mut index_md, "Initial non-zero registers:").unwrap();
            for reg in Reg::ALL {
                let value = test.json.initial_regs[reg as usize];
                if value != 0 {
                    writeln!(&mut index_md, "   * {} = 0x{:x}", reg.name_non_abi(), value).unwrap();
                }
            }

            writeln!(&mut index_md).unwrap();
        }

        writeln!(&mut index_md, "```\n{}```\n", test.disassembly).unwrap();

        if test
            .json
            .initial_regs
            .iter()
            .zip(test.json.expected_regs.iter())
            .any(|(old_value, new_value)| *old_value != *new_value)
        {
            writeln!(&mut index_md, "Registers after execution (only changed registers):").unwrap();
            for reg in Reg::ALL {
                let value_before = test.json.initial_regs[reg as usize];
                let value_after = test.json.expected_regs[reg as usize];
                if value_before != value_after {
                    writeln!(
                        &mut index_md,
                        "   * {} = 0x{:x} (initially was 0x{:x})",
                        reg.name_non_abi(),
                        value_after,
                        value_before
                    )
                    .unwrap();
                }
            }

            writeln!(&mut index_md).unwrap();
        }

        if !test.json.expected_memory.is_empty() {
            if test.json.expected_memory == test.json.initial_memory {
                writeln!(&mut index_md, "The memory contents after execution should be unchanged.").unwrap();
            } else {
                writeln!(&mut index_md, "Final non-zero memory chunks:").unwrap();
                for chunk in &test.json.expected_memory {
                    let contents: Vec<_> = chunk.contents.iter().map(|byte| format!("0x{:02x}", byte)).collect();
                    let contents = contents.join(", ");
                    writeln!(
                        &mut index_md,
                        "   * 0x{:x}-0x{:x} (0x{:x} bytes) = [{}]",
                        chunk.address,
                        chunk.address + chunk.contents.len() as u32,
                        chunk.contents.len(),
                        contents
                    )
                    .unwrap();
                }
            }

            writeln!(&mut index_md).unwrap();
        }

        writeln!(&mut index_md, "Program should end with: {}\n", test.json.expected_status).unwrap();
        writeln!(&mut index_md, "Final value of the program counter: {}\n", test.json.expected_pc).unwrap();
        writeln!(
            &mut index_md,
            "Gas consumed: {} -> {}\n",
            test.json.initial_gas, test.json.expected_gas
        )
        .unwrap();
        writeln!(&mut index_md).unwrap();
    }

    std::fs::write(root.join("output").join("TESTCASES.md"), index_md).unwrap();

    if found_errors {
        std::process::exit(1);
    }
}

fn prepare_file(engine: &Engine, path: &Path) -> Result<Testcase, String> {
    let name = path.file_stem().unwrap().to_string_lossy();
    let input = std::fs::read_to_string(path).unwrap();
    let input = input.lines().collect::<Vec<_>>().join("\n");
    prepare_input(&input, engine, &name, true)
}

fn main_test() {
    todo!();
}

fn main_prepare(input: PathBuf) {
    let mut config = polkavm::Config::new();
    config.set_backend(Some(polkavm::BackendKind::Interpreter));
    let engine = Engine::new(&config).unwrap();

    let test = prepare_file(&engine, &input);
    if let Ok(test) = test {
        let payload = serde_json::to_string_pretty(&test.json).unwrap();
        println!("{payload}");
    }
}
