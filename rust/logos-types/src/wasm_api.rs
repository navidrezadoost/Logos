use wasm_bindgen::prelude::*;

use crate::document::decode_and_validate_json;

/// Validate a serialized candidate document without retaining or mutating it.
/// The JSON result is `{ ok, document?, diagnostics }` and keeps the existing
/// Rust document module as the sole semantic validator.
#[wasm_bindgen]
pub fn validate_document_json(input: &str) -> String {
    let result = match decode_and_validate_json(input) {
        Ok(document) => serde_json::json!({
            "ok": true,
            "document": document,
            "diagnostics": [],
        }),
        Err(diagnostics) => serde_json::json!({
            "ok": false,
            "diagnostics": diagnostics,
        }),
    };
    result.to_string()
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::validate_document_json;

    const VALID: &str = include_str!(
        "../../../logos-app/src/document-core/fixtures/rust-candidate-v1.json"
    );

    #[wasm_bindgen_test]
    fn browser_abi_accepts_the_shared_react_adapter_candidate() {
        let response: serde_json::Value = serde_json::from_str(&validate_document_json(VALID)).unwrap();
        assert_eq!(response["ok"], true);
        assert_eq!(response["document"]["shapes"].as_array().unwrap().len(), 4);
        assert_eq!(response["diagnostics"], serde_json::json!([]));
    }

    #[wasm_bindgen_test]
    fn browser_abi_returns_rust_parent_child_diagnostic_for_invalid_candidate() {
        let mut candidate: serde_json::Value = serde_json::from_str(VALID).unwrap();
        candidate["shapes"][1]["shapes"] = serde_json::json!([]);
        let response: serde_json::Value =
            serde_json::from_str(&validate_document_json(&candidate.to_string())).unwrap();
        assert_eq!(response["ok"], false);
        assert!(response["diagnostics"].as_array().unwrap().iter().any(|item| {
            item["code"] == "PARENT_CHILD_MISMATCH"
        }));
    }
}
