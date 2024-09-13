use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub fn compile_assembly(assembly: &str) -> Result<String, String> {
    let engine = spectool::new_engine();
    let result = spectool::prepare_input(assembly, &engine, "wasm_asm");

    match result {
        Ok(testcase) => Ok(serde_json::to_string(&testcase.json).unwrap()),
        Err(err) => Err(format!("{:?}", err)),
    }
}
#[cfg(test)]
mod tests {
    use crate::compile_assembly;

    #[test]
    fn should_compile_assembly() {
        let assembly = r#"
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
        let result = compile_assembly(assembly);

        assert_eq!(result.is_ok(), true);
    }
}
