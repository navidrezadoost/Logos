//! Small JSON-in/JSON-out bridge to the production `logos-tokens` resolver.
//!
//! This crate contains no resolution logic. It accepts serialized contract
//! messages, calls the existing Rust resolver/plan validator, and returns
//! their serialized results through a simple linear-memory ABI.

use std::alloc::{alloc, dealloc, Layout};
use std::slice;
use std::sync::Mutex;

use logos_tokens::contract::{
    DocumentContext, ProposedPlan, ResolveRequest, TokenSnapshot, UserIntent,
};
use serde::Deserialize;

const ALIGN: usize = 8;
const MAX_INPUT_BYTES: u32 = 4 * 1024 * 1024;
static OUTPUT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResolveInput {
    snapshot: TokenSnapshot,
    request: ResolveRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ValidateInput {
    snapshot: TokenSnapshot,
    intent: UserIntent,
    context: DocumentContext,
    plan: ProposedPlan,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeError {
    status: &'static str,
    code: &'static str,
}

#[no_mangle]
pub unsafe extern "C" fn logos_tokens_alloc(len: u32) -> *mut u8 {
    if len == 0 || len > MAX_INPUT_BYTES {
        return std::ptr::null_mut();
    }
    let layout = Layout::from_size_align_unchecked(len as usize, ALIGN);
    alloc(layout)
}

#[no_mangle]
pub unsafe extern "C" fn logos_tokens_free_input(ptr: *mut u8, len: u32) {
    if !ptr.is_null() && len > 0 {
        dealloc(ptr, Layout::from_size_align_unchecked(len as usize, ALIGN));
    }
}

#[no_mangle]
pub extern "C" fn logos_tokens_output_ptr() -> *const u8 {
    OUTPUT
        .lock()
        .expect("token WASM output mutex poisoned")
        .as_ptr()
}

#[no_mangle]
pub extern "C" fn logos_tokens_free_output() {
    OUTPUT
        .lock()
        .expect("token WASM output mutex poisoned")
        .clear();
}

#[no_mangle]
pub unsafe extern "C" fn logos_tokens_resolve_json(ptr: *const u8, len: u32) -> u32 {
    invoke(ptr, len, |bytes| {
        let input: ResolveInput = serde_json::from_slice(bytes).map_err(|_| ())?;
        serde_json::to_vec(&logos_tokens::contract_resolver::resolve(
            &input.snapshot,
            &input.request,
        ))
        .map_err(|_| ())
    })
}

#[no_mangle]
pub unsafe extern "C" fn logos_tokens_validate_plan_json(ptr: *const u8, len: u32) -> u32 {
    invoke(ptr, len, |bytes| {
        let input: ValidateInput = serde_json::from_slice(bytes).map_err(|_| ())?;
        serde_json::to_vec(&logos_tokens::plan_validation::validate_plan(
            &input.snapshot,
            &input.intent,
            &input.context,
            &input.plan,
        ))
        .map_err(|_| ())
    })
}

unsafe fn invoke(ptr: *const u8, len: u32, run: impl FnOnce(&[u8]) -> Result<Vec<u8>, ()>) -> u32 {
    let result = if ptr.is_null() || len == 0 || len > MAX_INPUT_BYTES {
        Err(())
    } else {
        run(slice::from_raw_parts(ptr, len as usize))
    };
    let bytes = result.unwrap_or_else(|_| {
        serde_json::to_vec(&BridgeError {
            status: "ERROR",
            code: "INVALID_CONTRACT_MESSAGE",
        })
        .expect("static bridge error serializes")
    });
    let length = bytes.len().min(u32::MAX as usize) as u32;
    let mut output = OUTPUT.lock().expect("token WASM output mutex poisoned");
    output.clear();
    output.extend_from_slice(&bytes[..length as usize]);
    length
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_messages_return_a_stable_controlled_error() {
        let input = b"{}";
        let length = unsafe { logos_tokens_validate_plan_json(input.as_ptr(), input.len() as u32) };
        let output = unsafe { slice::from_raw_parts(logos_tokens_output_ptr(), length as usize) };
        assert_eq!(
            output,
            br#"{"status":"ERROR","code":"INVALID_CONTRACT_MESSAGE"}"#
        );
        logos_tokens_free_output();
    }

    #[test]
    fn oversized_inputs_fail_closed_before_allocation_or_parsing() {
        assert!(unsafe { logos_tokens_alloc(MAX_INPUT_BYTES + 1) }.is_null());
        let byte = 0_u8;
        let length =
            unsafe { logos_tokens_validate_plan_json(&byte as *const u8, MAX_INPUT_BYTES + 1) };
        let output = unsafe { slice::from_raw_parts(logos_tokens_output_ptr(), length as usize) };
        assert_eq!(
            output,
            br#"{"status":"ERROR","code":"INVALID_CONTRACT_MESSAGE"}"#
        );
        logos_tokens_free_output();
    }
}
