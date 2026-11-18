//! Isolated representation of the TypeScript Component/Instance domain v1.
//!
//! This is a qualification DTO, not part of `logos_types::Document`, a
//! persistent registry, or a semantic validator. Structs reject unknown keys;
//! maps are ordered for deterministic JSON output while arrays retain authored
//! order. Arbitrary values use JSON's exact value domain without coercion.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

pub const COMPONENT_INSTANCE_DOMAIN_VERSION: u32 = 1;
pub type OrderedJsonRecord<T> = BTreeMap<String, T>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ComponentReference {
    LocalComponent {
        component_id: String,
    },
    LocalSet {
        component_set_id: String,
    },
    External {
        file_id: String,
        component_id: String,
        #[serde(deserialize_with = "deserialize_nullable_string")]
        revision: NullableString,
    },
    Unresolved {
        locator: JsonValue,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum NullableString {
    Null,
    String(String),
}

impl<'de> Deserialize<'de> for NullableString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_nullable_string(deserializer)
    }
}

fn deserialize_nullable_string<'de, D>(deserializer: D) -> Result<NullableString, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match Option::<String>::deserialize(deserializer)? {
        Some(value) => NullableString::String(value),
        None => NullableString::Null,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VariantProperty {
    pub id: String,
    pub name: String,
    pub kind: VariantPropertyKind,
    pub values: Vec<String>,
    pub default_value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VariantPropertyKind {
    Variant,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ComponentProperty {
    Boolean {
        id: String,
        name: String,
        target_node_id: String,
        default_value: bool,
    },
    Text {
        id: String,
        name: String,
        target_node_id: String,
        default_value: String,
    },
    InstanceSwap {
        id: String,
        name: String,
        target_node_id: String,
        allowed_component_ids: Vec<String>,
        default_component_id: String,
    },
    Slot {
        id: String,
        name: String,
        target_node_id: String,
        #[serde(
            default,
            deserialize_with = "deserialize_optional_non_null",
            skip_serializing_if = "Option::is_none"
        )]
        guidance: Option<SlotGuidance>,
        #[serde(
            default,
            deserialize_with = "deserialize_optional_non_null",
            skip_serializing_if = "Option::is_none"
        )]
        only_allow_preferred_instances: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_optional_non_null",
            skip_serializing_if = "Option::is_none"
        )]
        hard_limits: Option<SlotLimits>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlotGuidance {
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub minimum_entries: Option<u64>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub maximum_entries: Option<u64>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub preferred_component_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlotLimits {
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub minimum_entries: Option<u64>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub maximum_entries: Option<u64>,
}

fn deserialize_optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = JsonValue::deserialize(deserializer)?;
    if value.is_null() {
        return Err(serde::de::Error::custom(
            "null is not a valid optional v1 field value",
        ));
    }
    serde_json::from_value(value)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DefinitionNode {
    Container {
        id: String,
        parent_id: NullableString,
        child_ids: Vec<String>,
    },
    Shape {
        id: String,
        parent_id: NullableString,
        child_ids: Vec<String>,
    },
    Text {
        id: String,
        parent_id: NullableString,
        child_ids: Vec<String>,
    },
    Instance {
        id: String,
        parent_id: NullableString,
        child_ids: Vec<String>,
        source: ComponentReference,
        variant_selection: OrderedJsonRecord<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentDefinition {
    pub id: String,
    pub name: String,
    pub set_id: NullableString,
    pub root_node_ids: Vec<String>,
    pub nodes: OrderedJsonRecord<DefinitionNode>,
    pub properties: OrderedJsonRecord<ComponentProperty>,
    pub variant_values: OrderedJsonRecord<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentSet {
    pub id: String,
    pub name: String,
    pub member_component_ids: Vec<String>,
    pub properties: OrderedJsonRecord<VariantProperty>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverrideAddress {
    pub target_node_id: String,
    pub field: OverrideField,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OverrideField {
    Name,
    Text,
    Fills,
    Strokes,
    Effects,
    LayoutGuides,
    ExportSettings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceOverride {
    pub address: OverrideAddress,
    pub value: JsonValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SlotContentNode {
    Container {
        id: String,
        parent_id: NullableString,
        child_ids: Vec<String>,
    },
    Shape {
        id: String,
        parent_id: NullableString,
        child_ids: Vec<String>,
    },
    Text {
        id: String,
        parent_id: NullableString,
        child_ids: Vec<String>,
    },
    Instance {
        id: String,
        parent_id: NullableString,
        child_ids: Vec<String>,
        source: ComponentReference,
        variant_selection: OrderedJsonRecord<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlotContent {
    pub root_node_ids: Vec<String>,
    pub nodes: OrderedJsonRecord<SlotContentNode>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentInstance {
    pub id: String,
    pub source: ComponentReference,
    pub variant_selection: OrderedJsonRecord<String>,
    pub property_values: OrderedJsonRecord<JsonValue>,
    pub overrides: Vec<InstanceOverride>,
    pub slot_content: OrderedJsonRecord<SlotContent>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentInstanceDocumentV1 {
    pub version: u32,
    pub components: OrderedJsonRecord<ComponentDefinition>,
    #[serde(rename = "componentSets")]
    pub component_sets: OrderedJsonRecord<ComponentSet>,
    pub instances: OrderedJsonRecord<ComponentInstance>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentDiagnostic {
    pub code: ComponentDiagnosticCode,
    pub path: String,
    pub severity: DiagnosticSeverity,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ComponentDiagnosticCode {
    ComponentDocumentMalformed,
    ComponentVersionUnsupported,
    ComponentUnknownField,
    ComponentJsonInvalid,
    ComponentRecordMalformed,
    ComponentIdMismatch,
    ComponentDuplicateId,
    ComponentPropertyKindUnsupported,
    ComponentPropertyInvalid,
    ComponentNodeGraphInvalid,
    ComponentNodeKindUnsupported,
    ComponentReferenceInvalid,
    ComponentReferenceMissing,
    ComponentReferenceUnresolved,
    ComponentReferenceCycle,
    ComponentSetMembershipInvalid,
    ComponentVariantInvalid,
    ComponentVariantDuplicate,
    ComponentVariantSelectionInvalid,
    ComponentPropertyValueInvalid,
    ComponentOverrideInvalid,
    ComponentOverrideTargetMissing,
    ComponentOverrideFieldInvalid,
    ComponentSlotInvalid,
    ComponentSlotGuidanceExceeded,
    ComponentSlotHardLimitExceeded,
    ComponentSlotContentNotAllowed,
    ComponentAmbiguousSource,
}

impl ComponentInstanceDocumentV1 {
    /// Decode known v1 fields without coercion. Semantic graph checks remain
    /// owned by the TypeScript reference validator for this milestone.
    pub fn from_v1_json(input: &str) -> Result<Self, serde_json::Error> {
        let document: Self = serde_json::from_str(input)?;
        if document.version != COMPONENT_INSTANCE_DOMAIN_VERSION {
            return Err(<serde_json::Error as serde::de::Error>::custom(
                "unsupported component-instance domain version",
            ));
        }
        Ok(document)
    }

    /// Emit deterministic JSON: object maps are key-sorted, authored arrays
    /// keep their order. This defines semantic JSON equivalence, not original
    /// input bytes or whitespace/key-order preservation.
    pub fn to_deterministic_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POSITIVE: &str =
        include_str!("../../../../test/fixtures/figma-semantics/component-instance-domain-v1.json");

    #[test]
    fn generated_fixture_round_trips_semantically_and_deterministically() {
        let original = ComponentInstanceDocumentV1::from_v1_json(POSITIVE).unwrap();
        let encoded = original.to_deterministic_json().unwrap();
        let decoded = ComponentInstanceDocumentV1::from_v1_json(&encoded).unwrap();
        assert_eq!(original, decoded);
        assert_eq!(encoded, decoded.to_deterministic_json().unwrap());

        assert_eq!(
            decoded.component_sets["icon-set"].member_component_ids,
            ["icon-small", "icon-large"]
        );
        assert_eq!(
            decoded.instances["card-instance"].slot_content["content-slot"].root_node_ids,
            ["authored-1", "authored-2"]
        );
        assert!(decoded.instances.contains_key("unresolved-instance"));
        assert!(matches!(
            decoded.instances["unresolved-instance"].source,
            ComponentReference::Unresolved { .. }
        ));
        assert!(matches!(
            &decoded.instances["external-instance"].source,
            ComponentReference::External {
                file_id,
                component_id,
                revision: NullableString::String(revision),
            } if file_id == "external-file" && component_id == "external-component" && revision == "r17"
        ));
        let external = &decoded.components["card"].nodes["card-icon"];
        assert!(matches!(
            external,
            DefinitionNode::Instance {
                source: ComponentReference::LocalSet { .. },
                ..
            }
        ));
        assert_eq!(
            decoded.instances["card-instance"].overrides[0]
                .address
                .target_node_id,
            "card-surface"
        );
        assert_eq!(
            decoded.instances["card-instance"].property_values["label-text"],
            JsonValue::String("A different label".into())
        );
    }

    #[test]
    fn schema_unknown_or_incompatible_fields_are_rejected_without_coercion() {
        let mut value: JsonValue = serde_json::from_str(POSITIVE).unwrap();
        value["futureField"] = JsonValue::Bool(true);
        assert!(ComponentInstanceDocumentV1::from_v1_json(&value.to_string()).is_err());

        let mut value: JsonValue = serde_json::from_str(POSITIVE).unwrap();
        value["components"]["card"]["properties"]["label-text"]["future"] = JsonValue::Null;
        assert!(ComponentInstanceDocumentV1::from_v1_json(&value.to_string()).is_err());

        let mut value: JsonValue = serde_json::from_str(POSITIVE).unwrap();
        value["components"]["card"]["nodes"]["card-icon"]["kind"] =
            JsonValue::String("future-node".into());
        assert!(ComponentInstanceDocumentV1::from_v1_json(&value.to_string()).is_err());

        let mut value: JsonValue = serde_json::from_str(POSITIVE).unwrap();
        assert!(value["components"]["card"]["nodes"]["card-icon"]
            .as_object_mut()
            .unwrap()
            .remove("id")
            .is_some());
        assert!(ComponentInstanceDocumentV1::from_v1_json(&value.to_string()).is_err());

        let mut value: JsonValue = serde_json::from_str(POSITIVE).unwrap();
        value["components"]["card"]["properties"]["content-slot"]["guidance"] = JsonValue::Null;
        assert!(ComponentInstanceDocumentV1::from_v1_json(&value.to_string()).is_err());
    }

    #[test]
    fn required_nullable_external_revision_preserves_null_and_rejects_missing_or_wrong_type() {
        let mut value: JsonValue = serde_json::from_str(POSITIVE).unwrap();
        value["instances"]["unresolved-instance"]["source"] = serde_json::json!({
            "kind":"external", "fileId":"file-1", "componentId":"component-1", "revision":null
        });
        let parsed = ComponentInstanceDocumentV1::from_v1_json(&value.to_string()).unwrap();
        assert!(matches!(
            parsed.instances["unresolved-instance"].source,
            ComponentReference::External {
                revision: NullableString::Null,
                ..
            }
        ));

        value["instances"]["unresolved-instance"]["source"]
            .as_object_mut()
            .unwrap()
            .remove("revision");
        assert!(ComponentInstanceDocumentV1::from_v1_json(&value.to_string()).is_err());
        value["instances"]["unresolved-instance"]["source"]["revision"] =
            JsonValue::Number(1.into());
        assert!(ComponentInstanceDocumentV1::from_v1_json(&value.to_string()).is_err());
    }
}
