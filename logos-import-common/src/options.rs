//! Import options shared across all importers.

/// Options that control import behavior.
#[derive(Clone, Debug)]
pub struct ImportOptions {
    /// Maximum number of nodes/elements to import (0 = no limit).
    pub max_elements: usize,
    /// Maximum recursion/nesting depth (0 = no limit).
    pub max_depth: usize,
    /// Maximum file size in bytes (0 = no limit).
    pub max_file_size: usize,
    /// Whether to import visual properties (fills, strokes, effects).
    pub import_styles: bool,
    /// Whether to import text content.
    pub import_text: bool,
    /// Whether to flatten the layer hierarchy.
    pub flatten: bool,
    /// Whether to generate deterministic UUIDs from source IDs.
    pub deterministic_ids: bool,
    /// Timeout in milliseconds (0 = no timeout).
    pub timeout_ms: u64,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            max_elements: 0,
            max_depth: 0,
            max_file_size: 0,
            import_styles: true,
            import_text: true,
            flatten: false,
            deterministic_ids: false,
            timeout_ms: 0,
        }
    }
}

impl ImportOptions {
    /// Quick preset: import everything with no limits.
    pub fn full() -> Self {
        Self::default()
    }

    /// Quick preset: fast import with limited depth and elements.
    pub fn fast() -> Self {
        Self {
            max_elements: 1000,
            max_depth: 10,
            import_styles: false,
            ..Self::default()
        }
    }

    /// Quick preset: preview mode — structure only, no styles.
    pub fn preview() -> Self {
        Self {
            import_styles: false,
            import_text: false,
            flatten: true,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options() {
        let opts = ImportOptions::default();
        assert_eq!(opts.max_elements, 0);
        assert!(opts.import_styles);
        assert!(opts.import_text);
        assert!(!opts.flatten);
    }

    #[test]
    fn test_fast_preset() {
        let opts = ImportOptions::fast();
        assert_eq!(opts.max_elements, 1000);
        assert_eq!(opts.max_depth, 10);
        assert!(!opts.import_styles);
    }

    #[test]
    fn test_preview_preset() {
        let opts = ImportOptions::preview();
        assert!(!opts.import_styles);
        assert!(!opts.import_text);
        assert!(opts.flatten);
    }
}
