use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub fn reset(_program: Vec<u8>, _gas: u32) {
}

#[wasm_bindgen]
pub fn get_program_counter() -> u32 {
    return 0;
}

/*
  /**
   * Get the current status of PVM
   */
  getStatus(): number;
  /**
   * Return gas left.
   */
  getGasLeft(): number;
  /**
   * Return registers dump.
   * 
   * We expect 13 values, 4 bytes each, representing the state of all registers as a single byte array.
   */
  getRegisters(): Uint8Array;
  /**
   * Perform a single step of PVM execution.
   */
  nextStep(): boolean;
  /**
   * Returns a undefined-length page from memory at given index.
   * 
   * it's up to the implementation to decide if this is going to return just a single memory cell,
   * or a page of some specific size.
   * The page sizes should always be the same though (i.e. the UI will assume that if page 0 has size `N`
   * every other page has the same size).
   */
  getPageDump(pageIndex: number): Uint8Array;
}
*/
