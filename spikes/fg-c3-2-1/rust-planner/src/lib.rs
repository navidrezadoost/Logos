//! Non-production JSON protocol wrapper around logos-types' immutable planner.

use logos_types::{apply_operation, decode_and_validate_json, DocumentOperation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    protocol_version: u32,
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

pub fn handle_json(raw: &str) -> String {
    let request: Request = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(_) => return error("REQUEST_INVALID", "/"),
    };
    if request.protocol_version != PROTOCOL_VERSION {
        return error("PROTOCOL_VERSION_UNSUPPORTED", "/protocolVersion");
    }
    match request.body {
        RequestBody::Evaluate {
            document,
            operation,
        } => evaluate(document, operation),
        RequestBody::DeriveIds { addresses } => derive_ids(addresses),
        RequestBody::CheckDetachIds {
            existing_ids,
            candidate_ids,
        } => check_detach_ids(existing_ids, candidate_ids),
    }
}

fn evaluate(document_value: Value, operation_value: Value) -> String {
    let document_json = match serde_json::to_string(&document_value) {
        Ok(value) => value,
        Err(_) => return error("REQUEST_INVALID", "/document"),
    };
    let document = match decode_and_validate_json(&document_json) {
        Ok(value) => value,
        Err(diagnostics) => return json!({"protocolVersion": PROTOCOL_VERSION, "ok": false, "phase": "input-validation", "diagnostics": diagnostics}).to_string(),
    };
    let operation: DocumentOperation = match serde_json::from_value(operation_value) {
        Ok(value) => value,
        Err(_) => return error("OPERATION_INVALID", "/operation"),
    };
    match apply_operation(&document, &operation) {
        Ok(candidate) => json!({"protocolVersion": PROTOCOL_VERSION, "ok": true, "phase": "evaluated", "document": candidate, "diagnostics": []}).to_string(),
        Err(diagnostics) => json!({"protocolVersion": PROTOCOL_VERSION, "ok": false, "phase": "operation-rejected", "diagnostics": diagnostics}).to_string(),
    }
}

fn derive_ids(addresses: Vec<ProjectionAddress>) -> String {
    let mut ids = Vec::with_capacity(addresses.len());
    for address in &addresses {
        let namespace = match Uuid::parse_str(&address.document_id) {
            Ok(value) => value,
            Err(_) => return error("PROJECTION_ADDRESS_INVALID", "/addresses/documentId"),
        };
        if address.instance_id.is_empty() || address.definition_node_id.is_empty() {
            return error("PROJECTION_ADDRESS_INVALID", "/addresses");
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
            Err(_) => return error("PROJECTION_ADDRESS_INVALID", "/addresses"),
        };
        ids.push(Uuid::new_v5(&namespace, &bytes).to_string());
    }
    json!({"protocolVersion": PROTOCOL_VERSION, "ok": true, "ids": ids}).to_string()
}

fn check_detach_ids(existing: Vec<String>, candidates: Vec<String>) -> String {
    let mut seen = std::collections::HashSet::new();
    for id in existing {
        if Uuid::parse_str(&id).is_err() {
            return error("DETACH_ID_INVALID", "/existingIds");
        }
        seen.insert(id.to_ascii_lowercase());
    }
    let mut accepted = Vec::with_capacity(candidates.len());
    for id in candidates {
        if Uuid::parse_str(&id).is_err() {
            return error("DETACH_ID_INVALID", "/candidateIds");
        }
        if !seen.insert(id.to_ascii_lowercase()) {
            return error("DETACH_ID_COLLISION", "/candidateIds");
        }
        accepted.push(id);
    }
    json!({"protocolVersion": PROTOCOL_VERSION, "ok": true, "ids": accepted}).to_string()
}

fn error(code: &str, path: &str) -> String {
    json!({"protocolVersion": PROTOCOL_VERSION, "ok": false, "phase": "protocol", "diagnostics": [{"code": code, "path": path}]}).to_string()
}

#[cfg(feature = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn evaluate_request_json(input: &str) -> String {
    handle_json(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detached_ids_reject_existing_and_duplicate_collisions() {
        let result: Value = serde_json::from_str(&check_detach_ids(
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
        let a: Value = serde_json::from_str(&derive_ids(vec![base, revised])).unwrap();
        assert_ne!(a["ids"][0], a["ids"][1]);
    }
}
