//! SwiftUI code generator — converts layer style data into SwiftUI view code.
//!
//! Produces idiomatic SwiftUI that can be pasted into Xcode projects.
//! Handles:
//! - `Rectangle` / `Ellipse` / `Text` views
//! - `.frame(width:height:)` for dimensions
//! - `.background()` / `.fill()` for fills
//! - `.overlay()` for strokes
//! - `.shadow()` for drop shadows
//! - `.cornerRadius()` / `.clipShape(RoundedRectangle)`
//! - `.opacity()`
//! - `.blendMode()`

use super::{
    num, CodeGenTarget, CodeGenerator, LayerStyleData,
};
use logos_core::style::{
    BlendMode, Color, Fill, Gradient, ShadowKind,
};
use std::fmt::Write;

/// SwiftUI code generator.
pub struct SwiftUiGenerator;

impl CodeGenerator for SwiftUiGenerator {
    fn target(&self) -> CodeGenTarget {
        CodeGenTarget::SwiftUI
    }

    fn generate(&self, data: &LayerStyleData) -> String {
        let mut out = String::new();

        // View type
        let view = match data.layer_type.as_str() {
            "ellipse" => "Ellipse()".to_string(),
            "text" => {
                let content = data.text_content.as_deref().unwrap_or("Text");
                format!("Text(\"{}\")", swift_escape(content))
            }
            _ => "Rectangle()".to_string(),
        };

        writeln!(out, "{view}").unwrap();

        // Fill
        generate_fill(&mut out, data);

        // Frame
        writeln!(out, "    .frame(width: {}, height: {})", num(data.width), num(data.height)).unwrap();

        // Corner radius
        generate_radii(&mut out, data);

        // Stroke overlay
        generate_stroke(&mut out, data);

        // Shadow
        generate_shadow(&mut out, data);

        // Opacity
        generate_opacity(&mut out, data);

        // Blend mode
        generate_blend_mode(&mut out, data);

        // Position offset
        if data.x != 0.0 || data.y != 0.0 {
            writeln!(out, "    .offset(x: {}, y: {})", num(data.x), num(data.y)).unwrap();
        }

        out
    }
}

fn swift_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn swift_color(c: &Color) -> String {
    if (c.a - 1.0).abs() < 0.001 {
        format!("Color(red: {:.3}, green: {:.3}, blue: {:.3})", c.r, c.g, c.b)
    } else {
        format!(
            "Color(red: {:.3}, green: {:.3}, blue: {:.3}).opacity({:.2})",
            c.r, c.g, c.b, c.a
        )
    }
}

fn generate_fill(out: &mut String, data: &LayerStyleData) {
    if data.style.fills.is_empty() {
        return;
    }
    match &data.style.fills[0] {
        Fill::Solid(c) => {
            writeln!(out, "    .fill({})", swift_color(c)).unwrap();
        }
        Fill::Gradient(g) => {
            writeln!(out, "    .fill({})", swift_gradient(g)).unwrap();
        }
    }
}

fn swift_gradient(g: &Gradient) -> String {
    let stops: Vec<String> = g
        .stops()
        .iter()
        .map(|s| format!(".init(color: {}, location: {:.2})", swift_color(&s.color), s.position))
        .collect();
    let stops_str = stops.join(", ");

    match g {
        Gradient::Linear { angle_deg, .. } => {
            // Convert CSS angle (0=up, 90=right) to SwiftUI UnitPoint
            let (start, end) = angle_to_unit_points(*angle_deg);
            format!(
                "LinearGradient(stops: [{stops_str}], startPoint: {start}, endPoint: {end})"
            )
        }
        Gradient::Radial { .. } => {
            format!(
                "RadialGradient(stops: [{stops_str}], center: .center, startRadius: 0, endRadius: 100)"
            )
        }
    }
}

fn angle_to_unit_points(angle: f32) -> (&'static str, &'static str) {
    let a = ((angle % 360.0) + 360.0) % 360.0;
    match a as u32 {
        0..=44 => (".bottom", ".top"),
        45..=134 => (".leading", ".trailing"),
        135..=224 => (".top", ".bottom"),
        225..=314 => (".trailing", ".leading"),
        _ => (".bottom", ".top"),
    }
}

fn generate_radii(out: &mut String, data: &LayerStyleData) {
    if !data.has_radius() {
        return;
    }
    let r = data.style.corner_radii;
    if data.has_uniform_radius() {
        writeln!(out, "    .cornerRadius({})", num(r[0])).unwrap();
    } else {
        // SwiftUI uses clipShape for non-uniform
        writeln!(
            out,
            "    .clipShape(UnevenRoundedRectangle(topLeadingRadius: {}, topTrailingRadius: {}, bottomTrailingRadius: {}, bottomLeadingRadius: {}))",
            num(r[0]), num(r[1]), num(r[2]), num(r[3])
        ).unwrap();
    }
}

fn generate_stroke(out: &mut String, data: &LayerStyleData) {
    if data.style.strokes.is_empty() {
        return;
    }
    let s = &data.style.strokes[0];
    writeln!(
        out,
        "    .overlay(RoundedRectangle(cornerRadius: {}).stroke({}, lineWidth: {}))",
        num(data.style.corner_radii[0]),
        swift_color(&s.color),
        num(s.width)
    )
    .unwrap();
}

fn generate_shadow(out: &mut String, data: &LayerStyleData) {
    for s in &data.style.shadows {
        if s.kind == ShadowKind::Drop {
            writeln!(
                out,
                "    .shadow(color: {}, radius: {}, x: {}, y: {})",
                swift_color(&s.color),
                num(s.blur_radius / 2.0), // CSS blur ≈ 2× SwiftUI radius
                num(s.offset_x),
                num(s.offset_y)
            )
            .unwrap();
        }
    }
}

fn generate_opacity(out: &mut String, data: &LayerStyleData) {
    if (data.style.opacity - 1.0).abs() > 0.001 {
        writeln!(out, "    .opacity({:.2})", data.style.opacity).unwrap();
    }
}

fn generate_blend_mode(out: &mut String, data: &LayerStyleData) {
    let mode = match data.style.blend_mode {
        BlendMode::Normal => return,
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::ColorDodge => "colorDodge",
        BlendMode::ColorBurn => "colorBurn",
        BlendMode::SoftLight => "softLight",
        BlendMode::HardLight => "hardLight",
        BlendMode::Difference => "difference",
        BlendMode::Exclusion => "exclusion",
    };
    writeln!(out, "    .blendMode(.{mode})").unwrap();
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
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 240.0,
            style: LayerStyle::default(),
            text_content: None,
        }
    }

    #[test]
    fn test_basic_rectangle() {
        let gen = SwiftUiGenerator;
        let code = gen.generate(&default_data());
        assert!(code.contains("Rectangle()"));
        assert!(code.contains(".frame(width: 320, height: 240)"));
    }

    #[test]
    fn test_ellipse_view() {
        let gen = SwiftUiGenerator;
        let mut data = default_data();
        data.layer_type = "ellipse".into();
        let code = gen.generate(&data);
        assert!(code.contains("Ellipse()"));
    }

    #[test]
    fn test_text_view() {
        let gen = SwiftUiGenerator;
        let mut data = default_data();
        data.layer_type = "text".into();
        data.text_content = Some("Hello World".into());
        let code = gen.generate(&data);
        assert!(code.contains("Text(\"Hello World\")"));
    }

    #[test]
    fn test_solid_fill() {
        let gen = SwiftUiGenerator;
        let mut data = default_data();
        data.style = LayerStyle::default()
            .with_fill(Fill::Solid(Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }));
        let code = gen.generate(&data);
        assert!(code.contains(".fill(Color(red:"));
    }

    #[test]
    fn test_gradient_fill() {
        let gen = SwiftUiGenerator;
        let mut data = default_data();
        let start = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
        let end = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };
        data.style = LayerStyle::default()
            .with_fill(Fill::linear_gradient(90.0, start, end));
        let code = gen.generate(&data);
        assert!(code.contains("LinearGradient"));
    }

    #[test]
    fn test_corner_radius() {
        let gen = SwiftUiGenerator;
        let mut data = default_data();
        data.style = LayerStyle::default().with_corner_radii([16.0; 4]);
        let code = gen.generate(&data);
        assert!(code.contains(".cornerRadius(16)"));
    }

    #[test]
    fn test_shadow() {
        let gen = SwiftUiGenerator;
        let mut data = default_data();
        let shadow = Shadow::drop_shadow(
            Color { r: 0.0, g: 0.0, b: 0.0, a: 0.25 },
            0.0, 4.0, 8.0,
        );
        data.style = LayerStyle::default().with_shadow(shadow);
        let code = gen.generate(&data);
        assert!(code.contains(".shadow(color:"));
    }

    #[test]
    fn test_opacity() {
        let gen = SwiftUiGenerator;
        let mut data = default_data();
        data.style = LayerStyle::default().with_opacity(0.75);
        let code = gen.generate(&data);
        assert!(code.contains(".opacity(0.75)"));
    }

    #[test]
    fn test_offset() {
        let gen = SwiftUiGenerator;
        let mut data = default_data();
        data.x = 50.0;
        data.y = 100.0;
        let code = gen.generate(&data);
        assert!(code.contains(".offset(x: 50, y: 100)"));
    }

    #[test]
    fn test_blend_mode() {
        let gen = SwiftUiGenerator;
        let mut data = default_data();
        data.style.blend_mode = BlendMode::Screen;
        let code = gen.generate(&data);
        assert!(code.contains(".blendMode(.screen)"));
    }

    #[test]
    fn test_stroke_overlay() {
        let gen = SwiftUiGenerator;
        let mut data = default_data();
        let stroke = Stroke::new(Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }, 2.0);
        data.style = LayerStyle::default().with_stroke(stroke);
        let code = gen.generate(&data);
        assert!(code.contains(".overlay("));
        assert!(code.contains(".stroke("));
    }
}
