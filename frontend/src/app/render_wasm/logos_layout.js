// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) KALEIDOS INC
//
// logos_layout.js
// ───────────────
// Thin JavaScript wrapper around the `logos-layout-wasm` Rust WASM module.
//
// Memory protocol (mirrors the Rust C-ABI in logos-layout-wasm/src/lib.rs):
//   1. Call `logos_alloc(len)` → ptr  (allocates an input buffer in WASM heap)
//   2. Write UTF-8 JSON bytes into `wasmMemory.buffer` at `ptr`
//   3. Call `logos_calc_flex_layout(ptr, len)` or `logos_calc_grid_layout(ptr, len)`
//      → returns the byte length of the JSON result stored in the output buffer
//   4. Read the output JSON from `logos_output_ptr()` for `resultLen` bytes
//   5. Call `logos_free_input(ptr, len)` and `logos_free_output()`

let instance = null;

/**
 * Initialise the module by compiling + instantiating the WASM binary.
 * Safe to call multiple times — subsequent calls are no-ops.
 *
 * @param {string} [wasmUrl="/js/logos_layout_wasm.wasm"]
 * @returns {Promise<void>}
 */
export async function init(wasmUrl = "/js/logos_layout_wasm.wasm") {
  if (instance) return;
  const result = await WebAssembly.instantiateStreaming(fetch(wasmUrl), {});
  instance = result.instance;
}

/** @throws {Error} if `init()` has not been called */
function exports() {
  if (!instance) throw new Error("logos_layout: call init() first");
  return instance.exports;
}

/** @returns {WebAssembly.Memory} */
function memory() {
  return exports().memory;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/**
 * @param {string} jsonInput
 * @param {(ptr: number, len: number) => number} calcFn
 * @returns {string} output JSON
 */
function callLayout(jsonInput, calcFn) {
  const exp = exports();
  const inputBytes = encoder.encode(jsonInput);
  const len = inputBytes.length;

  // 1. Allocate input buffer in WASM heap
  const ptr = exp.logos_alloc(len);

  // 2. Write JSON bytes
  const heap = new Uint8Array(memory().buffer);
  heap.set(inputBytes, ptr);

  // 3. Invoke layout function → output byte length
  const outLen = calcFn(ptr, len);

  let resultJson = "{}";
  if (outLen > 0) {
    // 4. Read output JSON
    const outPtr = exp.logos_output_ptr();
    const outBytes = new Uint8Array(memory().buffer, outPtr, outLen);
    resultJson = decoder.decode(outBytes);
    // 5a. Free output buffer
    exp.logos_free_output();
  }

  // 5b. Free input buffer
  exp.logos_free_input(ptr, len);

  return resultJson;
}

/**
 * Compute flex layout.
 *
 * @param {object} input  — see FlexInput in logos-layout-wasm/src/lib.rs
 * @returns {object}      — { children: [{id, x, y, width, height}, …] }
 */
export function flexLayout(input) {
  const exp = exports();
  const json = callLayout(JSON.stringify(input), (ptr, len) =>
    exp.logos_calc_flex_layout(ptr, len)
  );
  return JSON.parse(json);
}

/**
 * Compute grid layout.
 *
 * @param {object} input  — see GridInput in logos-layout-wasm/src/lib.rs
 * @returns {object}      — { resolved_columns, resolved_rows, children }
 */
export function gridLayout(input) {
  const exp = exports();
  const json = callLayout(JSON.stringify(input), (ptr, len) =>
    exp.logos_calc_grid_layout(ptr, len)
  );
  return JSON.parse(json);
}

/**
 * @returns {boolean} true if the module is loaded and ready
 */
export function isReady() {
  return instance !== null;
}
