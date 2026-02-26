//! CSS code generator — converts layer style data into CSS declarations.
//!
//! Produces standard CSS that can be copy-pasted into web projects.
//! Handles:
//! - Dimensions (`width`, `height`)
//! - Position (`left`, `top` as absolute positioning)
//! - Fill → `background` / `background-color` / `background-image`
//! - Stroke → `border`
//! - Shadow → `box-shadow`
//! - Corner radii → `border-radius`
//! - Opacity
//! - Blend mode → `mix-blend-mode`

use super::{
    color_to_rgba_string, px, CodeGenTarget, CodeGenerator, LayerStyleData,
};
use logos_core::style::{
    BlendMode, Fill, Gradient, Shadow, ShadowKind, StrokeAlign,
};
use std::fmt::Write;

/// CSS code generator.
pub struct CssGenerator;

impl CodeGenerator for CssGenerator {
    fn target(&self) -> CodeGenTarget {
        CodeGenTarget::Css
    }

    fn generate(&self, data: &LayerStyleData) -> String {
        let mut css = format!(".{} {{\n", css_class_name(&data.name));
        generate_dimensions(&mut css, data);
        generate_position(&mut css, data);
        generate_fills(&mut css, data);
        generate_strokes(&mut css, data);
        generate_shadows(&mut css, data);
        generate_radii(&mut css, data);
        generate_opacity(&mut css, data);
        generate_blend_mode(&mut css, data);
        css.push_str("}\n");
        css
    }
}

/// Sanitize a name into a valid CSS class name.
fn css_class_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else if c == ' ' {
            out.push('-');
        }
    }
    if out.is_empty() || out.chars().next().map_or(false, |c| c.is_numeric()) {
        out.insert(0, '_');
    }
    out
}

fn generate_dimensions(css: &mut String, data: &LayerStyleData) {
    writeln!(css, "  width: {};", px(data.width)).unwrap();
    writeln!(css, "  height: {};", px(data.height)).unwrap();
}

fn generate_position(css: &mut String, data: &LayerStyleData) {
    if data.x != 0.0 || data.y != 0.0 {
        writeln!(css, "  position: absolute;").unwrap();
        writeln!(css, "  left: {};", px(data.x)).unwrap();
        writeln!(css, "  top: {};", px(data.y)).unwrap();
    }
}

fn generate_fills(css: &mut String, data: &LayerStyleData) {
    if data.style.fills.is_empty() {
        return;
    }

    // Multiple fills: use background with layered values
    if data.style.fills.len() == 1 {
        match &data.style.fills[0] {
            Fill::Solid(c) => {
                writeln!(css, "  background-color: {};", color_to_rgba_string(c)).unwrap();
            }
            Fill::Gradient(g) => {
                writeln!(css, "  background: {};", gradient_to_css(g)).unwrap();
            }
        }
    } else {
        let layers: Vec<String> = data
            .style
            .fills
            .iter()
            .rev() // CSS layers are painted top-down (first = top)
            .map(|f| match f {
                Fill::Solid(c) => color_to_rgba_string(c),
                Fill::Gradient(g) => gradient_to_css(g),
            })
            .collect();
        writeln!(css, "  background: {};", layers.join(", ")).unwrap();
    }
}

fn gradient_to_css(g: &Gradient) -> String {
    let stops: Vec<String> = g
        .stops()
        .iter()
        .map(|s| format!("{} {}%", color_to_rgba_string(&s.color), (s.position * 100.0).round()))
        .collect();
    let stop_str = stops.join(", ");

    match g {
        Gradient::Linear { angle_deg, .. } => {
            format!("linear-gradient({angle_deg}deg, {stop_str})")
        }
        Gradient::Radial { .. } => {
            format!("radial-gradient(circle, {stop_str})")
        }
    }
}

fn generate_strokes(css: &mut String, data: &LayerStyleData) {
    if data.style.strokes.is_empty() {
        return;
    }
    let s = &data.style.strokes[0];
    let color_str = color_to_rgba_string(&s.color);
    match s.align {
        StrokeAlign::Center => {
            writeln!(css, "  border: {} solid {};", px(s.width), color_str).unwrap();
        }
        StrokeAlign::Inside => {
            // Inside stroke: use box-shadow inset
            writeln!(
                css,
                "  box-shadow: inset 0 0 0 {} {};",
                px(s.width),
                color_str
            )
            .unwrap();
        }
        StrokeAlign::Outside => {
            // Outside stroke: outline
            writeln!(css, "  outline: {} solid {};", px(s.width), color_str).unwrap();
        }
    }
}

fn generate_shadows(css: &mut String, data: &LayerStyleData) {
    if data.style.shadows.is_empty() {
        return;
    }
    let parts: Vec<String> = data
        .style
        .shadows
        .iter()
        .map(shadow_to_css)
        .collect();
    writeln!(css, "  box-shadow: {};", parts.join(", ")).unwrap();
}

fn shadow_to_css(s: &Shadow) -> String {
    let inset = if s.kind == ShadowKind::Inner {
        "inset "
    } else {
        ""
    };
    format!(
        "{}{} {} {} {}",
        inset,
        px(s.offset_x),
        px(s.offset_y),
        px(s.blur_radius),
        color_to_rgba_string(&s.color)
    )
}

fn generate_radii(css: &mut String, data: &LayerStyleData) {
    if !data.has_radius() {
        return;
    }
    let r = data.style.corner_radii;
    if data.has_uniform_radius() {
        writeln!(css, "  border-radius: {};", px(r[0])).unwrap();
    } else {
        writeln!(
            css,
            "  border-radius: {} {} {} {};",
            px(r[0]),
            px(r[1]),
            px(r[2]),
            px(r[3])
        )
        .unwrap();
    }
}

fn generate_opacity(css: &mut String, data: &LayerStyleData) {
    if (data.style.opacity - 1.0).abs() > 0.001 {
        writeln!(css, "  opacity: {:.2};", data.style.opacity).unwrap();
    }
}

fn generate_blend_mode(css: &mut String, data: &LayerStyleData) {
    let mode = match data.style.blend_mode {
        BlendMode::Normal => return,
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::ColorDodge => "color-dodge",
        BlendMode::ColorBurn => "color-burn",
        BlendMode::SoftLight => "soft-light",
        BlendMode::HardLight => "hard-light",
        BlendMode::Difference => "difference",
        BlendMode::Exclusion => "exclusion",
    };
    writeln!(css, "  mix-blend-mode: {};", mode).unwrap();
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::style::{Color, Fill, LayerStyle, Shadow, Stroke};

    fn default_data() -> LayerStyleData {
        LayerStyleData {
            layer_type: "rect".into(),
            name: "Card".into(),
            x: 100.0,
            y: 200.0,
            width: 320.0,
            height: 240.0,
            style: LayerStyle::default(),
            text_content: None,
        }
    }

    #[test]
    fn test_css_class_name() {
        assert_eq!(css_class_name("My Card"), "my-card");
        assert_eq!(css_class_name("123abc"), "_123abc");
        assert_eq!(css_class_name("hello_world"), "hello_world");
    }

    #[test]
    fn test_basic_rect() {
        let gen = CssGenerator;
        let data = default_data();
        let css = gen.generate(&data);
        assert!(css.contains(".card {"));
        assert!(css.contains("width: 320px;"));
        assert!(css.contains("height: 240px;"));
        assert!(css.contains("position: absolute;"));
        assert!(css.contains("left: 100px;"));
        assert!(css.contains("top: 200px;"));
    }

    #[test]
    fn test_solid_fill() {
        let gen = CssGenerator;
        let mut data = default_data();
        data.style = LayerStyle::default()
            .with_fill(Fill::Solid(Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }));
        let css = gen.generate(&data);
        assert!(css.contains("background-color: #ff0000;"));
    }

    #[test]
    fn test_gradient_fill() {
        let gen = CssGenerator;
        let mut data = default_data();
        let start = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
        let end = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };
        data.style = LayerStyle::default()
            .with_fill(Fill::linear_gradient(90.0, start, end));
        let css = gen.generate(&data);
        assert!(css.contains("linear-gradient(90deg"));
    }

    #[test]
    fn test_border_stroke() {
        let gen = CssGenerator;
        let mut data = default_data();
        let stroke = Stroke::new(Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }, 2.0);
        data.style = LayerStyle::default().with_stroke(stroke);
        let css = gen.generate(&data);
        assert!(css.contains("border: 2px solid #000000;"));
    }

    #[test]
    fn test_inside_stroke() {
        let gen = CssGenerator;
        let mut data = default_data();
        let stroke = Stroke::new(Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }, 1.0)
            .with_align(StrokeAlign::Inside);
        data.style = LayerStyle::default().with_stroke(stroke);
        let css = gen.generate(&data);
        assert!(css.contains("box-shadow: inset"));
    }

    #[test]
    fn test_drop_shadow() {
        let gen = CssGenerator;
        let mut data = default_data();
        let shadow = Shadow::drop_shadow(
            Color { r: 0.0, g: 0.0, b: 0.0, a: 0.25 },
            0.0, 4.0, 8.0,
        );
        data.style = LayerStyle::default().with_shadow(shadow);
        let css = gen.generate(&data);
        assert!(css.contains("box-shadow:"));
        assert!(css.contains("8px"));
    }

    #[test]
    fn test_uniform_border_radius() {
        let gen = CssGenerator;
        let mut data = default_data();
        data.style = LayerStyle::default().with_corner_radii([12.0; 4]);
        let css = gen.generate(&data);
        assert!(css.contains("border-radius: 12px;"));
    }

    #[test]
    fn test_non_uniform_border_radius() {
        let gen = CssGenerator;
        let mut data = default_data();
        data.style = LayerStyle::default().with_corner_radii([8.0, 16.0, 8.0, 0.0]);
        let css = gen.generate(&data);
        assert!(css.contains("border-radius: 8px 16px 8px 0;"));
    }

    #[test]
    fn test_opacity() {
        let gen = CssGenerator;
        let mut data = default_data();
        data.style = LayerStyle::default().with_opacity(0.5);
        let css = gen.generate(&data);
        assert!(css.contains("opacity: 0.50;"));
    }

    #[test]
    fn test_blend_mode() {
        let gen = CssGenerator;
        let mut data = default_data();
        data.style.blend_mode = BlendMode::Multiply;
        let css = gen.generate(&data);
        assert!(css.contains("mix-blend-mode: multiply;"));
    }

    #[test]
    fn test_no_position_at_origin() {
        let gen = CssGenerator;
        let mut data = default_data();
        data.x = 0.0;
        data.y = 0.0;
        let css = gen.generate(&data);
        assert!(!css.contains("position:"));
    }

    #[test]
    fn test_generate_all_multiple() {
        let gen = CssGenerator;
        let d1 = LayerStyleData {
            name: "a".into(),
            ..default_data()
        };
        let d2 = LayerStyleData {
            name: "b".into(),
            ..default_data()
        };
        let result = gen.generate_all(&[d1, d2]);
        assert!(result.contains(".a {"));
        assert!(result.contains(".b {"));
    }
}
