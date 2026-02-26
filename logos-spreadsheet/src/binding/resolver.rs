//! PropertyResolver trait — the bridge between spreadsheet and design.
//!
//! This trait is the only interface the spreadsheet needs to read/write
//! design properties. The host application (e.g., `logos-desktop`)
//! implements it by querying `logos-core` `Document` / `Layer` structs.
//!
//! The spreadsheet never imports `logos-core` directly.

use super::types::{DesignRef, ElementKind, ElementRef, PropertyPath};
use crate::types::Value;

// ---------------------------------------------------------------------------
// PropertyResolver trait
// ---------------------------------------------------------------------------

/// Resolves design element references and reads/writes their properties.
///
/// Implement this trait in the host application to connect spreadsheet
/// formulas to the actual design data model.
///
/// # Example implementation sketch
///
/// ```rust,ignore
/// struct LogosResolver {
///     document: Arc<RwLock<Document>>,
/// }
///
/// impl PropertyResolver for LogosResolver {
///     fn resolve_element(&self, name: &str, kind: ElementKind) -> Option<DesignRef> {
///         let doc = self.document.read().unwrap();
///         let layer = doc.find_layer_by_name(name)?;
///         Some(DesignRef::new(ElementRef::named(name), kind))
///     }
///
///     fn get_property(&self, element: &ElementRef, path: &PropertyPath) -> Value {
///         let doc = self.document.read().unwrap();
///         let layer = doc.find_layer(element.key())?;
///         match path.root() {
///             "width" => Value::Number(layer.bounds.width as f64),
///             "x" => Value::Number(layer.bounds.x as f64),
///             _ => Value::Error(SpreadsheetError::Field),
///         }
///     }
/// }
/// ```
pub trait PropertyResolver {
    /// Resolve an element name/ID to a `DesignRef`.
    ///
    /// Called when the evaluator encounters `LAYER("name")`.
    /// Returns `None` if the element doesn't exist → produces `#REF!` error.
    fn resolve_element(&self, name: &str, kind: ElementKind) -> Option<DesignRef>;

    /// Read a property value from a design element.
    ///
    /// Called when the evaluator encounters `.width` on a `DesignRef`.
    /// Returns `Value::Error` if the property doesn't exist.
    fn get_property(&self, element: &ElementRef, path: &PropertyPath) -> Value;

    /// Write a property value to a design element.
    ///
    /// Called for write/bidirectional bindings when a cell value changes.
    /// Returns `true` if the write was accepted, `false` if rejected.
    fn set_property(
        &self,
        element: &ElementRef,
        path: &PropertyPath,
        value: &Value,
    ) -> bool {
        let _ = (element, path, value);
        false // default: read-only
    }

    /// List available properties for an element.
    ///
    /// Used for autocomplete in the formula bar.
    fn list_properties(&self, element: &ElementRef) -> Vec<PropertyInfo> {
        let _ = element;
        Vec::new()
    }

    /// List all named elements of a given kind.
    ///
    /// Used for autocomplete (`LAYER("` → show available layer names).
    fn list_elements(&self, kind: ElementKind) -> Vec<ElementInfo> {
        let _ = kind;
        Vec::new()
    }

    /// Check if an element exists.
    fn element_exists(&self, name: &str, kind: ElementKind) -> bool {
        self.resolve_element(name, kind).is_some()
    }
}

// ---------------------------------------------------------------------------
// PropertyInfo — metadata for autocomplete
// ---------------------------------------------------------------------------

/// Metadata about a design property, used for autocomplete and validation.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyInfo {
    /// Property name (e.g., `"width"`).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Expected value type.
    pub value_type: PropertyType,
    /// Whether this property is writable.
    pub writable: bool,
}

impl PropertyInfo {
    /// Create a new property info.
    pub fn new(name: impl Into<String>, value_type: PropertyType, writable: bool) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            value_type,
            writable,
        }
    }

    /// With a description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
}

/// The type of a property value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyType {
    Number,
    Text,
    Boolean,
    Color,
    Enum,
}

// ---------------------------------------------------------------------------
// ElementInfo — metadata for element autocomplete
// ---------------------------------------------------------------------------

/// Metadata about a design element, used for formula autocomplete.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementInfo {
    /// Element name.
    pub name: String,
    /// Element kind.
    pub kind: ElementKind,
    /// UUID string (if available).
    pub id: Option<String>,
}

impl ElementInfo {
    pub fn new(name: impl Into<String>, kind: ElementKind) -> Self {
        Self {
            name: name.into(),
            kind,
            id: None,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

// ---------------------------------------------------------------------------
// MockResolver — test implementation
// ---------------------------------------------------------------------------

/// A mock resolver for testing, with an in-memory property store.
///
/// Properties are stored as `(element_name, property) → Value`.
#[derive(Debug, Default)]
pub struct MockResolver {
    /// Registered elements: `name → kind`.
    elements: std::collections::HashMap<String, ElementKind>,
    /// Property values: `(element_name, property_path) → Value`.
    properties: std::collections::HashMap<(String, String), Value>,
}

impl MockResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an element.
    pub fn add_element(&mut self, name: impl Into<String>, kind: ElementKind) {
        self.elements.insert(name.into(), kind);
    }

    /// Set a property value.
    pub fn set(&mut self, element: impl Into<String>, property: impl Into<String>, value: Value) {
        self.properties
            .insert((element.into(), property.into()), value);
    }

    /// Get a property value directly (for test assertions).
    pub fn get(&self, element: &str, property: &str) -> Option<&Value> {
        self.properties.get(&(element.to_string(), property.to_string()))
    }
}

impl PropertyResolver for MockResolver {
    fn resolve_element(&self, name: &str, kind: ElementKind) -> Option<DesignRef> {
        let stored_kind = self.elements.get(name)?;
        // If kind is Any, accept any stored kind. Otherwise must match.
        if kind != ElementKind::Any && *stored_kind != kind {
            return None;
        }
        Some(DesignRef::new(
            ElementRef::named(name),
            *stored_kind,
        ))
    }

    fn get_property(&self, element: &ElementRef, path: &PropertyPath) -> Value {
        self.properties
            .get(&(element.key().to_string(), path.to_dotted()))
            .cloned()
            .unwrap_or(Value::Error(crate::errors::SpreadsheetError::Field))
    }

    fn set_property(
        &self,
        _element: &ElementRef,
        _path: &PropertyPath,
        _value: &Value,
    ) -> bool {
        // MockResolver is immutable via trait; use interior mutability in real tests
        false
    }

    fn list_properties(&self, element: &ElementRef) -> Vec<PropertyInfo> {
        let name = element.key();
        self.properties
            .keys()
            .filter(|(el, _)| el == name)
            .map(|(_, prop)| PropertyInfo::new(prop.clone(), PropertyType::Number, true))
            .collect()
    }

    fn list_elements(&self, kind: ElementKind) -> Vec<ElementInfo> {
        self.elements
            .iter()
            .filter(|(_, k)| kind == ElementKind::Any || **k == kind)
            .map(|(name, k)| ElementInfo::new(name.clone(), *k))
            .collect()
    }
}

/// A mock resolver with interior mutability for testing writes.
#[derive(Debug, Default)]
pub struct WritableMockResolver {
    inner: std::cell::RefCell<MockResolver>,
}

impl WritableMockResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an element.
    pub fn add_element(&self, name: impl Into<String>, kind: ElementKind) {
        self.inner.borrow_mut().add_element(name, kind);
    }

    /// Set a property value.
    pub fn set(&self, element: impl Into<String>, property: impl Into<String>, value: Value) {
        self.inner.borrow_mut().set(element, property, value);
    }

    /// Get a property value directly (for test assertions).
    pub fn get(&self, element: &str, property: &str) -> Option<Value> {
        self.inner.borrow().get(element, property).cloned()
    }
}

impl PropertyResolver for WritableMockResolver {
    fn resolve_element(&self, name: &str, kind: ElementKind) -> Option<DesignRef> {
        self.inner.borrow().resolve_element(name, kind)
    }

    fn get_property(&self, element: &ElementRef, path: &PropertyPath) -> Value {
        self.inner.borrow().get_property(element, path)
    }

    fn set_property(
        &self,
        element: &ElementRef,
        path: &PropertyPath,
        value: &Value,
    ) -> bool {
        let key = (element.key().to_string(), path.to_dotted());
        self.inner.borrow_mut().properties.insert(key, value.clone());
        true
    }

    fn list_properties(&self, element: &ElementRef) -> Vec<PropertyInfo> {
        self.inner.borrow().list_properties(element)
    }

    fn list_elements(&self, kind: ElementKind) -> Vec<ElementInfo> {
        self.inner.borrow().list_elements(kind)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Value;

    #[test]
    fn mock_resolver_element_round_trip() {
        let mut resolver = MockResolver::new();
        resolver.add_element("rect-1", ElementKind::Layer);

        let design_ref = resolver.resolve_element("rect-1", ElementKind::Layer);
        assert!(design_ref.is_some());
        assert_eq!(design_ref.unwrap().kind, ElementKind::Layer);
    }

    #[test]
    fn mock_resolver_unknown_element() {
        let resolver = MockResolver::new();
        assert!(resolver.resolve_element("nope", ElementKind::Layer).is_none());
    }

    #[test]
    fn mock_resolver_kind_mismatch() {
        let mut resolver = MockResolver::new();
        resolver.add_element("rect-1", ElementKind::Layer);

        // Wrong kind
        assert!(resolver.resolve_element("rect-1", ElementKind::Text).is_none());
        // Any kind matches
        assert!(resolver.resolve_element("rect-1", ElementKind::Any).is_some());
    }

    #[test]
    fn mock_resolver_get_property() {
        let mut resolver = MockResolver::new();
        resolver.add_element("rect-1", ElementKind::Layer);
        resolver.set("rect-1", "width", Value::Number(200.0));

        let el = ElementRef::named("rect-1");
        let path = PropertyPath::new("width");
        let val = resolver.get_property(&el, &path);
        assert_eq!(val, Value::Number(200.0));
    }

    #[test]
    fn mock_resolver_missing_property_returns_field_error() {
        let mut resolver = MockResolver::new();
        resolver.add_element("rect-1", ElementKind::Layer);

        let el = ElementRef::named("rect-1");
        let path = PropertyPath::new("nonexistent");
        let val = resolver.get_property(&el, &path);
        assert!(matches!(val, Value::Error(_)));
    }

    #[test]
    fn mock_resolver_list_properties() {
        let mut resolver = MockResolver::new();
        resolver.add_element("rect-1", ElementKind::Layer);
        resolver.set("rect-1", "width", Value::Number(200.0));
        resolver.set("rect-1", "height", Value::Number(100.0));

        let el = ElementRef::named("rect-1");
        let props = resolver.list_properties(&el);
        assert_eq!(props.len(), 2);
    }

    #[test]
    fn mock_resolver_list_elements() {
        let mut resolver = MockResolver::new();
        resolver.add_element("rect-1", ElementKind::Layer);
        resolver.add_element("heading", ElementKind::Text);

        let layers = resolver.list_elements(ElementKind::Layer);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].name, "rect-1");

        let all = resolver.list_elements(ElementKind::Any);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn writable_mock_resolver_set_property() {
        let resolver = WritableMockResolver::new();
        resolver.add_element("rect-1", ElementKind::Layer);
        resolver.set("rect-1", "width", Value::Number(200.0));

        let el = ElementRef::named("rect-1");
        let path = PropertyPath::new("width");

        // Read
        assert_eq!(resolver.get_property(&el, &path), Value::Number(200.0));

        // Write
        let accepted = resolver.set_property(&el, &path, &Value::Number(300.0));
        assert!(accepted);

        // Read again — should be updated
        assert_eq!(resolver.get_property(&el, &path), Value::Number(300.0));
    }

    #[test]
    fn writable_mock_resolver_creates_new_property() {
        let resolver = WritableMockResolver::new();
        resolver.add_element("rect-1", ElementKind::Layer);

        let el = ElementRef::named("rect-1");
        let path = PropertyPath::new("opacity");

        // Write a new property
        resolver.set_property(&el, &path, &Value::Number(0.5));
        assert_eq!(resolver.get_property(&el, &path), Value::Number(0.5));
    }

    #[test]
    fn element_exists_shorthand() {
        let mut resolver = MockResolver::new();
        resolver.add_element("rect-1", ElementKind::Layer);

        assert!(resolver.element_exists("rect-1", ElementKind::Layer));
        assert!(resolver.element_exists("rect-1", ElementKind::Any));
        assert!(!resolver.element_exists("nope", ElementKind::Layer));
    }

    #[test]
    fn property_info_builder() {
        let info = PropertyInfo::new("width", PropertyType::Number, true)
            .with_description("Element width in pixels");
        assert_eq!(info.name, "width");
        assert_eq!(info.description, "Element width in pixels");
        assert!(info.writable);
    }

    #[test]
    fn element_info_builder() {
        let info = ElementInfo::new("rect-1", ElementKind::Layer)
            .with_id("550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(info.name, "rect-1");
        assert_eq!(info.id.unwrap(), "550e8400-e29b-41d4-a716-446655440000");
    }
}
