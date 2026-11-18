//! Pure, isolated semantic validator for the TypeScript Component/Instance v1
//! candidate contract. It intentionally accepts parsed JSON values so
//! structural errors retain TypeScript-compatible diagnostics instead of
//! becoming serde failures. The protocol-v2 caller applies bounded strict
//! parsing first; this module remains outside Document and persistence.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

thread_local! {
    static DIAGNOSTIC_MAX_COUNT: Cell<usize> = const { Cell::new(usize::MAX) };
    static DIAGNOSTIC_MAX_BYTES: Cell<usize> = const { Cell::new(usize::MAX) };
    static DIAGNOSTIC_BYTES: Cell<usize> = const { Cell::new(0) };
    static DIAGNOSTIC_OVERFLOW: Cell<bool> = const { Cell::new(false) };
    static EXPANSION_MAX_STEPS: Cell<usize> = const { Cell::new(usize::MAX) };
    static EXPANSION_STEPS: Cell<usize> = const { Cell::new(0) };
    static EXPANSION_OVERFLOW: Cell<bool> = const { Cell::new(false) };
    static SEMANTIC_MAX_DEPTH: Cell<usize> = const { Cell::new(usize::MAX) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationLimit {
    Diagnostics,
    ExpansionWork,
}

struct ValidationBudgetGuard {
    old: (usize, usize, usize, usize),
}

impl ValidationBudgetGuard {
    fn new(max_count: usize, max_bytes: usize, max_expansion_steps: usize) -> Self {
        let old = (
            DIAGNOSTIC_MAX_COUNT.with(|value| value.replace(max_count)),
            DIAGNOSTIC_MAX_BYTES.with(|value| value.replace(max_bytes)),
            EXPANSION_MAX_STEPS.with(|value| value.replace(max_expansion_steps)),
            SEMANTIC_MAX_DEPTH.with(|value| value.replace(64)),
        );
        // Count the serialized JSON diagnostic array, including its brackets
        // and separators. This matches the Go response validator's byte rule.
        DIAGNOSTIC_BYTES.with(|value| value.set(2));
        DIAGNOSTIC_OVERFLOW.with(|value| value.set(false));
        EXPANSION_STEPS.with(|value| value.set(0));
        EXPANSION_OVERFLOW.with(|value| value.set(false));
        Self { old }
    }

    fn exceeded(&self) -> Option<ValidationLimit> {
        if EXPANSION_OVERFLOW.with(Cell::get) {
            Some(ValidationLimit::ExpansionWork)
        } else if DIAGNOSTIC_OVERFLOW.with(Cell::get) {
            Some(ValidationLimit::Diagnostics)
        } else {
            None
        }
    }
}

impl Drop for ValidationBudgetGuard {
    fn drop(&mut self) {
        DIAGNOSTIC_MAX_COUNT.with(|value| value.set(self.old.0));
        DIAGNOSTIC_MAX_BYTES.with(|value| value.set(self.old.1));
        EXPANSION_MAX_STEPS.with(|value| value.set(self.old.2));
        SEMANTIC_MAX_DEPTH.with(|value| value.set(self.old.3));
    }
}

fn consume_expansion_step() -> bool {
    EXPANSION_STEPS.with(|steps| {
        let next = steps.get().saturating_add(1);
        let max = EXPANSION_MAX_STEPS.with(Cell::get);
        if next > max {
            EXPANSION_OVERFLOW.with(|overflow| overflow.set(true));
            false
        } else {
            steps.set(next);
            true
        }
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub path: String,
    pub severity: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    pub ok: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub document: Option<Value>,
}

fn diagnostic(
    ds: &mut Vec<Diagnostic>,
    code: &str,
    path: impl Into<String>,
    detail: impl Into<String>,
) {
    push_diagnostic(
        ds,
        Diagnostic {
            code: code.into(),
            path: path.into(),
            severity: "error".into(),
            detail: detail.into(),
        },
    );
}
fn warning(
    ds: &mut Vec<Diagnostic>,
    code: &str,
    path: impl Into<String>,
    detail: impl Into<String>,
) {
    push_diagnostic(
        ds,
        Diagnostic {
            code: code.into(),
            path: path.into(),
            severity: "warning".into(),
            detail: detail.into(),
        },
    );
}
fn push_diagnostic(ds: &mut Vec<Diagnostic>, value: Diagnostic) {
    let encoded_bytes = serde_json::to_vec(&value).map_or(usize::MAX, |json| json.len());
    let count_limit = DIAGNOSTIC_MAX_COUNT.with(Cell::get);
    let byte_limit = DIAGNOSTIC_MAX_BYTES.with(Cell::get);
    let current_bytes = DIAGNOSTIC_BYTES.with(Cell::get);
    let separator_bytes = usize::from(!ds.is_empty());
    if ds.len() >= count_limit
        || current_bytes
            .saturating_add(separator_bytes)
            .saturating_add(encoded_bytes)
            > byte_limit
    {
        DIAGNOSTIC_OVERFLOW.with(|overflow| overflow.set(true));
        return;
    }
    DIAGNOSTIC_BYTES.with(|bytes| {
        bytes.set(
            current_bytes
                .saturating_add(separator_bytes)
                .saturating_add(encoded_bytes),
        )
    });
    ds.push(value);
}
fn object(v: &Value) -> Option<&Map<String, Value>> {
    v.as_object()
}
fn s<'a>(v: &'a Value) -> Option<&'a str> {
    v.as_str()
}
fn nonempty(v: &Value) -> bool {
    s(v).is_some_and(|x| !x.is_empty())
}
fn arr(v: &Value) -> Option<&Vec<Value>> {
    v.as_array()
}
fn string_array(v: &Value) -> Option<Vec<&str>> {
    arr(v)?
        .iter()
        .map(|item| item.as_str().filter(|s| !s.is_empty()))
        .collect()
}
fn own<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    object(v)?.get(key)
}
fn has(v: &Value, key: &str) -> bool {
    object(v).is_some_and(|m| m.contains_key(key))
}
fn rec(v: &Value) -> bool {
    object(v).is_some()
}
fn keys(v: &Value) -> Vec<String> {
    object(v)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}
fn js_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}
fn js_sorted_keys(v: &Value) -> Vec<String> {
    let mut result = keys(v);
    result.sort_by(|a, b| js_cmp(a, b));
    result
}
fn map_keys_js(v: &Map<String, Value>) -> Vec<String> {
    let mut result: Vec<String> = v.keys().cloned().collect();
    result.sort_by(|a, b| js_cmp(a, b));
    result
}
fn fields(v: &Value, allowed: &[&str]) -> Option<String> {
    js_sorted_keys(v)
        .into_iter()
        .find(|k| !allowed.contains(&k.as_str()))
}
fn strict_record(v: &Value, path: &str, allowed: &[&str], ds: &mut Vec<Diagnostic>) -> bool {
    if !rec(v) {
        diagnostic(
            ds,
            "COMPONENT_RECORD_MALFORMED",
            path,
            "Expected a plain JSON object.",
        );
        return false;
    }
    if let Some(extra) = fields(v, allowed) {
        diagnostic(
            ds,
            "COMPONENT_UNKNOWN_FIELD",
            format!("{path}.{extra}"),
            "Unknown fields are rejected by component-domain v1.",
        );
        false
    } else {
        true
    }
}
fn path_member(base: &str, key: &str) -> String {
    format!("{base}.{key}")
}

fn validate_reference(
    refv: &Value,
    path: &str,
    ds: &mut Vec<Diagnostic>,
    component_ids: &BTreeSet<String>,
    set_ids: &BTreeSet<String>,
) {
    let Some(kind) = own(refv, "kind").and_then(Value::as_str) else {
        diagnostic(
            ds,
            "COMPONENT_REFERENCE_INVALID",
            path,
            "Expected an explicit source reference.",
        );
        return;
    };
    match kind {
        "local-component" => {
            if !strict_record(refv, path, &["kind", "componentId"], ds)
                || !own(refv, "componentId").is_some_and(nonempty)
            {
                diagnostic(
                    ds,
                    "COMPONENT_REFERENCE_INVALID",
                    path,
                    "Invalid local component reference.",
                );
                return;
            }
            let id = own(refv, "componentId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !component_ids.contains(id) {
                diagnostic(
                    ds,
                    "COMPONENT_REFERENCE_MISSING",
                    path,
                    format!("Unknown component ID {id}."),
                );
            }
        }
        "local-set" => {
            if !strict_record(refv, path, &["kind", "componentSetId"], ds)
                || !own(refv, "componentSetId").is_some_and(nonempty)
            {
                diagnostic(
                    ds,
                    "COMPONENT_REFERENCE_INVALID",
                    path,
                    "Invalid local component-set reference.",
                );
                return;
            }
            let id = own(refv, "componentSetId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !set_ids.contains(id) {
                diagnostic(
                    ds,
                    "COMPONENT_REFERENCE_MISSING",
                    path,
                    format!("Unknown component-set ID {id}."),
                );
            }
        }
        "external" => {
            if !strict_record(
                refv,
                path,
                &["kind", "fileId", "componentId", "revision"],
                ds,
            ) || !own(refv, "fileId").is_some_and(nonempty)
                || !own(refv, "componentId").is_some_and(nonempty)
                || !own(refv, "revision").is_some_and(|r| r.is_null() || nonempty(r))
            {
                diagnostic(
                    ds,
                    "COMPONENT_REFERENCE_INVALID",
                    path,
                    "External references require fileId, componentId, and nullable revision.",
                );
            } else {
                warning(ds, "COMPONENT_REFERENCE_UNRESOLVED", path, "External source identity is explicit but cannot be resolved from this isolated document.");
            }
        }
        "unresolved" => {
            if !strict_record(refv, path, &["kind", "locator"], ds) || !has(refv, "locator") {
                diagnostic(
                    ds,
                    "COMPONENT_REFERENCE_INVALID",
                    path,
                    "Unresolved source locator must be JSON data.",
                );
            } else {
                warning(
                    ds,
                    "COMPONENT_REFERENCE_UNRESOLVED",
                    path,
                    "Unresolved source locator is preserved; resolution is blocked.",
                );
            }
        }
        _ => diagnostic(
            ds,
            "COMPONENT_REFERENCE_INVALID",
            path,
            "Unknown reference kind.",
        ),
    }
}

fn validate_node_graph(
    nodes_value: &Value,
    roots_value: &Value,
    path: &str,
    ds: &mut Vec<Diagnostic>,
    kinds: &[&str],
) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let (Some(nodes), Some(roots)) = (object(nodes_value), string_array(roots_value)) else {
        diagnostic(
            ds,
            "COMPONENT_NODE_GRAPH_INVALID",
            path,
            "Expected a node map and ordered root ID array.",
        );
        return ids;
    };
    for (id, n) in nodes {
        let (Some(kind), Some(parent), Some(children)) = (
            own(n, "kind").and_then(Value::as_str),
            own(n, "parentId"),
            own(n, "childIds").and_then(string_array),
        ) else {
            diagnostic(
                ds,
                "COMPONENT_NODE_GRAPH_INVALID",
                path_member(&path_member(path, "nodes"), id),
                "Malformed node, key/ID mismatch, unsupported kind, or malformed membership.",
            );
            continue;
        };
        if id.is_empty()
            || own(n, "id").and_then(Value::as_str) != Some(id)
            || !kinds.contains(&kind)
            || !(parent.is_null() || nonempty(parent))
        {
            diagnostic(
                ds,
                "COMPONENT_NODE_GRAPH_INVALID",
                path_member(&path_member(path, "nodes"), id),
                "Malformed node, key/ID mismatch, unsupported kind, or malformed membership.",
            );
            continue;
        }
        let allowed: &[&str] = if kind == "instance" {
            &[
                "id",
                "kind",
                "parentId",
                "childIds",
                "source",
                "variantSelection",
            ]
        } else {
            &["id", "kind", "parentId", "childIds"]
        };
        if let Some(extra) = fields(n, allowed) {
            diagnostic(
                ds,
                "COMPONENT_UNKNOWN_FIELD",
                format!("{}.{}", path_member(&path_member(path, "nodes"), id), extra),
                "Unknown node fields are rejected by component-domain v1.",
            );
        }
        let node_path = path_member(&path_member(path, "nodes"), id);
        if kind == "instance"
            && (!has(n, "source")
                || !own(n, "variantSelection").is_some_and(rec)
                || own(n, "variantSelection")
                    .is_some_and(|x| object(x).unwrap().values().any(|v| !nonempty(v))))
        {
            diagnostic(
                ds,
                "COMPONENT_REFERENCE_INVALID",
                format!("{node_path}.source"),
                "Nested instance needs an explicit source and string-valued variant selection map.",
            );
        }
        if kind == "shape" && has(n, "source") {
            diagnostic(
                ds,
                "COMPONENT_NODE_GRAPH_INVALID",
                format!("{node_path}.source"),
                "Only instance nodes may carry a source reference.",
            );
        }
        if ["shape", "text", "instance"].contains(&kind) && !children.is_empty() {
            diagnostic(
                ds,
                "COMPONENT_NODE_GRAPH_INVALID",
                format!("{node_path}.childIds"),
                "Only container nodes may own structural children.",
            );
        }
        ids.insert(id.clone());
    }
    let mut root_seen = BTreeSet::new();
    for root in &roots {
        if !root_seen.insert(*root) || !ids.contains(*root) {
            diagnostic(
                ds,
                "COMPONENT_NODE_GRAPH_INVALID",
                format!("{path}.rootNodeIds"),
                "Roots must be unique node IDs in the graph.",
            );
            break;
        }
    }
    let mut memberships = BTreeMap::<String, String>::new();
    fn walk(
        id: &str,
        parent: Option<&str>,
        stack: &mut Vec<String>,
        nodes: &Map<String, Value>,
        ids: &BTreeSet<String>,
        memberships: &mut BTreeMap<String, String>,
        path: &str,
        ds: &mut Vec<Diagnostic>,
    ) -> bool {
        if !consume_expansion_step() || stack.len() >= SEMANTIC_MAX_DEPTH.with(Cell::get) {
            EXPANSION_OVERFLOW.with(|overflow| overflow.set(true));
            return false;
        }
        let Some(node) = nodes.get(id) else {
            return true;
        };
        if stack.iter().any(|x| x == id) {
            diagnostic(
                ds,
                "COMPONENT_NODE_GRAPH_INVALID",
                path_member(&path_member(path, "nodes"), id),
                "Node containment cycle detected.",
            );
            return true;
        }
        let expected = parent;
        let actual = own(node, "parentId").and_then(Value::as_str);
        let parent_matches = if expected.is_none() {
            own(node, "parentId").is_some_and(Value::is_null)
        } else {
            actual == expected
        };
        if !parent_matches {
            diagnostic(
                ds,
                "COMPONENT_NODE_GRAPH_INVALID",
                format!("{}.parentId", path_member(&path_member(path, "nodes"), id)),
                "Parent and ordered child membership are not reciprocal.",
            );
        }
        if memberships.contains_key(id) {
            diagnostic(
                ds,
                "COMPONENT_NODE_GRAPH_INVALID",
                path_member(&path_member(path, "nodes"), id),
                "Node has multiple owners.",
            );
        }
        memberships.insert(id.to_string(), parent.unwrap_or("<root>").to_string());
        stack.push(id.to_string());
        if let Some(children) = nodes
            .get(id)
            .and_then(|n| own(n, "childIds"))
            .and_then(string_array)
        {
            for child in children {
                if !ids.contains(child) {
                    diagnostic(
                        ds,
                        "COMPONENT_NODE_GRAPH_INVALID",
                        format!("{}.childIds", path_member(&path_member(path, "nodes"), id)),
                        format!("Missing child {child}."),
                    );
                } else {
                    if !walk(child, Some(id), stack, nodes, ids, memberships, path, ds) {
                        stack.pop();
                        return false;
                    }
                }
            }
        }
        stack.pop();
        true
    }
    for root in roots {
        if !walk(
            root,
            None,
            &mut Vec::new(),
            nodes,
            &ids,
            &mut memberships,
            path,
            ds,
        ) {
            break;
        }
    }
    if memberships.len() != ids.len() {
        diagnostic(
            ds,
            "COMPONENT_NODE_GRAPH_INVALID",
            path,
            "Graph contains unreachable nodes.",
        );
    }
    ids
}

fn validate_property(
    value: &Value,
    key: &str,
    path: &str,
    node_ids: &BTreeSet<String>,
    ds: &mut Vec<Diagnostic>,
    set_context: bool,
) {
    if !rec(value)
        || own(value, "id").and_then(Value::as_str) != Some(key)
        || !own(value, "name").is_some_and(nonempty)
        || own(value, "kind").and_then(Value::as_str).is_none()
    {
        diagnostic(
            ds,
            "COMPONENT_PROPERTY_INVALID",
            path,
            "Malformed property record or key/ID mismatch.",
        );
        return;
    }
    let kind = own(value, "kind").and_then(Value::as_str).unwrap();
    if kind == "variant" {
        if !set_context {
            diagnostic(
                ds,
                "COMPONENT_PROPERTY_KIND_UNSUPPORTED",
                format!("{path}.kind"),
                "Variant properties belong to a Component Set in v1.",
            );
            return;
        }
        let vals = own(value, "values").and_then(string_array);
        let valid = strict_record(
            value,
            path,
            &["id", "name", "kind", "values", "defaultValue"],
            ds,
        ) && vals
            .as_ref()
            .is_some_and(|v| !v.is_empty() && v.iter().collect::<BTreeSet<_>>().len() == v.len())
            && own(value, "defaultValue").is_some_and(nonempty)
            && vals.as_ref().is_some_and(|v| {
                own(value, "defaultValue")
                    .and_then(Value::as_str)
                    .is_some_and(|d| v.contains(&d))
            });
        if !valid {
            diagnostic(
                ds,
                "COMPONENT_VARIANT_INVALID",
                path,
                "Variant property values must be unique and include the default.",
            );
        }
        return;
    }
    if set_context {
        diagnostic(
            ds,
            "COMPONENT_VARIANT_INVALID",
            path,
            "Component Set properties must have kind variant.",
        );
        return;
    }
    if !own(value, "targetNodeId").is_some_and(nonempty)
        || !own(value, "targetNodeId")
            .and_then(Value::as_str)
            .is_some_and(|id| node_ids.contains(id))
    {
        diagnostic(
            ds,
            "COMPONENT_PROPERTY_INVALID",
            format!("{path}.targetNodeId"),
            "Property target is missing from this definition.",
        );
    }
    let (fields, valid, detail) = match kind {
        "boolean" => (
            &["id", "name", "kind", "targetNodeId", "defaultValue"][..],
            own(value, "defaultValue").is_some_and(Value::is_boolean),
            "Boolean default must be boolean.",
        ),
        "text" => (
            &["id", "name", "kind", "targetNodeId", "defaultValue"][..],
            own(value, "defaultValue").is_some_and(Value::is_string),
            "Text default must be a string.",
        ),
        "instance-swap" => {
            let allowed = own(value, "allowedComponentIds").and_then(string_array);
            let ok = allowed.as_ref().is_some_and(|v| {
                !v.is_empty() && v.iter().collect::<BTreeSet<_>>().len() == v.len()
            }) && own(value, "defaultComponentId").is_some_and(nonempty)
                && allowed.as_ref().is_some_and(|v| {
                    own(value, "defaultComponentId")
                        .and_then(Value::as_str)
                        .is_some_and(|d| v.contains(&d))
                });
            (
                &[
                    "id",
                    "name",
                    "kind",
                    "targetNodeId",
                    "allowedComponentIds",
                    "defaultComponentId",
                ][..],
                ok,
                "Instance-swap options must be unique and include the default component.",
            )
        }
        "slot" => {
            if !strict_record(
                value,
                path,
                &[
                    "id",
                    "name",
                    "kind",
                    "targetNodeId",
                    "guidance",
                    "onlyAllowPreferredInstances",
                    "hardLimits",
                ],
                ds,
            ) {
                return;
            }
            let only = own(value, "onlyAllowPreferredInstances");
            if only.is_some_and(|v| !v.is_boolean()) {
                diagnostic(
                    ds,
                    "COMPONENT_PROPERTY_INVALID",
                    format!("{path}.onlyAllowPreferredInstances"),
                    "Only-allow-preferred-instances must be boolean.",
                );
            }
            if only.and_then(Value::as_bool) == Some(true)
                && (!own(value, "guidance").is_some_and(rec)
                    || own(value, "guidance")
                        .and_then(|v| own(v, "preferredComponentIds"))
                        .and_then(string_array)
                        .is_none_or(|v| v.is_empty()))
            {
                diagnostic(
                    ds,
                    "COMPONENT_PROPERTY_INVALID",
                    format!("{path}.onlyAllowPreferredInstances"),
                    "An explicit allow-list requires at least one preferred component ID.",
                );
            }
            for key in ["guidance", "hardLimits"] {
                let Some(limits) = own(value, key) else {
                    continue;
                };
                let allowed: &[&str] = if key == "guidance" {
                    &["minimumEntries", "maximumEntries", "preferredComponentIds"]
                } else {
                    &["minimumEntries", "maximumEntries"]
                };
                let numeric = |field: &str| {
                    own(limits, field).is_none_or(|v| js_nonnegative_integer(v).is_some())
                };
                let min = own(limits, "minimumEntries").and_then(js_nonnegative_integer);
                let max = own(limits, "maximumEntries").and_then(js_nonnegative_integer);
                let valid = rec(limits)
                    && fields(limits, allowed).is_none()
                    && numeric("minimumEntries")
                    && numeric("maximumEntries")
                    && min.zip(max).is_none_or(|(a, b)| a <= b)
                    && (key != "guidance"
                        || own(limits, "preferredComponentIds")
                            .is_none_or(|v| string_array(v).is_some()));
                if !valid {
                    diagnostic(
                        ds,
                        "COMPONENT_PROPERTY_INVALID",
                        format!("{path}.{key}"),
                        "Invalid slot guidance or explicit hard limits.",
                    );
                }
            }
            return;
        }
        _ => {
            diagnostic(
                ds,
                "COMPONENT_PROPERTY_KIND_UNSUPPORTED",
                format!("{path}.kind"),
                format!("Unsupported property kind: {kind}."),
            );
            return;
        }
    };
    if !strict_record(value, path, fields, ds) || !valid {
        diagnostic(ds, "COMPONENT_PROPERTY_INVALID", path, detail);
    }
}

fn validate_property_target(property: &Value, nodes: &Value, path: &str, ds: &mut Vec<Diagnostic>) {
    let Some(id) = own(property, "targetNodeId").and_then(Value::as_str) else {
        return;
    };
    let Some(target) = own(nodes, id) else {
        return;
    };
    let Some(kind) = own(property, "kind").and_then(Value::as_str) else {
        return;
    };
    let target_kind = own(target, "kind").and_then(Value::as_str);
    let message = match kind {
        "text" if target_kind != Some("text") => Some("Text property must target a text node."),
        "instance-swap" if target_kind != Some("instance") => {
            Some("Instance-swap property must target a nested instance node.")
        }
        "slot" if target_kind != Some("container") => {
            Some("Slot property must target a container node.")
        }
        _ => None,
    };
    if let Some(detail) = message {
        diagnostic(
            ds,
            "COMPONENT_PROPERTY_INVALID",
            format!("{path}.targetNodeId"),
            detail,
        );
    }
}

fn validate_nested_selection(
    refv: &Value,
    selection: &Value,
    path: &str,
    ds: &mut Vec<Diagnostic>,
    components: &Value,
    sets: &Value,
) {
    if !rec(selection) || object(selection).is_some_and(|m| m.values().any(|v| !nonempty(v))) {
        diagnostic(
            ds,
            "COMPONENT_VARIANT_SELECTION_INVALID",
            format!("{path}.variantSelection"),
            "Nested instance selection must be a string-valued map.",
        );
        return;
    }
    match own(refv, "kind").and_then(Value::as_str) {
        Some("local-set") => {
            let Some(set_id) = own(refv, "componentSetId").and_then(Value::as_str) else {
                return;
            };
            let Some(set) = own(sets, set_id) else {
                return;
            };
            let (Some(props), Some(members)) = (
                own(set, "properties").and_then(object),
                own(set, "memberComponentIds").and_then(arr),
            ) else {
                return;
            };
            let selection_obj = object(selection).unwrap();
            let valid = selection_obj.len() == props.len()
                && selection_obj.iter().all(|(key, value)| {
                    own(&Value::Object(props.clone()), key)
                        .and_then(|p| own(p, "values"))
                        .and_then(arr)
                        .is_some_and(|v| v.contains(value))
                });
            if !valid {
                diagnostic(
                    ds,
                    "COMPONENT_VARIANT_SELECTION_INVALID",
                    format!("{path}.variantSelection"),
                    "Nested set reference needs one allowed value for every set property.",
                );
                return;
            }
            let matches = members
                .iter()
                .filter(|member_id| {
                    let id = member_id.as_str().unwrap_or_default();
                    let Some(vals) = own(components, id)
                        .and_then(|m| own(m, "variantValues"))
                        .and_then(object)
                    else {
                        return false;
                    };
                    selection_obj
                        .iter()
                        .all(|(key, value)| vals.get(key) == Some(value))
                })
                .count();
            if matches > 1 {
                diagnostic(
                    ds,
                    "COMPONENT_AMBIGUOUS_SOURCE",
                    path,
                    "Nested set reference matches multiple component definitions.",
                );
            } else if matches == 0 {
                diagnostic(
                    ds,
                    "COMPONENT_VARIANT_SELECTION_INVALID",
                    format!("{path}.variantSelection"),
                    "Nested set reference does not resolve to a member.",
                );
            }
        }
        Some("local-component") => {
            if !object(selection).unwrap().is_empty() {
                diagnostic(
                    ds,
                    "COMPONENT_VARIANT_SELECTION_INVALID",
                    format!("{path}.variantSelection"),
                    "A direct component reference cannot also carry set selection.",
                );
            }
        }
        _ => {
            if !object(selection).unwrap().is_empty() {
                diagnostic(
                    ds,
                    "COMPONENT_VARIANT_SELECTION_INVALID",
                    format!("{path}.variantSelection"),
                    "External or unresolved references cannot be locally variant-selected.",
                );
            }
        }
    }
}

fn resolve_local(
    refv: &Value,
    selection: &Value,
    components: &Value,
    sets: &Value,
) -> Option<String> {
    match own(refv, "kind").and_then(Value::as_str)? {
        "local-component" => {
            let id = own(refv, "componentId")?.as_str()?;
            own(components, id).map(|_| id.to_string())
        }
        "local-set" => {
            let sid = own(refv, "componentSetId")?.as_str()?;
            let set = own(sets, sid)?;
            let members = own(set, "memberComponentIds")?.as_array()?;
            let props = own(set, "properties")?.as_object()?;
            let sel = selection.as_object()?;
            let found: Vec<String> = members
                .iter()
                .filter_map(|id| {
                    let id = id.as_str()?;
                    let vals = own(components, id)?.get("variantValues")?.as_object()?;
                    (sel.len() == props.len() && sel.iter().all(|(k, v)| vals.get(k) == Some(v)))
                        .then(|| id.to_string())
                })
                .collect();
            (found.len() == 1).then(|| found[0].clone())
        }
        _ => None,
    }
}

fn validate_slot_content(
    content: &Value,
    path: &str,
    ds: &mut Vec<Diagnostic>,
    components: &Value,
    sets: &Value,
    component_ids: &BTreeSet<String>,
    set_ids: &BTreeSet<String>,
) -> BTreeSet<String> {
    if !rec(content) || fields(content, &["rootNodeIds", "nodes"]).is_some() {
        diagnostic(
            ds,
            "COMPONENT_SLOT_INVALID",
            path,
            "Slot content must contain only ordered roots and nodes.",
        );
        return BTreeSet::new();
    }
    let ids = validate_node_graph(
        own(content, "nodes").unwrap_or(&Value::Null),
        own(content, "rootNodeIds").unwrap_or(&Value::Null),
        path,
        ds,
        &["container", "shape", "text", "instance"],
    );
    if let Some(nodes) = own(content, "nodes").and_then(object) {
        for (id, node) in nodes {
            if own(node, "kind").and_then(Value::as_str) == Some("instance") {
                let node_path = path_member(&path_member(path, "nodes"), id);
                if fields(
                    node,
                    &[
                        "id",
                        "kind",
                        "parentId",
                        "childIds",
                        "source",
                        "variantSelection",
                    ],
                )
                .is_some()
                    || !has(node, "source")
                    || !own(node, "variantSelection").is_some_and(rec)
                {
                    diagnostic(
                        ds,
                        "COMPONENT_SLOT_INVALID",
                        node_path,
                        "Malformed slot instance node.",
                    );
                } else {
                    let source = own(node, "source").unwrap();
                    let selection = own(node, "variantSelection").unwrap();
                    validate_reference(
                        source,
                        &format!("{node_path}.source"),
                        ds,
                        component_ids,
                        set_ids,
                    );
                    validate_nested_selection(source, selection, &node_path, ds, components, sets);
                }
            } else if fields(node, &["id", "kind", "parentId", "childIds"]).is_some() {
                diagnostic(
                    ds,
                    "COMPONENT_SLOT_INVALID",
                    path_member(&path_member(path, "nodes"), id),
                    "Unknown slot-content node fields are rejected.",
                );
            }
        }
    }
    ids
}

fn json_value(v: &Value) -> bool {
    match v {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => true,
        Value::Array(a) => a.iter().all(json_value),
        Value::Object(o) => o.values().all(json_value),
    }
}
fn js_integer(v: &Value) -> Option<f64> {
    let n = v.as_f64()?;
    (n.is_finite() && n.fract() == 0.0).then_some(n)
}
fn js_nonnegative_integer(v: &Value) -> Option<f64> {
    js_integer(v).filter(|n| *n >= 0.0)
}
fn normalize_js_numbers(value: &mut Value) {
    match value {
        Value::Number(number) if number.is_i64() || number.is_u64() => {
            let exceeds_safe = number
                .as_i64()
                .is_some_and(|n| n.unsigned_abs() > 9_007_199_254_740_991)
                || number.as_u64().is_some_and(|n| n > 9_007_199_254_740_991);
            if exceeds_safe {
                if let Some(n) = number.as_f64().and_then(serde_json::Number::from_f64) {
                    *number = n;
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalize_js_numbers),
        Value::Object(items) => items.values_mut().for_each(normalize_js_numbers),
        _ => {}
    }
}
fn override_type_ok(field: &str, value: &Value) -> bool {
    if !json_value(value) {
        return false;
    }
    match field {
        "name" | "text" => value.is_string(),
        "fills" | "strokes" | "effects" | "layoutGuides" | "exportSettings" => value.is_array(),
        _ => false,
    }
}

fn validate_expansion_cycles(
    components: &Value,
    sets: &Value,
    instances: &Value,
    ds: &mut Vec<Diagnostic>,
) -> bool {
    let Some(instances) = object(instances) else {
        return true;
    };
    for instance_id in js_sorted_keys(&Value::Object(instances.clone())) {
        if !consume_expansion_step() {
            return false;
        }
        let instance = &instances[&instance_id];
        let (Some(source), Some(selection)) =
            (own(instance, "source"), own(instance, "variantSelection"))
        else {
            continue;
        };
        let Some(root) = resolve_local(source, selection, components, sets) else {
            continue;
        };
        let root_path = format!("$.instances.{instance_id}.source");
        let root_properties = own(instance, "propertyValues").unwrap_or(&Value::Null);
        let root_slots = own(instance, "slotContent").unwrap_or(&Value::Null);
        fn expand(
            component_id: &str,
            path: &str,
            stack: &mut Vec<String>,
            root_id: &str,
            root_properties: &Value,
            root_slots: &Value,
            components: &Value,
            sets: &Value,
            ds: &mut Vec<Diagnostic>,
        ) -> bool {
            if !consume_expansion_step() || stack.len() >= SEMANTIC_MAX_DEPTH.with(Cell::get) {
                EXPANSION_OVERFLOW.with(|overflow| overflow.set(true));
                return false;
            }
            if stack.iter().any(|x| x == component_id) {
                let mut chain = stack.clone();
                chain.push(component_id.into());
                diagnostic(
                    ds,
                    "COMPONENT_REFERENCE_CYCLE",
                    path,
                    format!("Active component expansion cycle: {}.", chain.join(" -> ")),
                );
                return true;
            }
            let Some(component) = own(components, component_id) else {
                return true;
            };
            let (Some(properties), Some(nodes)) = (
                own(component, "properties").and_then(object),
                own(component, "nodes").and_then(object),
            ) else {
                return true;
            };
            stack.push(component_id.to_string());
            for node_id in map_keys_js(nodes) {
                if !consume_expansion_step() {
                    stack.pop();
                    return false;
                }
                let node = &nodes[&node_id];
                if own(node, "kind").and_then(Value::as_str) != Some("instance") {
                    continue;
                }
                let source_node_path = format!("{path}.nodes.{node_id}.source");
                let mut swap = None;
                for prop_id in map_keys_js(properties) {
                    if !consume_expansion_step() {
                        stack.pop();
                        return false;
                    }
                    let p = &properties[&prop_id];
                    if own(p, "kind").and_then(Value::as_str) == Some("instance-swap")
                        && own(p, "targetNodeId").and_then(Value::as_str) == Some(node_id.as_str())
                    {
                        swap = Some((prop_id, p));
                        break;
                    }
                }
                let mut target = None;
                if let Some((prop_id, prop)) = swap {
                    let selected = if component_id == root_id {
                        own(root_properties, &prop_id)
                    } else {
                        None
                    };
                    if let Some(id) = selected
                        .and_then(|v| {
                            own(v, "kind")
                                .filter(|k| k.as_str() == Some("local-component"))
                                .and_then(|_| own(v, "componentId"))
                        })
                        .and_then(Value::as_str)
                    {
                        if own(prop, "allowedComponentIds")
                            .and_then(arr)
                            .is_some_and(|v| v.iter().any(|x| x.as_str() == Some(id)))
                        {
                            target = Some(id.to_string());
                        }
                    }
                    if target.is_none() {
                        target = own(prop, "defaultComponentId")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                    }
                }
                if target.is_none() {
                    if let (Some(src), Some(sel)) =
                        (own(node, "source"), own(node, "variantSelection"))
                    {
                        target = resolve_local(src, sel, components, sets);
                    }
                }
                if let Some(id) = target {
                    if !expand(
                        &id,
                        &source_node_path,
                        stack,
                        root_id,
                        root_properties,
                        root_slots,
                        components,
                        sets,
                        ds,
                    ) {
                        stack.pop();
                        return false;
                    }
                }
            }
            if component_id == root_id {
                for prop_id in map_keys_js(properties) {
                    if !consume_expansion_step() {
                        stack.pop();
                        return false;
                    }
                    let prop = &properties[&prop_id];
                    if own(prop, "kind").and_then(Value::as_str) != Some("slot") {
                        continue;
                    }
                    let Some(content) = own(root_slots, &prop_id) else {
                        continue;
                    };
                    let (Some(slot_nodes), Some(roots)) = (
                        own(content, "nodes").and_then(object),
                        own(content, "rootNodeIds").and_then(arr),
                    ) else {
                        continue;
                    };
                    fn walk_slot(
                        id: &str,
                        slot_nodes: &Map<String, Value>,
                        path: &str,
                        stack: &mut Vec<String>,
                        root_id: &str,
                        root_properties: &Value,
                        root_slots: &Value,
                        components: &Value,
                        sets: &Value,
                        ds: &mut Vec<Diagnostic>,
                        depth: usize,
                    ) -> bool {
                        if !consume_expansion_step() || depth >= SEMANTIC_MAX_DEPTH.with(Cell::get)
                        {
                            EXPANSION_OVERFLOW.with(|overflow| overflow.set(true));
                            return false;
                        }
                        let Some(node) = slot_nodes.get(id) else {
                            return true;
                        };
                        if own(node, "kind").and_then(Value::as_str) == Some("instance") {
                            if let (Some(src), Some(sel)) =
                                (own(node, "source"), own(node, "variantSelection"))
                            {
                                if let Some(target) = resolve_local(src, sel, components, sets) {
                                    if !expand(
                                        &target,
                                        &format!("{path}.nodes.{id}.source"),
                                        stack,
                                        root_id,
                                        root_properties,
                                        root_slots,
                                        components,
                                        sets,
                                        ds,
                                    ) {
                                        return false;
                                    }
                                }
                            }
                        }
                        if let Some(children) = own(node, "childIds").and_then(arr) {
                            for child in children.iter().filter_map(Value::as_str) {
                                if !walk_slot(
                                    child,
                                    slot_nodes,
                                    path,
                                    stack,
                                    root_id,
                                    root_properties,
                                    root_slots,
                                    components,
                                    sets,
                                    ds,
                                    depth + 1,
                                ) {
                                    return false;
                                }
                            }
                        }
                        true
                    }
                    let path = format!(
                        "$.instances.{}.slotContent.{}",
                        path.strip_prefix("$.instances.")
                            .and_then(|x| x.split('.').next())
                            .unwrap_or(""),
                        prop_id
                    );
                    for root_node in roots.iter().filter_map(Value::as_str) {
                        if !walk_slot(
                            root_node,
                            slot_nodes,
                            &path,
                            stack,
                            root_id,
                            root_properties,
                            root_slots,
                            components,
                            sets,
                            ds,
                            0,
                        ) {
                            stack.pop();
                            return false;
                        }
                    }
                }
            }
            stack.pop();
            true
        }
        if !expand(
            &root,
            &root_path,
            &mut Vec::new(),
            &root,
            root_properties,
            root_slots,
            components,
            sets,
            ds,
        ) {
            return false;
        }
    }
    true
}

/// Validate raw v1 JSON with TypeScript-compatible diagnostics. Success returns
/// the original JSON value; rejection never exposes a partial document.
pub fn validate_component_instance_document(input: &Value) -> ValidationResult {
    let mut normalized = input.clone();
    normalize_js_numbers(&mut normalized);
    validate_normalized_document(&normalized)
}

/// Protocol-bound validator entrypoint. Parsing is bounded by the caller;
/// this guard bounds diagnostic accumulation and recursive instance expansion.
pub fn validate_component_instance_document_bounded(
    input: &Value,
    max_diagnostics: usize,
    max_diagnostic_bytes: usize,
    max_expansion_steps: usize,
) -> Result<ValidationResult, ValidationLimit> {
    let guard =
        ValidationBudgetGuard::new(max_diagnostics, max_diagnostic_bytes, max_expansion_steps);
    let result = validate_component_instance_document(input);
    let exceeded = guard.exceeded();
    drop(guard);
    match exceeded {
        Some(limit) => Err(limit),
        None => Ok(result),
    }
}

fn validate_normalized_document(input: &Value) -> ValidationResult {
    let mut ds = Vec::new();
    if !input.is_object() {
        diagnostic(
            &mut ds,
            "COMPONENT_JSON_INVALID",
            "$",
            "Input must be acyclic plain JSON data with finite numbers.",
        );
        return result(ds);
    }
    if own(input, "version").and_then(Value::as_f64) != Some(1.0) {
        diagnostic(
            &mut ds,
            "COMPONENT_VERSION_UNSUPPORTED",
            "$.version",
            "Unsupported component-domain version.",
        );
        return result(ds);
    }
    if let Some(extra) = fields(
        input,
        &["version", "components", "componentSets", "instances"],
    ) {
        diagnostic(
            &mut ds,
            "COMPONENT_UNKNOWN_FIELD",
            format!("$.{extra}"),
            "Unknown fields are rejected by component-domain v1.",
        );
    }
    let (Some(components), Some(sets), Some(instances)) = (
        own(input, "components"),
        own(input, "componentSets"),
        own(input, "instances"),
    ) else {
        diagnostic(
            &mut ds,
            "COMPONENT_DOCUMENT_MALFORMED",
            "$",
            "components, componentSets, and instances must be plain records.",
        );
        return result(ds);
    };
    if !rec(components) || !rec(sets) || !rec(instances) {
        diagnostic(
            &mut ds,
            "COMPONENT_DOCUMENT_MALFORMED",
            "$",
            "components, componentSets, and instances must be plain records.",
        );
        return result(ds);
    }
    let component_order = js_sorted_keys(components);
    let set_order = js_sorted_keys(sets);
    let instance_order = js_sorted_keys(instances);
    let component_ids: BTreeSet<String> = component_order.iter().cloned().collect();
    let set_ids: BTreeSet<String> = set_order.iter().cloned().collect();
    let mut component_nodes = BTreeMap::<String, BTreeSet<String>>::new();
    let mut sets_by_member = BTreeMap::<String, String>::new();
    let mut identity_owner = BTreeMap::<String, String>::new();
    let claim =
        |id: &str, path: &str, owners: &mut BTreeMap<String, String>, ds: &mut Vec<Diagnostic>| {
            if let Some(previous) = owners.get(id) {
                diagnostic(
                    ds,
                    "COMPONENT_DUPLICATE_ID",
                    path,
                    format!("ID {id} is already used at {previous}."),
                );
            } else {
                owners.insert(id.into(), path.into());
            }
        };
    for id in &component_order {
        let path = format!("$.components.{id}");
        let value = own(components, id).unwrap();
        if !strict_record(
            value,
            &path,
            &[
                "id",
                "name",
                "setId",
                "rootNodeIds",
                "nodes",
                "properties",
                "variantValues",
            ],
            &mut ds,
        ) {
            continue;
        }
        if !own(value, "id").is_some_and(nonempty)
            || !own(value, "name").is_some_and(nonempty)
            || !own(value, "setId").is_some_and(|x| x.is_null() || nonempty(x))
            || !own(value, "properties").is_some_and(rec)
            || !own(value, "variantValues").is_some_and(rec)
        {
            diagnostic(
                &mut ds,
                "COMPONENT_RECORD_MALFORMED",
                &path,
                "Malformed component definition.",
            );
            continue;
        }
        if own(value, "id").and_then(Value::as_str) != Some(id) {
            diagnostic(
                &mut ds,
                "COMPONENT_ID_MISMATCH",
                format!("{path}.id"),
                "Record ID must match its map key.",
            );
        }
        claim(id, &path, &mut identity_owner, &mut ds);
        let set_id = own(value, "setId").and_then(Value::as_str);
        if set_id.is_some_and(|x| !set_ids.contains(x)) {
            diagnostic(
                &mut ds,
                "COMPONENT_SET_MEMBERSHIP_INVALID",
                format!("{path}.setId"),
                "Component references a missing set.",
            );
        }
        if set_id.is_none()
            && own(value, "variantValues")
                .and_then(object)
                .is_some_and(|x| !x.is_empty())
        {
            diagnostic(
                &mut ds,
                "COMPONENT_VARIANT_INVALID",
                format!("{path}.variantValues"),
                "Standalone component cannot carry set variant values.",
            );
        }
        let node_ids = validate_node_graph(
            own(value, "nodes").unwrap_or(&Value::Null),
            own(value, "rootNodeIds").unwrap_or(&Value::Null),
            &path,
            &mut ds,
            &["container", "shape", "text", "instance"],
        );
        let mut node_order: Vec<String> = node_ids.iter().cloned().collect();
        node_order.sort_by(|a, b| js_cmp(a, b));
        for node_id in &node_order {
            claim(
                node_id,
                &format!("{path}.nodes.{node_id}"),
                &mut identity_owner,
                &mut ds,
            );
        }
        component_nodes.insert(id.clone(), node_ids.clone());
        if let (Some(props), Some(nodes)) = (
            own(value, "properties").and_then(object),
            own(value, "nodes"),
        ) {
            for (prop_id, prop) in props {
                let p = path_member(&path_member(&path, "properties"), prop_id);
                validate_property(prop, prop_id, &p, &node_ids, &mut ds, false);
                validate_property_target(prop, nodes, &p, &mut ds);
            }
            for (prop_id, prop) in props {
                if own(prop, "kind").and_then(Value::as_str) == Some("instance-swap") {
                    if let Some(ids) = own(prop, "allowedComponentIds").and_then(arr) {
                        for allowed in ids {
                            if let Some(allowed) = allowed.as_str() {
                                if !component_ids.contains(allowed) {
                                    diagnostic(
                                        &mut ds,
                                        "COMPONENT_PROPERTY_INVALID",
                                        format!("{path}.properties.{prop_id}.allowedComponentIds"),
                                        format!("Unknown allowed component ID {allowed}."),
                                    );
                                }
                            }
                        }
                    }
                }
                if own(prop, "kind").and_then(Value::as_str) == Some("slot") {
                    if let Some(ids) = own(prop, "guidance")
                        .and_then(|x| own(x, "preferredComponentIds"))
                        .and_then(arr)
                    {
                        for preferred in ids {
                            if let Some(preferred) = preferred.as_str() {
                                if !component_ids.contains(preferred) {
                                    diagnostic(&mut ds,"COMPONENT_PROPERTY_INVALID",format!("{path}.properties.{prop_id}.guidance.preferredComponentIds"),format!("Unknown preferred component ID {preferred}."));
                                }
                            }
                        }
                    }
                }
            }
        }
        if set_id.is_some()
            && own(value, "properties")
                .and_then(object)
                .is_some_and(|x| !x.is_empty())
        {
            diagnostic(
                &mut ds,
                "COMPONENT_SET_MEMBERSHIP_INVALID",
                format!("{path}.properties"),
                "Set member properties are declared on the set, not duplicated on each variant.",
            );
        }
        if let Some(nodes) = own(value, "nodes").and_then(object) {
            for (node_id, node) in nodes {
                if own(node, "kind").and_then(Value::as_str) == Some("instance")
                    && has(node, "source")
                {
                    let source = own(node, "source").unwrap();
                    let npath = format!("{path}.nodes.{node_id}.source");
                    validate_reference(source, &npath, &mut ds, &component_ids, &set_ids);
                    if let Some(sel) = own(node, "variantSelection") {
                        validate_nested_selection(
                            source,
                            sel,
                            &format!("{path}.nodes.{node_id}"),
                            &mut ds,
                            components,
                            sets,
                        );
                    }
                }
            }
        }
        if let Some(set_id) = set_id {
            sets_by_member.insert(id.clone(), set_id.into());
        }
        component_nodes.entry(id.clone()).or_default();
    }
    for id in &set_order {
        let path = format!("$.componentSets.{id}");
        let value = own(sets, id).unwrap();
        if !strict_record(
            value,
            &path,
            &["id", "name", "memberComponentIds", "properties"],
            &mut ds,
        ) {
            continue;
        }
        let members = own(value, "memberComponentIds").and_then(string_array);
        let props = own(value, "properties").and_then(object);
        if !own(value, "id").is_some_and(nonempty)
            || !own(value, "name").is_some_and(nonempty)
            || members.as_ref().is_none_or(|x| x.is_empty())
            || props.is_none_or(|x| x.is_empty())
        {
            diagnostic(
                &mut ds,
                "COMPONENT_RECORD_MALFORMED",
                &path,
                "Malformed component set.",
            );
            continue;
        }
        if own(value, "id").and_then(Value::as_str) != Some(id) {
            diagnostic(
                &mut ds,
                "COMPONENT_ID_MISMATCH",
                format!("{path}.id"),
                "Record ID must match its map key.",
            );
        }
        claim(id, &path, &mut identity_owner, &mut ds);
        let members = members.unwrap();
        if members.iter().collect::<BTreeSet<_>>().len() != members.len() {
            diagnostic(
                &mut ds,
                "COMPONENT_SET_MEMBERSHIP_INVALID",
                format!("{path}.memberComponentIds"),
                "Set member order cannot contain duplicate IDs.",
            );
        }
        for member in &members {
            if !component_ids.contains(*member) || sets_by_member.get(*member) != Some(id) {
                diagnostic(
                    &mut ds,
                    "COMPONENT_SET_MEMBERSHIP_INVALID",
                    format!("{path}.memberComponentIds"),
                    format!("Membership is not reciprocal for {member}."),
                );
            }
        }
        let props = props.unwrap();
        for (prop_id, prop) in props {
            validate_property(
                prop,
                prop_id,
                &format!("{path}.properties.{prop_id}"),
                &BTreeSet::new(),
                &mut ds,
                true,
            );
        }
        let mut combos = BTreeSet::<String>::new();
        for member_id in members {
            let Some(member) = own(components, member_id) else {
                continue;
            };
            let Some(vals) = own(member, "variantValues").and_then(object) else {
                continue;
            };
            let variant_keys: Vec<String> = props.keys().cloned().collect();
            let valid = vals.len() == variant_keys.len()
                && variant_keys.iter().all(|key| {
                    let value = vals.get(key);
                    let values = own(&props[key], "values").and_then(string_array);
                    value
                        .and_then(Value::as_str)
                        .is_some_and(|v| values.is_some_and(|xs| xs.contains(&v)))
                });
            if !valid {
                diagnostic(
                    &mut ds,
                    "COMPONENT_VARIANT_INVALID",
                    format!("$.components.{member_id}.variantValues"),
                    "Variant member must provide exactly one allowed value for each set property.",
                );
                continue;
            }
            let tuple = variant_keys
                .iter()
                .map(|key| format!("{key}={}", vals[key].as_str().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("\0");
            if !combos.insert(tuple) {
                diagnostic(
                    &mut ds,
                    "COMPONENT_VARIANT_DUPLICATE",
                    format!("$.components.{member_id}.variantValues"),
                    "Variant property combination duplicates another member.",
                );
            }
        }
    }
    for (member_id, set_id) in &sets_by_member {
        let listed = own(sets, set_id)
            .and_then(|x| own(x, "memberComponentIds"))
            .and_then(arr)
            .is_some_and(|xs| xs.iter().any(|x| x.as_str() == Some(member_id)));
        if !listed {
            diagnostic(
                &mut ds,
                "COMPONENT_SET_MEMBERSHIP_INVALID",
                format!("$.components.{member_id}.setId"),
                "Set does not list this member in its ordered membership.",
            );
        }
    }
    for id in &instance_order {
        let path = format!("$.instances.{id}");
        let value = own(instances, id).unwrap();
        if !strict_record(
            value,
            &path,
            &[
                "id",
                "source",
                "variantSelection",
                "propertyValues",
                "overrides",
                "slotContent",
            ],
            &mut ds,
        ) {
            continue;
        }
        let (Some(selection), Some(property_values), Some(overrides), Some(slot_content)) = (
            own(value, "variantSelection").and_then(object),
            own(value, "propertyValues").and_then(object),
            own(value, "overrides").and_then(arr),
            own(value, "slotContent").and_then(object),
        ) else {
            diagnostic(
                &mut ds,
                "COMPONENT_RECORD_MALFORMED",
                &path,
                "Malformed instance record.",
            );
            continue;
        };
        if !own(value, "id").is_some_and(nonempty) {
            diagnostic(
                &mut ds,
                "COMPONENT_RECORD_MALFORMED",
                &path,
                "Malformed instance record.",
            );
            continue;
        }
        if own(value, "id").and_then(Value::as_str) != Some(id) {
            diagnostic(
                &mut ds,
                "COMPONENT_ID_MISMATCH",
                format!("{path}.id"),
                "Record ID must match its map key.",
            );
        }
        claim(id, &path, &mut identity_owner, &mut ds);
        let source = own(value, "source").unwrap_or(&Value::Null);
        validate_reference(
            source,
            &format!("{path}.source"),
            &mut ds,
            &component_ids,
            &set_ids,
        );
        let mut definition: Option<&Value> = None;
        let mut set: Option<&Value> = None;
        match own(source, "kind").and_then(Value::as_str) {
            Some("local-component") => {
                if let Some(cid) = own(source, "componentId").and_then(Value::as_str) {
                    definition = own(components, cid);
                }
            }
            Some("local-set") => {
                if let Some(sid) = own(source, "componentSetId").and_then(Value::as_str) {
                    set = own(sets, sid);
                }
            }
            _ => {}
        }
        if let Some(set) = set {
            let empty_props = Map::new();
            let props = own(set, "properties")
                .and_then(object)
                .unwrap_or(&empty_props);
            let valid = selection.len() == props.len()
                && selection.iter().all(|(p, v)| {
                    own(&Value::Object(props.clone()), p)
                        .and_then(|x| own(x, "values"))
                        .and_then(string_array)
                        .is_some_and(|values| values.contains(&v.as_str().unwrap_or_default()))
                });
            if !valid {
                diagnostic(
                    &mut ds,
                    "COMPONENT_VARIANT_SELECTION_INVALID",
                    format!("{path}.variantSelection"),
                    "Selection must provide one allowed value for each set property.",
                );
            } else if let Some(members) = own(set, "memberComponentIds").and_then(arr) {
                let matches: Vec<&Value> = members
                    .iter()
                    .filter_map(|member| own(components, member.as_str()?))
                    .filter(|member| {
                        let vals = own(member, "variantValues").and_then(object);
                        vals.is_some_and(|vals| {
                            selection.len() == props.len()
                                && selection.iter().all(|(p, v)| vals.get(p) == Some(v))
                        })
                    })
                    .collect();
                if matches.len() > 1 {
                    diagnostic(
                        &mut ds,
                        "COMPONENT_AMBIGUOUS_SOURCE",
                        format!("{path}.variantSelection"),
                        "Variant selection matches more than one component definition.",
                    );
                } else if matches.is_empty() {
                    diagnostic(
                        &mut ds,
                        "COMPONENT_VARIANT_SELECTION_INVALID",
                        format!("{path}.variantSelection"),
                        "Variant selection does not resolve to a set member.",
                    );
                } else {
                    definition = matches.first().copied();
                }
            }
        } else if !selection.is_empty() {
            diagnostic(
                &mut ds,
                "COMPONENT_VARIANT_SELECTION_INVALID",
                format!("{path}.variantSelection"),
                "Only a reference to a component set may carry variant selection.",
            );
        }
        if let Some(definition) = definition {
            let empty_props = Map::new();
            let node_ids = own(definition, "id")
                .and_then(Value::as_str)
                .and_then(|cid| component_nodes.get(cid))
                .cloned()
                .unwrap_or_default();
            let props = own(definition, "properties")
                .and_then(object)
                .unwrap_or(&empty_props);
            for (prop_id, prop_value) in property_values {
                let prop = props.get(prop_id);
                let ppath = format!("{path}.propertyValues.{prop_id}");
                let Some(prop) = prop else {
                    diagnostic(
                        &mut ds,
                        "COMPONENT_PROPERTY_VALUE_INVALID",
                        ppath,
                        "Value refers to an undeclared property.",
                    );
                    continue;
                };
                let kind = own(prop, "kind").and_then(Value::as_str);
                let invalid = match kind {
                    Some("boolean") => !prop_value.is_boolean(),
                    Some("text") => !prop_value.is_string(),
                    Some("instance-swap") => {
                        !rec(prop_value)
                            || fields(prop_value, &["kind", "componentId"]).is_some()
                            || own(prop_value, "kind").and_then(Value::as_str)
                                != Some("local-component")
                            || own(prop, "allowedComponentIds")
                                .and_then(arr)
                                .is_none_or(|xs| {
                                    !xs.contains(
                                        own(prop_value, "componentId").unwrap_or(&Value::Null),
                                    )
                                })
                    }
                    Some("slot") => true,
                    _ => false,
                };
                if invalid {
                    let detail = if kind == Some("instance-swap") {
                        "Swap value is not an explicitly allowed local component reference."
                    } else if kind == Some("slot") {
                        "Slot values belong in slotContent."
                    } else {
                        "Value does not match property kind."
                    };
                    diagnostic(&mut ds, "COMPONENT_PROPERTY_VALUE_INVALID", ppath, detail);
                }
            }
            let mut override_addresses = BTreeSet::new();
            for (index, override_value) in overrides.iter().enumerate() {
                let opath = format!("{path}.overrides.{index}");
                let address = own(override_value, "address");
                if !rec(override_value)
                    || fields(override_value, &["address", "value"]).is_some()
                    || address.is_none_or(|x| {
                        !rec(x)
                            || fields(x, &["targetNodeId", "field"]).is_some()
                            || !own(x, "targetNodeId").is_some_and(nonempty)
                            || !own(x, "field").and_then(Value::as_str).is_some_and(|f| {
                                [
                                    "name",
                                    "text",
                                    "fills",
                                    "strokes",
                                    "effects",
                                    "layoutGuides",
                                    "exportSettings",
                                ]
                                .contains(&f)
                            })
                    })
                {
                    diagnostic(
                        &mut ds,
                        "COMPONENT_OVERRIDE_INVALID",
                        opath,
                        "Malformed typed override address.",
                    );
                    continue;
                }
                let address = address.unwrap();
                let target = own(address, "targetNodeId")
                    .and_then(Value::as_str)
                    .unwrap();
                let field = own(address, "field").and_then(Value::as_str).unwrap();
                if !node_ids.contains(target) {
                    diagnostic(
                        &mut ds,
                        "COMPONENT_OVERRIDE_TARGET_MISSING",
                        format!("{opath}.address.targetNodeId"),
                        "Override target is absent from selected definition.",
                    );
                }
                if !override_type_ok(field, own(override_value, "value").unwrap_or(&Value::Null)) {
                    diagnostic(
                        &mut ds,
                        "COMPONENT_OVERRIDE_FIELD_INVALID",
                        format!("{opath}.value"),
                        "Override value does not match its typed field or is not JSON data.",
                    );
                }
                let key = format!("{target}\0{field}");
                if !override_addresses.insert(key) {
                    diagnostic(
                        &mut ds,
                        "COMPONENT_OVERRIDE_INVALID",
                        opath,
                        "An instance cannot carry duplicate overrides for the same typed address.",
                    );
                }
            }
            for (prop_id, content) in slot_content {
                let slot_path = format!("{path}.slotContent.{prop_id}");
                let prop = props.get(prop_id);
                if prop.is_none_or(|p| own(p, "kind").and_then(Value::as_str) != Some("slot")) {
                    diagnostic(
                        &mut ds,
                        "COMPONENT_SLOT_INVALID",
                        slot_path,
                        "Slot content must be owned by a declared slot property.",
                    );
                    continue;
                }
                let prop = prop.unwrap();
                let local_ids = validate_slot_content(
                    content,
                    &slot_path,
                    &mut ds,
                    components,
                    sets,
                    &component_ids,
                    &set_ids,
                );
                let (Some(roots), Some(nodes)) = (
                    own(content, "rootNodeIds").and_then(arr),
                    own(content, "nodes").and_then(object),
                ) else {
                    continue;
                };
                let entry_count = roots.len() as f64;
                let guidance = own(prop, "guidance").unwrap_or(&Value::Null);
                let hard = own(prop, "hardLimits").unwrap_or(&Value::Null);
                if own(guidance, "minimumEntries")
                    .and_then(js_nonnegative_integer)
                    .is_some_and(|v| entry_count < v)
                    || own(guidance, "maximumEntries")
                        .and_then(js_nonnegative_integer)
                        .is_some_and(|v| entry_count > v)
                {
                    warning(&mut ds,"COMPONENT_SLOT_GUIDANCE_EXCEEDED",&slot_path,format!("Slot has {entry_count} direct entries outside advisory guidance; retained as a warning."));
                }
                if own(hard, "minimumEntries")
                    .and_then(js_nonnegative_integer)
                    .is_some_and(|v| entry_count < v)
                    || own(hard, "maximumEntries")
                        .and_then(js_nonnegative_integer)
                        .is_some_and(|v| entry_count > v)
                {
                    diagnostic(&mut ds,"COMPONENT_SLOT_HARD_LIMIT_EXCEEDED",&slot_path,format!("Slot has {entry_count} direct entries outside explicitly declared hard limits."));
                }
                if own(prop, "onlyAllowPreferredInstances").and_then(Value::as_bool) == Some(true) {
                    let preferred = own(guidance, "preferredComponentIds")
                        .and_then(arr)
                        .cloned()
                        .unwrap_or_default();
                    for root in roots {
                        let root_id = root.as_str().unwrap_or_default();
                        let node = nodes.get(root_id);
                        let target = node
                            .filter(|n| own(n, "kind").and_then(Value::as_str) == Some("instance"))
                            .and_then(|n| {
                                Some(resolve_local(
                                    own(n, "source")?,
                                    own(n, "variantSelection")?,
                                    components,
                                    sets,
                                )?)
                            });
                        if target
                            .as_ref()
                            .is_none_or(|id| !preferred.iter().any(|x| x.as_str() == Some(id)))
                        {
                            diagnostic(&mut ds,"COMPONENT_SLOT_CONTENT_NOT_ALLOWED",&slot_path,format!("Slot entry {root_id} is not an explicitly preferred component instance."));
                        }
                    }
                }
                if local_ids.len() != nodes.len() {
                    diagnostic(
                        &mut ds,
                        "COMPONENT_SLOT_INVALID",
                        &slot_path,
                        "Slot content graph identity is ambiguous.",
                    );
                }
            }
        } else if !property_values.is_empty() || !overrides.is_empty() || !slot_content.is_empty() {
            warning(&mut ds,"COMPONENT_REFERENCE_UNRESOLVED",&path,"Instance payload is preserved, but dependent values cannot be validated until its local source resolves.");
        }
    }
    validate_expansion_cycles(components, sets, instances, &mut ds);
    ds.sort_by(|a, b| {
        js_cmp(&a.path, &b.path)
            .then(js_cmp(&a.code, &b.code))
            .then(js_cmp(&a.severity, &b.severity))
    });
    let ok = ds.iter().all(|d| d.severity != "error");
    ValidationResult {
        ok,
        diagnostics: ds,
        document: if ok { Some(input.clone()) } else { None },
    }
}

/// Decode and validate a JSON text boundary, keeping syntax failures distinct
/// from well-formed JSON that is structurally or semantically invalid.
pub fn validate_component_instance_json(source: &str) -> ValidationResult {
    match serde_json::from_str::<Value>(source) {
        Ok(value) => validate_component_instance_document(&value),
        Err(_) => result(vec![Diagnostic {
            code: "COMPONENT_JSON_INVALID".into(),
            path: "$".into(),
            severity: "error".into(),
            detail: "Input must be acyclic plain JSON data with finite numbers.".into(),
        }]),
    }
}

fn result(mut diagnostics: Vec<Diagnostic>) -> ValidationResult {
    diagnostics.sort_by(|a, b| {
        js_cmp(&a.path, &b.path)
            .then(js_cmp(&a.code, &b.code))
            .then(js_cmp(&a.severity, &b.severity))
    });
    ValidationResult {
        ok: false,
        diagnostics,
        document: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POSITIVE: &str =
        include_str!("../../../../test/fixtures/figma-semantics/component-instance-domain-v1.json");
    const NEGATIVE: &str = include_str!(
        "../../../../test/fixtures/figma-semantics/component-instance-v1-invalid-cases.json"
    );

    fn set_path(input: &mut Value, path: &[Value], value: Value) {
        let (last, parents) = path.split_last().unwrap();
        let mut at = input;
        for part in parents {
            at = match part {
                Value::String(key) => &mut at[key],
                Value::Number(n) => &mut at[n.as_u64().unwrap() as usize],
                _ => panic!("unsupported fixture path segment"),
            };
        }
        match last {
            Value::String(key) => at[key] = value,
            Value::Number(n) => at[n.as_u64().unwrap() as usize] = value,
            _ => panic!("unsupported fixture path segment"),
        }
    }

    #[test]
    fn shared_generated_fixture_is_pure_deterministic_and_warning_only_valid() {
        let input: Value = serde_json::from_str(POSITIVE).unwrap();
        let before = input.clone();
        let first = validate_component_instance_document(&input);
        let second = validate_component_instance_document(&input);
        assert!(first.ok, "{:?}", first.diagnostics);
        assert!(first.diagnostics.iter().all(|d| d.severity == "warning"));
        assert_eq!(first, second);
        assert_eq!(input, before, "validator changed caller-owned JSON");
        assert_eq!(first.document.as_ref(), Some(&input));
    }

    #[test]
    fn shared_negative_fixtures_match_typescript_expected_codes_without_mutation() {
        let base: Value = serde_json::from_str(POSITIVE).unwrap();
        let negatives: Value = serde_json::from_str(NEGATIVE).unwrap();
        for case in negatives.as_array().unwrap() {
            let mut input = base.clone();
            let path = own(case, "path").unwrap().as_array().unwrap();
            set_path(&mut input, path, own(case, "value").unwrap().clone());
            let before = input.clone();
            let first = validate_component_instance_document(&input);
            let second = validate_component_instance_document(&input);
            let expected = own(case, "expected").unwrap().as_str().unwrap();
            assert!(
                !first.ok,
                "{} unexpectedly passed",
                own(case, "name").unwrap()
            );
            assert!(
                first.diagnostics.iter().any(|d| d.code == expected),
                "{}: {:?}",
                own(case, "name").unwrap(),
                first.diagnostics
            );
            assert!(first.document.is_none());
            assert_eq!(
                first,
                second,
                "{} diagnostics are unstable",
                own(case, "name").unwrap()
            );
            assert_eq!(input, before, "{} was mutated", own(case, "name").unwrap());
        }
    }

    #[test]
    fn syntax_failure_is_json_invalid_while_well_formed_wrong_shape_is_structural_json_invalid() {
        let syntax = validate_component_instance_json("{not-json");
        let wrong_shape = validate_component_instance_json("\"not a document\"");
        assert_eq!(syntax, wrong_shape);
        assert_eq!(syntax.diagnostics[0].code, "COMPONENT_JSON_INVALID");
        assert!(syntax.document.is_none());
    }

    #[test]
    fn protocol_budget_count_and_serialized_bytes_are_inclusive_and_fail_closed() {
        let input: Value = serde_json::from_str(POSITIVE).unwrap();
        let semantic = validate_component_instance_document(&input);
        let exact_count = semantic.diagnostics.len();
        let exact_bytes = serde_json::to_vec(&semantic.diagnostics).unwrap().len();
        assert!(exact_count > 0);
        assert!(exact_bytes > 2);
        assert!(validate_component_instance_document_bounded(
            &input,
            exact_count,
            exact_bytes,
            100_000,
        )
        .is_ok());
        assert_eq!(
            validate_component_instance_document_bounded(
                &input,
                exact_count - 1,
                exact_bytes,
                100_000,
            ),
            Err(ValidationLimit::Diagnostics)
        );
        assert_eq!(
            validate_component_instance_document_bounded(
                &input,
                exact_count,
                exact_bytes - 1,
                100_000,
            ),
            Err(ValidationLimit::Diagnostics)
        );
        assert_eq!(
            validate_component_instance_document_bounded(&input, exact_count, exact_bytes, 0),
            Err(ValidationLimit::ExpansionWork)
        );
    }
}
