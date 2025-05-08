use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub fn compile_assembly(assembly: &str) -> Result<String, String> {
    let engine = spectool::new_engine();
    let result = spectool::prepare_input(assembly, &engine, "wasm_asm", "wasm_asm", false);

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

        assert_eq!(result.map(|_| ()), Ok(()));
    }

    #[test]
    fn should_compile_other_assembly() {
        let result = compile_assembly(
            r#"pub @main:
	r7 = 0x4d2
	jump @block2 if r7 == 1235
pub @expected_exit:
	trap
@block2:
	r7 = 0xdeadbeef
"#,
        )
        .map(|_| ());
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn should_compile_yet_another_assembly() {
        let result = compile_assembly(
            r#"
pre: r0 = 4294901760
pre: r7 = 9

pub @main:
	r8 = 0x1
	r9 = 0x1
	jump @block2
@block1:
	trap
@block2:
	r7 = r7 + 1
	jump @block4 if r7 == 0
@block3:
	r10 = r8
	r8 = r8 + r9
	r9 = r10
	jump @block2
@block4:
	r7 = r8
	r8 = 0x0
	r9 = 0x0
	fallthrough
@block5:
	jump [r0 + 0]
"#,
        )
        .map(|_| ());
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn should_disassemble_code() {
        let engine = spectool::new_engine();
        let result = spectool::prepare_input(ASSEMBLY, &engine, "wasm_asm", "wasm_asm", false).unwrap();
        let code_and_jump_table = result.json.program;

        let result = disassemble(code_and_jump_table).unwrap();
        assert_eq!(result, DISASSEMBLED_CODE);
    }

    const DISASSEMBLED_CODE: &str = r#"      : @0
     0: r8 = 0x1
     3: r9 = 0x1
     6: jump @2
      : @1
     8: trap
      : @2
     9: r7 = r7 + 0xffffffffffffffff
    12: jump @4 if r7 == 0
      : @3
    15: r10 = r8
    17: r8 = r8 + r9
    20: r9 = r10
    22: jump @2
      : @4
    24: r7 = r8
    26: r8 = 0
    28: r9 = 0
    30: fallthrough
      : @5
    31: jump [r0 + 0]
"#;
}
