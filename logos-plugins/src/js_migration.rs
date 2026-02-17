//! # JavaScript-to-WASM Migration Layer
//!
//! Deprecates the Boa ES2023 engine in favor of routing all JavaScript
//! plugins through the Wasmtime-based WASM runtime. This provides:
//!
//! 1. **Unified runtime**: Single execution engine (Wasmtime) instead of two
//! 2. **Better sandboxing**: Wasmtime's capability-based security > Boa's
//! 3. **Performance**: Cranelift JIT > interpreter for hot paths
//! 4. **Maintenance**: One runtime to audit, fuzz, and upgrade
//!
//! ## Migration Strategy (Dragon Book, Ch. 8: Code Generation)
//!
//! ```text
//! .js plugin ──► JsCompiler  ──► QuickJS WASM ──► WasmRuntime
//!                (precompile)    (embedded engine)  (Wasmtime 29)
//! ```
//!
//! Instead of shipping a JS→native compiler, we embed a JavaScript engine
//! *compiled to WASM* (e.g., QuickJS). The `.js` source is passed as data
//! to the QuickJS WASM module, which evaluates it inside the sandbox.
//!
//! This means:
//! - All `.js` plugins run inside Wasmtime (not Boa)
//! - Host functions are the same WASM imports (`logos.*`)
//! - Fuel metering and memory limits apply uniformly
//! - Boa and boa_gc dependencies can be removed entirely

use crate::manifest::PluginManifest;
use crate::runtime::{ResourceLimits, RuntimeError, RuntimeResult};
use crate::permissions::PermissionSet;

// ═══════════════════════════════════════════════════════════════════
// Deprecation notices
// ═══════════════════════════════════════════════════════════════════

/// Deprecation status for the Boa JavaScript engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeprecationStatus {
    /// Active but deprecated — will be removed.
    Deprecated {
        since: &'static str,
        removal_target: &'static str,
        replacement: &'static str,
    },
    /// Fully removed.
    Removed { since: &'static str },
}

/// Get the current deprecation status of the Boa JS engine.
pub fn boa_deprecation_status() -> DeprecationStatus {
    DeprecationStatus::Deprecated {
        since: "0.2.0",
        removal_target: "0.3.0",
        replacement: "WASM runtime (Wasmtime) with embedded JS engine",
    }
}

// ═══════════════════════════════════════════════════════════════════
// JS-to-WASM compilation bridge
// ═══════════════════════════════════════════════════════════════════

/// Wraps JavaScript source code for execution within a WASM-hosted
/// JavaScript engine (QuickJS or similar compiled to WASM).
///
/// ## How it works
///
/// 1. The `.js` source is serialized into the WASM module's linear memory
/// 2. The embedded JS engine (inside WASM) evaluates it
/// 3. Host function calls (`Logos.*`) go through the standard WASM import
///    mechanism, not through Boa's `NativeFunction` interface
///
/// This provides identical sandboxing to native `.wasm` plugins.
pub struct JsWasmBridge {
    /// Plugin name for diagnostics.
    name: String,
    /// Resource limits applied to the WASM runtime.
    limits: ResourceLimits,
    /// Permissions for this plugin.
    permissions: PermissionSet,
    /// Pre-processed JS source (minimized, validated).
    source: Option<String>,
    /// Whether the JS source has been validated.
    validated: bool,
    /// Compilation stats.
    stats: CompilationStats,
}

/// Statistics from JS→WASM compilation/bridging.
#[derive(Debug, Clone, Default)]
pub struct CompilationStats {
    /// Original JS source size in bytes.
    pub source_bytes: usize,
    /// Time to validate the JS source (microseconds).
    pub validation_time_us: u64,
    /// Time to prepare WASM module (microseconds).
    pub preparation_time_us: u64,
    /// Number of detected host API calls.
    pub host_api_calls: usize,
    /// Number of detected `Logos.*` references.
    pub logos_api_refs: usize,
}

impl JsWasmBridge {
    /// Create a new bridge for a JS plugin.
    pub fn new(name: &str, limits: ResourceLimits, permissions: PermissionSet) -> Self {
        Self {
            name: name.to_string(),
            limits,
            permissions,
            source: None,
            validated: false,
            stats: CompilationStats::default(),
        }
    }

    /// Load and validate JavaScript source code.
    ///
    /// Performs static analysis to detect:
    /// - Syntax errors (basic bracket/brace matching)
    /// - `Logos.*` API usage (for permission checking)
    /// - Forbidden patterns (eval, Function constructor, etc.)
    pub fn load_source(&mut self, source: &str) -> RuntimeResult<&CompilationStats> {
        let start = std::time::Instant::now();

        self.stats.source_bytes = source.len();

        // Basic validation
        if source.is_empty() {
            return Err(RuntimeError::CompileError(
                "empty JavaScript source".to_string(),
            ));
        }

        // Count Logos API references
        self.stats.logos_api_refs = count_pattern(source, "Logos.");
        self.stats.host_api_calls = count_pattern(source, "Logos.get")
            + count_pattern(source, "Logos.create")
            + count_pattern(source, "Logos.delete")
            + count_pattern(source, "Logos.set")
            + count_pattern(source, "Logos.on");

        // Check for dangerous patterns
        let forbidden = ["eval(", "new Function(", "import(", "require("];
        for pattern in &forbidden {
            if source.contains(pattern) {
                return Err(RuntimeError::CompileError(format!(
                    "forbidden pattern detected: `{pattern}` — use Logos.* API instead",
                )));
            }
        }

        // Bracket matching (basic syntax check)
        let balanced = check_brackets(source);
        if !balanced {
            return Err(RuntimeError::CompileError(
                "unbalanced brackets in JavaScript source".to_string(),
            ));
        }

        self.stats.validation_time_us = start.elapsed().as_micros() as u64;
        self.source = Some(source.to_string());
        self.validated = true;

        Ok(&self.stats)
    }

    /// Get the validated source code.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Whether the source has been validated.
    pub fn is_validated(&self) -> bool {
        self.validated
    }

    /// Get the plugin name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get compilation/validation stats.
    pub fn stats(&self) -> &CompilationStats {
        &self.stats
    }

    /// Get resource limits.
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Get permissions.
    pub fn permissions(&self) -> &PermissionSet {
        &self.permissions
    }

    /// Generate a WASM-compatible wrapper that bootstraps the JS source
    /// inside an embedded JS engine.
    ///
    /// The wrapper:
    /// 1. Allocates the JS source string in WASM linear memory
    /// 2. Calls the embedded JS engine's `eval()` function
    /// 3. Marshals host function calls through WASM imports
    ///
    /// Returns the wrapper data that can be passed to WasmRuntime.
    pub fn prepare_wasm_payload(&mut self) -> RuntimeResult<WasmPayload> {
        let start = std::time::Instant::now();

        let source = self
            .source
            .as_ref()
            .ok_or_else(|| RuntimeError::CompileError("no source loaded".to_string()))?;

        if !self.validated {
            return Err(RuntimeError::CompileError(
                "source not validated".to_string(),
            ));
        }

        // The payload contains the JS source + metadata for the WASM-hosted
        // JS engine to evaluate
        let payload = WasmPayload {
            js_source: source.clone(),
            plugin_name: self.name.clone(),
            host_api_calls: self.stats.host_api_calls,
            logos_api_refs: self.stats.logos_api_refs,
            fuel_limit: self.limits.max_execution_time.as_micros() as u64 * 100,
            memory_limit_bytes: self.limits.max_memory_bytes,
        };

        self.stats.preparation_time_us = start.elapsed().as_micros() as u64;
        Ok(payload)
    }
}

/// Payload for the WASM-hosted JS engine.
#[derive(Debug, Clone)]
pub struct WasmPayload {
    /// The JavaScript source code to evaluate.
    pub js_source: String,
    /// Plugin name for diagnostics.
    pub plugin_name: String,
    /// Number of detected host API calls.
    pub host_api_calls: usize,
    /// Number of detected `Logos.*` references.
    pub logos_api_refs: usize,
    /// Fuel limit for Wasmtime.
    pub fuel_limit: u64,
    /// Memory limit in bytes.
    pub memory_limit_bytes: usize,
}

impl WasmPayload {
    /// Serialize the payload to JSON for passing to the WASM module.
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"source":"{}","plugin":"{}","host_calls":{},"api_refs":{},"fuel":{},"memory":{}}}"#,
            escape_json_string(&self.js_source),
            escape_json_string(&self.plugin_name),
            self.host_api_calls,
            self.logos_api_refs,
            self.fuel_limit,
            self.memory_limit_bytes,
        )
    }

    /// Estimated WASM memory needed (source + overhead).
    pub fn estimated_memory(&self) -> usize {
        self.js_source.len() + 1024 * 1024 // source + 1MB overhead for QuickJS
    }
}

// ═══════════════════════════════════════════════════════════════════
// Migration coordinator
// ═══════════════════════════════════════════════════════════════════

/// Result of analyzing a plugin manifest for migration readiness.
#[derive(Debug, Clone)]
pub struct MigrationAnalysis {
    /// Plugin ID.
    pub plugin_id: String,
    /// Plugin name.
    pub plugin_name: String,
    /// Current runtime type.
    pub current_runtime: String,
    /// Whether the plugin can be migrated to WASM.
    pub can_migrate: bool,
    /// Reasons migration might not work.
    pub blockers: Vec<String>,
    /// Warnings (non-blocking issues).
    pub warnings: Vec<String>,
    /// Detected Logos API usage.
    pub api_usage: Vec<String>,
}

/// Analyze a plugin manifest to determine if it can be migrated from
/// Boa JS to the WASM runtime.
pub fn analyze_migration(manifest: &PluginManifest) -> MigrationAnalysis {
    let is_js = manifest.entry_point.ends_with(".js");
    let is_wasm = manifest.entry_point.ends_with(".wasm");
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    let current_runtime = if is_wasm {
        "wasm".to_string()
    } else if is_js {
        "javascript (boa)".to_string()
    } else {
        "sandbox".to_string()
    };

    if !is_js {
        blockers.push("not a JavaScript plugin — no migration needed".to_string());
    }

    if manifest.hooks.is_empty() && manifest.commands.is_empty() {
        warnings.push("plugin has no hooks or commands — may be a library plugin".to_string());
    }

    if manifest.max_execution_time.is_none() {
        warnings.push("no execution time limit set — WASM runtime will apply default".to_string());
    }

    MigrationAnalysis {
        plugin_id: manifest.id.to_string(),
        plugin_name: manifest.name.clone(),
        current_runtime,
        can_migrate: is_js && blockers.is_empty(),
        blockers,
        warnings,
        api_usage: Vec::new(),
    }
}

/// Batch-analyze all plugins for migration readiness.
pub fn analyze_all_migrations(
    manifests: &[PluginManifest],
) -> Vec<MigrationAnalysis> {
    manifests.iter().map(|m| analyze_migration(m)).collect()
}

/// Summary of a migration batch analysis.
#[derive(Debug, Clone)]
pub struct MigrationSummary {
    pub total_plugins: usize,
    pub js_plugins: usize,
    pub wasm_plugins: usize,
    pub sandbox_plugins: usize,
    pub can_migrate: usize,
    pub blocked: usize,
}

/// Produce a summary from a batch analysis.
pub fn summarize_migrations(analyses: &[MigrationAnalysis]) -> MigrationSummary {
    let mut summary = MigrationSummary {
        total_plugins: analyses.len(),
        js_plugins: 0,
        wasm_plugins: 0,
        sandbox_plugins: 0,
        can_migrate: 0,
        blocked: 0,
    };
    for a in analyses {
        match a.current_runtime.as_str() {
            r if r.starts_with("javascript") => summary.js_plugins += 1,
            "wasm" => summary.wasm_plugins += 1,
            _ => summary.sandbox_plugins += 1,
        }
        if a.can_migrate {
            summary.can_migrate += 1;
        } else if a.current_runtime.starts_with("javascript") {
            summary.blocked += 1;
        }
    }
    summary
}

// ═══════════════════════════════════════════════════════════════════
// Utilities
// ═══════════════════════════════════════════════════════════════════

/// Count non-overlapping occurrences of `pattern` in `text`.
fn count_pattern(text: &str, pattern: &str) -> usize {
    text.matches(pattern).count()
}

/// Basic bracket balance check.
fn check_brackets(source: &str) -> bool {
    let mut stack = Vec::new();
    let mut in_string = false;
    let mut string_char = ' ';
    let mut prev = ' ';

    for ch in source.chars() {
        if in_string {
            if ch == string_char && prev != '\\' {
                in_string = false;
            }
            prev = ch;
            continue;
        }
        match ch {
            '"' | '\'' | '`' => {
                in_string = true;
                string_char = ch;
            }
            '(' | '[' | '{' => stack.push(ch),
            ')' => {
                if stack.pop() != Some('(') {
                    return false;
                }
            }
            ']' => {
                if stack.pop() != Some('[') {
                    return false;
                }
            }
            '}' => {
                if stack.pop() != Some('{') {
                    return false;
                }
            }
            _ => {}
        }
        prev = ch;
    }
    stack.is_empty()
}

/// Escape a string for JSON embedding.
fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::SemVer;
    use std::collections::HashMap;
    use std::time::Duration;

    fn test_manifest(entry: &str) -> PluginManifest {
        PluginManifest {
            id: uuid::Uuid::new_v4(),
            name: "test-plugin".to_string(),
            version: SemVer::new(1, 0, 0),
            description: "test".to_string(),
            author: "test".to_string(),
            entry_point: entry.to_string(),
            permissions: PermissionSet::none(),
            min_logos_version: SemVer::new(0, 1, 0),
            hooks: Vec::new(),
            commands: Vec::new(),
            tags: Vec::new(),
            icon: None,
            homepage: None,
            max_execution_time: Some(Duration::from_millis(100)),
            ui_entry_point: None,
            category: crate::manifest::PluginCategory::Other,
            license: None,
            repository: None,
            icons: HashMap::new(),
        }
    }

    // ── DeprecationStatus ───────────────────────────────────────

    #[test]
    fn test_boa_deprecation_status() {
        let status = boa_deprecation_status();
        match status {
            DeprecationStatus::Deprecated {
                since,
                removal_target,
                replacement,
            } => {
                assert_eq!(since, "0.2.0");
                assert_eq!(removal_target, "0.3.0");
                assert!(replacement.contains("WASM"));
            }
            _ => panic!("expected Deprecated status"),
        }
    }

    // ── JsWasmBridge ────────────────────────────────────────────

    #[test]
    fn test_bridge_new() {
        let bridge = JsWasmBridge::new(
            "test",
            ResourceLimits::default(),
            PermissionSet::none(),
        );
        assert_eq!(bridge.name(), "test");
        assert!(!bridge.is_validated());
        assert!(bridge.source().is_none());
    }

    #[test]
    fn test_bridge_load_source() {
        let mut bridge = JsWasmBridge::new(
            "test",
            ResourceLimits::default(),
            PermissionSet::none(),
        );
        let source = r#"
            const layers = Logos.getLayers();
            Logos.createRect(10, 20, 100, 50);
            console.log("done");
        "#;
        let stats = bridge.load_source(source).unwrap();
        assert!(stats.source_bytes > 0);
        assert_eq!(stats.logos_api_refs, 2);
        assert!(stats.host_api_calls > 0);
        assert!(bridge.is_validated());
    }

    #[test]
    fn test_bridge_empty_source() {
        let mut bridge = JsWasmBridge::new(
            "test",
            ResourceLimits::default(),
            PermissionSet::none(),
        );
        let result = bridge.load_source("");
        assert!(result.is_err());
    }

    #[test]
    fn test_bridge_forbidden_eval() {
        let mut bridge = JsWasmBridge::new(
            "test",
            ResourceLimits::default(),
            PermissionSet::none(),
        );
        let result = bridge.load_source("eval('alert(1)')");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("forbidden pattern"));
    }

    #[test]
    fn test_bridge_forbidden_function_constructor() {
        let mut bridge = JsWasmBridge::new(
            "test",
            ResourceLimits::default(),
            PermissionSet::none(),
        );
        let result = bridge.load_source("const f = new Function('return 1')");
        assert!(result.is_err());
    }

    #[test]
    fn test_bridge_unbalanced_brackets() {
        let mut bridge = JsWasmBridge::new(
            "test",
            ResourceLimits::default(),
            PermissionSet::none(),
        );
        let result = bridge.load_source("function foo() {");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("unbalanced"));
    }

    #[test]
    fn test_bridge_prepare_payload() {
        let mut bridge = JsWasmBridge::new(
            "test",
            ResourceLimits::default(),
            PermissionSet::none(),
        );
        bridge
            .load_source("Logos.createRect(0, 0, 100, 100)")
            .unwrap();
        let payload = bridge.prepare_wasm_payload().unwrap();
        assert!(!payload.js_source.is_empty());
        assert_eq!(payload.plugin_name, "test");
        assert!(payload.logos_api_refs >= 1);
    }

    #[test]
    fn test_bridge_prepare_without_validation() {
        let mut bridge = JsWasmBridge::new(
            "test",
            ResourceLimits::default(),
            PermissionSet::none(),
        );
        let result = bridge.prepare_wasm_payload();
        assert!(result.is_err());
    }

    // ── WasmPayload ────────────────────────────────────────────

    #[test]
    fn test_wasm_payload_to_json() {
        let payload = WasmPayload {
            js_source: "console.log(\"hello\")".to_string(),
            plugin_name: "test".to_string(),
            host_api_calls: 3,
            logos_api_refs: 5,
            fuel_limit: 100000,
            memory_limit_bytes: 50 * 1024 * 1024,
        };
        let json = payload.to_json();
        assert!(json.contains("source"));
        assert!(json.contains("hello"));
        assert!(json.contains("100000"));
    }

    #[test]
    fn test_wasm_payload_estimated_memory() {
        let payload = WasmPayload {
            js_source: "x".repeat(1000),
            plugin_name: "test".to_string(),
            host_api_calls: 0,
            logos_api_refs: 0,
            fuel_limit: 0,
            memory_limit_bytes: 0,
        };
        assert_eq!(payload.estimated_memory(), 1000 + 1024 * 1024);
    }

    // ── MigrationAnalysis ───────────────────────────────────────

    #[test]
    fn test_analyze_js_plugin() {
        let manifest = test_manifest("plugin.js");
        let analysis = analyze_migration(&manifest);
        assert!(analysis.can_migrate);
        assert_eq!(analysis.current_runtime, "javascript (boa)");
        assert!(analysis.blockers.is_empty());
    }

    #[test]
    fn test_analyze_wasm_plugin() {
        let manifest = test_manifest("plugin.wasm");
        let analysis = analyze_migration(&manifest);
        assert!(!analysis.can_migrate);
        assert_eq!(analysis.current_runtime, "wasm");
    }

    #[test]
    fn test_analyze_sandbox_plugin() {
        let manifest = test_manifest("plugin.expr");
        let analysis = analyze_migration(&manifest);
        assert!(!analysis.can_migrate);
        assert_eq!(analysis.current_runtime, "sandbox");
    }

    #[test]
    fn test_analyze_all_migrations() {
        let manifests = vec![
            test_manifest("a.js"),
            test_manifest("b.wasm"),
            test_manifest("c.js"),
            test_manifest("d.expr"),
        ];
        let analyses = analyze_all_migrations(&manifests);
        assert_eq!(analyses.len(), 4);
        let summary = summarize_migrations(&analyses);
        assert_eq!(summary.total_plugins, 4);
        assert_eq!(summary.js_plugins, 2);
        assert_eq!(summary.wasm_plugins, 1);
        assert_eq!(summary.sandbox_plugins, 1);
        assert_eq!(summary.can_migrate, 2);
    }

    // ── Utilities ───────────────────────────────────────────────

    #[test]
    fn test_count_pattern() {
        assert_eq!(count_pattern("Logos.a Logos.b Logos.c", "Logos."), 3);
        assert_eq!(count_pattern("no matches here", "Logos."), 0);
    }

    #[test]
    fn test_check_brackets_balanced() {
        assert!(check_brackets("function foo() { return [1, 2]; }"));
        assert!(check_brackets("()[]{}"));
        assert!(check_brackets(""));
    }

    #[test]
    fn test_check_brackets_unbalanced() {
        assert!(!check_brackets("{"));
        assert!(!check_brackets("(]}"));
        assert!(!check_brackets("function() {"));
    }

    #[test]
    fn test_check_brackets_in_strings() {
        assert!(check_brackets(r#"const s = "unbalanced { here";"#));
        assert!(check_brackets("const s = 'open ( paren';"));
    }

    #[test]
    fn test_escape_json_string() {
        assert_eq!(escape_json_string("hello"), "hello");
        assert_eq!(escape_json_string("he\"llo"), "he\\\"llo");
        assert_eq!(escape_json_string("a\nb"), "a\\nb");
        assert_eq!(escape_json_string("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_compilation_stats_default() {
        let stats = CompilationStats::default();
        assert_eq!(stats.source_bytes, 0);
        assert_eq!(stats.logos_api_refs, 0);
        assert_eq!(stats.host_api_calls, 0);
    }

    // ── MigrationSummary ────────────────────────────────────────

    #[test]
    fn test_migration_summary_empty() {
        let summary = summarize_migrations(&[]);
        assert_eq!(summary.total_plugins, 0);
        assert_eq!(summary.js_plugins, 0);
    }
}
