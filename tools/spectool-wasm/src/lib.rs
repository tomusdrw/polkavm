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
