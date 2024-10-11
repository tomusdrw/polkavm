use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub fn compile_assembly(assembly: &str) -> Result<String, String> {
    let engine = spectool::new_engine();
    let result = spectool::prepare_input(assembly, &engine, "wasm_asm", false);

    let testcase = result?;
    Ok(serde_json::to_string(&testcase.json).unwrap())
}

#[wasm_bindgen]
pub fn disassemble(bytecode: Vec<u8>) -> Result<String, String> {
    spectool::disassemble(bytecode)
}

#[cfg(test)]
mod tests {
    use spectool::disassemble;

    use crate::compile_assembly;

    const ASSEMBLY: &'static str = r#"
pre: a0 = 9
pre: ra = 0xffff0000

pub @main:
    // first & second
    a1 = 1
    a2 = 1
    jump @loop
    trap

@loop:
    a0 = a0 - 1
    jump @end if a0 == 0
    a3 = a1
    a1 = a1 + a2
    a2 = a3
    jump @loop

@end:
    a0 = a1
    a1 = 0
    a2 = 0

pub @expected_exit:
    ret
"#;

    #[test]
    fn should_compile_assembly() {
        let result = compile_assembly(&ASSEMBLY);

        assert!(result.is_ok());
    }

    #[test]
    fn should_compile_other_assembly() {
        let result = compile_assembly(
            r#"@block0:
	r7 = 0x4d2
	jump @block2 if r7 == 1235
@block1:
	trap
@block2:
	r7 = 0xdeadbeef
"#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn should_disassemble_code() {
        let engine = spectool::new_engine();
        let result = spectool::prepare_input(ASSEMBLY, &engine, "wasm_asm", false).unwrap();
        let code_and_jump_table = result.json.program;

        let result = disassemble(code_and_jump_table).unwrap();
        assert_eq!(result, "xx");
    }
}
