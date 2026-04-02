//! Sandbox environment — isolated runtime for safe agent testing.
//!
//! `SandboxEnv` provides a fully hermetic execution context:
//! - Mock file system with read/write/delete operations
//! - In-memory canvas state (layers)
//! - Clipboard (copy/paste)
//! - Configurable resource limits (CPU time, memory, network policy)
//!
//! No real I/O escapes the sandbox.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SandboxError {
    #[error("file not found: {0}")]
    FileNotFound(String),

    #[error("file already exists: {0}")]
    FileAlreadyExists(String),

    #[error("layer not found: {0}")]
    LayerNotFound(String),

    #[error("resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("network access denied — sandbox is offline")]
    NetworkDenied,

    #[error("sandbox already finalised")]
    AlreadyFinalised,

    #[error("operation failed: {0}")]
    OperationFailed(String),
}

pub type SandboxResult<T> = Result<T, SandboxError>;

// ── Resource limits ───────────────────────────────────────────────────────────

/// Configurable hard limits applied during a sandbox run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum wall-clock execute time allowed (milliseconds).
    pub max_duration_ms: u64,
    /// Maximum synthetic memory allowed (bytes; 0 = unlimited).
    pub max_memory_bytes: usize,
    /// Whether outbound network calls are blocked.
    pub network_blocked: bool,
    /// Maximum number of file-system operations per run.
    pub max_fs_ops: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_duration_ms: 10_000,
            max_memory_bytes: 64 * 1024 * 1024, // 64 MiB
            network_blocked: true,
            max_fs_ops: 1_000,
        }
    }
}

impl ResourceLimits {
    pub fn lenient() -> Self {
        Self {
            max_duration_ms: 60_000,
            max_memory_bytes: 256 * 1024 * 1024,
            network_blocked: false,
            max_fs_ops: 10_000,
        }
    }

    pub fn strict() -> Self {
        Self {
            max_duration_ms: 2_000,
            max_memory_bytes: 8 * 1024 * 1024,
            network_blocked: true,
            max_fs_ops: 100,
        }
    }
}

// ── Mock file system ──────────────────────────────────────────────────────────

/// In-memory file system with path → bytes mapping.
#[derive(Debug, Default, Clone)]
pub struct MockFileSystem {
    files: HashMap<String, Vec<u8>>,
    op_count: u32,
}

impl MockFileSystem {
    pub fn new() -> Self {
        Self::default()
    }

    /// Write (or overwrite) a file.
    pub fn write(&mut self, path: impl Into<String>, data: Vec<u8>) -> SandboxResult<()> {
        self.op_count += 1;
        self.files.insert(path.into(), data);
        Ok(())
    }

    /// Read a file's bytes.
    pub fn read(&mut self, path: &str) -> SandboxResult<&[u8]> {
        self.op_count += 1;
        self.files
            .get(path)
            .map(|v| v.as_slice())
            .ok_or_else(|| SandboxError::FileNotFound(path.to_string()))
    }

    /// Delete a file.
    pub fn delete(&mut self, path: &str) -> SandboxResult<()> {
        self.op_count += 1;
        if self.files.remove(path).is_none() {
            return Err(SandboxError::FileNotFound(path.to_string()));
        }
        Ok(())
    }

    pub fn exists(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn total_bytes(&self) -> usize {
        self.files.values().map(|v| v.len()).sum()
    }

    pub fn op_count(&self) -> u32 {
        self.op_count
    }

    pub fn list_files(&self) -> Vec<&str> {
        let mut paths: Vec<&str> = self.files.keys().map(|k| k.as_str()).collect();
        paths.sort();
        paths
    }
}

// ── Canvas layer ──────────────────────────────────────────────────────────────

/// A minimal representation of a design layer in the mock canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasLayer {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub fill: Option<String>,
    pub opacity: f32,
    pub visible: bool,
    pub locked: bool,
    pub properties: HashMap<String, String>,
}

impl CanvasLayer {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        kind: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: kind.into(),
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            fill: None,
            opacity: 1.0,
            visible: true,
            locked: false,
            properties: HashMap::new(),
        }
    }

    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.x = x;
        self.y = y;
        self
    }

    pub fn with_size(mut self, w: f32, h: f32) -> Self {
        self.width = w;
        self.height = h;
        self
    }

    pub fn with_fill(mut self, fill: impl Into<String>) -> Self {
        self.fill = Some(fill.into());
        self
    }

    pub fn set_property(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.properties.insert(key.into(), value.into());
    }
}

// ── Canvas state ──────────────────────────────────────────────────────────────

/// Mock canvas holding all layers in insertion order.
#[derive(Debug, Default, Clone)]
pub struct CanvasState {
    layers: Vec<CanvasLayer>,
    selected_ids: Vec<String>,
    viewport_x: f32,
    viewport_y: f32,
    zoom: f32,
}

impl CanvasState {
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            ..Default::default()
        }
    }

    pub fn add_layer(&mut self, layer: CanvasLayer) {
        self.layers.push(layer);
    }

    pub fn remove_layer(&mut self, id: &str) -> SandboxResult<()> {
        let pos = self
            .layers
            .iter()
            .position(|l| l.id == id)
            .ok_or_else(|| SandboxError::LayerNotFound(id.to_string()))?;
        self.layers.remove(pos);
        self.selected_ids.retain(|s| s != id);
        Ok(())
    }

    pub fn find_layer(&self, id: &str) -> Option<&CanvasLayer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn find_layer_mut(&mut self, id: &str) -> Option<&mut CanvasLayer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    pub fn select(&mut self, ids: &[&str]) {
        self.selected_ids = ids.iter().map(|s| s.to_string()).collect();
    }

    pub fn deselect_all(&mut self) {
        self.selected_ids.clear();
    }

    pub fn selected_ids(&self) -> &[String] {
        &self.selected_ids
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(0.01, 64.0);
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn set_viewport(&mut self, x: f32, y: f32) {
        self.viewport_x = x;
        self.viewport_y = y;
    }

    pub fn viewport(&self) -> (f32, f32) {
        (self.viewport_x, self.viewport_y)
    }

    pub fn layers(&self) -> &[CanvasLayer] {
        &self.layers
    }
}

// ── Clipboard ─────────────────────────────────────────────────────────────────

/// Simple text and structured clipboard for mock copy/paste operations.
#[derive(Debug, Default, Clone)]
pub struct Clipboard {
    text_contents: Option<String>,
    layer_data: Option<Vec<CanvasLayer>>,
}

impl Clipboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn copy_text(&mut self, text: impl Into<String>) {
        self.text_contents = Some(text.into());
        self.layer_data = None;
    }

    pub fn copy_layers(&mut self, layers: Vec<CanvasLayer>) {
        self.layer_data = Some(layers);
        self.text_contents = None;
    }

    pub fn paste_text(&self) -> Option<&str> {
        self.text_contents.as_deref()
    }

    pub fn paste_layers(&self) -> Option<&[CanvasLayer]> {
        self.layer_data.as_deref()
    }

    pub fn is_empty(&self) -> bool {
        self.text_contents.is_none() && self.layer_data.is_none()
    }

    pub fn clear(&mut self) {
        self.text_contents = None;
        self.layer_data = None;
    }
}

// ── Sandbox environment ───────────────────────────────────────────────────────

/// The top-level isolated sandbox context.
///
/// All mock sub-systems are contained here. The sandbox tracks total operation
/// counts and wall-clock time.
#[derive(Debug)]
pub struct SandboxEnv {
    pub id: String,
    pub fs: MockFileSystem,
    pub canvas: CanvasState,
    pub clipboard: Clipboard,
    pub limits: ResourceLimits,
    created_ts: u64,
    finalised: bool,
    op_log: Vec<String>,
}

impl SandboxEnv {
    /// Create a new sandbox with default resource limits.
    pub fn new(id: impl Into<String>) -> Self {
        Self::with_limits(id, ResourceLimits::default())
    }

    /// Create a new sandbox with custom resource limits.
    pub fn with_limits(id: impl Into<String>, limits: ResourceLimits) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id: id.into(),
            fs: MockFileSystem::new(),
            canvas: CanvasState::new(),
            clipboard: Clipboard::new(),
            limits,
            created_ts: ts,
            finalised: false,
            op_log: Vec::new(),
        }
    }

    /// Log an operation description (for diagnostics/reporting).
    pub fn log_op(&mut self, op: impl Into<String>) {
        if !self.finalised {
            self.op_log.push(op.into());
        }
    }

    pub fn op_log(&self) -> &[String] {
        &self.op_log
    }

    pub fn op_count(&self) -> usize {
        self.op_log.len()
    }

    pub fn created_ts(&self) -> u64 {
        self.created_ts
    }

    /// Mark the sandbox as done — no further logging.
    pub fn finalise(&mut self) -> SandboxResult<()> {
        if self.finalised {
            return Err(SandboxError::AlreadyFinalised);
        }
        self.finalised = true;
        Ok(())
    }

    pub fn is_finalised(&self) -> bool {
        self.finalised
    }

    /// Check whether the sandbox has exceeded its file-system op limit.
    pub fn check_fs_limit(&self) -> SandboxResult<()> {
        if self.fs.op_count() >= self.limits.max_fs_ops {
            Err(SandboxError::ResourceLimitExceeded(format!(
                "fs_ops {} ≥ limit {}",
                self.fs.op_count(),
                self.limits.max_fs_ops
            )))
        } else {
            Ok(())
        }
    }

    /// Reset all mutable state (fs, canvas, clipboard, log) while keeping limits.
    pub fn reset(&mut self) {
        self.fs = MockFileSystem::new();
        self.canvas = CanvasState::new();
        self.clipboard = Clipboard::new();
        self.op_log.clear();
        self.finalised = false;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── MockFileSystem ────────────────────────────────────────────────────────

    #[test]
    fn fs_write_and_read() {
        let mut fs = MockFileSystem::new();
        fs.write("greet.txt", b"hello".to_vec()).unwrap();
        assert_eq!(fs.read("greet.txt").unwrap(), b"hello");
    }

    #[test]
    fn fs_read_missing_errors() {
        let mut fs = MockFileSystem::new();
        assert_eq!(
            fs.read("missing.txt"),
            Err(SandboxError::FileNotFound("missing.txt".into()))
        );
    }

    #[test]
    fn fs_delete_existing() {
        let mut fs = MockFileSystem::new();
        fs.write("del.txt", vec![1, 2, 3]).unwrap();
        assert!(fs.delete("del.txt").is_ok());
        assert!(!fs.exists("del.txt"));
    }

    #[test]
    fn fs_delete_missing_errors() {
        let mut fs = MockFileSystem::new();
        assert!(matches!(fs.delete("nope"), Err(SandboxError::FileNotFound(_))));
    }

    #[test]
    fn fs_list_sorted() {
        let mut fs = MockFileSystem::new();
        fs.write("z.txt", vec![]).unwrap();
        fs.write("a.txt", vec![]).unwrap();
        let listing = fs.list_files();
        assert_eq!(listing, vec!["a.txt", "z.txt"]);
    }

    #[test]
    fn fs_total_bytes() {
        let mut fs = MockFileSystem::new();
        fs.write("a", vec![0u8; 10]).unwrap();
        fs.write("b", vec![0u8; 20]).unwrap();
        assert_eq!(fs.total_bytes(), 30);
    }

    // ── CanvasState ───────────────────────────────────────────────────────────

    #[test]
    fn canvas_add_and_count() {
        let mut canvas = CanvasState::new();
        canvas.add_layer(CanvasLayer::new("l1", "Rect", "rectangle"));
        assert_eq!(canvas.layer_count(), 1);
    }

    #[test]
    fn canvas_remove_layer() {
        let mut canvas = CanvasState::new();
        canvas.add_layer(CanvasLayer::new("l1", "Rect", "rectangle"));
        canvas.remove_layer("l1").unwrap();
        assert_eq!(canvas.layer_count(), 0);
    }

    #[test]
    fn canvas_remove_missing_layer_errors() {
        let mut canvas = CanvasState::new();
        assert!(matches!(
            canvas.remove_layer("nope"),
            Err(SandboxError::LayerNotFound(_))
        ));
    }

    #[test]
    fn canvas_find_layer() {
        let mut canvas = CanvasState::new();
        canvas.add_layer(CanvasLayer::new("abc", "Frame", "frame"));
        assert!(canvas.find_layer("abc").is_some());
        assert!(canvas.find_layer("xyz").is_none());
    }

    #[test]
    fn canvas_select_and_deselect() {
        let mut canvas = CanvasState::new();
        canvas.add_layer(CanvasLayer::new("l1", "R", "r"));
        canvas.add_layer(CanvasLayer::new("l2", "R", "r"));
        canvas.select(&["l1", "l2"]);
        assert_eq!(canvas.selected_ids().len(), 2);
        canvas.deselect_all();
        assert!(canvas.selected_ids().is_empty());
    }

    #[test]
    fn canvas_zoom_clamped() {
        let mut c = CanvasState::new();
        c.set_zoom(200.0);
        assert!((c.zoom() - 64.0).abs() < 0.001);
        c.set_zoom(-1.0);
        assert!((c.zoom() - 0.01).abs() < 0.001);
    }

    // ── Clipboard ─────────────────────────────────────────────────────────────

    #[test]
    fn clipboard_copy_and_paste_text() {
        let mut cb = Clipboard::new();
        cb.copy_text("hello world");
        assert_eq!(cb.paste_text(), Some("hello world"));
    }

    #[test]
    fn clipboard_copy_layers_clears_text() {
        let mut cb = Clipboard::new();
        cb.copy_text("was text");
        cb.copy_layers(vec![CanvasLayer::new("l1", "R", "r")]);
        assert!(cb.paste_text().is_none());
        assert!(cb.paste_layers().is_some());
    }

    #[test]
    fn clipboard_empty_initially() {
        assert!(Clipboard::new().is_empty());
    }

    #[test]
    fn clipboard_clear() {
        let mut cb = Clipboard::new();
        cb.copy_text("x");
        cb.clear();
        assert!(cb.is_empty());
    }

    // ── SandboxEnv ────────────────────────────────────────────────────────────

    #[test]
    fn sandbox_new_and_id() {
        let sb = SandboxEnv::new("test-sb-01");
        assert_eq!(sb.id, "test-sb-01");
        assert!(!sb.is_finalised());
    }

    #[test]
    fn sandbox_log_ops() {
        let mut sb = SandboxEnv::new("sb");
        sb.log_op("create layer");
        sb.log_op("set fill");
        assert_eq!(sb.op_count(), 2);
    }

    #[test]
    fn sandbox_finalise_once() {
        let mut sb = SandboxEnv::new("sb");
        assert!(sb.finalise().is_ok());
        assert!(matches!(sb.finalise(), Err(SandboxError::AlreadyFinalised)));
    }

    #[test]
    fn sandbox_reset_clears_state() {
        let mut sb = SandboxEnv::new("sb");
        sb.fs.write("f.txt", b"data".to_vec()).unwrap();
        sb.log_op("op");
        sb.reset();
        assert_eq!(sb.fs.file_count(), 0);
        assert_eq!(sb.op_count(), 0);
        assert!(!sb.is_finalised());
    }

    #[test]
    fn sandbox_check_fs_limit_strict() {
        let mut sb = SandboxEnv::with_limits("sb", ResourceLimits { max_fs_ops: 1, ..ResourceLimits::default() });
        sb.fs.write("x", vec![]).unwrap(); // op_count = 1 after this
        // op_count (1) >= max_fs_ops (1) → limit exceeded
        assert!(sb.check_fs_limit().is_err());
    }

    #[test]
    fn resource_limits_default_blocks_network() {
        assert!(ResourceLimits::default().network_blocked);
        assert!(!ResourceLimits::lenient().network_blocked);
    }
}
