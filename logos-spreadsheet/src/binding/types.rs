//! Core types for the design ↔ spreadsheet data binding system.
//!
//! These types define how spreadsheet cells reference design elements
//! and their properties, without any direct dependency on `logos-core`.
//! The host application bridges the gap via the [`PropertyResolver`](super::resolver::PropertyResolver) trait.
//!
//! # Terminology
//!
//! - **Element**: A design element (layer, frame, text, shape) identified by name or ID.
//! - **Property**: A named attribute of an element (e.g., `width`, `opacity`, `fill`).
//! - **DesignRef**: A resolved reference to a specific element, used as a spreadsheet `Value`.
//! - **Binding**: A live connection between a spreadsheet cell and a design property.

use std::fmt;

// ---------------------------------------------------------------------------
// ElementRef — identifies a design element
// ---------------------------------------------------------------------------

/// A reference to a design element, resolved from a formula like `LAYER("rect-1")`.
///
/// Elements can be referenced by human-readable name or by UUID string.
/// The host application resolves these to actual design objects.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ElementRef {
    /// Reference by human-readable name (e.g., `"rect-1"`, `"Header Text"``).
    ByName(String),
    /// Reference by UUID string (e.g., `"550e8400-e29b-41d4-a716-446655440000"`).
    ById(String),
}

impl ElementRef {
    /// Create a reference by name.
    pub fn named(name: impl Into<String>) -> Self {
        Self::ByName(name.into())
    }

    /// Create a reference by ID.
    pub fn id(id: impl Into<String>) -> Self {
        Self::ById(id.into())
    }

    /// Get the name or ID string.
    pub fn key(&self) -> &str {
        match self {
            Self::ByName(s) | Self::ById(s) => s,
        }
    }

    /// Whether this is a name reference.
    pub fn is_name(&self) -> bool {
        matches!(self, Self::ByName(_))
    }

    /// Whether this is an ID reference.
    pub fn is_id(&self) -> bool {
        matches!(self, Self::ById(_))
    }
}

impl fmt::Display for ElementRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ByName(name) => write!(f, "\"{}\"", name),
            Self::ById(id) => write!(f, "#{}", id),
        }
    }
}

// ---------------------------------------------------------------------------
// PropertyPath — identifies a property on an element
// ---------------------------------------------------------------------------

/// A path to a specific property on a design element.
///
/// Simple properties are single-segment: `width`, `opacity`.
/// Nested properties use dotted paths: `fill.color.r`, `stroke.width`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyPath {
    segments: Vec<String>,
}

impl PropertyPath {
    /// Create from a single property name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            segments: vec![name.into()],
        }
    }

    /// Create from a dotted path string (e.g., `"fill.color.r"`).
    pub fn from_dotted(path: &str) -> Self {
        Self {
            segments: path.split('.').map(String::from).collect(),
        }
    }

    /// Create from multiple segments.
    pub fn from_segments(segments: Vec<String>) -> Self {
        Self { segments }
    }

    /// Append a segment to the path.
    pub fn push(&mut self, segment: impl Into<String>) {
        self.segments.push(segment.into());
    }

    /// Get the first (root) segment.
    pub fn root(&self) -> &str {
        &self.segments[0]
    }

    /// Get all segments.
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Number of segments.
    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    /// Whether this is a simple (single-segment) property path.
    pub fn is_simple(&self) -> bool {
        self.segments.len() == 1
    }

    /// Convert to dotted string representation.
    pub fn to_dotted(&self) -> String {
        self.segments.join(".")
    }
}

impl fmt::Display for PropertyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_dotted())
    }
}

impl From<&str> for PropertyPath {
    fn from(s: &str) -> Self {
        if s.contains('.') {
            Self::from_dotted(s)
        } else {
            Self::new(s)
        }
    }
}

impl From<String> for PropertyPath {
    fn from(s: String) -> Self {
        if s.contains('.') {
            Self::from_dotted(&s)
        } else {
            Self::new(s)
        }
    }
}

// ---------------------------------------------------------------------------
// DesignRef — a resolved design object reference (carried in Value)
// ---------------------------------------------------------------------------

/// A resolved reference to a design element, used as a spreadsheet `Value`.
///
/// When a formula calls `LAYER("rect-1")`, the evaluator produces a
/// `Value::DesignRef(DesignRef { .. })`. Subsequent member access (`.width`)
/// resolves the property via the `PropertyResolver`.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignRef {
    /// The element reference (by name or ID).
    pub element: ElementRef,
    /// The element kind (for error messages and type checking).
    pub kind: ElementKind,
}

impl DesignRef {
    /// Create a new design reference.
    pub fn new(element: ElementRef, kind: ElementKind) -> Self {
        Self { element, kind }
    }

    /// Create a layer reference by name.
    pub fn layer(name: impl Into<String>) -> Self {
        Self::new(ElementRef::named(name), ElementKind::Layer)
    }

    /// Create a text element reference by name.
    pub fn text(name: impl Into<String>) -> Self {
        Self::new(ElementRef::named(name), ElementKind::Text)
    }

    /// Create a frame reference by name.
    pub fn frame(name: impl Into<String>) -> Self {
        Self::new(ElementRef::named(name), ElementKind::Frame)
    }
}

impl fmt::Display for DesignRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.kind, self.element)
    }
}

// ---------------------------------------------------------------------------
// ElementKind — type of design element
// ---------------------------------------------------------------------------

/// The kind of design element being referenced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementKind {
    /// Any layer type (rect, ellipse, path, etc.).
    Layer,
    /// Text layer specifically.
    Text,
    /// Frame (group/container).
    Frame,
    /// Page.
    Page,
    /// Style properties (fill, stroke, shadows).
    Style,
    /// Generic / auto-detect.
    Any,
}

impl fmt::Display for ElementKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Layer => write!(f, "LAYER"),
            Self::Text => write!(f, "TEXT"),
            Self::Frame => write!(f, "FRAME"),
            Self::Page => write!(f, "PAGE"),
            Self::Style => write!(f, "STYLE"),
            Self::Any => write!(f, "ELEMENT"),
        }
    }
}

// ---------------------------------------------------------------------------
// BindingDirection — which direction data flows
// ---------------------------------------------------------------------------

/// The direction of a data binding between a cell and a design property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingDirection {
    /// Design → Spreadsheet: cell reads the design property.
    /// `=LAYER("rect-1").width` — cell displays the width value.
    ReadOnly,

    /// Spreadsheet → Design: cell value is pushed to the design property.
    /// The cell's computed value is written back to the design element.
    WriteOnly,

    /// Bidirectional: cell reads from design AND writes back.
    /// Changes on either side propagate to the other.
    Bidirectional,
}

impl BindingDirection {
    /// Whether this binding reads from design.
    pub fn reads(&self) -> bool {
        matches!(self, Self::ReadOnly | Self::Bidirectional)
    }

    /// Whether this binding writes to design.
    pub fn writes(&self) -> bool {
        matches!(self, Self::WriteOnly | Self::Bidirectional)
    }
}

// ---------------------------------------------------------------------------
// DesignDep — a dependency on a design property
// ---------------------------------------------------------------------------

/// A dependency on a design element's property, analogous to `CellCoord`
/// for cell-to-cell dependencies.
///
/// Used by the dependency graph to track which cells depend on which
/// design properties, enabling recalculation when design changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DesignDep {
    /// The element being depended on.
    pub element: ElementRef,
    /// The specific property (if known). `None` means any property change.
    pub property: Option<PropertyPath>,
}

impl DesignDep {
    /// Depend on a specific property of a named element.
    pub fn property(name: impl Into<String>, prop: impl Into<PropertyPath>) -> Self {
        Self {
            element: ElementRef::named(name),
            property: Some(prop.into()),
        }
    }

    /// Depend on any change to a named element.
    pub fn any(name: impl Into<String>) -> Self {
        Self {
            element: ElementRef::named(name),
            property: None,
        }
    }

    /// Whether this dep matches a given element + property change.
    pub fn matches(&self, element: &ElementRef, property: &str) -> bool {
        if &self.element != element {
            return false;
        }
        match &self.property {
            None => true, // watches any property
            Some(p) => p.root() == property || p.to_dotted() == property,
        }
    }
}

impl fmt::Display for DesignDep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.element)?;
        if let Some(prop) = &self.property {
            write!(f, ".{}", prop)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Binding — a live cell-to-design connection
// ---------------------------------------------------------------------------

/// A live binding between a spreadsheet cell and a design property.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    /// The cell that participates in this binding.
    pub cell: (u32, u32),
    /// The design element.
    pub element: ElementRef,
    /// The property on the element.
    pub property: PropertyPath,
    /// The direction of data flow.
    pub direction: BindingDirection,
}

impl Binding {
    /// Create a new binding.
    pub fn new(
        cell: (u32, u32),
        element: ElementRef,
        property: PropertyPath,
        direction: BindingDirection,
    ) -> Self {
        Self {
            cell,
            element,
            property,
            direction,
        }
    }

    /// Create a read-only binding (design → cell).
    pub fn read(
        cell: (u32, u32),
        element: impl Into<String>,
        property: impl Into<PropertyPath>,
    ) -> Self {
        Self::new(
            cell,
            ElementRef::named(element),
            property.into(),
            BindingDirection::ReadOnly,
        )
    }

    /// Create a write binding (cell → design).
    pub fn write(
        cell: (u32, u32),
        element: impl Into<String>,
        property: impl Into<PropertyPath>,
    ) -> Self {
        Self::new(
            cell,
            ElementRef::named(element),
            property.into(),
            BindingDirection::WriteOnly,
        )
    }

    /// Create a bidirectional binding.
    pub fn bidirectional(
        cell: (u32, u32),
        element: impl Into<String>,
        property: impl Into<PropertyPath>,
    ) -> Self {
        Self::new(
            cell,
            ElementRef::named(element),
            property.into(),
            BindingDirection::Bidirectional,
        )
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let arrow = match self.direction {
            BindingDirection::ReadOnly => "←",
            BindingDirection::WriteOnly => "→",
            BindingDirection::Bidirectional => "↔",
        };
        write!(
            f,
            "({},{}) {} {}.{}",
            self.cell.0, self.cell.1, arrow, self.element, self.property
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- ElementRef --

    #[test]
    fn element_ref_by_name() {
        let r = ElementRef::named("rect-1");
        assert!(r.is_name());
        assert!(!r.is_id());
        assert_eq!(r.key(), "rect-1");
        assert_eq!(r.to_string(), "\"rect-1\"");
    }

    #[test]
    fn element_ref_by_id() {
        let r = ElementRef::id("550e8400-e29b-41d4-a716-446655440000");
        assert!(r.is_id());
        assert_eq!(r.key(), "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(r.to_string(), "#550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn element_ref_equality() {
        let a = ElementRef::named("rect-1");
        let b = ElementRef::named("rect-1");
        let c = ElementRef::named("rect-2");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // -- PropertyPath --

    #[test]
    fn property_path_simple() {
        let p = PropertyPath::new("width");
        assert!(p.is_simple());
        assert_eq!(p.depth(), 1);
        assert_eq!(p.root(), "width");
        assert_eq!(p.to_dotted(), "width");
    }

    #[test]
    fn property_path_dotted() {
        let p = PropertyPath::from_dotted("fill.color.r");
        assert!(!p.is_simple());
        assert_eq!(p.depth(), 3);
        assert_eq!(p.root(), "fill");
        assert_eq!(p.segments(), &["fill", "color", "r"]);
        assert_eq!(p.to_dotted(), "fill.color.r");
    }

    #[test]
    fn property_path_from_str() {
        let p: PropertyPath = "width".into();
        assert!(p.is_simple());

        let p2: PropertyPath = "stroke.width".into();
        assert_eq!(p2.depth(), 2);
    }

    #[test]
    fn property_path_push() {
        let mut p = PropertyPath::new("fill");
        p.push("color");
        p.push("r");
        assert_eq!(p.to_dotted(), "fill.color.r");
        assert_eq!(p.depth(), 3);
    }

    // -- DesignRef --

    #[test]
    fn design_ref_layer() {
        let r = DesignRef::layer("rect-1");
        assert_eq!(r.kind, ElementKind::Layer);
        assert_eq!(r.element, ElementRef::named("rect-1"));
        assert_eq!(r.to_string(), "LAYER(\"rect-1\")");
    }

    #[test]
    fn design_ref_text() {
        let r = DesignRef::text("heading");
        assert_eq!(r.kind, ElementKind::Text);
        assert_eq!(r.to_string(), "TEXT(\"heading\")");
    }

    #[test]
    fn design_ref_frame() {
        let r = DesignRef::frame("card");
        assert_eq!(r.kind, ElementKind::Frame);
        assert_eq!(r.to_string(), "FRAME(\"card\")");
    }

    // -- ElementKind --

    #[test]
    fn element_kind_display() {
        assert_eq!(ElementKind::Layer.to_string(), "LAYER");
        assert_eq!(ElementKind::Any.to_string(), "ELEMENT");
        assert_eq!(ElementKind::Style.to_string(), "STYLE");
    }

    // -- BindingDirection --

    #[test]
    fn binding_direction_reads_writes() {
        assert!(BindingDirection::ReadOnly.reads());
        assert!(!BindingDirection::ReadOnly.writes());
        assert!(!BindingDirection::WriteOnly.reads());
        assert!(BindingDirection::WriteOnly.writes());
        assert!(BindingDirection::Bidirectional.reads());
        assert!(BindingDirection::Bidirectional.writes());
    }

    // -- DesignDep --

    #[test]
    fn design_dep_property() {
        let d = DesignDep::property("rect-1", "width");
        assert_eq!(d.element, ElementRef::named("rect-1"));
        assert_eq!(d.property.as_ref().unwrap().root(), "width");
    }

    #[test]
    fn design_dep_any() {
        let d = DesignDep::any("rect-1");
        assert!(d.property.is_none());
    }

    #[test]
    fn design_dep_matches() {
        let d = DesignDep::property("rect-1", "width");
        let el = ElementRef::named("rect-1");

        assert!(d.matches(&el, "width"));
        assert!(!d.matches(&el, "height"));
        assert!(!d.matches(&ElementRef::named("rect-2"), "width"));
    }

    #[test]
    fn design_dep_any_matches_all_props() {
        let d = DesignDep::any("rect-1");
        let el = ElementRef::named("rect-1");

        assert!(d.matches(&el, "width"));
        assert!(d.matches(&el, "height"));
        assert!(d.matches(&el, "opacity"));
        assert!(!d.matches(&ElementRef::named("other"), "width"));
    }

    // -- Binding --

    #[test]
    fn binding_read() {
        let b = Binding::read((0, 0), "rect-1", "width");
        assert_eq!(b.cell, (0, 0));
        assert_eq!(b.element, ElementRef::named("rect-1"));
        assert_eq!(b.property.root(), "width");
        assert_eq!(b.direction, BindingDirection::ReadOnly);
        assert!(b.to_string().contains("←"));
    }

    #[test]
    fn binding_write() {
        let b = Binding::write((1, 2), "rect-1", "width");
        assert_eq!(b.direction, BindingDirection::WriteOnly);
        assert!(b.to_string().contains("→"));
    }

    #[test]
    fn binding_bidirectional() {
        let b = Binding::bidirectional((0, 0), "rect-1", "opacity");
        assert_eq!(b.direction, BindingDirection::Bidirectional);
        assert!(b.to_string().contains("↔"));
    }

    #[test]
    fn binding_display() {
        let b = Binding::read((2, 3), "heading", "font_size");
        assert_eq!(b.to_string(), "(2,3) ← \"heading\".font_size");
    }
}
