/* tslint:disable */
/* eslint-disable */
/**
* @param {number} pc
* @param {bigint} gas
*/
export function resume(pc: number, gas: bigint): void;
/**
* @param {Uint8Array} program
* @param {Uint8Array} registers
* @param {bigint} gas
*/
export function reset(program: Uint8Array, registers: Uint8Array, gas: bigint): void;
/**
* @param {Uint8Array} program
* @param {Uint8Array} registers
* @param {bigint} gas
*/
export function resetGeneric(program: Uint8Array, registers: Uint8Array, gas: bigint): void;
/**
* @param {Uint8Array} program
* @param {Uint8Array} registers
* @param {Uint8Array} page_map
* @param {Uint8Array} chunks
* @param {bigint} gas
*/
export function resetGenericWithMemory(program: Uint8Array, registers: Uint8Array, page_map: Uint8Array, chunks: Uint8Array, gas: bigint): void;
/**
* @returns {boolean}
*/
export function nextStep(): boolean;
/**
* @param {number} steps
* @returns {boolean}
*/
export function run(steps: number): boolean;
/**
* @returns {number}
*/
export function getProgramCounter(): number;
/**
* @param {number} pc
*/
export function setNextProgramCounter(pc: number): void;
/**
* @returns {number}
*/
export function getStatus(): number;
/**
* @returns {number}
*/
export function getExitArg(): number;
/**
* @returns {bigint}
*/
export function getGasLeft(): bigint;
/**
* @param {bigint} gas
*/
export function setGasLeft(gas: bigint): void;
/**
* @returns {Uint8Array}
*/
export function getRegisters(): Uint8Array;
/**
* @param {Uint8Array} registers
*/
export function setRegisters(registers: Uint8Array): void;
/**
* @param {number} index
* @returns {Uint8Array}
*/
export function getPageDump(index: number): Uint8Array;
/**
* @param {number} address
* @param {Uint8Array} data
*/
export function setMemory(address: number, data: Uint8Array): void;
/**
*/
export enum Status {
  Ok = 255,
  Halt = 0,
  Panic = 1,
  Fault = 2,
  Host = 3,
  OutOfGas = 4,
}
