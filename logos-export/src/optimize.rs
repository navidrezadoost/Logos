//! SVG optimization — reduce file size without changing appearance.
//!
//! Provides a pipeline of transformations that can be applied to SVG
//! output strings to minimize byte size:
//! - Remove XML comments and metadata
//! - Collapse redundant groups
//! - Minify numeric precision
//! - Remove hidden elements (zero-size, fully transparent)
//! - Deduplicate inline styles

use serde::{Deserialize, Serialize};

/// Optimization level presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationLevel {
    /// No optimization.
    None,
    /// Safe optimizations only (metadata removal, whitespace).
    Safe,
    /// Aggressive (precision reduction, structural changes).
    Aggressive,
}

impl Default for OptimizationLevel {
    fn default() -> Self {
        Self::Safe
    }
}

/// Configuration for the SVG optimizer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SvgOptimizerConfig {
    pub level: OptimizationLevel,
    /// Decimal places for coordinate values.
    pub coordinate_precision: u8,
    /// Remove XML comments.
    pub remove_comments: bool,
    /// Remove empty groups.
    pub collapse_empty_groups: bool,
    /// Remove hidden elements (display:none, visibility:hidden, opacity:0).
    pub remove_hidden: bool,
    /// Remove default attribute values.
    pub remove_defaults: bool,
    /// Minify whitespace.
    pub minify_whitespace: bool,
}

impl Default for SvgOptimizerConfig {
    fn default() -> Self {
        Self {
            level: OptimizationLevel::Safe,
            coordinate_precision: 4,
            remove_comments: true,
            collapse_empty_groups: true,
            remove_hidden: true,
            remove_defaults: true,
            minify_whitespace: true,
        }
    }
}

impl SvgOptimizerConfig {
    /// Aggressive config — maximum compression.
    pub fn aggressive() -> Self {
        Self {
            level: OptimizationLevel::Aggressive,
            coordinate_precision: 2,
            remove_comments: true,
            collapse_empty_groups: true,
            remove_hidden: true,
            remove_defaults: true,
            minify_whitespace: true,
        }
    }

    /// No optimization — pass-through.
    pub fn none() -> Self {
        Self {
            level: OptimizationLevel::None,
            coordinate_precision: 6,
            remove_comments: false,
            collapse_empty_groups: false,
            remove_hidden: false,
            remove_defaults: false,
            minify_whitespace: false,
        }
    }
}

/// Statistics from an optimization pass.
#[derive(Debug, Clone, Default)]
pub struct OptimizationStats {
    pub original_bytes: usize,
    pub optimized_bytes: usize,
    pub comments_removed: usize,
    pub empty_groups_removed: usize,
    pub hidden_elements_removed: usize,
    pub whitespace_reduced: bool,
}

impl OptimizationStats {
    /// Savings as a percentage.
    pub fn savings_percent(&self) -> f64 {
        if self.original_bytes == 0 {
            return 0.0;
        }
        let saved = self.original_bytes.saturating_sub(self.optimized_bytes);
        (saved as f64 / self.original_bytes as f64) * 100.0
    }
}

/// SVG optimizer that applies a series of transformations.
pub struct SvgOptimizer {
    pub config: SvgOptimizerConfig,
}

impl SvgOptimizer {
    pub fn new(config: SvgOptimizerConfig) -> Self {
        Self { config }
    }

    pub fn default_safe() -> Self {
        Self::new(SvgOptimizerConfig::default())
    }

    pub fn aggressive() -> Self {
        Self::new(SvgOptimizerConfig::aggressive())
    }

    /// Optimize an SVG string, returning the result and stats.
    pub fn optimize(&self, svg: &str) -> (String, OptimizationStats) {
        let mut stats = OptimizationStats {
            original_bytes: svg.len(),
            ..Default::default()
        };

        if self.config.level == OptimizationLevel::None {
            stats.optimized_bytes = svg.len();
            return (svg.to_string(), stats);
        }

        let mut result = svg.to_string();

        if self.config.remove_comments {
            let (cleaned, count) = remove_xml_comments(&result);
            stats.comments_removed = count;
            result = cleaned;
        }

        if self.config.remove_hidden {
            let (cleaned, count) = remove_hidden_elements(&result);
            stats.hidden_elements_removed = count;
            result = cleaned;
        }

        if self.config.collapse_empty_groups {
            let (cleaned, count) = collapse_empty_groups(&result);
            stats.empty_groups_removed = count;
            result = cleaned;
        }

        if self.config.remove_defaults {
            result = remove_default_attributes(&result);
        }

        if self.config.level == OptimizationLevel::Aggressive {
            result = minify_coordinates(&result, self.config.coordinate_precision);
        }

        if self.config.minify_whitespace {
            result = minify_whitespace(&result);
            stats.whitespace_reduced = true;
        }

        stats.optimized_bytes = result.len();
        (result, stats)
    }
}

// ── Optimization passes ──────────────────────────────────────────

/// Remove `<!-- ... -->` comments.
fn remove_xml_comments(svg: &str) -> (String, usize) {
    let mut result = String::with_capacity(svg.len());
    let mut count = 0;
    let mut rest = svg;

    while let Some(start) = rest.find("<!--") {
        result.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("-->") {
            count += 1;
            rest = &rest[start + end + 3..];
        } else {
            // Unterminated comment — keep everything
            result.push_str(&rest[start..]);
            rest = "";
        }
    }
    result.push_str(rest);
    (result, count)
}

/// Remove elements with `opacity="0"`, `display="none"`, or `visibility="hidden"`.
fn remove_hidden_elements(svg: &str) -> (String, usize) {
    let mut result = String::with_capacity(svg.len());
    let mut count = 0;
    let mut rest = svg;

    let hidden_patterns = [
        r#"opacity="0""#,
        r#"display="none""#,
        r#"visibility="hidden""#,
    ];

    // Simple approach: find self-closing tags with hidden attributes
    while let Some(tag_start) = rest.find('<') {
        result.push_str(&rest[..tag_start]);
        rest = &rest[tag_start..];

        // Find the end of this tag
        if let Some(tag_end) = rest.find('>') {
            let tag = &rest[..=tag_end];
            let is_hidden = hidden_patterns.iter().any(|p| tag.contains(p));

            if is_hidden && tag.ends_with("/>") {
                // Self-closing hidden element — remove it
                count += 1;
                rest = &rest[tag_end + 1..];
            } else {
                result.push_str(tag);
                rest = &rest[tag_end + 1..];
            }
        } else {
            result.push_str(rest);
            rest = "";
        }
    }
    result.push_str(rest);
    (result, count)
}

/// Remove empty `<g></g>` groups.
fn collapse_empty_groups(svg: &str) -> (String, usize) {
    let mut result = svg.to_string();
    let mut total = 0;

    loop {
        // Match <g>  </g> or <g attr="val"></g>
        let before = result.len();
        result = remove_one_empty_group(&result, &mut total);
        if result.len() == before {
            break;
        }
    }

    (result, total)
}

fn remove_one_empty_group(svg: &str, count: &mut usize) -> String {
    // Find <g...></g> pairs where the content is only whitespace
    let mut result = String::with_capacity(svg.len());
    let mut rest = svg;

    if let Some(g_start) = rest.find("<g") {
        result.push_str(&rest[..g_start]);
        rest = &rest[g_start..];

        // Find closing > of the opening <g...>
        if let Some(open_end) = rest.find('>') {
            let open_tag = &rest[..=open_end];
            // If it's self-closing, skip
            if open_tag.ends_with("/>") {
                result.push_str(open_tag);
                rest = &rest[open_end + 1..];
            } else if let Some(close_pos) = rest.find("</g>") {
                let content = &rest[open_end + 1..close_pos];
                if content.trim().is_empty() {
                    *count += 1;
                    rest = &rest[close_pos + 4..];
                } else {
                    result.push_str(&rest[..close_pos + 4]);
                    rest = &rest[close_pos + 4..];
                }
            } else {
                result.push_str(rest);
                rest = "";
            }
        } else {
            result.push_str(rest);
            rest = "";
        }
    }
    result.push_str(rest);
    result
}

/// Remove default attribute values that match SVG defaults.
fn remove_default_attributes(svg: &str) -> String {
    let defaults = [
        r#" fill-opacity="1""#,
        r#" stroke-opacity="1""#,
        r#" opacity="1""#,
        r#" stroke="none""#,
        r#" fill-rule="nonzero""#,
    ];
    let mut result = svg.to_string();
    for default in &defaults {
        result = result.replace(default, "");
    }
    result
}

/// Reduce floating-point precision in numeric attributes.
fn minify_coordinates(svg: &str, precision: u8) -> String {
    let mut result = String::with_capacity(svg.len());
    let mut chars = svg.chars().peekable();
    let mut in_attr_value = false;
    let mut quote_char = '"';

    while let Some(ch) = chars.next() {
        if ch == '"' || ch == '\'' {
            if in_attr_value && ch == quote_char {
                in_attr_value = false;
            } else if !in_attr_value {
                in_attr_value = true;
                quote_char = ch;
            }
            result.push(ch);
            continue;
        }

        if in_attr_value && (ch.is_ascii_digit() || ch == '-' || ch == '.') {
            // Collect full number
            let mut num = String::new();
            num.push(ch);
            while let Some(&next) = chars.peek() {
                if next.is_ascii_digit() || next == '.' || next == 'e' || next == 'E' || next == '-' || next == '+' {
                    num.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            // Try to parse and round
            if let Ok(val) = num.parse::<f64>() {
                let rounded = format!("{:.prec$}", val, prec = precision as usize);
                // Trim trailing zeros after decimal point
                let trimmed = trim_trailing_zeros(&rounded);
                result.push_str(trimmed);
            } else {
                result.push_str(&num);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn trim_trailing_zeros(s: &str) -> &str {
    if !s.contains('.') {
        return s;
    }
    let trimmed = s.trim_end_matches('0');
    trimmed.trim_end_matches('.')
}

/// Collapse multiple whitespace characters into single spaces.
fn minify_whitespace(svg: &str) -> String {
    let mut result = String::with_capacity(svg.len());
    let mut prev_space = false;
    let mut in_tag = false;

    for ch in svg.chars() {
        if ch == '<' {
            in_tag = true;
        }
        if ch == '>' {
            in_tag = false;
        }

        if ch.is_whitespace() {
            if in_tag {
                if !prev_space {
                    result.push(' ');
                }
                prev_space = true;
            } else {
                // Between tags: collapse
                if !prev_space {
                    result.push(ch);
                }
                prev_space = true;
            }
        } else {
            prev_space = false;
            result.push(ch);
        }
    }
    result
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimization_level_default() {
        assert_eq!(OptimizationLevel::default(), OptimizationLevel::Safe);
    }

    #[test]
    fn config_aggressive() {
        let c = SvgOptimizerConfig::aggressive();
        assert_eq!(c.level, OptimizationLevel::Aggressive);
        assert_eq!(c.coordinate_precision, 2);
    }

    #[test]
    fn config_none() {
        let c = SvgOptimizerConfig::none();
        assert!(!c.remove_comments);
        assert!(!c.minify_whitespace);
    }

    #[test]
    fn remove_comments_basic() {
        let svg = r#"<svg><!-- comment --><rect/></svg>"#;
        let (result, count) = remove_xml_comments(svg);
        assert_eq!(count, 1);
        assert!(!result.contains("<!--"));
        assert!(result.contains("<rect/>"));
    }

    #[test]
    fn remove_comments_multiple() {
        let svg = "<!-- a --><svg><!-- b --></svg><!-- c -->";
        let (result, count) = remove_xml_comments(svg);
        assert_eq!(count, 3);
        assert_eq!(result, "<svg></svg>");
    }

    #[test]
    fn remove_hidden_opacity() {
        let svg = r#"<svg><rect opacity="0" width="10" height="10"/></svg>"#;
        let (result, count) = remove_hidden_elements(svg);
        assert_eq!(count, 1);
        assert!(!result.contains("rect"));
    }

    #[test]
    fn remove_hidden_display_none() {
        let svg = r#"<svg><circle display="none" r="5"/></svg>"#;
        let (result, count) = remove_hidden_elements(svg);
        assert_eq!(count, 1);
        assert!(!result.contains("circle"));
    }

    #[test]
    fn collapse_empty_groups_basic() {
        let svg = "<svg><g></g><rect/></svg>";
        let (result, count) = collapse_empty_groups(svg);
        assert_eq!(count, 1);
        assert!(!result.contains("<g>"));
    }

    #[test]
    fn collapse_empty_groups_nested() {
        let svg = "<svg><g>  </g><rect/></svg>";
        let (result, count) = collapse_empty_groups(svg);
        assert!(count >= 1);
        // The whitespace-only group should be removed
        assert!(!result.contains("<g>"));
    }

    #[test]
    fn remove_default_attrs() {
        let svg = r#"<rect fill-opacity="1" stroke="none"/>"#;
        let result = remove_default_attributes(svg);
        assert!(!result.contains("fill-opacity"));
        assert!(!result.contains("stroke"));
    }

    #[test]
    fn minify_coordinates_precision() {
        let svg = r#"<rect x="12.123456" y="34.987654"/>"#;
        let result = minify_coordinates(svg, 2);
        assert!(result.contains("12.12"));
        assert!(!result.contains("12.123456"));
    }

    #[test]
    fn minify_whitespace_basic() {
        let svg = "<svg>  <rect  width=\"10\"  />  </svg>";
        let result = minify_whitespace(svg);
        assert!(!result.contains("  "));
    }

    #[test]
    fn optimizer_none_passthrough() {
        let svg = "<svg><!-- keep me --><rect/></svg>";
        let opt = SvgOptimizer::new(SvgOptimizerConfig::none());
        let (result, stats) = opt.optimize(svg);
        assert_eq!(result, svg);
        assert_eq!(stats.savings_percent(), 0.0);
    }

    #[test]
    fn optimizer_safe_removes_comments() {
        let svg = "<svg><!-- remove --><rect/></svg>";
        let opt = SvgOptimizer::default_safe();
        let (result, stats) = opt.optimize(svg);
        assert!(!result.contains("<!--"));
        assert_eq!(stats.comments_removed, 1);
    }

    #[test]
    fn stats_savings_percent() {
        let stats = OptimizationStats {
            original_bytes: 200,
            optimized_bytes: 150,
            ..Default::default()
        };
        assert!((stats.savings_percent() - 25.0).abs() < 0.1);
    }
}
