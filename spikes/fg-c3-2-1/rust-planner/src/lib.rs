//! Non-production, bounded protocol wrapper around logos-types' immutable planner.

pub mod component_instance_v1;
pub mod component_instance_validator;

use logos_types::{apply_operation, decode_and_validate_json, DocumentOperation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 2;
pub const SEMANTIC_VERSION: u32 = 1;
pub const DIAGNOSTIC_VERSION: u32 = 1;
pub const MAX_REQUEST_BYTES: usize = 1 << 20;
pub const MAX_RESPONSE_BYTES: usize = 1 << 20;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    protocol_version: u32,
    request_id: String,
    semantic_version: u32,
    diagnostic_version: u32,
    #[serde(flatten)]
    body: RequestBody,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum RequestBody {
    Capabilities,
    Evaluate {
        document: Value,
        operation: Value,
    },
    DeriveIds {
        addresses: Vec<ProjectionAddress>,
    },
    CheckDetachIds {
        existing_ids: Vec<String>,
        candidate_ids: Vec<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionStep {
    pub component_id: String,
    pub through_instance_node_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotStep {
    pub property_id: String,
    pub authored_node_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionAddress {
    pub document_id: String,
    pub instance_id: String,
    pub definition_route: Vec<DefinitionStep>,
    pub definition_node_id: String,
    #[serde(default)]
    pub slot_route: Vec<SlotStep>,
    #[serde(default)]
    pub external_revision: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IdentityName<'a> {
    instance_id: &'a str,
    definition_route: &'a [DefinitionStep],
    definition_node_id: &'a str,
    slot_route: &'a [SlotStep],
    external_revision: &'a Option<String>,
}

/// Evaluate one already framed protocol request. Complete malformed JSON is a
/// request-level error; callers can safely continue with the next frame.
pub fn handle_json(raw: &str) -> String {
    let value: Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(_) => return error(None, "REQUEST_INVALID", "/", "Request is not valid JSON."),
    };
    let request_id = value
        .get("requestId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 128)
        .map(str::to_owned);
    let request: Request = match serde_json::from_value(value) {
        Ok(value) => value,
        Err(_) => {
            return error(
                request_id.as_deref(),
                "REQUEST_INVALID",
                "/",
                "Request envelope or operation fields are invalid.",
            )
        }
    };
    if request.protocol_version != PROTOCOL_VERSION {
        return error(
            Some(&request.request_id),
            "PROTOCOL_VERSION_UNSUPPORTED",
            "/protocolVersion",
            "Protocol version is not supported.",
        );
    }
    if request.semantic_version != SEMANTIC_VERSION {
        return error(
            Some(&request.request_id),
            "SEMANTIC_VERSION_UNSUPPORTED",
            "/semanticVersion",
            "Semantic operation version is not supported.",
        );
    }
    if request.diagnostic_version != DIAGNOSTIC_VERSION {
        return error(
            Some(&request.request_id),
            "DIAGNOSTIC_VERSION_UNSUPPORTED",
            "/diagnosticVersion",
            "Diagnostic format version is not supported.",
        );
    }
    if request.request_id.is_empty() || request.request_id.len() > 128 {
        return error(
            None,
            "REQUEST_ID_INVALID",
            "/requestId",
            "Request ID must be non-empty and no longer than 128 bytes.",
        );
    }
    let request_id = request.request_id;
    match request.body {
        RequestBody::Capabilities => json!({
            "protocolVersion": PROTOCOL_VERSION,
            "semanticVersion": SEMANTIC_VERSION,
            "diagnosticVersion": DIAGNOSTIC_VERSION,
            "requestId": request_id,
            "ok": true,
            "limits": {
                "maxRequestBytes": MAX_REQUEST_BYTES,
                "maxResponseBytes": MAX_RESPONSE_BYTES,
                "framing": "uint32-be-length-prefixed"
            },
            "capabilities": {
                "documentOperations": {"version": 1, "operations": ["applyOperation", "validateDocument"]},
                "projectionIdentity": {"version": 1, "operations": ["deriveIds", "checkDetachIds"]},
                "componentInstanceDomain": {"status": "not-supported-here", "owner": "TypeScript componentInstanceDomainV1"},
                "responseContract": {"version": 1, "correlation": ["requestId", "operationKind"]},
                "diagnostics": {"version": DIAGNOSTIC_VERSION}
            }
        })
        .to_string(),
        RequestBody::Evaluate {
            document,
            operation,
        } => evaluate(&request_id, document, operation),
        RequestBody::DeriveIds { addresses } => derive_ids(&request_id, addresses),
        RequestBody::CheckDetachIds {
            existing_ids,
            candidate_ids,
        } => check_detach_ids(&request_id, existing_ids, candidate_ids),
    }
}

fn evaluate(request_id: &str, document_value: Value, operation_value: Value) -> String {
    let operation_kind = operation_value
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let document_json = match serde_json::to_string(&document_value) {
        Ok(value) => value,
        Err(_) => {
            return error(
                Some(request_id),
                "REQUEST_INVALID",
                "/document",
                "Document encoding failed.",
            )
        }
    };
    let document = match decode_and_validate_json(&document_json) {
        Ok(value) => value,
        Err(diagnostics) => return json!({"protocolVersion": PROTOCOL_VERSION, "semanticVersion": SEMANTIC_VERSION, "diagnosticVersion": DIAGNOSTIC_VERSION, "requestId": request_id, "ok": false, "phase": "input-validation", "diagnostics": diagnostics}).to_string(),
    };
    let operation: DocumentOperation = match serde_json::from_value(operation_value) {
        Ok(value) => value,
        Err(_) => {
            return error(
                Some(request_id),
                "OPERATION_INVALID",
                "/operation",
                "Operation is invalid.",
            )
        }
    };
    match apply_operation(&document, &operation) {
        Ok(candidate) => json!({"protocolVersion": PROTOCOL_VERSION, "semanticVersion": SEMANTIC_VERSION, "diagnosticVersion": DIAGNOSTIC_VERSION, "requestId": request_id, "ok": true, "phase": "evaluated", "operationKind": operation_kind, "document": candidate, "diagnostics": []}).to_string(),
        Err(diagnostics) => json!({"protocolVersion": PROTOCOL_VERSION, "semanticVersion": SEMANTIC_VERSION, "diagnosticVersion": DIAGNOSTIC_VERSION, "requestId": request_id, "ok": false, "phase": "operation-rejected", "diagnostics": diagnostics}).to_string(),
    }
}

fn derive_ids(request_id: &str, addresses: Vec<ProjectionAddress>) -> String {
    let mut ids = Vec::with_capacity(addresses.len());
    for address in &addresses {
        let namespace = match Uuid::parse_str(&address.document_id) {
            Ok(value) => value,
            Err(_) => {
                return error(
                    Some(request_id),
                    "PROJECTION_ADDRESS_INVALID",
                    "/addresses/documentId",
                    "Projection document ID is invalid.",
                )
            }
        };
        if address.instance_id.is_empty() || address.definition_node_id.is_empty() {
            return error(
                Some(request_id),
                "PROJECTION_ADDRESS_INVALID",
                "/addresses",
                "Projection address is incomplete.",
            );
        }
        let name = IdentityName {
            instance_id: &address.instance_id,
            definition_route: &address.definition_route,
            definition_node_id: &address.definition_node_id,
            slot_route: &address.slot_route,
            external_revision: &address.external_revision,
        };
        let bytes = match serde_json::to_vec(&name) {
            Ok(value) => value,
            Err(_) => {
                return error(
                    Some(request_id),
                    "PROJECTION_ADDRESS_INVALID",
                    "/addresses",
                    "Projection address encoding failed.",
                )
            }
        };
        ids.push(Uuid::new_v5(&namespace, &bytes).to_string());
    }
    json!({"protocolVersion": PROTOCOL_VERSION, "semanticVersion": SEMANTIC_VERSION, "diagnosticVersion": DIAGNOSTIC_VERSION, "requestId": request_id, "ok": true, "ids": ids})
        .to_string()
}

fn check_detach_ids(request_id: &str, existing: Vec<String>, candidates: Vec<String>) -> String {
    let mut seen = std::collections::HashSet::new();
    for id in existing {
        if Uuid::parse_str(&id).is_err() {
            return error(
                Some(request_id),
                "DETACH_ID_INVALID",
                "/existingIds",
                "Existing authored ID is invalid.",
            );
        }
        seen.insert(id.to_ascii_lowercase());
    }
    let mut accepted = Vec::with_capacity(candidates.len());
    for id in candidates {
        if Uuid::parse_str(&id).is_err() {
            return error(
                Some(request_id),
                "DETACH_ID_INVALID",
                "/candidateIds",
                "Candidate authored ID is invalid.",
            );
        }
        if !seen.insert(id.to_ascii_lowercase()) {
            return error(
                Some(request_id),
                "DETACH_ID_COLLISION",
                "/candidateIds",
                "Candidate authored ID collides with an existing or candidate ID.",
            );
        }
        accepted.push(id);
    }
    json!({"protocolVersion": PROTOCOL_VERSION, "semanticVersion": SEMANTIC_VERSION, "diagnosticVersion": DIAGNOSTIC_VERSION, "requestId": request_id, "ok": true, "ids": accepted}).to_string()
}

pub fn error(request_id: Option<&str>, code: &str, path: &str, message: &str) -> String {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "semanticVersion": SEMANTIC_VERSION,
        "diagnosticVersion": DIAGNOSTIC_VERSION,
        "requestId": request_id,
        "ok": false,
        "error": {"code": code, "path": path, "message": message},
        "diagnostics": [{"code": code, "path": path}]
    })
    .to_string()
}

pub fn bounded_response(raw: String, max_bytes: usize, request_id: Option<&str>) -> String {
    if raw.len() <= max_bytes {
        raw
    } else {
        error(
            request_id,
            "RESPONSE_TOO_LARGE",
            "/",
            "Planner response exceeds the protocol limit.",
        )
    }
}

#[cfg(feature = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn evaluate_request_json(input: &str) -> String {
    bounded_response(
        handle_json(input),
        MAX_RESPONSE_BYTES,
        serde_json::from_str::<Value>(input)
            .ok()
            .and_then(|value| {
                value
                    .get("requestId")
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty() && id.len() <= 128)
                    .map(str::to_owned)
            })
            .as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(kind: &str) -> Value {
        json!({"protocolVersion": 2, "semanticVersion": 1, "diagnosticVersion": 1, "requestId": "unit-1", "kind": kind})
    }

    #[test]
    fn capabilities_separate_document_and_component_domain_ownership() {
        let result: Value =
            serde_json::from_str(&handle_json(&envelope("capabilities").to_string())).unwrap();
        assert_eq!(result["capabilities"]["documentOperations"]["version"], 1);
        assert_eq!(
            result["capabilities"]["componentInstanceDomain"]["status"],
            "not-supported-here"
        );
        assert_eq!(result["capabilities"]["responseContract"]["version"], 1);
    }

    #[test]
    fn version_and_malformed_requests_fail_with_correlated_errors() {
        let mut unsupported = envelope("capabilities");
        unsupported["protocolVersion"] = json!(1);
        let response: Value = serde_json::from_str(&handle_json(&unsupported.to_string())).unwrap();
        assert_eq!(response["requestId"], "unit-1");
        assert_eq!(response["error"]["code"], "PROTOCOL_VERSION_UNSUPPORTED");
        let malformed: Value = serde_json::from_str(&handle_json("{bad")).unwrap();
        assert_eq!(malformed["error"]["code"], "REQUEST_INVALID");
    }

    #[test]
    fn bounded_response_replaces_large_payload_with_structured_error() {
        let result: Value =
            serde_json::from_str(&bounded_response("x".repeat(300), 100, Some("unit-1"))).unwrap();
        assert_eq!(result["error"]["code"], "RESPONSE_TOO_LARGE");
    }

    #[test]
    fn detach_ids_reject_existing_and_duplicate_collisions() {
        let result: Value = serde_json::from_str(&check_detach_ids(
            "unit-1",
            vec!["11111111-1111-4111-8111-111111111111".into()],
            vec!["11111111-1111-4111-8111-111111111111".into()],
        ))
        .unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(result["diagnostics"][0]["code"], "DETACH_ID_COLLISION");
    }

    #[test]
    fn source_revision_participates_only_when_explicitly_pinned() {
        let base = ProjectionAddress {
            document_id: "11111111-1111-4111-8111-111111111111".into(),
            instance_id: "22222222-2222-4222-8222-222222222222".into(),
            definition_route: vec![],
            definition_node_id: "33333333-3333-4333-8333-333333333333".into(),
            slot_route: vec![],
            external_revision: None,
        };
        let mut revised = base.clone();
        revised.external_revision = Some("r2".into());
        let a: Value = serde_json::from_str(&derive_ids("unit-1", vec![base, revised])).unwrap();
        assert_ne!(a["ids"][0], a["ids"][1]);
    }
}
