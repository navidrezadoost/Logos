//! Agent API — parse natural language commands into structured Logos operations
//!
//! Users or external agents send text commands such as
//! "create a blue rectangle at (100,200) sized 300×150".
//! This module parses those into typed `AgentCommand` values that can be
//! handed off to the Logos command executor.

use serde::{Deserialize, Serialize};

// ── Layer kind ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerKind {
    Rectangle,
    Ellipse,
    Text,
    Path,
    Frame,
    Image,
    Component,
}

impl LayerKind {
    pub fn from_text(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "rectangle" | "rect" | "box" => Some(LayerKind::Rectangle),
            "ellipse" | "circle" | "oval" => Some(LayerKind::Ellipse),
            "text" | "label" | "heading" | "paragraph" => Some(LayerKind::Text),
            "path" | "vector" | "shape" => Some(LayerKind::Path),
            "frame" | "artboard" | "container" => Some(LayerKind::Frame),
            "image" | "photo" | "picture" => Some(LayerKind::Image),
            "component" | "symbol" => Some(LayerKind::Component),
            _ => None,
        }
    }
}

// ── Agent command ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentCommand {
    CreateLayer {
        kind: LayerKind,
        x: Option<f32>,
        y: Option<f32>,
        width: Option<f32>,
        height: Option<f32>,
        name: Option<String>,
    },
    SetFill {
        layer_id: Option<String>,
        color: String,
    },
    SetOpacity {
        layer_id: Option<String>,
        opacity: f32,
    },
    SetStroke {
        layer_id: Option<String>,
        color: Option<String>,
        width: Option<f32>,
    },
    DeleteLayer {
        layer_id: String,
    },
    MoveLayer {
        layer_id: String,
        x: f32,
        y: f32,
    },
    ResizeLayer {
        layer_id: String,
        width: f32,
        height: f32,
    },
    GroupLayers {
        ids: Vec<String>,
        name: Option<String>,
    },
    RunAiPipeline {
        pipeline: String,
    },
    CheckAccessibility,
    GeneratePalette {
        base_color: String,
        scheme: Option<String>,
    },
    WriteFormula {
        cell: String,
        formula: String,
    },
    BindCell {
        layer_id: String,
        property: String,
        cell: String,
    },
    Undo,
    Redo,
    Help { topic: Option<String> },
    Unknown { raw: String },
}

// ── Parsed command ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedCommand {
    pub raw_input: String,
    pub command: AgentCommand,
    pub confidence: f32,  // 0.0–1.0
    pub warnings: Vec<String>,
}

// ── Command result ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl CommandResult {
    pub fn ok(msg: impl Into<String>) -> Self {
        CommandResult { success: true, message: msg.into(), data: None }
    }
    pub fn ok_with_data(msg: impl Into<String>, data: serde_json::Value) -> Self {
        CommandResult { success: true, message: msg.into(), data: Some(data) }
    }
    pub fn err(msg: impl Into<String>) -> Self {
        CommandResult { success: false, message: msg.into(), data: None }
    }
}

// ── Agent request / response ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub session_id: String,
    pub request_id: String,
    pub command_text: String,
    pub context: serde_json::Value,
}

impl AgentRequest {
    pub fn new(session_id: impl Into<String>, text: impl Into<String>) -> Self {
        AgentRequest {
            session_id: session_id.into(),
            request_id: uuid::Uuid::new_v4().to_string(),
            command_text: text.into(),
            context: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub request_id: String,
    pub success: bool,
    pub result: serde_json::Value,
    pub error: Option<String>,
    pub latency_ms: u64,
}

impl AgentResponse {
    pub fn ok(request_id: impl Into<String>, result: serde_json::Value) -> Self {
        AgentResponse {
            request_id: request_id.into(),
            success: true,
            result,
            error: None,
            latency_ms: 0,
        }
    }
    pub fn err(request_id: impl Into<String>, msg: impl Into<String>) -> Self {
        AgentResponse {
            request_id: request_id.into(),
            success: false,
            result: serde_json::Value::Null,
            error: Some(msg.into()),
            latency_ms: 0,
        }
    }
}

// ── Command parser ────────────────────────────────────────────────────────────

pub struct CommandParser;

impl CommandParser {
    pub fn parse(input: &str) -> ParsedCommand {
        let lower = input.to_lowercase();
        let mut warnings = Vec::new();

        // Helper: extract first number after keyword
        fn extract_f32_after(text: &str, keyword: &str) -> Option<f32> {
            let pos = text.find(keyword)?;
            let rest = &text[pos + keyword.len()..];
            rest.split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
                .next()
                .and_then(|s| s.parse().ok())
        }

        // Helper: extract hex color
        fn extract_hex(text: &str) -> Option<String> {
            // Find #RRGGBB or #RGB
            let bytes = text.as_bytes();
            for i in 0..bytes.len() {
                if bytes[i] == b'#' {
                    let rest = &text[i+1..];
                    let hex: String = rest.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
                    if hex.len() == 6 || hex.len() == 3 {
                        return Some(format!("#{}", hex));
                    }
                }
            }
            // Named colors
            for (name, value) in &[
                ("red", "#ff0000"), ("blue", "#3b82f6"), ("green", "#22c55e"),
                ("white", "#ffffff"), ("black", "#000000"), ("gray", "#6b7280"),
                ("yellow", "#fbbf24"), ("purple", "#8b5cf6"), ("orange", "#f97316"),
            ] {
                if text.contains(name) {
                    return Some(value.to_string());
                }
            }
            None
        }

        // Parse: undo/redo
        if lower == "undo" || lower == "ctrl+z" {
            return ParsedCommand { raw_input: input.into(), command: AgentCommand::Undo, confidence: 1.0, warnings };
        }
        if lower == "redo" || lower == "ctrl+y" {
            return ParsedCommand { raw_input: input.into(), command: AgentCommand::Redo, confidence: 1.0, warnings };
        }

        // Parse: help
        if lower.starts_with("help") || lower.starts_with("what can you") || lower.starts_with("?") {
            let topic = if lower.contains("spreadsheet") { Some("spreadsheet".into()) }
                else if lower.contains("plugin") { Some("plugins".into()) }
                else if lower.contains("ai") { Some("ai".into()) }
                else { None };
            return ParsedCommand { raw_input: input.into(), command: AgentCommand::Help { topic }, confidence: 1.0, warnings };
        }

        // Parse: delete layer
        if lower.contains("delete") || lower.contains("remove") {
            let id = Self::extract_quoted(input).unwrap_or_else(|| "selected".to_string());
            return ParsedCommand {
                raw_input: input.into(),
                command: AgentCommand::DeleteLayer { layer_id: id },
                confidence: 0.85,
                warnings,
            };
        }

        // Parse: create layer
        let create_words = ["create", "add", "draw", "insert", "make", "new"];
        if create_words.iter().any(|w| lower.contains(w)) {
            // Detect kind
            let kind = ["rectangle", "rect", "box", "ellipse", "circle", "oval",
                "text", "label", "frame", "artboard", "image", "path", "component"]
                .iter()
                .find_map(|w| if lower.contains(w) { LayerKind::from_text(w) } else { None })
                .unwrap_or_else(|| {
                    warnings.push("Could not determine layer type; defaulting to Rectangle.".into());
                    LayerKind::Rectangle
                });

            let x = extract_f32_after(&lower, "x=")
                .or_else(|| extract_f32_after(&lower, "x:"));
            let y = extract_f32_after(&lower, "y=")
                .or_else(|| extract_f32_after(&lower, "y:"));
            let width = extract_f32_after(&lower, "width=")
                .or_else(|| extract_f32_after(&lower, "w="))
                .or_else(|| extract_f32_after(&lower, "width:"));
            let height = extract_f32_after(&lower, "height=")
                .or_else(|| extract_f32_after(&lower, "h="))
                .or_else(|| extract_f32_after(&lower, "height:"));

            let name = Self::extract_quoted(input);

            return ParsedCommand {
                raw_input: input.into(),
                command: AgentCommand::CreateLayer { kind, x, y, width, height, name },
                confidence: 0.9,
                warnings,
            };
        }

        // Parse: set fill / color
        if lower.contains("fill") || lower.contains("color") || lower.contains("colour") {
            if let Some(color) = extract_hex(&lower) {
                let layer_id = Self::extract_quoted(input);
                return ParsedCommand {
                    raw_input: input.into(),
                    command: AgentCommand::SetFill { layer_id, color },
                    confidence: 0.88,
                    warnings,
                };
            }
        }

        // Parse: set opacity
        if lower.contains("opacity") || lower.contains("transparent") {
            // Try to find % or 0.x value
            let opacity = extract_f32_after(&lower, "opacity ")
                .or_else(|| extract_f32_after(&lower, "opacity="))
                .or_else(|| extract_f32_after(&lower, "to "))
                .map(|v| if v > 1.0 { v / 100.0 } else { v })
                .unwrap_or(1.0);

            let layer_id = Self::extract_quoted(input);
            return ParsedCommand {
                raw_input: input.into(),
                command: AgentCommand::SetOpacity { layer_id, opacity },
                confidence: 0.82,
                warnings,
            };
        }

        // Parse: move layer
        if lower.contains("move") || lower.contains("position") {
            let layer_id = Self::extract_quoted(input).unwrap_or_else(|| "selected".to_string());
            let x = extract_f32_after(&lower, "x=")
                .or_else(|| extract_f32_after(&lower, "x:"))
                .unwrap_or(0.0);
            let y = extract_f32_after(&lower, "y=")
                .or_else(|| extract_f32_after(&lower, "y:"))
                .unwrap_or(0.0);
            return ParsedCommand {
                raw_input: input.into(),
                command: AgentCommand::MoveLayer { layer_id, x, y },
                confidence: 0.80,
                warnings,
            };
        }

        // Parse: generate palette
        if lower.contains("palette") || lower.contains("color scheme") || lower.contains("colour scheme") {
            let base = extract_hex(&lower).unwrap_or_else(|| "#3b82f6".to_string());
            let scheme = if lower.contains("complementary") { Some("complementary".into()) }
                else if lower.contains("triadic") { Some("triadic".into()) }
                else if lower.contains("analogous") { Some("analogous".into()) }
                else { None };
            return ParsedCommand {
                raw_input: input.into(),
                command: AgentCommand::GeneratePalette { base_color: base, scheme },
                confidence: 0.85,
                warnings,
            };
        }

        // Parse: accessibility check
        if lower.contains("accessibility") || lower.contains("wcag") || lower.contains("contrast") {
            return ParsedCommand {
                raw_input: input.into(),
                command: AgentCommand::CheckAccessibility,
                confidence: 0.90,
                warnings,
            };
        }

        // Parse: formula
        if lower.contains("formula") || lower.contains("=sum") || lower.contains("=if") || lower.contains("=average") {
            let cell = Self::extract_cell_ref(input).unwrap_or_else(|| "A1".to_string());
            // Extract formula: everything after '='
            let formula = input.find('=')
                .map(|i| input[i..].trim().to_string())
                .unwrap_or_else(|| "=".to_string());
            return ParsedCommand {
                raw_input: input.into(),
                command: AgentCommand::WriteFormula { cell, formula },
                confidence: 0.85,
                warnings,
            };
        }

        // Parse: AI pipeline
        if lower.contains("pipeline") || lower.contains("run ai") || lower.contains("analyze") {
            let pipeline = if lower.contains("accessibility") { "Accessibility".into() }
                else if lower.contains("color") { "ColorHarmony".into() }
                else { "DesignAnalysis".into() };
            return ParsedCommand {
                raw_input: input.into(),
                command: AgentCommand::RunAiPipeline { pipeline },
                confidence: 0.78,
                warnings,
            };
        }

        // Fallback
        ParsedCommand {
            raw_input: input.into(),
            command: AgentCommand::Unknown { raw: input.to_string() },
            confidence: 0.0,
            warnings: vec!["Could not parse command.".into()],
        }
    }

    /// Extract a quoted string like 'btn-1' or "Button".
    fn extract_quoted(input: &str) -> Option<String> {
        for (open, close) in &[('\'', '\''), ('"', '"'), ('`', '`')] {
            if let Some(start) = input.find(*open) {
                if let Some(end) = input[start+1..].find(*close) {
                    return Some(input[start+1..start+1+end].to_string());
                }
            }
        }
        None
    }

    /// Extract a cell reference like A1, B12.
    fn extract_cell_ref(input: &str) -> Option<String> {
        let bytes = input.as_bytes();
        for i in 0..bytes.len().saturating_sub(1) {
            if bytes[i].is_ascii_uppercase() {
                let rest = &input[i..];
                let letters: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
                let digits_start = i + letters.len();
                if digits_start < input.len() {
                    let digits: String = input[digits_start..].chars().take_while(|c| c.is_ascii_digit()).collect();
                    if !digits.is_empty() && letters.len() <= 2 {
                        return Some(format!("{}{}", letters, digits));
                    }
                }
            }
        }
        None
    }
}

// ── Agent API handler ─────────────────────────────────────────────────────────

/// Routes parsed commands to the appropriate action (mock implementation).
pub struct AgentApiHandler;

impl AgentApiHandler {
    pub fn handle(request: &AgentRequest) -> AgentResponse {
        let parsed = CommandParser::parse(&request.command_text);
        let result = Self::execute(&parsed.command);

        let response_data = serde_json::json!({
            "command": format!("{:?}", parsed.command),
            "confidence": parsed.confidence,
            "result": result.message,
            "warnings": parsed.warnings,
        });

        if result.success {
            AgentResponse::ok(&request.request_id, response_data)
        } else {
            AgentResponse::err(&request.request_id, result.message)
        }
    }

    fn execute(cmd: &AgentCommand) -> CommandResult {
        match cmd {
            AgentCommand::CreateLayer { kind, x, y, width, height, name } => {
                CommandResult::ok(format!(
                    "Created {:?} layer '{}' at ({:?},{:?}) size {:?}×{:?}",
                    kind, name.as_deref().unwrap_or("Layer"), x, y, width, height
                ))
            }
            AgentCommand::SetFill { layer_id, color } => {
                CommandResult::ok(format!("Set fill of {:?} to {}", layer_id, color))
            }
            AgentCommand::SetOpacity { layer_id, opacity } => {
                if *opacity > 1.0 || *opacity < 0.0 {
                    return CommandResult::err(format!("Opacity must be 0.0–1.0, got {}", opacity));
                }
                CommandResult::ok(format!("Set opacity of {:?} to {:.0}%", layer_id, opacity * 100.0))
            }
            AgentCommand::DeleteLayer { layer_id } => {
                CommandResult::ok(format!("Deleted layer '{}'", layer_id))
            }
            AgentCommand::MoveLayer { layer_id, x, y } => {
                CommandResult::ok(format!("Moved '{}' to ({}, {})", layer_id, x, y))
            }
            AgentCommand::ResizeLayer { layer_id, width, height } => {
                CommandResult::ok(format!("Resized '{}' to {}×{}", layer_id, width, height))
            }
            AgentCommand::GroupLayers { ids, name } => {
                CommandResult::ok(format!("Grouped {} layers into '{}'",
                    ids.len(), name.as_deref().unwrap_or("Group")))
            }
            AgentCommand::CheckAccessibility => {
                CommandResult::ok_with_data("Accessibility check queued",
                    serde_json::json!({"issues": [], "wcag_level": "AA"}))
            }
            AgentCommand::GeneratePalette { base_color, scheme } => {
                CommandResult::ok(format!("Generating {:?} palette from {}",
                    scheme.as_deref().unwrap_or("complementary"), base_color))
            }
            AgentCommand::WriteFormula { cell, formula } => {
                CommandResult::ok(format!("Wrote formula {} in cell {}", formula, cell))
            }
            AgentCommand::BindCell { layer_id, property, cell } => {
                CommandResult::ok(format!("Bound {}→{} to cell {}", layer_id, property, cell))
            }
            AgentCommand::RunAiPipeline { pipeline } => {
                CommandResult::ok(format!("Running AI pipeline: {}", pipeline))
            }
            AgentCommand::Undo => CommandResult::ok("Undo applied"),
            AgentCommand::Redo => CommandResult::ok("Redo applied"),
            AgentCommand::Help { topic } => {
                let msg = match topic.as_deref() {
                    Some("spreadsheet") => "Spreadsheet: use =FORMULA in cells, @bind(layer,prop,cell) for data binding.",
                    Some("plugins") => "Plugins: call_plugin(id, fn, args). See plugin documentation.",
                    Some("ai") => "AI: analyze_design, check_accessibility, generate_palette, run_pipeline.",
                    _ => "Available topics: layers, styling, spreadsheet, plugins, ai, collaboration.",
                };
                CommandResult::ok(msg)
            }
            AgentCommand::SetStroke { layer_id, color, width } => {
                CommandResult::ok(format!("Set stroke on {:?}: color={:?}, width={:?}", layer_id, color, width))
            }
            AgentCommand::Unknown { raw } => {
                CommandResult::err(format!("Unknown command: {}", raw))
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> ParsedCommand {
        CommandParser::parse(input)
    }

    #[test]
    fn parse_create_rectangle() {
        let p = parse("Create a rectangle at x=10 y=20 width=100 height=50");
        assert!(matches!(p.command, AgentCommand::CreateLayer { kind: LayerKind::Rectangle, .. }));
        assert!(p.confidence > 0.0);
    }

    #[test]
    fn parse_create_extracts_coords() {
        let p = parse("Add a rectangle x=100 y=200 width=300 height=150");
        if let AgentCommand::CreateLayer { x, y, width, height, .. } = p.command {
            assert_eq!(x, Some(100.0));
            assert_eq!(y, Some(200.0));
            assert_eq!(width, Some(300.0));
            assert_eq!(height, Some(150.0));
        } else {
            panic!("Wrong command type");
        }
    }

    #[test]
    fn parse_create_ellipse() {
        let p = parse("Draw a circle at x=50 y=50");
        assert!(matches!(p.command, AgentCommand::CreateLayer { kind: LayerKind::Ellipse, .. }));
    }

    #[test]
    fn parse_set_fill_hex() {
        let p = parse("Set fill to #3b82f6 for layer 'button'");
        assert!(matches!(p.command, AgentCommand::SetFill { .. }));
        if let AgentCommand::SetFill { color, .. } = p.command {
            assert!(color.contains("3b82f6"));
        }
    }

    #[test]
    fn parse_set_fill_named_color() {
        let p = parse("Set the fill color to red");
        assert!(matches!(p.command, AgentCommand::SetFill { color, .. } if color == "#ff0000"));
    }

    #[test]
    fn parse_set_opacity() {
        let p = parse("Set opacity to 0.5 on 'icon'");
        assert!(matches!(p.command, AgentCommand::SetOpacity { opacity, .. } if (opacity - 0.5).abs() < 0.01));
    }

    #[test]
    fn parse_opacity_percentage() {
        let p = parse("Set opacity to 50%");
        if let AgentCommand::SetOpacity { opacity, .. } = p.command {
            assert!((opacity - 0.5).abs() < 0.01, "Expected 0.5, got {}", opacity);
        }
    }

    #[test]
    fn parse_delete_layer() {
        let p = parse("Delete layer 'old-header'");
        assert!(matches!(p.command, AgentCommand::DeleteLayer { layer_id } if layer_id == "old-header"));
    }

    #[test]
    fn parse_move_layer() {
        let p = parse("Move layer 'icon' to x=50 y=100");
        if let AgentCommand::MoveLayer { layer_id, x, y } = p.command {
            assert_eq!(layer_id, "icon");
            assert_eq!(x, 50.0);
            assert_eq!(y, 100.0);
        } else {
            panic!("Expected MoveLayer");
        }
    }

    #[test]
    fn parse_accessibility_check() {
        let p = parse("Check accessibility and contrast");
        assert!(matches!(p.command, AgentCommand::CheckAccessibility));
    }

    #[test]
    fn parse_generate_palette() {
        let p = parse("Generate a complementary palette from #ff5733");
        if let AgentCommand::GeneratePalette { base_color, scheme } = p.command {
            assert!(base_color.contains("ff5733"));
            assert_eq!(scheme, Some("complementary".into()));
        } else {
            panic!("Expected GeneratePalette");
        }
    }

    #[test]
    fn parse_undo() {
        assert!(matches!(parse("undo").command, AgentCommand::Undo));
    }

    #[test]
    fn parse_redo() {
        assert!(matches!(parse("redo").command, AgentCommand::Redo));
    }

    #[test]
    fn parse_help() {
        let p = parse("help spreadsheet");
        assert!(matches!(p.command, AgentCommand::Help { topic: Some(t) } if t == "spreadsheet"));
    }

    #[test]
    fn parse_unknown_returns_low_confidence() {
        let p = parse("xyzzy frob the glork");
        assert_eq!(p.confidence, 0.0);
        assert!(matches!(p.command, AgentCommand::Unknown { .. }));
    }

    #[test]
    fn handler_create_layer_succeeds() {
        let req = AgentRequest::new("s1", "Create a rectangle x=10 y=20 width=100 height=50");
        let resp = AgentApiHandler::handle(&req);
        assert!(resp.success);
    }

    #[test]
    fn handler_unknown_returns_error() {
        let req = AgentRequest::new("s1", "xyzzy frob!");
        let resp = AgentApiHandler::handle(&req);
        assert!(!resp.success);
    }

    #[test]
    fn layer_kind_from_text() {
        assert_eq!(LayerKind::from_text("rect"), Some(LayerKind::Rectangle));
        assert_eq!(LayerKind::from_text("circle"), Some(LayerKind::Ellipse));
        assert_eq!(LayerKind::from_text("label"), Some(LayerKind::Text));
        assert_eq!(LayerKind::from_text("gobbledygook"), None);
    }
}
