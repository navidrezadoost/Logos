//! Plugin SDK scaffolding — templates and starter code generators.
//!
//! Provides the data model for generating new plugin projects from
//! templates. Supports Rust (WASM) and AssemblyScript starters with
//! customizable manifest metadata.

use crate::manifest::{PluginCategory, PluginHook};

// ── Template Kind ────────────────────────────────────────────

/// Supported plugin template languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKind {
    /// Rust → compile to WASM.
    RustWasm,
    /// AssemblyScript → compile to WASM.
    AssemblyScript,
    /// JavaScript (boa engine).
    JavaScript,
}

impl TemplateKind {
    /// File extension for the main entry point.
    pub fn entry_extension(&self) -> &'static str {
        match self {
            Self::RustWasm => "rs",
            Self::AssemblyScript => "ts",
            Self::JavaScript => "js",
        }
    }

    /// Build tool used for this template.
    pub fn build_tool(&self) -> &'static str {
        match self {
            Self::RustWasm => "cargo",
            Self::AssemblyScript => "asc",
            Self::JavaScript => "none",
        }
    }

    /// Label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::RustWasm => "Rust (WASM)",
            Self::AssemblyScript => "AssemblyScript",
            Self::JavaScript => "JavaScript",
        }
    }
}

// ── Scaffold Config ──────────────────────────────────────────

/// Configuration for generating a new plugin project.
#[derive(Debug, Clone)]
pub struct ScaffoldConfig {
    /// Plugin name (alpha-numeric + hyphens).
    pub name: String,
    /// Plugin description.
    pub description: String,
    /// Author name.
    pub author: String,
    /// Template language.
    pub template: TemplateKind,
    /// Plugin category.
    pub category: PluginCategory,
    /// Hooks the plugin will use.
    pub hooks: Vec<PluginHook>,
    /// Whether to include UI panel boilerplate.
    pub include_ui: bool,
    /// Whether to include example host API calls.
    pub include_examples: bool,
}

impl ScaffoldConfig {
    /// Create a minimal scaffold config.
    pub fn new(name: &str, template: TemplateKind) -> Self {
        Self {
            name: name.to_string(),
            description: format!("A Logos plugin: {}", name),
            author: String::new(),
            template,
            category: PluginCategory::Other,
            hooks: vec![PluginHook::OnLoad],
            include_ui: false,
            include_examples: true,
        }
    }

    /// Set the author.
    pub fn with_author(mut self, author: &str) -> Self {
        self.author = author.to_string();
        self
    }

    /// Set the category.
    pub fn with_category(mut self, cat: PluginCategory) -> Self {
        self.category = cat;
        self
    }

    /// Enable UI panel boilerplate.
    pub fn with_ui(mut self) -> Self {
        self.include_ui = true;
        self
    }

    /// Add a hook.
    pub fn with_hook(mut self, hook: PluginHook) -> Self {
        if !self.hooks.contains(&hook) {
            self.hooks.push(hook);
        }
        self
    }
}

// ── Generated File ───────────────────────────────────────────

/// A file to write when scaffolding a new plugin.
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    /// Relative path within the project directory.
    pub path: String,
    /// File contents.
    pub content: String,
}

impl GeneratedFile {
    /// Create a new generated file.
    pub fn new(path: &str, content: &str) -> Self {
        Self {
            path: path.to_string(),
            content: content.to_string(),
        }
    }

    /// File extension.
    pub fn extension(&self) -> &str {
        self.path.rsplit('.').next().unwrap_or("")
    }

    /// Content size in bytes.
    pub fn size(&self) -> usize {
        self.content.len()
    }
}

// ── Plugin Scaffold Generator ────────────────────────────────

/// Generates plugin project files from a scaffold config.
pub struct PluginScaffold;

impl PluginScaffold {
    /// Generate all files for a new plugin project.
    pub fn generate(config: &ScaffoldConfig) -> Vec<GeneratedFile> {
        let mut files = vec![
            Self::generate_manifest(config),
            Self::generate_readme(config),
        ];

        match config.template {
            TemplateKind::RustWasm => {
                files.push(Self::generate_cargo_toml(config));
                files.push(Self::generate_rust_main(config));
            }
            TemplateKind::AssemblyScript => {
                files.push(Self::generate_package_json(config));
                files.push(Self::generate_as_main(config));
            }
            TemplateKind::JavaScript => {
                files.push(Self::generate_js_main(config));
            }
        }

        files
    }

    /// Generate the plugin.toml manifest.
    fn generate_manifest(config: &ScaffoldConfig) -> GeneratedFile {
        let hooks: Vec<String> = config
            .hooks
            .iter()
            .map(|h| format!("\"{}\"", hook_name(h)))
            .collect();
        let hooks_line = hooks.join(", ");

        let entry = format!("src/main.{}", config.template.entry_extension());

        let content = format!(
            r#"[plugin]
id = "{name}"
name = "{name}"
version = "0.1.0"
author = "{author}"
description = "{description}"
entry_point = "{entry}"
category = "{category}"
hooks = [{hooks}]

[permissions]
document_read = true
document_write = false
"#,
            name = config.name,
            author = config.author,
            description = config.description,
            entry = entry,
            category = category_name(&config.category),
            hooks = hooks_line,
        );

        GeneratedFile::new("plugin.toml", &content)
    }

    /// Generate README.md.
    fn generate_readme(config: &ScaffoldConfig) -> GeneratedFile {
        let content = format!(
            "# {name}\n\n{desc}\n\n## Development\n\nBuild: `{tool}`\n\nTemplate: {template}\n",
            name = config.name,
            desc = config.description,
            tool = config.template.build_tool(),
            template = config.template.label(),
        );
        GeneratedFile::new("README.md", &content)
    }

    /// Generate Cargo.toml for Rust plugins.
    fn generate_cargo_toml(config: &ScaffoldConfig) -> GeneratedFile {
        let content = format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
logos-plugin-sdk = "0.1"
"#,
            name = config.name,
        );
        GeneratedFile::new("Cargo.toml", &content)
    }

    /// Generate src/main.rs for Rust plugins.
    fn generate_rust_main(config: &ScaffoldConfig) -> GeneratedFile {
        let mut code = String::from(
            r#"//! Logos plugin: auto-generated from template.
#![no_std]

extern crate alloc;

#[no_mangle]
pub extern "C" fn on_load() -> i32 {
    // Plugin initialization code here
    0
}
"#,
        );

        if config.include_examples {
            code.push_str(
                r#"
#[no_mangle]
pub extern "C" fn on_selection_change() -> i32 {
    // Handle selection changes
    0
}
"#,
            );
        }

        GeneratedFile::new("src/main.rs", &code)
    }

    /// Generate package.json for AssemblyScript plugins.
    fn generate_package_json(config: &ScaffoldConfig) -> GeneratedFile {
        let content = format!(
            r#"{{
  "name": "{name}",
  "version": "0.1.0",
  "scripts": {{
    "build": "asc src/main.ts --outFile build/plugin.wasm --optimize"
  }},
  "devDependencies": {{
    "assemblyscript": "^0.27.0"
  }}
}}
"#,
            name = config.name,
        );
        GeneratedFile::new("package.json", &content)
    }

    /// Generate src/main.ts for AssemblyScript plugins.
    fn generate_as_main(config: &ScaffoldConfig) -> GeneratedFile {
        let mut code = String::from(
            r#"// Logos plugin: auto-generated from template.

export function on_load(): i32 {
  // Plugin initialization
  return 0;
}
"#,
        );

        if config.include_examples {
            code.push_str(
                r#"
export function on_selection_change(): i32 {
  // Handle selection changes
  return 0;
}
"#,
            );
        }

        GeneratedFile::new("src/main.ts", &code)
    }

    /// Generate src/main.js for JavaScript plugins.
    fn generate_js_main(config: &ScaffoldConfig) -> GeneratedFile {
        let mut code = format!(
            "// Logos plugin: {}\n\nLogos.on('load', () => {{\n  Logos.log('Plugin loaded');\n}});\n",
            config.name,
        );

        if config.include_examples {
            code.push_str(
                "\nLogos.on('selectionChange', () => {\n  const layers = Logos.getLayers();\n  Logos.log(`Selected: ${layers.length} layers`);\n});\n",
            );
        }

        GeneratedFile::new("src/main.js", &code)
    }
}

fn hook_name(hook: &PluginHook) -> &'static str {
    match hook {
        PluginHook::OnLoad => "on_load",
        PluginHook::OnSave => "on_save",
        PluginHook::OnSelectionChange => "on_selection_change",
        PluginHook::OnFrame => "on_frame",
        PluginHook::OnLayerCreate => "on_layer_create",
        PluginHook::OnLayerDelete => "on_layer_delete",
        PluginHook::OnExport => "on_export",
    }
}

fn category_name(cat: &PluginCategory) -> &'static str {
    match cat {
        PluginCategory::Layout => "layout",
        PluginCategory::Color => "color",
        PluginCategory::Typography => "typography",
        PluginCategory::Export => "export",
        PluginCategory::Accessibility => "accessibility",
        PluginCategory::Animation => "animation",
        PluginCategory::Collaboration => "collaboration",
        PluginCategory::DevTools => "devtools",
        PluginCategory::Assets => "assets",
        PluginCategory::Other => "other",
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_kind_extensions() {
        assert_eq!(TemplateKind::RustWasm.entry_extension(), "rs");
        assert_eq!(TemplateKind::AssemblyScript.entry_extension(), "ts");
        assert_eq!(TemplateKind::JavaScript.entry_extension(), "js");
    }

    #[test]
    fn template_kind_build_tools() {
        assert_eq!(TemplateKind::RustWasm.build_tool(), "cargo");
        assert_eq!(TemplateKind::AssemblyScript.build_tool(), "asc");
        assert_eq!(TemplateKind::JavaScript.build_tool(), "none");
    }

    #[test]
    fn scaffold_config_defaults() {
        let cfg = ScaffoldConfig::new("my-plugin", TemplateKind::RustWasm);
        assert_eq!(cfg.name, "my-plugin");
        assert_eq!(cfg.category, PluginCategory::Other);
        assert!(cfg.include_examples);
        assert!(!cfg.include_ui);
        assert_eq!(cfg.hooks.len(), 1);
    }

    #[test]
    fn scaffold_config_builder() {
        let cfg = ScaffoldConfig::new("test", TemplateKind::JavaScript)
            .with_author("Alice")
            .with_category(PluginCategory::Layout)
            .with_ui()
            .with_hook(PluginHook::OnSave);

        assert_eq!(cfg.author, "Alice");
        assert_eq!(cfg.category, PluginCategory::Layout);
        assert!(cfg.include_ui);
        assert_eq!(cfg.hooks.len(), 2);
    }

    #[test]
    fn scaffold_config_no_duplicate_hooks() {
        let cfg = ScaffoldConfig::new("test", TemplateKind::RustWasm)
            .with_hook(PluginHook::OnLoad) // already present
            .with_hook(PluginHook::OnLoad);
        assert_eq!(cfg.hooks.len(), 1);
    }

    #[test]
    fn generate_rust_project() {
        let cfg = ScaffoldConfig::new("auto-align", TemplateKind::RustWasm)
            .with_author("Dev")
            .with_category(PluginCategory::Layout);

        let files = PluginScaffold::generate(&cfg);
        assert_eq!(files.len(), 4); // manifest, readme, Cargo.toml, main.rs

        let manifest = files.iter().find(|f| f.path == "plugin.toml").unwrap();
        assert!(manifest.content.contains("auto-align"));
        assert!(manifest.content.contains("layout"));

        let cargo = files.iter().find(|f| f.path == "Cargo.toml").unwrap();
        assert!(cargo.content.contains("cdylib"));

        let main = files.iter().find(|f| f.path == "src/main.rs").unwrap();
        assert!(main.content.contains("on_load"));
    }

    #[test]
    fn generate_javascript_project() {
        let cfg = ScaffoldConfig::new("quick-export", TemplateKind::JavaScript);
        let files = PluginScaffold::generate(&cfg);
        assert_eq!(files.len(), 3); // manifest, readme, main.js (no package.json)

        let main = files.iter().find(|f| f.path == "src/main.js").unwrap();
        assert!(main.content.contains("Logos.on"));
    }

    #[test]
    fn generate_assemblyscript_project() {
        let cfg = ScaffoldConfig::new("my-plugin", TemplateKind::AssemblyScript);
        let files = PluginScaffold::generate(&cfg);
        assert_eq!(files.len(), 4); // manifest, readme, package.json, main.ts

        let pkg = files.iter().find(|f| f.path == "package.json").unwrap();
        assert!(pkg.content.contains("assemblyscript"));
    }

    #[test]
    fn generated_file_properties() {
        let f = GeneratedFile::new("src/main.rs", "fn main() {}");
        assert_eq!(f.extension(), "rs");
        assert_eq!(f.size(), 12);
    }
}
