//! Candidate versioned document envelope and pure graph validation boundary.
//!
//! This module is an additive contract proposal. It is not wired to React,
//! Go, file persistence, or existing saved-document reads/writes.

use std::collections::{BTreeMap, HashMap, HashSet};

use logos_layout::{Matrix, Rect};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{Shape, ShapeType};

pub const DOCUMENT_SCHEMA_VERSION: u32 = 1;
const NIL_RENDERER_ROOT_ID: Uuid = Uuid::nil();
/// Candidate coordinate/transform magnitude bound in document units.
const MAX_ABS_GEOMETRY: f64 = 1_000_000_000_000.0;
const MATRIX_INVERSE_TOLERANCE: f64 = 1.0e-8;

/// Candidate document envelope. `pages` order is document page order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Document {
    pub id: Uuid,
    pub schema_version: u32,
    pub pages: Vec<Page>,
    /// Flat records avoid recursive deserialization and allow graph validation
    /// to report duplicate IDs before any tree traversal.
    pub shapes: Vec<ShapeRecord>,
    /// Unknown top-level fields survive a read/serialize cycle.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Page {
    pub id: Uuid,
    pub name: String,
    /// Ordered document roots; order is semantic and is never inferred from a map.
    pub root_shape_ids: Vec<Uuid>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// A Rust `Shape` plus explicit page ownership and an open extension envelope.
/// Shape fields serialize at the same object level as `page-id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ShapeRecord {
    pub page_id: Uuid,
    #[serde(flatten)]
    pub shape: Shape,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Lossless quarantine for a source payload that has not passed an explicit
/// adapter. It is deliberately not a `Document` and cannot enter operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CompatibilityEnvelope {
    pub source_format: String,
    pub source_schema: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ValidationDiagnosticCode {
    DocumentSchemaInvalid,
    UnsupportedSchemaVersion,
    MalformedId,
    ReservedRendererId,
    DuplicateId,
    MissingPage,
    MissingShape,
    CrossPageReference,
    RootParentMismatch,
    DuplicateRootMembership,
    DuplicateChild,
    DuplicateChildMembership,
    InvalidSiblingOrder,
    ParentChildMismatch,
    NonContainerParent,
    Cycle,
    OrphanShape,
    InvalidGeometry,
    GeometryOutOfRange,
    NonFiniteTransform,
    SingularTransform,
    TransformInverseMismatch,
    InvalidOpacity,
    InvalidFrameReference,
    FrameMembershipMismatch,
    InvalidComponentReference,
    ExtensionKeyCollision,
    OperationPreconditionFailed,
    InvalidOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationDiagnostic {
    pub code: ValidationDiagnosticCode,
    /// Stable JSON Pointer-like location in the candidate contract.
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_id: Option<Uuid>,
}

impl ValidationDiagnostic {
    fn new(code: ValidationDiagnosticCode, path: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
            object_id: None,
            related_id: None,
        }
    }

    fn ids(mut self, object_id: Uuid, related_id: Option<Uuid>) -> Self {
        self.object_id = Some(object_id);
        self.related_id = related_id;
        self
    }
}

/// Parse and validate an untrusted JSON candidate. UUID checks happen before
/// typed deserialization so malformed identifiers receive stable diagnostics.
pub fn decode_and_validate_json(input: &str) -> Result<Document, Vec<ValidationDiagnostic>> {
    let value: Value = serde_json::from_str(input).map_err(|_| {
        vec![ValidationDiagnostic::new(
            ValidationDiagnosticCode::DocumentSchemaInvalid,
            "$",
        )]
    })?;
    let mut malformed = malformed_id_diagnostics(&value);
    if !malformed.is_empty() {
        sort_diagnostics(&mut malformed);
        return Err(malformed);
    }
    let document: Document = serde_json::from_value(value).map_err(|_| {
        vec![ValidationDiagnostic::new(
            ValidationDiagnosticCode::DocumentSchemaInvalid,
            "$",
        )]
    })?;
    let diagnostics = validate_document(&document);
    if diagnostics.is_empty() {
        Ok(document)
    } else {
        Err(diagnostics)
    }
}

/// Validate an already decoded candidate without changing it.
pub fn validate_document(document: &Document) -> Vec<ValidationDiagnostic> {
    let mut diagnostics = Vec::new();
    if document.schema_version != DOCUMENT_SCHEMA_VERSION {
        diagnostics.push(ValidationDiagnostic::new(
            ValidationDiagnosticCode::UnsupportedSchemaVersion,
            "/schema-version",
        ));
    }

    let mut object_ids = HashSet::new();
    check_id(document.id, "/id", &mut object_ids, &mut diagnostics);

    let mut pages_by_id = HashMap::new();
    for (index, page) in document.pages.iter().enumerate() {
        let path = format!("/pages/{index}");
        check_id(
            page.id,
            format!("{path}/id"),
            &mut object_ids,
            &mut diagnostics,
        );
        if pages_by_id.insert(page.id, (index, page)).is_some() {
            diagnostics.push(
                ValidationDiagnostic::new(
                    ValidationDiagnosticCode::DuplicateId,
                    format!("{path}/id"),
                )
                .ids(page.id, None),
            );
        }
        check_extension_collisions(
            &page.extensions,
            &["id", "name", "root-shape-ids"],
            &path,
            &mut diagnostics,
        );
    }

    let mut shapes_by_id: HashMap<Uuid, (usize, &ShapeRecord)> = HashMap::new();
    for (index, record) in document.shapes.iter().enumerate() {
        let path = format!("/shapes/{index}");
        let id = record.shape.id;
        check_id(id, format!("{path}/id"), &mut object_ids, &mut diagnostics);
        if shapes_by_id.insert(id, (index, record)).is_some() {
            diagnostics.push(
                ValidationDiagnostic::new(
                    ValidationDiagnosticCode::DuplicateId,
                    format!("{path}/id"),
                )
                .ids(id, None),
            );
        }
        check_extension_collisions(
            &record.extensions,
            &[
                "page-id",
                "id",
                "type",
                "name",
                "selrect",
                "transform",
                "transform-inverse",
                "rotation",
                "points",
                "parent-id",
                "frame-id",
                "shapes",
                "hidden",
                "opacity",
                "masked-group",
                "fills",
                "strokes",
                "shadow",
                "blur",
                "rx",
                "ry",
                "r1",
                "r2",
                "r3",
                "r4",
                "constraints-h",
                "constraints-v",
                "fixed-scroll",
                "component-id",
                "component-file",
                "component-root",
                "main-instance",
                "metadata",
                "bool-type",
                "plugin-data",
            ],
            &path,
            &mut diagnostics,
        );
        validate_numbers(record, &path, &mut diagnostics);
    }
    let expected_frames = expected_frame_memberships(&shapes_by_id);

    // Validate every page/root relation and retain the declared sibling order.
    let mut root_owner = HashMap::<Uuid, Uuid>::new();
    for (page_index, page) in document.pages.iter().enumerate() {
        let mut local_roots = HashSet::new();
        for (root_index, root_id) in page.root_shape_ids.iter().copied().enumerate() {
            let path = format!("/pages/{page_index}/root-shape-ids/{root_index}");
            if !local_roots.insert(root_id) || root_owner.insert(root_id, page.id).is_some() {
                diagnostics.push(
                    ValidationDiagnostic::new(
                        ValidationDiagnosticCode::DuplicateRootMembership,
                        path,
                    )
                    .ids(root_id, None),
                );
                continue;
            }
            let Some((_, root)) = shapes_by_id.get(&root_id).copied() else {
                diagnostics.push(
                    ValidationDiagnostic::new(ValidationDiagnosticCode::MissingShape, path)
                        .ids(root_id, None),
                );
                continue;
            };
            if root.page_id != page.id {
                diagnostics.push(
                    ValidationDiagnostic::new(ValidationDiagnosticCode::CrossPageReference, path)
                        .ids(root_id, Some(page.id)),
                );
            }
            if root.shape.parent_id.is_some() {
                diagnostics.push(
                    ValidationDiagnostic::new(
                        ValidationDiagnosticCode::RootParentMismatch,
                        format!("/shapes/{}/parent-id", shapes_by_id[&root_id].0),
                    )
                    .ids(root_id, root.shape.parent_id),
                );
            }
        }
    }

    let mut child_owner = HashMap::<Uuid, Uuid>::new();
    for (index, record) in document.shapes.iter().enumerate() {
        let shape = &record.shape;
        let path = format!("/shapes/{index}");
        if !pages_by_id.contains_key(&record.page_id) {
            diagnostics.push(
                ValidationDiagnostic::new(
                    ValidationDiagnosticCode::MissingPage,
                    format!("{path}/page-id"),
                )
                .ids(shape.id, Some(record.page_id)),
            );
        }
        let mut local_children = HashSet::new();
        for (child_index, child_id) in shape.shapes.iter().copied().enumerate() {
            let child_path = format!("{path}/shapes/{child_index}");
            if !local_children.insert(child_id) {
                diagnostics.push(
                    ValidationDiagnostic::new(ValidationDiagnosticCode::DuplicateChild, child_path)
                        .ids(shape.id, Some(child_id)),
                );
                continue;
            }
            if !shape.shape_type.is_container() {
                diagnostics.push(
                    ValidationDiagnostic::new(
                        ValidationDiagnosticCode::NonContainerParent,
                        child_path.clone(),
                    )
                    .ids(shape.id, Some(child_id)),
                );
            }
            if let Some(previous_parent) = child_owner.insert(child_id, shape.id) {
                diagnostics.push(
                    ValidationDiagnostic::new(
                        ValidationDiagnosticCode::DuplicateChildMembership,
                        child_path,
                    )
                    .ids(child_id, Some(previous_parent)),
                );
                continue;
            }
            match shapes_by_id.get(&child_id).copied() {
                None => diagnostics.push(
                    ValidationDiagnostic::new(ValidationDiagnosticCode::MissingShape, child_path)
                        .ids(shape.id, Some(child_id)),
                ),
                Some((child_record_index, child)) => {
                    if child.page_id != record.page_id {
                        diagnostics.push(
                            ValidationDiagnostic::new(
                                ValidationDiagnosticCode::CrossPageReference,
                                child_path.clone(),
                            )
                            .ids(shape.id, Some(child_id)),
                        );
                    }
                    if child.shape.parent_id != Some(shape.id) {
                        diagnostics.push(
                            ValidationDiagnostic::new(
                                ValidationDiagnosticCode::ParentChildMismatch,
                                format!("/shapes/{child_record_index}/parent-id"),
                            )
                            .ids(child_id, Some(shape.id)),
                        );
                    }
                }
            }
        }
    }

    for (index, record) in document.shapes.iter().enumerate() {
        let shape = &record.shape;
        let path = format!("/shapes/{index}");
        if shape.id == NIL_RENDERER_ROOT_ID {
            diagnostics.push(
                ValidationDiagnostic::new(
                    ValidationDiagnosticCode::ReservedRendererId,
                    format!("{path}/id"),
                )
                .ids(shape.id, None),
            );
        }
        match shape.parent_id {
            Some(parent_id) => match shapes_by_id.get(&parent_id).copied() {
                None => diagnostics.push(
                    ValidationDiagnostic::new(
                        ValidationDiagnosticCode::MissingShape,
                        format!("{path}/parent-id"),
                    )
                    .ids(shape.id, Some(parent_id)),
                ),
                Some((parent_index, parent)) => {
                    if parent.page_id != record.page_id {
                        diagnostics.push(
                            ValidationDiagnostic::new(
                                ValidationDiagnosticCode::CrossPageReference,
                                format!("{path}/parent-id"),
                            )
                            .ids(shape.id, Some(parent_id)),
                        );
                    }
                    if !parent.shape.shape_type.is_container() {
                        diagnostics.push(
                            ValidationDiagnostic::new(
                                ValidationDiagnosticCode::NonContainerParent,
                                format!("{path}/parent-id"),
                            )
                            .ids(shape.id, Some(parent_id)),
                        );
                    }
                    if !parent.shape.shapes.contains(&shape.id) {
                        diagnostics.push(
                            ValidationDiagnostic::new(
                                ValidationDiagnosticCode::ParentChildMismatch,
                                format!("/shapes/{parent_index}/shapes"),
                            )
                            .ids(shape.id, Some(parent_id)),
                        );
                    }
                }
            },
            None if !root_owner.contains_key(&shape.id) => diagnostics.push(
                ValidationDiagnostic::new(
                    ValidationDiagnosticCode::OrphanShape,
                    format!("{path}/id"),
                )
                .ids(shape.id, None),
            ),
            None => {}
        }

        let expected_frame_id = expected_frames.get(&shape.id).copied().flatten();
        if shape.frame_id != expected_frame_id {
            let code = if shape.frame_id.is_some_and(|frame_id| {
                !matches!(
                    shapes_by_id.get(&frame_id),
                    Some((_, frame))
                        if frame.page_id == record.page_id
                            && frame.shape.shape_type == ShapeType::Frame
                )
            }) {
                ValidationDiagnosticCode::InvalidFrameReference
            } else {
                ValidationDiagnosticCode::FrameMembershipMismatch
            };
            diagnostics.push(
                ValidationDiagnostic::new(code, format!("{path}/frame-id"))
                    .ids(shape.id, shape.frame_id.or(expected_frame_id)),
            );
        }
        if let Some(component_id) = shape.component_id {
            if shape.component_file.is_none()
                && !matches!(
                    shapes_by_id
                        .get(&component_id)
                        .map(|(_, record)| record.shape.shape_type),
                    Some(ShapeType::Component)
                )
            {
                diagnostics.push(
                    ValidationDiagnostic::new(
                        ValidationDiagnosticCode::InvalidComponentReference,
                        format!("{path}/component-id"),
                    )
                    .ids(shape.id, Some(component_id)),
                );
            }
        }
    }

    validate_cycles(&shapes_by_id, &mut diagnostics);
    sort_diagnostics(&mut diagnostics);
    diagnostics
}

fn expected_frame_memberships(
    shapes: &HashMap<Uuid, (usize, &ShapeRecord)>,
) -> HashMap<Uuid, Option<Uuid>> {
    let mut ids: Vec<_> = shapes.keys().copied().collect();
    ids.sort();
    let mut expected = HashMap::with_capacity(ids.len());

    for start in ids {
        if expected.contains_key(&start) {
            continue;
        }
        let mut path = Vec::new();
        let mut visiting = HashSet::new();
        let mut cursor = Some(start);
        let mut parent = None;
        let mut inherited_frame = None;
        let mut resolvable = true;

        while let Some(id) = cursor {
            if expected.contains_key(&id) {
                parent = Some(id);
                inherited_frame = expected[&id];
                break;
            }
            if !visiting.insert(id) {
                resolvable = false;
                break;
            }
            let Some((_, record)) = shapes.get(&id).copied() else {
                resolvable = false;
                break;
            };
            path.push(id);
            cursor = record.shape.parent_id;
            if cursor.is_none() {
                break;
            }
        }

        if !resolvable {
            for id in path {
                expected.insert(id, None);
            }
            continue;
        }

        for id in path.into_iter().rev() {
            let enclosing = match parent {
                Some(parent_id)
                    if shapes
                        .get(&parent_id)
                        .is_some_and(|(_, record)| record.shape.shape_type == ShapeType::Frame) =>
                {
                    Some(parent_id)
                }
                _ => inherited_frame,
            };
            expected.insert(id, enclosing);
            inherited_frame = enclosing;
            parent = Some(id);
        }
    }
    expected
}

fn check_id(
    id: Uuid,
    path: impl Into<String>,
    seen: &mut HashSet<Uuid>,
    diagnostics: &mut Vec<ValidationDiagnostic>,
) {
    let path = path.into();
    if id == NIL_RENDERER_ROOT_ID {
        diagnostics.push(
            ValidationDiagnostic::new(ValidationDiagnosticCode::ReservedRendererId, path)
                .ids(id, None),
        );
    } else if !seen.insert(id) {
        diagnostics.push(
            ValidationDiagnostic::new(ValidationDiagnosticCode::DuplicateId, path).ids(id, None),
        );
    }
}

fn check_extension_collisions(
    extensions: &BTreeMap<String, Value>,
    reserved: &[&str],
    path: &str,
    diagnostics: &mut Vec<ValidationDiagnostic>,
) {
    for key in extensions.keys() {
        if reserved.contains(&key.as_str()) {
            diagnostics.push(ValidationDiagnostic::new(
                ValidationDiagnosticCode::ExtensionKeyCollision,
                format!("{path}/{}", escape_json_pointer(key)),
            ));
        }
    }
}

fn escape_json_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn validate_numbers(record: &ShapeRecord, path: &str, diagnostics: &mut Vec<ValidationDiagnostic>) {
    let shape = &record.shape;
    let rect = shape.selrect;
    let geometry = [rect.x, rect.y, rect.width, rect.height, shape.rotation];
    if geometry.iter().any(|value| !value.is_finite()) || rect.width < 0.0 || rect.height < 0.0 {
        diagnostics.push(
            ValidationDiagnostic::new(
                ValidationDiagnosticCode::InvalidGeometry,
                format!("{path}/selrect"),
            )
            .ids(shape.id, None),
        );
    } else if geometry.iter().any(|value| value.abs() > MAX_ABS_GEOMETRY)
        || (rect.x + rect.width).abs() > MAX_ABS_GEOMETRY
        || (rect.y + rect.height).abs() > MAX_ABS_GEOMETRY
    {
        diagnostics.push(
            ValidationDiagnostic::new(
                ValidationDiagnosticCode::GeometryOutOfRange,
                format!("{path}/selrect"),
            )
            .ids(shape.id, None),
        );
    }
    let transform = shape.transform;
    let inverse = shape.transform_inverse;
    let matrix_values = [
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        transform.e,
        transform.f,
        inverse.a,
        inverse.b,
        inverse.c,
        inverse.d,
        inverse.e,
        inverse.f,
    ];
    if matrix_values.iter().any(|value| !value.is_finite()) {
        diagnostics.push(
            ValidationDiagnostic::new(
                ValidationDiagnosticCode::NonFiniteTransform,
                format!("{path}/transform"),
            )
            .ids(shape.id, None),
        );
    } else if matrix_values
        .iter()
        .any(|value| value.abs() > MAX_ABS_GEOMETRY)
    {
        diagnostics.push(
            ValidationDiagnostic::new(
                ValidationDiagnosticCode::GeometryOutOfRange,
                format!("{path}/transform"),
            )
            .ids(shape.id, None),
        );
    } else if transform.inverse().is_none() {
        diagnostics.push(
            ValidationDiagnostic::new(
                ValidationDiagnosticCode::SingularTransform,
                format!("{path}/transform"),
            )
            .ids(shape.id, None),
        );
    } else {
        let identity = Matrix::identity();
        if !(transform * inverse).close_to(&identity, MATRIX_INVERSE_TOLERANCE)
            || !(inverse * transform).close_to(&identity, MATRIX_INVERSE_TOLERANCE)
        {
            diagnostics.push(
                ValidationDiagnostic::new(
                    ValidationDiagnosticCode::TransformInverseMismatch,
                    format!("{path}/transform-inverse"),
                )
                .ids(shape.id, None),
            );
        }
    }
    if !shape.opacity.is_finite() || !(0.0..=1.0).contains(&shape.opacity) {
        diagnostics.push(
            ValidationDiagnostic::new(
                ValidationDiagnosticCode::InvalidOpacity,
                format!("{path}/opacity"),
            )
            .ids(shape.id, None),
        );
    }
}

fn validate_cycles(
    shapes: &HashMap<Uuid, (usize, &ShapeRecord)>,
    diagnostics: &mut Vec<ValidationDiagnostic>,
) {
    let mut ids: Vec<_> = shapes.keys().copied().collect();
    ids.sort();
    let mut complete = HashSet::new();
    for start in ids {
        if complete.contains(&start) {
            continue;
        }
        let mut path = Vec::new();
        let mut positions = HashMap::new();
        let mut cursor = Some(start);
        while let Some(id) = cursor {
            if complete.contains(&id) {
                break;
            }
            if let Some(position) = positions.get(&id).copied() {
                let cycle_start = path[position];
                let index = shapes[&cycle_start].0;
                diagnostics.push(
                    ValidationDiagnostic::new(
                        ValidationDiagnosticCode::Cycle,
                        format!("/shapes/{index}/parent-id"),
                    )
                    .ids(cycle_start, Some(id)),
                );
                break;
            }
            positions.insert(id, path.len());
            path.push(id);
            cursor = shapes
                .get(&id)
                .and_then(|(_, record)| record.shape.parent_id);
        }
        complete.extend(path);
    }
}

fn malformed_id_diagnostics(value: &Value) -> Vec<ValidationDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(root) = value.as_object() else {
        return vec![ValidationDiagnostic::new(
            ValidationDiagnosticCode::DocumentSchemaInvalid,
            "$",
        )];
    };
    check_json_uuid(root.get("id"), "/id", &mut diagnostics);
    if let Some(pages) = root.get("pages").and_then(Value::as_array) {
        for (page_index, page) in pages.iter().enumerate() {
            let path = format!("/pages/{page_index}");
            check_json_uuid(page.get("id"), &format!("{path}/id"), &mut diagnostics);
            if let Some(roots) = page.get("root-shape-ids").and_then(Value::as_array) {
                for (root_index, id) in roots.iter().enumerate() {
                    check_json_uuid(
                        Some(id),
                        &format!("{path}/root-shape-ids/{root_index}"),
                        &mut diagnostics,
                    );
                }
            }
        }
    }
    if let Some(shapes) = root.get("shapes").and_then(Value::as_array) {
        for (index, shape) in shapes.iter().enumerate() {
            let path = format!("/shapes/{index}");
            for field in [
                "id",
                "page-id",
                "parent-id",
                "frame-id",
                "component-id",
                "component-file",
            ] {
                let field_value = shape.get(field);
                if field_value.is_some_and(|v| !v.is_null()) {
                    check_json_uuid(field_value, &format!("{path}/{field}"), &mut diagnostics);
                }
            }
            if let Some(children) = shape.get("shapes").and_then(Value::as_array) {
                for (child_index, id) in children.iter().enumerate() {
                    check_json_uuid(
                        Some(id),
                        &format!("{path}/shapes/{child_index}"),
                        &mut diagnostics,
                    );
                }
            }
        }
    }
    diagnostics
}

fn check_json_uuid(value: Option<&Value>, path: &str, diagnostics: &mut Vec<ValidationDiagnostic>) {
    match value
        .and_then(Value::as_str)
        .and_then(|text| Uuid::parse_str(text).ok())
    {
        Some(_) => {}
        None => diagnostics.push(ValidationDiagnostic::new(
            ValidationDiagnosticCode::MalformedId,
            path,
        )),
    }
}

fn sort_diagnostics(diagnostics: &mut Vec<ValidationDiagnostic>) {
    diagnostics.sort_by(|left, right| {
        (left.code, &left.path, left.object_id, left.related_id).cmp(&(
            right.code,
            &right.path,
            right.object_id,
            right.related_id,
        ))
    });
    diagnostics.dedup();
}

/// Deterministic UUIDv5 import identity from a caller-owned stable namespace
/// and source identity. This is only an ID suggestion: importers must still
/// run duplicate-ID validation before use.
pub fn stable_import_id(namespace: Uuid, source_format: &str, source_id: &str) -> Uuid {
    let mut name = b"logos-import-v1\0".to_vec();
    name.extend_from_slice(&(source_format.len() as u64).to_be_bytes());
    name.extend_from_slice(source_format.as_bytes());
    name.extend_from_slice(&(source_id.len() as u64).to_be_bytes());
    name.extend_from_slice(source_id.as_bytes());
    Uuid::new_v5(&namespace, &name)
}

/// Derive importer IDs and fail if any two source identities map to the same
/// UUID. Callers must apply this check before adding imported records to a
/// candidate document; they must never silently suffix or reassign collisions.
pub fn stable_import_ids<I, S>(
    namespace: Uuid,
    source_format: &str,
    source_ids: I,
) -> Result<Vec<Uuid>, ValidationDiagnostic>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for (index, source_id) in source_ids.into_iter().enumerate() {
        let derived = stable_import_id(namespace, source_format, source_id.as_ref());
        if !seen.insert(derived) {
            return Err(ValidationDiagnostic::new(
                ValidationDiagnosticCode::DuplicateId,
                format!("/import/source-identities/{index}"),
            )
            .ids(derived, None));
        }
        result.push(derived);
    }
    Ok(result)
}

/// Candidate immutable operations. These are not connected to production
/// Zustand actions, Go RPCs, or saved documents.
#[derive(Debug, Clone, PartialEq)]
pub enum DocumentOperation {
    CreateShape {
        record: ShapeRecord,
        parent_id: Option<Uuid>,
        index: usize,
    },
    UpdateShape {
        shape_id: Uuid,
        expected_parent_id: Option<Uuid>,
        name: Option<String>,
        selrect: Option<Rect>,
        transform: Option<Matrix>,
        opacity: Option<f64>,
    },
    DeleteSubtree {
        shape_id: Uuid,
        expected_parent_id: Option<Uuid>,
    },
    ReparentShape {
        shape_id: Uuid,
        expected_parent_id: Option<Uuid>,
        new_parent_id: Option<Uuid>,
        new_page_id: Uuid,
        index: usize,
    },
    ReorderChildren {
        page_id: Uuid,
        parent_id: Option<Uuid>,
        expected_ids: Vec<Uuid>,
        ordered_ids: Vec<Uuid>,
    },
}

/// Apply a proposal to a clone and return a new validated candidate. The input
/// remains unchanged on success and failure. Reparent keeps local coordinates;
/// visual-position preservation is intentionally not guessed.
pub fn apply_operation(
    document: &Document,
    operation: &DocumentOperation,
) -> Result<Document, Vec<ValidationDiagnostic>> {
    let base_diagnostics = validate_document(document);
    if !base_diagnostics.is_empty() {
        return Err(base_diagnostics);
    }
    let mut candidate = document.clone();
    let result = apply_to_candidate(&mut candidate, operation);
    if let Err(diagnostic) = result {
        return Err(vec![diagnostic]);
    }
    let diagnostics = validate_document(&candidate);
    if diagnostics.is_empty() {
        Ok(candidate)
    } else {
        Err(diagnostics)
    }
}

fn apply_to_candidate(
    document: &mut Document,
    operation: &DocumentOperation,
) -> Result<(), ValidationDiagnostic> {
    match operation {
        DocumentOperation::CreateShape {
            record,
            parent_id,
            index,
        } => {
            if document
                .shapes
                .iter()
                .any(|item| item.shape.id == record.shape.id)
            {
                return Err(op_error(record.shape.id, None));
            }
            if !document.pages.iter().any(|page| page.id == record.page_id) {
                return Err(missing_page(record.shape.id, record.page_id));
            }
            let mut new_record = record.clone();
            if let Some(parent_id) = parent_id {
                let parent = document
                    .shapes
                    .iter()
                    .find(|item| item.shape.id == *parent_id)
                    .ok_or_else(|| missing_shape(record.shape.id, *parent_id))?;
                if parent.page_id != record.page_id || !parent.shape.shape_type.is_container() {
                    return Err(op_error(record.shape.id, Some(*parent_id)));
                }
                if record.shape.parent_id.is_some_and(|id| id != *parent_id) {
                    return Err(op_error(record.shape.id, record.shape.parent_id));
                }
                if *index > parent.shape.shapes.len() {
                    return Err(op_error(record.shape.id, Some(*parent_id)));
                }
                new_record.shape.parent_id = Some(*parent_id);
                new_record.shape.frame_id = nearest_frame_for_parent(document, Some(*parent_id));
                let parent = document
                    .shapes
                    .iter_mut()
                    .find(|item| item.shape.id == *parent_id)
                    .expect("checked above");
                parent.shape.shapes.insert(*index, new_record.shape.id);
                document.shapes.push(new_record);
            } else {
                let page_index = document
                    .pages
                    .iter()
                    .position(|page| page.id == record.page_id)
                    .unwrap();
                if *index > document.pages[page_index].root_shape_ids.len() {
                    return Err(op_error(record.shape.id, None));
                }
                if record.shape.parent_id.is_some() {
                    return Err(op_error(record.shape.id, record.shape.parent_id));
                }
                new_record.shape.frame_id = None;
                document.pages[page_index]
                    .root_shape_ids
                    .insert(*index, record.shape.id);
                document.shapes.push(new_record);
            }
        }
        DocumentOperation::UpdateShape {
            shape_id,
            expected_parent_id,
            name,
            selrect,
            transform,
            opacity,
        } => {
            let record = document
                .shapes
                .iter_mut()
                .find(|item| item.shape.id == *shape_id)
                .ok_or_else(|| missing_shape(*shape_id, *shape_id))?;
            if record.shape.parent_id != *expected_parent_id {
                return Err(op_error(*shape_id, record.shape.parent_id));
            }
            if let Some(name) = name {
                record.shape.name = name.clone();
            }
            if let Some(selrect) = selrect {
                record.shape.selrect = *selrect;
            }
            if let Some(transform) = transform {
                let Some(inverse) = transform.inverse() else {
                    return Err(ValidationDiagnostic::new(
                        ValidationDiagnosticCode::InvalidOperation,
                        "/operation/transform",
                    )
                    .ids(*shape_id, None));
                };
                record.shape.transform = *transform;
                record.shape.transform_inverse = inverse;
            }
            if let Some(opacity) = opacity {
                record.shape.opacity = *opacity;
            }
        }
        DocumentOperation::DeleteSubtree {
            shape_id,
            expected_parent_id,
        } => {
            let target_index = document
                .shapes
                .iter()
                .position(|item| item.shape.id == *shape_id)
                .ok_or_else(|| missing_shape(*shape_id, *shape_id))?;
            if document.shapes[target_index].shape.parent_id != *expected_parent_id {
                return Err(op_error(
                    *shape_id,
                    document.shapes[target_index].shape.parent_id,
                ));
            }
            let page_id = document.shapes[target_index].page_id;
            let subtree = subtree_ids(document, *shape_id);
            if let Some(parent_id) = expected_parent_id {
                if let Some(parent) = document
                    .shapes
                    .iter_mut()
                    .find(|item| item.shape.id == *parent_id)
                {
                    parent.shape.shapes.retain(|id| !subtree.contains(id));
                }
            } else if let Some(page) = document.pages.iter_mut().find(|page| page.id == page_id) {
                page.root_shape_ids.retain(|id| !subtree.contains(id));
            }
            document
                .shapes
                .retain(|record| !subtree.contains(&record.shape.id));
        }
        DocumentOperation::ReparentShape {
            shape_id,
            expected_parent_id,
            new_parent_id,
            new_page_id,
            index,
        } => {
            let source_index = document
                .shapes
                .iter()
                .position(|record| record.shape.id == *shape_id)
                .ok_or_else(|| missing_shape(*shape_id, *shape_id))?;
            if document.shapes[source_index].shape.parent_id != *expected_parent_id {
                return Err(op_error(
                    *shape_id,
                    document.shapes[source_index].shape.parent_id,
                ));
            }
            if !document.pages.iter().any(|page| page.id == *new_page_id) {
                return Err(missing_page(*shape_id, *new_page_id));
            }
            if let Some(parent_id) = new_parent_id {
                let parent = document
                    .shapes
                    .iter()
                    .find(|record| record.shape.id == *parent_id)
                    .ok_or_else(|| missing_shape(*shape_id, *parent_id))?;
                if parent.page_id != *new_page_id || !parent.shape.shape_type.is_container() {
                    return Err(op_error(*shape_id, Some(*parent_id)));
                }
                if *parent_id == *shape_id || is_descendant(document, *parent_id, *shape_id) {
                    return Err(ValidationDiagnostic::new(
                        ValidationDiagnosticCode::Cycle,
                        format!("/shapes/{source_index}/parent-id"),
                    )
                    .ids(*shape_id, Some(*parent_id)));
                }
            }
            let old_page_id = document.shapes[source_index].page_id;
            let destination_len = if let Some(parent_id) = new_parent_id {
                let parent = document
                    .shapes
                    .iter()
                    .find(|record| record.shape.id == *parent_id)
                    .unwrap();
                parent.shape.shapes.len() - usize::from(*expected_parent_id == Some(*parent_id))
            } else {
                let page = document
                    .pages
                    .iter()
                    .find(|page| page.id == *new_page_id)
                    .unwrap();
                page.root_shape_ids.len()
                    - usize::from(expected_parent_id.is_none() && old_page_id == *new_page_id)
            };
            if *index > destination_len {
                return Err(op_error(*shape_id, *new_parent_id));
            }
            let moved_ids = subtree_ids(document, *shape_id);
            detach_from_current_parent(document, source_index);
            let mut record = document.shapes.remove(source_index);
            record.page_id = *new_page_id;
            record.shape.parent_id = *new_parent_id;
            if let Some(parent_id) = new_parent_id {
                let parent = document
                    .shapes
                    .iter_mut()
                    .find(|item| item.shape.id == *parent_id)
                    .expect("checked above");
                parent.shape.shapes.insert(*index, *shape_id);
            } else {
                let page = document
                    .pages
                    .iter_mut()
                    .find(|page| page.id == *new_page_id)
                    .expect("checked above");
                page.root_shape_ids.insert(*index, *shape_id);
            }
            document.shapes.push(record);
            update_subtree_page_and_frames(document, *shape_id, *new_page_id, &moved_ids);
        }
        DocumentOperation::ReorderChildren {
            page_id,
            parent_id,
            expected_ids,
            ordered_ids,
        } => {
            if ordered_ids.len() != expected_ids.len()
                || ordered_ids.iter().copied().collect::<HashSet<_>>().len() != ordered_ids.len()
                || expected_ids.iter().copied().collect::<HashSet<_>>()
                    != ordered_ids.iter().copied().collect::<HashSet<_>>()
            {
                return Err(ValidationDiagnostic::new(
                    ValidationDiagnosticCode::InvalidSiblingOrder,
                    "/operation/ordered-ids",
                )
                .ids(parent_id.unwrap_or(*page_id), None));
            }
            match parent_id {
                Some(parent_id) => {
                    let parent = document
                        .shapes
                        .iter_mut()
                        .find(|record| record.shape.id == *parent_id)
                        .ok_or_else(|| missing_shape(*parent_id, *parent_id))?;
                    if parent.page_id != *page_id || parent.shape.shapes != *expected_ids {
                        return Err(op_error(*parent_id, None));
                    }
                    parent.shape.shapes = ordered_ids.clone();
                }
                None => {
                    let page = document
                        .pages
                        .iter_mut()
                        .find(|page| page.id == *page_id)
                        .ok_or_else(|| missing_page(*page_id, *page_id))?;
                    if page.root_shape_ids != *expected_ids {
                        return Err(op_error(*page_id, None));
                    }
                    page.root_shape_ids = ordered_ids.clone();
                }
            }
        }
    }
    Ok(())
}

fn detach_from_current_parent(document: &mut Document, record_index: usize) {
    let record = &document.shapes[record_index];
    let id = record.shape.id;
    let page_id = record.page_id;
    if let Some(parent_id) = record.shape.parent_id {
        if let Some(parent) = document
            .shapes
            .iter_mut()
            .find(|item| item.shape.id == parent_id)
        {
            parent.shape.shapes.retain(|child| *child != id);
        }
    } else if let Some(page) = document.pages.iter_mut().find(|page| page.id == page_id) {
        page.root_shape_ids.retain(|root| *root != id);
    }
}

fn subtree_ids(document: &Document, root_id: Uuid) -> HashSet<Uuid> {
    let index_by_id: HashMap<_, _> = document
        .shapes
        .iter()
        .enumerate()
        .map(|(index, record)| (record.shape.id, index))
        .collect();
    let mut subtree = HashSet::new();
    let mut stack = vec![root_id];
    while let Some(id) = stack.pop() {
        if !subtree.insert(id) {
            continue;
        }
        if let Some(index) = index_by_id.get(&id) {
            stack.extend(document.shapes[*index].shape.shapes.iter().copied());
        }
    }
    subtree
}

fn nearest_frame_for_parent(document: &Document, mut parent_id: Option<Uuid>) -> Option<Uuid> {
    let index_by_id: HashMap<_, _> = document
        .shapes
        .iter()
        .enumerate()
        .map(|(index, record)| (record.shape.id, index))
        .collect();
    let mut visited = HashSet::new();
    while let Some(id) = parent_id {
        if !visited.insert(id) {
            return None;
        }
        let parent = &document.shapes[*index_by_id.get(&id)?];
        if parent.shape.shape_type == ShapeType::Frame {
            return Some(id);
        }
        parent_id = parent.shape.parent_id;
    }
    None
}

fn update_subtree_page_and_frames(
    document: &mut Document,
    root_id: Uuid,
    page_id: Uuid,
    subtree: &HashSet<Uuid>,
) {
    let index_by_id: HashMap<_, _> = document
        .shapes
        .iter()
        .enumerate()
        .map(|(index, record)| (record.shape.id, index))
        .collect();
    let root_index = index_by_id[&root_id];
    let root_frame =
        nearest_frame_for_parent(document, document.shapes[root_index].shape.parent_id);
    let mut stack = vec![(root_id, root_frame)];
    while let Some((id, frame)) = stack.pop() {
        if !subtree.contains(&id) {
            continue;
        }
        let Some(index) = index_by_id.get(&id).copied() else {
            continue;
        };
        let record = &mut document.shapes[index];
        record.page_id = page_id;
        record.shape.frame_id = frame;
        let child_frame = if record.shape.shape_type == ShapeType::Frame {
            Some(id)
        } else {
            frame
        };
        stack.extend(
            record
                .shape
                .shapes
                .iter()
                .rev()
                .copied()
                .map(|child_id| (child_id, child_frame)),
        );
    }
}

fn is_descendant(document: &Document, possible_descendant: Uuid, ancestor: Uuid) -> bool {
    let index_by_id: HashMap<_, _> = document
        .shapes
        .iter()
        .enumerate()
        .map(|(index, record)| (record.shape.id, index))
        .collect();
    let mut visited = HashSet::new();
    let mut current = Some(possible_descendant);
    while let Some(id) = current {
        if !visited.insert(id) {
            return true;
        }
        let Some(index) = index_by_id.get(&id).copied() else {
            return false;
        };
        let parent_id = document.shapes[index].shape.parent_id;
        if parent_id == Some(ancestor) {
            return true;
        }
        current = parent_id;
    }
    false
}

fn op_error(object_id: Uuid, related_id: Option<Uuid>) -> ValidationDiagnostic {
    ValidationDiagnostic {
        code: ValidationDiagnosticCode::OperationPreconditionFailed,
        path: "/operation".into(),
        object_id: Some(object_id),
        related_id,
    }
}

fn missing_shape(object_id: Uuid, related_id: Uuid) -> ValidationDiagnostic {
    ValidationDiagnostic::new(
        ValidationDiagnosticCode::MissingShape,
        "/operation/shape-id",
    )
    .ids(object_id, Some(related_id))
}

fn missing_page(object_id: Uuid, related_id: Uuid) -> ValidationDiagnostic {
    ValidationDiagnostic::new(ValidationDiagnosticCode::MissingPage, "/operation/page-id")
        .ids(object_id, Some(related_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Shape;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn record(shape: Shape, page_id: Uuid) -> ShapeRecord {
        ShapeRecord {
            page_id,
            shape,
            extensions: BTreeMap::new(),
        }
    }

    fn valid_document() -> Document {
        let page_id = id(1);
        let frame_id = id(2);
        let child_id = id(3);
        let mut frame = Shape::frame(frame_id, "Frame", 0.0, 0.0, 100.0, 100.0);
        frame.shapes = vec![child_id];
        let mut child = Shape::rect(child_id, 1.0, 2.0, 20.0, 30.0);
        child.parent_id = Some(frame_id);
        child.frame_id = Some(frame_id);
        Document {
            id: id(10),
            schema_version: DOCUMENT_SCHEMA_VERSION,
            pages: vec![Page {
                id: page_id,
                name: "Page".into(),
                root_shape_ids: vec![frame_id],
                extensions: BTreeMap::new(),
            }],
            shapes: vec![record(frame, page_id), record(child, page_id)],
            extensions: BTreeMap::new(),
        }
    }

    #[test]
    fn valid_nested_document_round_trips_with_unknown_extensions() {
        let mut document = valid_document();
        document
            .extensions
            .insert("future-doc-field".into(), serde_json::json!({"a": 1}));
        document.shapes[0]
            .extensions
            .insert("future-shape-field".into(), serde_json::json!([1, 2]));
        let json = serde_json::to_string(&document).unwrap();
        let decoded = decode_and_validate_json(&json).unwrap();
        assert_eq!(decoded, document);
        assert_eq!(validate_document(&decoded), vec![]);
    }

    #[test]
    fn invalid_graph_diagnostics_are_stable_and_input_is_immutable() {
        let mut document = valid_document();
        document.shapes[0].shape.shapes.push(id(999));
        let before = serde_json::to_value(&document).unwrap();
        let first = validate_document(&document);
        let second = validate_document(&document);
        assert_eq!(first, second);
        assert!(first
            .iter()
            .any(|item| item.code == ValidationDiagnosticCode::MissingShape));
        assert_eq!(serde_json::to_value(&document).unwrap(), before);
    }

    #[test]
    fn malformed_ids_return_stable_path_diagnostics() {
        let input = r#"{"id":"bad","schema-version":1,"pages":[],"shapes":[]}"#;
        let result = decode_and_validate_json(input).unwrap_err();
        assert_eq!(result[0].code, ValidationDiagnosticCode::MalformedId);
        assert_eq!(result[0].path, "/id");
    }

    #[test]
    fn stable_import_id_depends_on_namespace_format_and_source_id() {
        let namespace = id(44);
        assert_eq!(
            stable_import_id(namespace, "xd", "artboard-1"),
            stable_import_id(namespace, "xd", "artboard-1")
        );
        assert_ne!(
            stable_import_id(namespace, "xd", "artboard-1"),
            stable_import_id(namespace, "xd", "artboard-2")
        );
        assert_ne!(
            stable_import_id(namespace, "xd", "artboard-1"),
            stable_import_id(id(45), "xd", "artboard-1")
        );
        assert_ne!(
            stable_import_id(namespace, "xd:a", "b"),
            stable_import_id(namespace, "xd", "a:b")
        );
        assert_eq!(
            Uuid::new_v5(&Uuid::NAMESPACE_DNS, b"www.widgets.com").to_string(),
            "21f7f8de-8051-5b89-8680-0195ef798b6a"
        );
        let duplicate =
            stable_import_ids(namespace, "xd", ["artboard-1", "artboard-1"]).unwrap_err();
        assert_eq!(duplicate.code, ValidationDiagnosticCode::DuplicateId);
    }

    #[test]
    fn shared_frame_ancestry_requires_nearest_frame_even_through_groups() {
        let mut document = valid_document();
        let group_id = id(4);
        let mut group = Shape::new(group_id, ShapeType::Group, "Group");
        group.parent_id = Some(id(2));
        group.frame_id = Some(id(2));
        group.shapes.push(id(3));
        document.shapes[0].shape.shapes = vec![group_id];
        document.shapes[1].shape.parent_id = Some(group_id);
        document.shapes.push(record(group, id(1)));
        // Put group before its descendant in storage to demonstrate order is
        // expressed by graph links rather than vector order.
        document.shapes.swap(1, 2);
        assert!(validate_document(&document).is_empty());

        let leaf_index = document
            .shapes
            .iter()
            .position(|r| r.shape.id == id(3))
            .unwrap();
        document.shapes[leaf_index].shape.frame_id = None;
        assert!(validate_document(&document)
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::FrameMembershipMismatch));
    }

    #[test]
    fn cross_page_reparent_updates_page_and_nearest_frame_closure() {
        let mut document = valid_document();
        let second_page = id(11);
        let second_frame = id(12);
        let frame = Shape::frame(second_frame, "Second frame", 0.0, 0.0, 100.0, 100.0);
        document.pages.push(Page {
            id: second_page,
            name: "Page 2".into(),
            root_shape_ids: vec![second_frame],
            extensions: BTreeMap::new(),
        });
        document.shapes.push(record(frame, second_page));
        let moved = apply_operation(
            &document,
            &DocumentOperation::ReparentShape {
                shape_id: id(2),
                expected_parent_id: None,
                new_parent_id: None,
                new_page_id: second_page,
                index: 1,
            },
        )
        .unwrap();
        assert!(moved.pages[0].root_shape_ids.is_empty());
        assert_eq!(moved.pages[1].root_shape_ids, vec![second_frame, id(2)]);
        let frame_index = moved
            .shapes
            .iter()
            .position(|r| r.shape.id == id(2))
            .unwrap();
        let child_index = moved
            .shapes
            .iter()
            .position(|r| r.shape.id == id(3))
            .unwrap();
        assert_eq!(moved.shapes[frame_index].page_id, second_page);
        assert_eq!(moved.shapes[frame_index].shape.frame_id, None);
        assert_eq!(moved.shapes[child_index].page_id, second_page);
        assert_eq!(moved.shapes[child_index].shape.frame_id, Some(id(2)));
        assert!(validate_document(&moved).is_empty());
    }

    #[test]
    fn same_parent_reparent_index_is_measured_after_detach() {
        let mut document = valid_document();
        let extra_id = id(4);
        document.shapes[0].shape.shapes.push(extra_id);
        let mut extra = Shape::rect(extra_id, 2.0, 3.0, 4.0, 5.0);
        extra.parent_id = Some(id(2));
        extra.frame_id = Some(id(2));
        document.shapes.push(record(extra, id(1)));
        assert!(validate_document(&document).is_empty());

        let moved = apply_operation(
            &document,
            &DocumentOperation::ReparentShape {
                shape_id: id(3),
                expected_parent_id: Some(id(2)),
                new_parent_id: Some(id(2)),
                new_page_id: id(1),
                index: 1,
            },
        )
        .unwrap();
        assert_eq!(moved.shapes[0].shape.shapes, vec![id(4), id(3)]);
        assert!(validate_document(&moved).is_empty());
    }

    #[test]
    fn rejected_same_parent_reparent_index_does_not_mutate_input() {
        let document = valid_document();
        let before = serde_json::to_value(&document).unwrap();
        let result = apply_operation(
            &document,
            &DocumentOperation::ReparentShape {
                shape_id: id(3),
                expected_parent_id: Some(id(2)),
                new_parent_id: Some(id(2)),
                new_page_id: id(1),
                index: 2,
            },
        );
        assert!(result.is_err());
        assert_eq!(serde_json::to_value(&document).unwrap(), before);
    }

    #[test]
    fn delete_rejects_removing_a_still_referenced_component() {
        let page_id = id(1);
        let component_id = id(2);
        let instance_id = id(3);
        let mut component = Shape::new(component_id, ShapeType::Component, "Master");
        component.main_instance = false;
        let mut instance = Shape::rect(instance_id, 0.0, 0.0, 10.0, 10.0);
        instance.component_id = Some(component_id);
        let document = Document {
            id: id(10),
            schema_version: DOCUMENT_SCHEMA_VERSION,
            pages: vec![Page {
                id: page_id,
                name: "Components".into(),
                root_shape_ids: vec![component_id, instance_id],
                extensions: BTreeMap::new(),
            }],
            shapes: vec![record(component, page_id), record(instance, page_id)],
            extensions: BTreeMap::new(),
        };
        assert!(validate_document(&document).is_empty());
        let before = serde_json::to_value(&document).unwrap();
        let result = apply_operation(
            &document,
            &DocumentOperation::DeleteSubtree {
                shape_id: component_id,
                expected_parent_id: None,
            },
        );
        assert!(result
            .unwrap_err()
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::InvalidComponentReference));
        assert_eq!(serde_json::to_value(&document).unwrap(), before);
    }

    #[test]
    fn transform_inverse_numeric_and_geometry_boundary_policy_is_explicit() {
        let mut document = valid_document();
        let child_index = document
            .shapes
            .iter()
            .position(|r| r.shape.id == id(3))
            .unwrap();
        document.shapes[child_index].shape.transform = Matrix::translate_dist(12.0, -8.0);
        document.shapes[child_index].shape.transform_inverse = Matrix::translate_dist(-12.0, 8.0);
        document.shapes[child_index].shape.selrect = Rect::new(-MAX_ABS_GEOMETRY, 0.0, 0.0, 0.0);
        assert!(validate_document(&document).is_empty());

        document.shapes[child_index].shape.transform_inverse = Matrix::identity();
        assert!(validate_document(&document)
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::TransformInverseMismatch));

        document.shapes[child_index].shape.transform = Matrix::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        document.shapes[child_index].shape.transform_inverse =
            Matrix::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert!(validate_document(&document)
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::SingularTransform));

        document.shapes[child_index].shape.transform = Matrix::identity();
        document.shapes[child_index].shape.transform_inverse = Matrix::identity();
        document.shapes[child_index].shape.selrect.x = MAX_ABS_GEOMETRY + 1.0;
        assert!(validate_document(&document)
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::GeometryOutOfRange));

        document.shapes[child_index].shape.selrect.x = 0.0;
        document.shapes[child_index].shape.selrect.width = -0.01;
        assert!(validate_document(&document)
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::InvalidGeometry));
    }

    #[test]
    fn delete_subtree_is_immutable_and_removes_descendants() {
        let document = valid_document();
        let before = serde_json::to_value(&document).unwrap();
        let result = apply_operation(
            &document,
            &DocumentOperation::DeleteSubtree {
                shape_id: id(2),
                expected_parent_id: None,
            },
        )
        .unwrap();
        assert_eq!(serde_json::to_value(&document).unwrap(), before);
        assert!(result.shapes.is_empty());
        assert!(result.pages[0].root_shape_ids.is_empty());
    }

    #[test]
    fn successful_create_update_reparent_and_reorder_preserve_contract() {
        let document = valid_document();
        let create = DocumentOperation::CreateShape {
            record: record(Shape::rect(id(4), 5.0, 6.0, 7.0, 8.0), id(1)),
            parent_id: None,
            index: 1,
        };
        let created = apply_operation(&document, &create).unwrap();
        assert_eq!(created.pages[0].root_shape_ids, vec![id(2), id(4)]);

        let reparented = apply_operation(
            &created,
            &DocumentOperation::ReparentShape {
                shape_id: id(3),
                expected_parent_id: Some(id(2)),
                new_parent_id: None,
                new_page_id: id(1),
                index: 1,
            },
        )
        .unwrap();
        assert_eq!(
            reparented.pages[0].root_shape_ids,
            vec![id(2), id(3), id(4)]
        );
        assert_eq!(
            reparented
                .shapes
                .iter()
                .find(|r| r.shape.id == id(3))
                .unwrap()
                .shape
                .parent_id,
            None
        );

        let reordered = apply_operation(
            &reparented,
            &DocumentOperation::ReorderChildren {
                page_id: id(1),
                parent_id: None,
                expected_ids: vec![id(2), id(3), id(4)],
                ordered_ids: vec![id(4), id(3), id(2)],
            },
        )
        .unwrap();
        assert_eq!(reordered.pages[0].root_shape_ids, vec![id(4), id(3), id(2)]);

        let translated = Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 20.0,
            f: 30.0,
        };
        let updated = apply_operation(
            &reordered,
            &DocumentOperation::UpdateShape {
                shape_id: id(4),
                expected_parent_id: None,
                name: Some("Updated".into()),
                selrect: None,
                transform: Some(translated),
                opacity: Some(0.5),
            },
        )
        .unwrap();
        let updated_shape = &updated
            .shapes
            .iter()
            .find(|r| r.shape.id == id(4))
            .unwrap()
            .shape;
        assert_eq!(updated_shape.name, "Updated");
        assert_eq!(updated_shape.opacity, 0.5);
        assert_eq!(updated_shape.transform_inverse.e, -20.0);
        assert!(validate_document(&updated).is_empty());
    }

    #[test]
    fn failed_reorder_leaves_candidate_unchanged() {
        let document = valid_document();
        let before = serde_json::to_value(&document).unwrap();
        let result = apply_operation(
            &document,
            &DocumentOperation::ReorderChildren {
                page_id: id(1),
                parent_id: Some(id(2)),
                expected_ids: vec![id(3)],
                ordered_ids: vec![id(999)],
            },
        );
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err()[0].code,
            ValidationDiagnosticCode::InvalidSiblingOrder
        );
        assert_eq!(serde_json::to_value(&document).unwrap(), before);
    }

    #[test]
    fn validator_rejects_duplicate_membership_cycles_and_bad_container_with_stable_order() {
        let mut document = valid_document();
        document.shapes[0].shape.shapes = vec![id(3), id(3)];
        document.shapes[1].shape.parent_id = Some(id(3));
        document.shapes[1].shape.shapes = vec![id(2)];
        let before = serde_json::to_value(&document).unwrap();
        let first = validate_document(&document);
        let second = validate_document(&document);
        assert_eq!(first, second);
        assert!(first
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::DuplicateChild));
        assert!(first
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::Cycle));
        assert!(first
            .windows(2)
            .all(|pair| (pair[0].code, &pair[0].path) <= (pair[1].code, &pair[1].path)));
        assert_eq!(serde_json::to_value(&document).unwrap(), before);
    }

    #[test]
    fn invalid_numbers_and_renderer_reserved_id_are_rejected() {
        let mut document = valid_document();
        document.shapes[1].shape.opacity = f64::NAN;
        document.shapes[1].shape.selrect.width = -1.0;
        document.shapes[1].shape.transform.e = f64::INFINITY;
        let diagnostics = validate_document(&document);
        assert!(diagnostics
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::InvalidOpacity));
        assert!(diagnostics
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::InvalidGeometry));
        assert!(diagnostics
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::NonFiniteTransform));
        let mut reserved = valid_document();
        reserved.shapes[0].shape.id = Uuid::nil();
        assert!(validate_document(&reserved)
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::ReservedRendererId));
    }

    #[test]
    fn failed_update_and_reparent_are_rolled_back() {
        let document = valid_document();
        let before = serde_json::to_value(&document).unwrap();
        let singular = Matrix {
            a: 0.0,
            b: 0.0,
            c: 0.0,
            d: 0.0,
            e: 0.0,
            f: 0.0,
        };
        let failed = apply_operation(
            &document,
            &DocumentOperation::UpdateShape {
                shape_id: id(3),
                expected_parent_id: Some(id(2)),
                name: None,
                selrect: None,
                transform: Some(singular),
                opacity: None,
            },
        );
        assert!(failed.is_err());
        let failed = apply_operation(
            &document,
            &DocumentOperation::ReparentShape {
                shape_id: id(2),
                expected_parent_id: None,
                new_parent_id: Some(id(3)),
                new_page_id: id(1),
                index: 0,
            },
        );
        assert!(failed.is_err());
        assert_eq!(serde_json::to_value(&document).unwrap(), before);
    }

    #[test]
    fn large_flat_graph_validates_without_recursive_walk() {
        let page_id = id(1);
        let mut page = Page {
            id: page_id,
            name: "large".into(),
            root_shape_ids: vec![],
            extensions: BTreeMap::new(),
        };
        let mut shapes = Vec::with_capacity(2_000);
        for n in 2..2_002 {
            let shape_id = id(n);
            page.root_shape_ids.push(shape_id);
            shapes.push(record(Shape::rect(shape_id, 0.0, 0.0, 1.0, 1.0), page_id));
        }
        let document = Document {
            id: id(10_000),
            schema_version: DOCUMENT_SCHEMA_VERSION,
            pages: vec![page],
            shapes,
            extensions: BTreeMap::new(),
        };
        assert!(validate_document(&document).is_empty());
    }

    #[test]
    fn deep_component_group_hierarchy_validates_iteratively() {
        let page_id = id(1);
        let root_id = id(2);
        let mut root = Shape::new(root_id, ShapeType::Component, "Component");
        let mut shapes: Vec<ShapeRecord> = vec![];
        let mut previous_id = root_id;
        let depth = 512u128;
        for n in 0..depth {
            let child_id = id(10 + n);
            if n == 0 {
                root.shapes.push(child_id);
            }
            let child_type = if n + 1 == depth {
                ShapeType::Rect
            } else {
                ShapeType::Group
            };
            let mut child = Shape::new(child_id, child_type, "Nested");
            child.parent_id = Some(previous_id);
            if let Some(parent) = shapes.last_mut() {
                parent.shape.shapes.push(child_id);
            }
            shapes.push(record(child, page_id));
            previous_id = child_id;
        }
        shapes.insert(0, record(root, page_id));
        let document = Document {
            id: id(9000),
            schema_version: DOCUMENT_SCHEMA_VERSION,
            pages: vec![Page {
                id: page_id,
                name: "nested".into(),
                root_shape_ids: vec![root_id],
                extensions: BTreeMap::new(),
            }],
            shapes,
            extensions: BTreeMap::new(),
        };
        assert!(validate_document(&document).is_empty());
    }

    #[test]
    fn compatibility_envelopes_preserve_react_go_and_xd_source_without_promoting_them() {
        for (format, fixture) in [
            (
                "react-projection",
                include_str!("../tests/fixtures/g1-b1/react-projection.json"),
            ),
            (
                "go-filedata-v67",
                include_str!("../tests/fixtures/g1-b1/go-filedata-v67.json"),
            ),
            (
                "xd-import",
                include_str!("../tests/fixtures/g1-b1/xd-artboard-hierarchy.json"),
            ),
        ] {
            let payload: Value = serde_json::from_str(fixture).unwrap();
            let envelope = CompatibilityEnvelope {
                source_format: format.into(),
                source_schema: "fixture".into(),
                payload,
            };
            let encoded = serde_json::to_value(&envelope).unwrap();
            assert_eq!(encoded["payload"], envelope.payload);
            assert!(serde_json::from_value::<Document>(envelope.payload.clone()).is_err());
        }
    }

    #[test]
    fn rust_shape_contract_fixture_and_unknown_future_data_round_trip() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/g1-b1/rust-contract-candidate.json"
        ))
        .unwrap();
        let decoded = decode_and_validate_json(&fixture.to_string()).unwrap();
        let encoded = serde_json::to_value(decoded).unwrap();
        assert_eq!(
            encoded["future-document-extension"],
            fixture["future-document-extension"]
        );
        assert_eq!(
            encoded["shapes"][0]["future-shape-extension"],
            fixture["shapes"][0]["future-shape-extension"]
        );
    }

    #[test]
    fn candidate_decode_normalize_encode_matches_golden_output_and_identity() {
        let source = include_str!("../tests/fixtures/g1-b1/rust-contract-candidate.json");
        let expected =
            include_str!("../tests/fixtures/g1-b1/rust-contract-candidate-normalized.json");
        let decoded = decode_and_validate_json(source).unwrap();
        assert_eq!(
            decoded.id,
            Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap()
        );
        assert_eq!(
            decoded.pages[0].id,
            Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap()
        );
        assert_eq!(
            decoded.shapes[0].shape.id,
            Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap()
        );
        assert_eq!(decoded.shapes[0].shape.selrect.width, 30.0);
        let encoded = serde_json::to_value(decoded).unwrap();
        let expected: Value = serde_json::from_str(expected).unwrap();
        assert_eq!(encoded, expected);
    }

    #[test]
    fn nested_component_group_golden_is_valid_and_invalid_graph_is_rejected() {
        let nested = include_str!("../tests/fixtures/g1-b1/nested-component-group.json");
        assert!(decode_and_validate_json(nested).is_ok());
        let invalid = include_str!("../tests/fixtures/g1-b1/invalid-page-graph.json");
        let diagnostics = decode_and_validate_json(invalid).unwrap_err();
        assert!(diagnostics
            .iter()
            .any(|d| d.code == ValidationDiagnosticCode::DuplicateRootMembership));
        assert_eq!(diagnostics, decode_and_validate_json(invalid).unwrap_err());
    }
}
