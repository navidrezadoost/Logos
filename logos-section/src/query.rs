//! Section queries — search, filter, and inspect section trees.
//!
//! Provides composable query functions to find sections and layers
//! within a section hierarchy. All functions are read-only
//! (take `&SectionData`).

use logos_core::container::{SectionColor, SectionData};
use logos_core::Layer;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// By‐name search
// ═══════════════════════════════════════════════════════════════════

/// Find the first section whose name matches exactly.
pub fn find_by_name<'a>(section: &'a SectionData, name: &str) -> Option<&'a SectionData> {
    if section.name == name {
        return Some(section);
    }
    for child in &section.children {
        if let Layer::Section(s) = child {
            if let Some(found) = find_by_name(s, name) {
                return Some(found);
            }
        }
    }
    None
}

/// Find all sections whose name contains `pattern` (case-insensitive).
pub fn search_by_name<'a>(section: &'a SectionData, pattern: &str) -> Vec<&'a SectionData> {
    let pattern_lower = pattern.to_lowercase();
    let mut results = Vec::new();
    if section.name.to_lowercase().contains(&pattern_lower) {
        results.push(section);
    }
    search_name_recursive(&section.children, &pattern_lower, &mut results);
    results
}

fn search_name_recursive<'a>(
    children: &'a [Layer],
    pattern: &str,
    results: &mut Vec<&'a SectionData>,
) {
    for child in children {
        if let Layer::Section(s) = child {
            if s.name.to_lowercase().contains(pattern) {
                results.push(s);
            }
            search_name_recursive(&s.children, pattern, results);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// By‐color filter
// ═══════════════════════════════════════════════════════════════════

/// Collect all sections with a specific color label.
pub fn filter_by_color<'a>(section: &'a SectionData, color: SectionColor) -> Vec<&'a SectionData> {
    let mut results = Vec::new();
    if section.color == color {
        results.push(section);
    }
    color_recursive(&section.children, color, &mut results);
    results
}

fn color_recursive<'a>(
    children: &'a [Layer],
    color: SectionColor,
    results: &mut Vec<&'a SectionData>,
) {
    for child in children {
        if let Layer::Section(s) = child {
            if s.color == color {
                results.push(s);
            }
            color_recursive(&s.children, color, results);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// State filters
// ═══════════════════════════════════════════════════════════════════

/// Collect all collapsed sections.
pub fn collapsed_sections(section: &SectionData) -> Vec<&SectionData> {
    let mut results = Vec::new();
    if section.is_collapsed {
        results.push(section);
    }
    state_recursive(&section.children, |s| s.is_collapsed, &mut results);
    results
}

/// Collect all locked sections.
pub fn locked_sections(section: &SectionData) -> Vec<&SectionData> {
    let mut results = Vec::new();
    if section.is_locked {
        results.push(section);
    }
    state_recursive(&section.children, |s| s.is_locked, &mut results);
    results
}

/// Collect all hidden (not visible) sections.
pub fn hidden_sections(section: &SectionData) -> Vec<&SectionData> {
    let mut results = Vec::new();
    if !section.is_visible {
        results.push(section);
    }
    state_recursive(&section.children, |s| !s.is_visible, &mut results);
    results
}

fn state_recursive<'a>(
    children: &'a [Layer],
    predicate: fn(&SectionData) -> bool,
    results: &mut Vec<&'a SectionData>,
) {
    for child in children {
        if let Layer::Section(s) = child {
            if predicate(s) {
                results.push(s);
            }
            state_recursive(&s.children, predicate, results);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Layer containment
// ═══════════════════════════════════════════════════════════════════

/// Find the section that directly contains a layer with `layer_id`.
/// Returns None if the layer is not found.
pub fn parent_section_of<'a>(section: &'a SectionData, layer_id: Uuid) -> Option<&'a SectionData> {
    // Check direct children
    if section.children.iter().any(|c| c.id() == layer_id) {
        return Some(section);
    }
    // Recurse into sub-sections
    for child in &section.children {
        if let Layer::Section(s) = child {
            if let Some(found) = parent_section_of(s, layer_id) {
                return Some(found);
            }
        }
    }
    None
}

/// Collect the full path of sections from root to the section
/// containing `layer_id`.
pub fn section_path(section: &SectionData, layer_id: Uuid) -> Vec<Uuid> {
    let mut path = Vec::new();
    if section_path_recursive(section, layer_id, &mut path) {
        path.insert(0, section.id);
    }
    path
}

fn section_path_recursive(section: &SectionData, target: Uuid, path: &mut Vec<Uuid>) -> bool {
    for child in &section.children {
        if child.id() == target {
            return true;
        }
        if let Layer::Section(s) = child {
            if section_path_recursive(s, target, path) {
                path.insert(0, s.id);
                return true;
            }
        }
    }
    false
}

// ═══════════════════════════════════════════════════════════════════
// Section listing
// ═══════════════════════════════════════════════════════════════════

/// List all empty sections (no children).
pub fn empty_sections(section: &SectionData) -> Vec<&SectionData> {
    let mut results = Vec::new();
    if section.children.is_empty() {
        results.push(section);
    }
    empty_recursive(&section.children, &mut results);
    results
}

fn empty_recursive<'a>(children: &'a [Layer], results: &mut Vec<&'a SectionData>) {
    for child in children {
        if let Layer::Section(s) = child {
            if s.children.is_empty() {
                results.push(s);
            }
            empty_recursive(&s.children, results);
        }
    }
}

/// List all sections with descriptions.
pub fn sections_with_descriptions(section: &SectionData) -> Vec<&SectionData> {
    let mut results = Vec::new();
    if !section.description.is_empty() {
        results.push(section);
    }
    desc_recursive(&section.children, &mut results);
    results
}

fn desc_recursive<'a>(children: &'a [Layer], results: &mut Vec<&'a SectionData>) {
    for child in children {
        if let Layer::Section(s) = child {
            if !s.description.is_empty() {
                results.push(s);
            }
            desc_recursive(&s.children, results);
        }
    }
}

/// Count all non-section layers within a section (recursively).
pub fn count_leaves(section: &SectionData) -> usize {
    let mut count = 0;
    for child in &section.children {
        match child {
            Layer::Section(s) => count += count_leaves(s),
            _ => count += 1,
        }
    }
    count
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::RectLayer;

    fn make_section(name: &str) -> SectionData {
        SectionData::new(name)
    }

    fn make_rect(x: f32, y: f32, w: f32, h: f32) -> Layer {
        Layer::Rect(RectLayer::new(x, y, w, h))
    }

    // ─── find_by_name ───────────────────────────────────────────

    #[test]
    fn test_find_by_name_root() {
        let s = make_section("Root");
        assert!(find_by_name(&s, "Root").is_some());
    }

    #[test]
    fn test_find_by_name_nested() {
        let mut root = make_section("Root");
        let child = make_section("Settings");
        root.add_child(Layer::Section(child));

        let found = find_by_name(&root, "Settings");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Settings");
    }

    #[test]
    fn test_find_by_name_not_found() {
        let s = make_section("Root");
        assert!(find_by_name(&s, "Missing").is_none());
    }

    // ─── search_by_name ─────────────────────────────────────────

    #[test]
    fn test_search_case_insensitive() {
        let mut root = make_section("Design System");
        root.add_child(Layer::Section(make_section("Button Design")));
        root.add_child(Layer::Section(make_section("Card Design")));
        root.add_child(Layer::Section(make_section("Icons")));

        let results = search_by_name(&root, "DESIGN");
        assert_eq!(results.len(), 3); // root + 2 children
    }

    #[test]
    fn test_search_partial_match() {
        let mut root = make_section("Root");
        root.add_child(Layer::Section(make_section("Homepage Header")));
        root.add_child(Layer::Section(make_section("Footer")));

        let results = search_by_name(&root, "head");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Homepage Header");
    }

    #[test]
    fn test_search_no_results() {
        let root = make_section("Root");
        let results = search_by_name(&root, "xyz");
        assert!(results.is_empty());
    }

    // ─── filter_by_color ────────────────────────────────────────

    #[test]
    fn test_filter_by_color() {
        let mut root = make_section("Root");
        let mut red = make_section("Important");
        red.color = SectionColor::Red;
        let mut blue = make_section("In Progress");
        blue.color = SectionColor::Blue;
        root.add_child(Layer::Section(red));
        root.add_child(Layer::Section(blue));

        let reds = filter_by_color(&root, SectionColor::Red);
        assert_eq!(reds.len(), 1);
        assert_eq!(reds[0].name, "Important");

        let blues = filter_by_color(&root, SectionColor::Blue);
        assert_eq!(blues.len(), 1);
    }

    #[test]
    fn test_filter_by_color_none() {
        let root = make_section("Root"); // default color is None
        let results = filter_by_color(&root, SectionColor::None);
        assert_eq!(results.len(), 1);
    }

    // ─── state filters ──────────────────────────────────────────

    #[test]
    fn test_collapsed_sections() {
        let mut root = make_section("Root");
        let mut collapsed = make_section("Collapsed");
        collapsed.is_collapsed = true;
        root.add_child(Layer::Section(collapsed));
        root.add_child(Layer::Section(make_section("Open")));

        let results = collapsed_sections(&root);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Collapsed");
    }

    #[test]
    fn test_locked_sections() {
        let mut root = make_section("Root");
        let mut locked = make_section("Locked");
        locked.is_locked = true;
        root.add_child(Layer::Section(locked));

        let results = locked_sections(&root);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Locked");
    }

    #[test]
    fn test_hidden_sections() {
        let mut root = make_section("Root");
        let mut hidden = make_section("Hidden");
        hidden.is_visible = false;
        root.add_child(Layer::Section(hidden));

        let results = hidden_sections(&root);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Hidden");
    }

    // ─── parent_section_of ──────────────────────────────────────

    #[test]
    fn test_parent_of_direct_child() {
        let mut root = make_section("Root");
        let rect = make_rect(0.0, 0.0, 10.0, 10.0);
        let rect_id = rect.id();
        root.add_child(rect);

        let parent = parent_section_of(&root, rect_id);
        assert!(parent.is_some());
        assert_eq!(parent.unwrap().name, "Root");
    }

    #[test]
    fn test_parent_of_nested_child() {
        let mut root = make_section("Root");
        let mut child = make_section("Child");
        let rect = make_rect(10.0, 10.0, 20.0, 20.0);
        let rect_id = rect.id();
        child.add_child(rect);
        root.add_child(Layer::Section(child));

        let parent = parent_section_of(&root, rect_id);
        assert!(parent.is_some());
        assert_eq!(parent.unwrap().name, "Child");
    }

    #[test]
    fn test_parent_not_found() {
        let root = make_section("Root");
        assert!(parent_section_of(&root, Uuid::new_v4()).is_none());
    }

    // ─── section_path ───────────────────────────────────────────

    #[test]
    fn test_section_path_direct() {
        let mut root = make_section("Root");
        let rect = make_rect(0.0, 0.0, 10.0, 10.0);
        let rect_id = rect.id();
        root.add_child(rect);

        let path = section_path(&root, rect_id);
        assert_eq!(path, vec![root.id]);
    }

    #[test]
    fn test_section_path_nested() {
        let mut root = make_section("Root");
        let mut child = make_section("Child");
        let child_id = child.id;
        let rect = make_rect(0.0, 0.0, 10.0, 10.0);
        let rect_id = rect.id();
        child.add_child(rect);
        root.add_child(Layer::Section(child));

        let path = section_path(&root, rect_id);
        assert_eq!(path, vec![root.id, child_id]);
    }

    #[test]
    fn test_section_path_not_found() {
        let root = make_section("Root");
        let path = section_path(&root, Uuid::new_v4());
        assert!(path.is_empty());
    }

    // ─── empty_sections ─────────────────────────────────────────

    #[test]
    fn test_empty_sections_root_only() {
        let root = make_section("Empty");
        let results = empty_sections(&root);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_empty_sections_mixed() {
        let mut root = make_section("Root");
        root.add_child(make_rect(0.0, 0.0, 10.0, 10.0));
        root.add_child(Layer::Section(make_section("Empty Child")));

        let results = empty_sections(&root);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Empty Child");
    }

    // ─── sections_with_descriptions ─────────────────────────────

    #[test]
    fn test_sections_with_descriptions() {
        let mut root = make_section("Root");
        root.description = "Project root".to_string();
        root.add_child(Layer::Section(make_section("No Desc")));

        let results = sections_with_descriptions(&root);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Root");
    }

    // ─── count_leaves ───────────────────────────────────────────

    #[test]
    fn test_count_leaves_empty() {
        let root = make_section("Root");
        assert_eq!(count_leaves(&root), 0);
    }

    #[test]
    fn test_count_leaves_flat() {
        let mut root = make_section("Root");
        root.add_child(make_rect(0.0, 0.0, 10.0, 10.0));
        root.add_child(make_rect(20.0, 0.0, 10.0, 10.0));
        assert_eq!(count_leaves(&root), 2);
    }

    #[test]
    fn test_count_leaves_nested() {
        let mut root = make_section("Root");
        root.add_child(make_rect(0.0, 0.0, 10.0, 10.0));
        let mut child = make_section("Child");
        child.add_child(make_rect(20.0, 0.0, 10.0, 10.0));
        child.add_child(make_rect(40.0, 0.0, 10.0, 10.0));
        root.add_child(Layer::Section(child));

        assert_eq!(count_leaves(&root), 3); // 1 in root + 2 in child
    }
}
