//! Jetpack Compose code generator — converts layer style data into
//! Kotlin Compose UI code.
//!
//! Produces idiomatic Jetpack Compose code for Android projects.
//! Handles:
//! - `Box` / `Canvas` / `Text` composables
//! - `Modifier.size()`, `.offset()`
//! - `Modifier.background()` / `Brush.linearGradient()`
//! - `Modifier.border()`
//! - `Modifier.shadow()`
//! - `Modifier.clip(RoundedCornerShape())`
//! - `Modifier.alpha()`

use super::{num, CodeGenTarget, CodeGenerator, LayerStyleData};
use logos_core::style::{
    Color, Fill, Gradient, ShadowKind,
};
use std::fmt::Write;

/// Jetpack Compose code generator.
pub struct ComposeGenerator;

impl CodeGenerator for ComposeGenerator {
    fn target(&self) -> CodeGenTarget {
        CodeGenTarget::Compose
    }

    fn generate(&self, data: &LayerStyleData) -> String {
        let mut out = String::new();

        // Composable function
        let name = compose_func_name(&data.name);
        writeln!(out, "@Composable").unwrap();
        writeln!(out, "fun {name}() {{").unwrap();

        let (composable, modifier) = match data.layer_type.as_str() {
            "text" => {
                let content = data.text_content.as_deref().unwrap_or("Text");
                (format!("    Text(\"{}\"", kt_escape(content)), true)
            }
            "ellipse" => {
                writeln!(out, "    Canvas(").unwrap();
                writeln!(out, "        modifier = {}",
                    build_modifier(data)
                ).unwrap();
                writeln!(out, "    ) {{").unwrap();
                writeln!(out, "        drawOval({}", compose_fill_draw(&data.style.fills)).unwrap();
                writeln!(out, "    }}").unwrap();
                writeln!(out, "}}").unwrap();
                return out;
            }
            _ => ("    Box(".to_string(), true),
        };

        if modifier {
            writeln!(out, "{composable},").unwrap();
            writeln!(out, "        modifier = {}", build_modifier(data)).unwrap();
            writeln!(out, "    )").unwrap();
        }

        writeln!(out, "}}").unwrap();
        out
    }
}

/// Sanitize name into a valid Kotlin function name.
fn compose_func_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut capitalize_next = true;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if capitalize_next {
                out.extend(c.to_uppercase());
                capitalize_next = false;
            } else {
                out.push(c);
            }
        } else {
            capitalize_next = true;
        }
    }
    if out.is_empty() || out.chars().next().map_or(false, |c| c.is_lowercase()) {
        // PascalCase for Compose
        let mut chars = out.chars();
        if let Some(first) = chars.next() {
            out = first.to_uppercase().collect::<String>() + chars.as_str();
        }
    }
    if out.is_empty() {
        "Component".to_string()
    } else {
        out
    }
}

fn kt_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
}

fn compose_color(c: &Color) -> String {
    let r = (c.r * 255.0).round() as u8;
    let g = (c.g * 255.0).round() as u8;
    let b = (c.b * 255.0).round() as u8;
    let a = (c.a * 255.0).round() as u8;
    if a == 255 {
        format!("Color(0xFF{r:02X}{g:02X}{b:02X})")
    } else {
        format!("Color(0x{a:02X}{r:02X}{g:02X}{b:02X})")
    }
}

fn build_modifier(data: &LayerStyleData) -> String {
    let mut parts = vec!["Modifier".to_string()];

    // Size
    parts.push(format!(".size({}.dp, {}.dp)", num(data.width), num(data.height)));

    // Offset
    if data.x != 0.0 || data.y != 0.0 {
        parts.push(format!(".offset(x = {}.dp, y = {}.dp)", num(data.x), num(data.y)));
    }

    // Clip (must come before background for proper rendering)
    if data.has_radius() {
        let r = data.style.corner_radii;
        if data.has_uniform_radius() {
            parts.push(format!(".clip(RoundedCornerShape({}.dp))", num(r[0])));
        } else {
            parts.push(format!(
                ".clip(RoundedCornerShape(topStart = {}.dp, topEnd = {}.dp, bottomEnd = {}.dp, bottomStart = {}.dp))",
                num(r[0]), num(r[1]), num(r[2]), num(r[3])
            ));
        }
    }

    // Background
    if !data.style.fills.is_empty() {
        match &data.style.fills[0] {
            Fill::Solid(c) => {
                parts.push(format!(".background({})", compose_color(c)));
            }
            Fill::Gradient(g) => {
                parts.push(format!(".background({})", compose_gradient(g)));
            }
        }
    }

    // Border (stroke)
    if !data.style.strokes.is_empty() {
        let s = &data.style.strokes[0];
        parts.push(format!(
            ".border({}.dp, {})",
            num(s.width),
            compose_color(&s.color)
        ));
    }

    // Shadow
    for s in &data.style.shadows {
        if s.kind == ShadowKind::Drop {
            parts.push(format!(
                ".shadow(elevation = {}.dp)",
                num(s.blur_radius / 2.0)
            ));
        }
    }

    // Alpha
    if (data.style.opacity - 1.0).abs() > 0.001 {
        parts.push(format!(".alpha({:.2}f)", data.style.opacity));
    }

    parts.join("\n            ")
}

fn compose_gradient(g: &Gradient) -> String {
    let colors: Vec<String> = g
        .stops()
        .iter()
        .map(|s| compose_color(&s.color))
        .collect();
    let colors_str = colors.join(", ");

    match g {
        Gradient::Linear { .. } => {
            format!("Brush.linearGradient(listOf({colors_str}))")
        }
        Gradient::Radial { .. } => {
            format!("Brush.radialGradient(listOf({colors_str}))")
        }
    }
}

fn compose_fill_draw(fills: &[Fill]) -> String {
    if fills.is_empty() {
        return "color = Color.Black)".to_string();
    }
    match &fills[0] {
        Fill::Solid(c) => format!("color = {})", compose_color(c)),
        Fill::Gradient(g) => format!("brush = {})", compose_gradient(g)),
    }
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
            name: "card_view".into(),
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 240.0,
            style: LayerStyle::default(),
            text_content: None,
        }
    }

    #[test]
    fn test_compose_func_name() {
        assert_eq!(compose_func_name("my card"), "MyCard");
        assert_eq!(compose_func_name("hello_world"), "HelloWorld");
        assert_eq!(compose_func_name(""), "Component");
    }

    #[test]
    fn test_compose_color() {
        let c = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
        assert_eq!(compose_color(&c), "Color(0xFFFF0000)");
    }

    #[test]
    fn test_compose_color_alpha() {
        let c = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.5 };
        let s = compose_color(&c);
        assert!(s.starts_with("Color(0x80"));
    }

    #[test]
    fn test_basic_box() {
        let gen = ComposeGenerator;
        let code = gen.generate(&default_data());
        assert!(code.contains("@Composable"));
        assert!(code.contains("fun CardView()"));
        assert!(code.contains("Box("));
        assert!(code.contains(".size(320.dp, 240.dp)"));
    }

    #[test]
    fn test_text_composable() {
        let gen = ComposeGenerator;
        let mut data = default_data();
        data.layer_type = "text".into();
        data.text_content = Some("Hello".into());
        let code = gen.generate(&data);
        assert!(code.contains("Text(\"Hello\""));
    }

    #[test]
    fn test_solid_background() {
        let gen = ComposeGenerator;
        let mut data = default_data();
        data.style = LayerStyle::default()
            .with_fill(Fill::Solid(Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }));
        let code = gen.generate(&data);
        assert!(code.contains(".background(Color(0xFFFF0000))"));
    }

    #[test]
    fn test_gradient_background() {
        let gen = ComposeGenerator;
        let mut data = default_data();
        let start = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
        let end = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };
        data.style = LayerStyle::default()
            .with_fill(Fill::linear_gradient(90.0, start, end));
        let code = gen.generate(&data);
        assert!(code.contains("Brush.linearGradient"));
    }

    #[test]
    fn test_rounded_corners() {
        let gen = ComposeGenerator;
        let mut data = default_data();
        data.style = LayerStyle::default().with_corner_radii([12.0; 4]);
        let code = gen.generate(&data);
        assert!(code.contains(".clip(RoundedCornerShape(12.dp))"));
    }

    #[test]
    fn test_border() {
        let gen = ComposeGenerator;
        let mut data = default_data();
        let stroke = Stroke::new(Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }, 2.0);
        data.style = LayerStyle::default().with_stroke(stroke);
        let code = gen.generate(&data);
        assert!(code.contains(".border(2.dp,"));
    }

    #[test]
    fn test_shadow() {
        let gen = ComposeGenerator;
        let mut data = default_data();
        let shadow = Shadow::drop_shadow(
            Color { r: 0.0, g: 0.0, b: 0.0, a: 0.25 },
            0.0, 4.0, 8.0,
        );
        data.style = LayerStyle::default().with_shadow(shadow);
        let code = gen.generate(&data);
        assert!(code.contains(".shadow(elevation ="));
    }

    #[test]
    fn test_alpha() {
        let gen = ComposeGenerator;
        let mut data = default_data();
        data.style = LayerStyle::default().with_opacity(0.5);
        let code = gen.generate(&data);
        assert!(code.contains(".alpha(0.50f)"));
    }

    #[test]
    fn test_offset() {
        let gen = ComposeGenerator;
        let mut data = default_data();
        data.x = 16.0;
        data.y = 32.0;
        let code = gen.generate(&data);
        assert!(code.contains(".offset(x = 16.dp, y = 32.dp)"));
    }
}
