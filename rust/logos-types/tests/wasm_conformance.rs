#![cfg(all(target_arch = "wasm32", feature = "wasm"))]

mod conformance_vectors;

use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn wasm_shared_golden_vectors() {
    conformance_vectors::run_shared_vectors();
}
