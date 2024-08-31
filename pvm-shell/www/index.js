import { reset, getProgramCounter, getStatus, getRegisters, getPageDump, nextStep, getGasLeft } from "pvm-shell";
import { memory } from "pvm-shell/pvm_shell_bg";


console.log(reset, getProgramCounter);

const program = new Uint8Array(5);
const initRegisters = new Uint8Array(4 * 13);
const initGas = 10000;

reset(program, initRegisters, initGas);

do {
  const pc = getProgramCounter();
  const status = getStatus();
  const gas = getGasLeft();
  const registers = getRegisters();
  const pageDump = getPageDump(0);

  console.log('PVM state:', {
    pc, status, gas, registers, pageDump,
  });
} while(nextStep());
