// logos-collab/src/exporter.rs
//
//! # Code Exporter — CSS / Tailwind / Sass
//!
//! Generates ready-to-paste code from a `LayerInspection` in three formats:
//! plain CSS, Tailwind utility classes, and Sass (SCSS).

use crate::handoff::{AutoLayout, Color, LayerInspection, LayoutDirection, Shadow, Typography};

// ── Export format ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Css,
    Tailwind,
    Sass,
}

// ── Public API ────────────────────────────────────────────────────────────────

pub struct CodeExporter;

impl CodeExporter {
    /// Export `inspection` in the requested `format`.
    pub fn export(inspection: &LayerInspection, format: ExportFormat) -> String {
        match format {
            ExportFormat::Css      => Self::to_css(inspection),
            ExportFormat::Tailwind => Self::to_tailwind(inspection),
            ExportFormat::Sass     => Self::to_sass(inspection),
        }
    }

    // ── CSS ───────────────────────────────────────────────────────────────────

    fn to_css(l: &LayerInspection) -> String {
        let selector = css_selector(&l.layer_name);
        let mut props: Vec<String> = vec![
            format!("  position: absolute;"),
            format!("  left: {}px;", l.x),
            format!("  top: {}px;", l.y),
            format!("  width: {}px;", l.width),
            format!("  height: {}px;", l.height),
        ];
        if (l.opacity - 1.0).abs() > 0.001 {
            props.push(format!("  opacity: {:.3};", l.opacity));
        }
        if l.rotation_deg != 0.0 {
            props.push(format!("  transform: rotate({}deg);", l.rotation_deg));
        }
        let r = &l.border_radius;
        if r.iter().any(|&v| v != 0.0) {
            if r.windows(2).all(|w| (w[0] - w[1]).abs() < 0.001) {
                props.push(format!("  border-radius: {}px;", r[0]));
            } else {
                props.push(format!("  border-radius: {}px {}px {}px {}px;", r[0], r[1], r[2], r[3]));
            }
        }
        if let Some(ref c) = l.fill_color {
            props.push(format!("  background-color: {};", c.to_css()));
        }
        if let Some(ref bc) = l.border_color {
            props.push(format!("  border: {}px solid {};", l.border_width, bc.to_css()));
        }
        if !l.shadows.is_empty() {
            let ss: Vec<_> = l.shadows.iter().map(|s| s.to_css()).collect();
            props.push(format!("  box-shadow: {};", ss.join(", ")));
        }
        if let Some(ref ty) = l.typography {
            append_typography_css(&mut props, ty, "  ");
        }
        if let Some(ref al) = l.auto_layout {
            append_auto_layout_css(&mut props, al, "  ");
        }
        format!(".{} {{\n{}\n}}", selector, props.join("\n"))
    }

    // ── Tailwind ──────────────────────────────────────────────────────────────

    fn to_tailwind(l: &LayerInspection) -> String {
        let mut classes: Vec<String> = vec![
            "absolute".into(),
            format!("w-[{}px]",  l.width),
            format!("h-[{}px]",  l.height),
            format!("left-[{}px]", l.x),
            format!("top-[{}px]",  l.y),
        ];
        if (l.opacity - 1.0).abs() > 0.001 {
            classes.push(format!("opacity-[{:.3}]", l.opacity));
        }
        if l.rotation_deg != 0.0 {
            classes.push(format!("rotate-[{}deg]", l.rotation_deg));
        }
        let r = &l.border_radius;
        if r.iter().any(|&v| v != 0.0) {
            if r.windows(2).all(|w| (w[0] - w[1]).abs() < 0.001) {
                classes.push(format!("rounded-[{}px]", r[0]));
            } else {
                classes.push(format!(
                    "rounded-tl-[{}px] rounded-tr-[{}px] rounded-br-[{}px] rounded-bl-[{}px]",
                    r[0], r[1], r[2], r[3]
                ));
            }
        }
        if let Some(ref c) = l.fill_color {
            classes.push(format!("bg-[{}]", c.to_css()));
        }
        if let Some(ref bc) = l.border_color {
            classes.push(format!("border-[{}px]", l.border_width as u32));
            classes.push("border-solid".into());
            classes.push(format!("border-[{}]", bc.to_css()));
        }
        if !l.shadows.is_empty() {
            let ss: Vec<_> = l.shadows.iter().map(|s| s.to_css()).collect();
            classes.push(format!("shadow-[{}]", ss.join("_")));
        }
        if let Some(ref ty) = l.typography {
            classes.push(format!("text-[{}px]", ty.font_size));
            classes.push(format!("font-['{}']", ty.font_family));
            classes.push(format!("font-[{}]",   ty.font_weight));
            if let Some(lh) = ty.line_height {
                classes.push(format!("leading-[{}px]", lh));
            }
            if ty.letter_spacing != 0.0 {
                classes.push(format!("tracking-[{}px]", ty.letter_spacing));
            }
        }
        if let Some(ref al) = l.auto_layout {
            classes.push("flex".into());
            classes.push(match al.direction {
                LayoutDirection::Row    => "flex-row".into(),
                LayoutDirection::Column => "flex-col".into(),
            });
            classes.push(format!("gap-[{}px]", al.gap));
            classes.push(format!(
                "px-[{}px] py-[{}px]",
                al.padding_left.min(al.padding_right),
                al.padding_top.min(al.padding_bottom),
            ));
            classes.push(format!("items-[{}]", al.align_items));
            classes.push(format!("justify-[{}]", al.justify_content));
            if al.wrap { classes.push("flex-wrap".into()); }
        }
        classes.join(" ")
    }

    // ── Sass (SCSS) ───────────────────────────────────────────────────────────

    fn to_sass(l: &LayerInspection) -> String {
        let selector = css_selector(&l.layer_name);
        let mut props: Vec<String> = vec![
            format!("  position: absolute;"),
            format!("  left: {}px;", l.x),
            format!("  top: {}px;", l.y),
            format!("  width: {}px;", l.width),
            format!("  height: {}px;", l.height),
        ];
        if (l.opacity - 1.0).abs() > 0.001 {
            props.push(format!("  opacity: {:.3};", l.opacity));
        }
        if l.rotation_deg != 0.0 {
            props.push(format!("  transform: rotate({}deg);", l.rotation_deg));
        }
        let r = &l.border_radius;
        if r.iter().any(|&v| v != 0.0) {
            if r.windows(2).all(|w| (w[0] - w[1]).abs() < 0.001) {
                props.push(format!("  border-radius: {}px;", r[0]));
            } else {
                props.push(format!("  border-radius: {}px {}px {}px {}px;", r[0], r[1], r[2], r[3]));
            }
        }
        if let Some(ref c) = l.fill_color {
            props.push(format!("  background-color: {};", c.to_css()));
        }
        if let Some(ref bc) = l.border_color {
            props.push(format!("  border: {}px solid {};", l.border_width, bc.to_css()));
        }
        if !l.shadows.is_empty() {
            let ss: Vec<_> = l.shadows.iter().map(|s| s.to_css()).collect();
            props.push(format!("  box-shadow: {};", ss.join(", ")));
        }
        if let Some(ref ty) = l.typography {
            append_typography_css(&mut props, ty, "  ");
        }
        if let Some(ref al) = l.auto_layout {
            append_auto_layout_css(&mut props, al, "  ");
        }
        // Sass-specific: produce $variable block at top.
        let mut vars: Vec<String> = Vec::new();
        if let Some(ref c) = l.fill_color {
            vars.push(format!("${}-bg: {};", selector, c.to_css()));
        }
        if let Some(ref ty) = l.typography {
            vars.push(format!("${}-font-size: {}px;", selector, ty.font_size));
        }
        let var_block = if vars.is_empty() {
            String::new()
        } else {
            format!("{}\n\n", vars.join("\n"))
        };
        format!("{}.{} {{\n{}\n}}", var_block, selector, props.join("\n"))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Convert a layer name to a valid CSS class name (kebab-case, lowercase).
fn css_selector(name: &str) -> String {
    name.to_lowercase()
        .replace(|c: char| c.is_whitespace() || c == '/', "-")
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "")
}

fn append_typography_css(props: &mut Vec<String>, ty: &Typography, indent: &str) {
    props.push(format!("{}font-family: '{}';", indent, ty.font_family));
    props.push(format!("{}font-size: {}px;",   indent, ty.font_size));
    props.push(format!("{}font-weight: {};",   indent, ty.font_weight));
    if let Some(lh) = ty.line_height {
        props.push(format!("{}line-height: {}px;", indent, lh));
    }
    if ty.letter_spacing != 0.0 {
        props.push(format!("{}letter-spacing: {}px;", indent, ty.letter_spacing));
    }
    if let Some(ref tt) = ty.text_transform {
        props.push(format!("{}text-transform: {};", indent, tt));
    }
}

fn append_auto_layout_css(props: &mut Vec<String>, al: &AutoLayout, indent: &str) {
    props.push(format!("{}display: flex;", indent));
    props.push(format!("{}flex-direction: {};", indent,
        match al.direction { LayoutDirection::Row => "row", LayoutDirection::Column => "column" }));
    props.push(format!("{}gap: {}px;", indent, al.gap));
    props.push(format!("{}padding: {}px {}px {}px {}px;",
        indent, al.padding_top, al.padding_right, al.padding_bottom, al.padding_left));
    props.push(format!("{}align-items: {};",     indent, al.align_items));
    props.push(format!("{}justify-content: {};", indent, al.justify_content));
    if al.wrap { props.push(format!("{}flex-wrap: wrap;", indent)); }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handoff::{LayerInspection, Color, Shadow, Typography, AutoLayout, LayoutDirection};
    use uuid::Uuid;

    fn base() -> LayerInspection {
        LayerInspection {
            layer_id: Uuid::new_v4(),
            layer_name: "Card Component".into(),
            x: 0.0, y: 0.0,
            width: 200.0, height: 100.0,
            rotation_deg: 0.0,
            opacity: 1.0,
            border_radius: [0.0; 4],
            border_width: 0.0,
            border_color: None,
            fill_color: Some(Color::rgba(1.0, 1.0, 1.0, 1.0)),
            shadows: vec![],
            typography: None,
            auto_layout: None,
        }
    }

    // E-01: CSS selector is kebab-case.
    #[test]
    fn e_01_css_selector_kebab() {
        assert_eq!(css_selector("Card Component"), "card-component");
    }

    // E-02: CSS selector strips special chars.
    #[test]
    fn e_02_css_selector_strips() {
        assert_eq!(css_selector("My(Layer)!"), "mylayer");
    }

    // E-03: CSS output starts with selector block.
    #[test]
    fn e_03_css_block_open() {
        let out = CodeExporter::export(&base(), ExportFormat::Css);
        assert!(out.contains(".card-component {"));
        assert!(out.ends_with('}'));
    }

    // E-04: CSS contains width and height.
    #[test]
    fn e_04_css_dimensions() {
        let out = CodeExporter::export(&base(), ExportFormat::Css);
        assert!(out.contains("width: 200px"));
        assert!(out.contains("height: 100px"));
    }

    // E-05: CSS contains fill.
    #[test]
    fn e_05_css_fill() {
        let out = CodeExporter::export(&base(), ExportFormat::Css);
        assert!(out.contains("background-color:"));
    }

    // E-06: CSS contains typography block.
    #[test]
    fn e_06_css_typography() {
        let mut l = base();
        l.typography = Some(Typography {
            font_family: "Roboto".into(), font_size: 14.0, font_weight: 400,
            line_height: Some(21.0), letter_spacing: 0.0, text_transform: None,
        });
        let out = CodeExporter::export(&l, ExportFormat::Css);
        assert!(out.contains("font-family: 'Roboto'"));
        assert!(out.contains("font-size: 14px"));
    }

    // E-07: CSS contains auto-layout.
    #[test]
    fn e_07_css_auto_layout() {
        let mut l = base();
        l.auto_layout = Some(AutoLayout {
            direction: LayoutDirection::Row, gap: 12.0,
            padding_top: 8.0, padding_right: 16.0, padding_bottom: 8.0, padding_left: 16.0,
            align_items: "center".into(), justify_content: "flex-start".into(), wrap: false,
        });
        let out = CodeExporter::export(&l, ExportFormat::Css);
        assert!(out.contains("display: flex"));
        assert!(out.contains("gap: 12px"));
    }

    // E-08: CSS contains box-shadow for shadows.
    #[test]
    fn e_08_css_shadows() {
        let mut l = base();
        l.shadows = vec![Shadow { offset_x: 0.0, offset_y: 4.0, blur: 8.0, spread: 0.0,
            color: Color::rgba(0.0, 0.0, 0.0, 0.15), inset: false }];
        let out = CodeExporter::export(&l, ExportFormat::Css);
        assert!(out.contains("box-shadow:"));
    }

    // E-09: CSS includes border when border_color set.
    #[test]
    fn e_09_css_border() {
        let mut l = base();
        l.border_width = 2.0;
        l.border_color = Some(Color::rgba(0.0, 0.0, 0.0, 1.0));
        let out = CodeExporter::export(&l, ExportFormat::Css);
        assert!(out.contains("border:"));
    }

    // E-10: CSS includes opacity when not 1.0.
    #[test]
    fn e_10_css_opacity() {
        let mut l = base();
        l.opacity = 0.8;
        assert!(CodeExporter::export(&l, ExportFormat::Css).contains("opacity:"));
    }

    // E-11: CSS includes rotation.
    #[test]
    fn e_11_css_rotation() {
        let mut l = base();
        l.rotation_deg = 30.0;
        assert!(CodeExporter::export(&l, ExportFormat::Css).contains("rotate(30deg)"));
    }

    // E-12: Tailwind output contains absolute.
    #[test]
    fn e_12_tailwind_absolute() {
        let out = CodeExporter::export(&base(), ExportFormat::Tailwind);
        assert!(out.contains("absolute"));
    }

    // E-13: Tailwind output contains w-[…] and h-[…].
    #[test]
    fn e_13_tailwind_width_height() {
        let out = CodeExporter::export(&base(), ExportFormat::Tailwind);
        assert!(out.contains("w-[200px]"));
        assert!(out.contains("h-[100px]"));
    }

    // E-14: Tailwind output contains bg-[…].
    #[test]
    fn e_14_tailwind_bg() {
        let out = CodeExporter::export(&base(), ExportFormat::Tailwind);
        assert!(out.contains("bg-["));
    }

    // E-15: Tailwind output contains rounded-[…] for uniform radius.
    #[test]
    fn e_15_tailwind_radius_uniform() {
        let mut l = base();
        l.border_radius = [8.0; 4];
        let out = CodeExporter::export(&l, ExportFormat::Tailwind);
        assert!(out.contains("rounded-[8px]"));
    }

    // E-16: Tailwind output per-corner radius.
    #[test]
    fn e_16_tailwind_radius_per_corner() {
        let mut l = base();
        l.border_radius = [4.0, 8.0, 12.0, 0.0];
        let out = CodeExporter::export(&l, ExportFormat::Tailwind);
        assert!(out.contains("rounded-tl-[4px]"));
    }

    // E-17: Tailwind typography classes.
    #[test]
    fn e_17_tailwind_typography() {
        let mut l = base();
        l.typography = Some(Typography { font_family: "Inter".into(), font_size: 16.0,
            font_weight: 500, line_height: None, letter_spacing: 0.0, text_transform: None });
        let out = CodeExporter::export(&l, ExportFormat::Tailwind);
        assert!(out.contains("text-[16px]"));
        assert!(out.contains("font-[500]"));
    }

    // E-18: Tailwind flex classes.
    #[test]
    fn e_18_tailwind_flex() {
        let mut l = base();
        l.auto_layout = Some(AutoLayout {
            direction: LayoutDirection::Column, gap: 4.0,
            padding_top: 0.0, padding_right: 0.0, padding_bottom: 0.0, padding_left: 0.0,
            align_items: "stretch".into(), justify_content: "center".into(), wrap: false,
        });
        let out = CodeExporter::export(&l, ExportFormat::Tailwind);
        assert!(out.contains("flex"));
        assert!(out.contains("flex-col"));
    }

    // E-19: Sass output starts with optional variables.
    #[test]
    fn e_19_sass_vars() {
        let out = CodeExporter::export(&base(), ExportFormat::Sass);
        assert!(out.contains("$card-component-bg:"));
    }

    // E-20: Sass output contains selector block.
    #[test]
    fn e_20_sass_selector() {
        let out = CodeExporter::export(&base(), ExportFormat::Sass);
        assert!(out.contains(".card-component {"));
    }

    // E-21: Sass output contains dimensions.
    #[test]
    fn e_21_sass_dimensions() {
        let out = CodeExporter::export(&base(), ExportFormat::Sass);
        assert!(out.contains("width: 200px"));
        assert!(out.contains("height: 100px"));
    }

    // E-22: Sass output contains typography var.
    #[test]
    fn e_22_sass_typography_var() {
        let mut l = base();
        l.typography = Some(Typography { font_family: "Poppins".into(), font_size: 18.0,
            font_weight: 700, line_height: None, letter_spacing: 0.0, text_transform: None });
        let out = CodeExporter::export(&l, ExportFormat::Sass);
        assert!(out.contains("$card-component-font-size:"));
    }

    // E-23: All three formats produce non-empty output.
    #[test]
    fn e_23_all_formats_non_empty() {
        let l = base();
        assert!(!CodeExporter::export(&l, ExportFormat::Css).is_empty());
        assert!(!CodeExporter::export(&l, ExportFormat::Tailwind).is_empty());
        assert!(!CodeExporter::export(&l, ExportFormat::Sass).is_empty());
    }

    // E-24: Layer names with slashes become hyphens.
    #[test]
    fn e_24_selector_slashes() {
        assert_eq!(css_selector("icons/arrow"), "icons-arrow");
    }

    // E-25: CSS auto-layout column direction.
    #[test]
    fn e_25_css_auto_layout_column() {
        let mut l = base();
        l.auto_layout = Some(AutoLayout {
            direction: LayoutDirection::Column, gap: 0.0,
            padding_top: 0.0, padding_right: 0.0, padding_bottom: 0.0, padding_left: 0.0,
            align_items: "flex-start".into(), justify_content: "flex-start".into(), wrap: false,
        });
        let out = CodeExporter::export(&l, ExportFormat::Css);
        assert!(out.contains("flex-direction: column"));
    }
}
