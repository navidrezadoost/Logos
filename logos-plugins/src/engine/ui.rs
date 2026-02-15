//! Plugin UI system — panels, message bridge, and component schemas.
//!
//! Provides the Rust-side infrastructure for plugin UI rendering.
//! Plugins create panels (iframe-equivalent sandboxed containers) and
//! communicate via a typed postMessage-style bridge.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────┐
//! │          Plugin JS Runtime           │
//! │   Logos.ui.createPanel(...)           │
//! │   Logos.ui.sendMessage(panelId, msg)  │
//! │   Logos.ui.onMessage(panelId, cb)     │
//! └──────────────┬───────────────────────┘
//!                │ UiMessage
//!                ▼
//! ┌──────────────────────────────────────┐
//! │            UiBridge                  │
//! │  ┌─────────────────────────────────┐ │
//! │  │ Permission check (UiPermission) │ │
//! │  │ Message routing (id → panel)    │ │
//! │  │ Response tracking (req/res)     │ │
//! │  │ Rate limiting (16ms throttle)   │ │
//! │  └─────────────────────────────────┘ │
//! └──────────────┬───────────────────────┘
//!                │ UiPanel
//!                ▼
//! ┌──────────────────────────────────────┐
//! │  Panel Registry (host-side render)   │
//! │  - DockPosition (left/right/bottom)  │
//! │  - Size constraints (min/max/pref)   │
//! │  - Component schema (pre-built UI)   │
//! │  - Lifecycle (open → active → closed)│
//! └──────────────────────────────────────┘
//! ```
//!
//! ## Security
//!
//! - Panels run in isolated contexts (no cross-panel access)
//! - UI permissions are separate from document permissions
//! - Message payloads are serialized/deserialized (no shared memory)
//! - Rate limiting prevents UI flooding (max 60fps updates)
//!
//! ## Pre-built Components
//!
//! Plugins can use component schemas to describe UI declaratively:
//!
//! | Component | Description |
//! |-----------|-------------|
//! | `PropertyEditor` | Auto-generated from layer properties |
//! | `LayerList` | Scrollable list with icons |
//! | `ColorPicker` | With opacity and eyedropper |
//! | `NumberInput` | Drag-adjustable numeric field |
//! | `Button` | Action button with icon |
//! | `Label` | Read-only text |
//!
//! Reference: OWASP — Iframe Sandboxing
//! Reference: WebKit Blog — postMessage Security

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════
// Panel Types
// ═══════════════════════════════════════════════════════════════

/// Where a panel docks in the editor layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DockPosition {
    /// Left sidebar (properties, layers)
    Left,
    /// Right sidebar (inspector, plugins)
    Right,
    /// Bottom bar (console, output)
    Bottom,
    /// Floating window
    Float,
}

impl DockPosition {
    /// Parse from a string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "bottom" => Some(Self::Bottom),
            "float" => Some(Self::Float),
            _ => None,
        }
    }

    /// Convert to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Float => "float",
        }
    }
}

/// Panel lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelState {
    /// Created but not yet rendered
    Created,
    /// Visible and receiving messages
    Active,
    /// Hidden but still alive (can be re-shown)
    Hidden,
    /// Permanently closed and cleaned up
    Closed,
}

impl PanelState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Active => "active",
            Self::Hidden => "hidden",
            Self::Closed => "closed",
        }
    }
}

/// Size constraints for a panel.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PanelSize {
    /// Preferred width in logical pixels
    pub preferred_width: u32,
    /// Preferred height in logical pixels
    pub preferred_height: u32,
    /// Minimum width (panel cannot shrink below)
    pub min_width: u32,
    /// Minimum height
    pub min_height: u32,
    /// Maximum width (0 = no limit)
    pub max_width: u32,
    /// Maximum height (0 = no limit)
    pub max_height: u32,
}

impl Default for PanelSize {
    fn default() -> Self {
        Self {
            preferred_width: 280,
            preferred_height: 400,
            min_width: 200,
            min_height: 100,
            max_width: 0,
            max_height: 0,
        }
    }
}

impl PanelSize {
    /// Create a simple fixed-size panel.
    pub fn fixed(width: u32, height: u32) -> Self {
        Self {
            preferred_width: width,
            preferred_height: height,
            min_width: width,
            min_height: height,
            max_width: width,
            max_height: height,
        }
    }

    /// Validate that constraints are consistent.
    pub fn is_valid(&self) -> bool {
        self.preferred_width >= self.min_width
            && self.preferred_height >= self.min_height
            && (self.max_width == 0 || self.preferred_width <= self.max_width)
            && (self.max_height == 0 || self.preferred_height <= self.max_height)
            && self.min_width > 0
            && self.min_height > 0
    }
}

/// A plugin UI panel — the equivalent of an iframe in a web context.
///
/// Each panel is an isolated rendering surface with its own message
/// channel, permissions, and lifecycle. The host (desktop shell or
/// WASM runtime) is responsible for actually rendering the panel
/// content based on the component schema or custom HTML.
#[derive(Debug)]
pub struct UiPanel {
    /// Unique panel ID
    pub id: Uuid,
    /// Plugin that owns this panel
    pub plugin_id: Uuid,
    /// Human-readable title (shown in tab/titlebar)
    pub title: String,
    /// Dock position
    pub dock: DockPosition,
    /// Size constraints
    pub size: PanelSize,
    /// Current lifecycle state
    pub state: PanelState,
    /// Component tree (declarative UI)
    pub components: Vec<UiComponent>,
    /// When the panel was created
    pub created_at: Instant,
    /// Panel-specific data (arbitrary key-value from plugin)
    pub data: HashMap<String, String>,
}

impl UiPanel {
    /// Create a new panel.
    pub fn new(
        plugin_id: Uuid,
        title: impl Into<String>,
        dock: DockPosition,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            plugin_id,
            title: title.into(),
            dock,
            size: PanelSize::default(),
            state: PanelState::Created,
            components: Vec::new(),
            created_at: Instant::now(),
            data: HashMap::new(),
        }
    }

    /// Builder: set custom size constraints.
    pub fn with_size(mut self, size: PanelSize) -> Self {
        self.size = size;
        self
    }

    /// Builder: add components.
    pub fn with_components(mut self, components: Vec<UiComponent>) -> Self {
        self.components = components;
        self
    }

    /// Activate the panel (make visible).
    pub fn activate(&mut self) {
        if self.state != PanelState::Closed {
            self.state = PanelState::Active;
        }
    }

    /// Hide the panel (keep alive but not visible).
    pub fn hide(&mut self) {
        if self.state == PanelState::Active {
            self.state = PanelState::Hidden;
        }
    }

    /// Close the panel permanently.
    pub fn close(&mut self) {
        self.state = PanelState::Closed;
    }

    /// Whether the panel can receive messages.
    pub fn is_active(&self) -> bool {
        self.state == PanelState::Active
    }

    /// Whether the panel is alive (not closed).
    pub fn is_alive(&self) -> bool {
        self.state != PanelState::Closed
    }
}

// ═══════════════════════════════════════════════════════════════
// Component Schema (Declarative UI)
// ═══════════════════════════════════════════════════════════════

/// A declarative UI component that a plugin can use without writing HTML.
///
/// The host renders these natively, matching the app's look and feel.
/// This is the equivalent of React pre-built components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiComponent {
    /// Static text label
    Label {
        text: String,
    },
    /// Action button
    Button {
        label: String,
        /// Action ID sent back when clicked
        action: String,
    },
    /// Numeric input with drag support
    NumberInput {
        label: String,
        /// Binding key (sent in change messages)
        key: String,
        value: f64,
        min: f64,
        max: f64,
        step: f64,
    },
    /// Text input field
    TextInput {
        label: String,
        key: String,
        value: String,
        placeholder: String,
    },
    /// Color picker with opacity
    ColorPicker {
        label: String,
        key: String,
        /// RGBA hex string (#RRGGBBAA)
        value: String,
    },
    /// Checkbox / toggle
    Toggle {
        label: String,
        key: String,
        value: bool,
    },
    /// Dropdown select
    Select {
        label: String,
        key: String,
        value: String,
        options: Vec<String>,
    },
    /// Scrollable layer list (auto-populated from document)
    LayerList {
        /// If true, syncs with document selection
        sync_selection: bool,
    },
    /// Auto-generated property editor for selected layers
    PropertyEditor,
    /// Horizontal separator
    Separator,
    /// Group of components with a collapsible header
    Group {
        label: String,
        collapsed: bool,
        children: Vec<UiComponent>,
    },
}

impl UiComponent {
    /// Get the binding key, if any.
    pub fn key(&self) -> Option<&str> {
        match self {
            Self::NumberInput { key, .. } => Some(key),
            Self::TextInput { key, .. } => Some(key),
            Self::ColorPicker { key, .. } => Some(key),
            Self::Toggle { key, .. } => Some(key),
            Self::Select { key, .. } => Some(key),
            _ => None,
        }
    }

    /// Whether this component is interactive (can generate events).
    pub fn is_interactive(&self) -> bool {
        matches!(
            self,
            Self::Button { .. }
                | Self::NumberInput { .. }
                | Self::TextInput { .. }
                | Self::ColorPicker { .. }
                | Self::Toggle { .. }
                | Self::Select { .. }
                | Self::LayerList { .. }
        )
    }
}

// ═══════════════════════════════════════════════════════════════
// UI Messages (PostMessage Protocol)
// ═══════════════════════════════════════════════════════════════

/// Unique message identifier for request/response correlation.
pub type MessageId = u64;

/// Direction of a UI message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDirection {
    /// Plugin → Panel (host renders)
    ToPanel,
    /// Panel → Plugin (user interaction)
    ToPlugin,
}

/// A typed message in the plugin ↔ panel protocol.
///
/// This is the Rust equivalent of `window.postMessage()`. Messages are
/// serialized across the boundary (no shared memory between plugin and UI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiMessage {
    // ─── Plugin → Panel ───

    /// Initial render: set the component tree
    SetComponents {
        components: Vec<UiComponent>,
    },

    /// Update a single component's value by key
    UpdateValue {
        key: String,
        value: UiValue,
    },

    /// Show a toast/notification in the panel
    ShowNotification {
        message: String,
        level: NotificationLevel,
    },

    /// Set panel title
    SetTitle {
        title: String,
    },

    // ─── Panel → Plugin ───

    /// User clicked a button
    ButtonClicked {
        action: String,
    },

    /// User changed an input value
    ValueChanged {
        key: String,
        value: UiValue,
    },

    /// User selected layers in LayerList
    LayerSelected {
        ids: Vec<String>,
    },

    /// Panel lifecycle event
    PanelEvent {
        kind: PanelEventKind,
    },

    // ─── Bidirectional ───

    /// Custom message (plugin-defined)
    Custom {
        kind: String,
        data: HashMap<String, UiValue>,
    },

    /// Request/response pair (with correlation ID)
    Request {
        id: MessageId,
        method: String,
        params: HashMap<String, UiValue>,
    },

    /// Response to a request
    Response {
        id: MessageId,
        result: Option<UiValue>,
        error: Option<String>,
    },
}

/// A value that can be sent in UI messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UiValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<UiValue>),
    Object(HashMap<String, UiValue>),
}

impl UiValue {
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Try to extract a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to extract a number.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Try to extract a bool.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// Notification severity level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
    Success,
}

/// Panel lifecycle events sent from host to plugin.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PanelEventKind {
    /// Panel became visible
    Shown,
    /// Panel was hidden
    Hidden,
    /// Panel is about to close (plugin can clean up)
    Closing,
    /// Panel was resized
    Resized,
    /// Panel gained focus
    Focused,
    /// Panel lost focus
    Blurred,
}

// ═══════════════════════════════════════════════════════════════
// UI Permissions
// ═══════════════════════════════════════════════════════════════

/// UI-specific permission flags.
///
/// These are checked by the UiBridge before routing messages.
/// They are separate from document permissions (a plugin can
/// have UI permission without document access, and vice versa).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiPermission {
    /// Can create and show panels (basic UI)
    Render,
    /// Can read document data in UI context
    ReadDocument,
    /// Can modify document from UI interactions
    WriteDocument,
    /// Can make network requests (through plugin runtime)
    Network,
}

impl UiPermission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Render => "ui:render",
            Self::ReadDocument => "ui:read",
            Self::WriteDocument => "ui:write",
            Self::Network => "ui:network",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "ui:render" | "render" => Some(Self::Render),
            "ui:read" | "readDocument" => Some(Self::ReadDocument),
            "ui:write" | "writeDocument" => Some(Self::WriteDocument),
            "ui:network" | "network" => Some(Self::Network),
        _ => None,
        }
    }
}

/// Set of granted UI permissions for a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPermissionSet {
    granted: Vec<UiPermission>,
}

impl UiPermissionSet {
    /// No UI permissions (default for untrusted plugins).
    pub fn none() -> Self {
        Self { granted: Vec::new() }
    }

    /// Render-only (can show UI but not access document).
    pub fn render_only() -> Self {
        Self { granted: vec![UiPermission::Render] }
    }

    /// Full UI access (render + read + write).
    pub fn full() -> Self {
        Self {
            granted: vec![
                UiPermission::Render,
                UiPermission::ReadDocument,
                UiPermission::WriteDocument,
            ],
        }
    }

    /// Check if a specific permission is granted.
    pub fn has(&self, perm: UiPermission) -> bool {
        self.granted.contains(&perm)
    }

    /// Check a permission, returning Err if denied.
    pub fn check(&self, perm: UiPermission) -> Result<(), String> {
        if self.has(perm) {
            Ok(())
        } else {
            Err(format!("UI permission denied: {}", perm.as_str()))
        }
    }

    /// Grant an additional permission.
    pub fn grant(&mut self, perm: UiPermission) {
        if !self.granted.contains(&perm) {
            self.granted.push(perm);
        }
    }

    /// Revoke a permission.
    pub fn revoke(&mut self, perm: UiPermission) {
        self.granted.retain(|p| *p != perm);
    }

    /// Get all granted permissions.
    pub fn granted(&self) -> &[UiPermission] {
        &self.granted
    }
}

// ═══════════════════════════════════════════════════════════════
// UI Bridge (Message Router)
// ═══════════════════════════════════════════════════════════════

/// Tracks a pending request awaiting a response.
#[derive(Debug)]
struct PendingResponse {
    /// When the request was sent
    sent_at: Instant,
    /// Maximum time to wait for response
    timeout: Duration,
}

impl PendingResponse {
    fn new(timeout: Duration) -> Self {
        Self {
            sent_at: Instant::now(),
            timeout,
        }
    }

    fn is_expired(&self) -> bool {
        self.sent_at.elapsed() > self.timeout
    }
}

/// An outbound message waiting in the bridge queue.
#[derive(Debug)]
#[allow(dead_code)]
struct QueuedMessage {
    panel_id: Uuid,
    message: UiMessage,
    direction: MessageDirection,
}

/// The UI bridge routes messages between plugins and their panels.
///
/// This is the Rust equivalent of the postMessage channel between
/// an iframe and its parent. It enforces:
///
/// - **Permission checks**: Plugin must have UiPermission::Render
/// - **Panel ownership**: Plugin can only message its own panels
/// - **Rate limiting**: Max 60fps message rate per panel
/// - **Request/response tracking**: Correlates req IDs with responses
/// - **Timeout**: Pending requests expire after configurable duration
///
/// ## Thread Safety
///
/// UiBridge is designed to be wrapped in `Arc<RwLock<UiBridge>>`.
/// Panel operations take `&mut self`, message queueing takes `&mut self`.
pub struct UiBridge {
    /// Registered panels by ID
    panels: HashMap<Uuid, UiPanel>,
    /// Plugin → panels index (one plugin can have multiple panels)
    plugin_panels: HashMap<Uuid, Vec<Uuid>>,
    /// UI permissions per plugin
    permissions: HashMap<Uuid, UiPermissionSet>,
    /// Pending request/response tracking
    pending: HashMap<MessageId, PendingResponse>,
    /// Outbound message queue (drained by host renderer)
    outbox: Vec<QueuedMessage>,
    /// Inbound message queue (from UI events, drained by flush)
    inbox: Vec<QueuedMessage>,
    /// Next message ID (monotonic)
    next_message_id: MessageId,
    /// Rate limiting: last message time per panel
    last_send: HashMap<Uuid, Instant>,
    /// Minimum interval between messages to same panel (16ms = 60fps)
    min_interval: Duration,
    /// Request timeout
    request_timeout: Duration,
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Total messages dropped (rate limited)
    pub messages_dropped: u64,
}

impl UiBridge {
    /// Create a new UI bridge.
    pub fn new() -> Self {
        Self {
            panels: HashMap::new(),
            plugin_panels: HashMap::new(),
            permissions: HashMap::new(),
            pending: HashMap::new(),
            outbox: Vec::new(),
            inbox: Vec::new(),
            next_message_id: 1,
            last_send: HashMap::new(),
            min_interval: Duration::from_millis(16),
            request_timeout: Duration::from_secs(5),
            messages_sent: 0,
            messages_received: 0,
            messages_dropped: 0,
        }
    }

    /// Register UI permissions for a plugin.
    pub fn set_permissions(&mut self, plugin_id: Uuid, perms: UiPermissionSet) {
        self.permissions.insert(plugin_id, perms);
    }

    /// Get UI permissions for a plugin.
    pub fn get_permissions(&self, plugin_id: Uuid) -> Option<&UiPermissionSet> {
        self.permissions.get(&plugin_id)
    }

    /// Number of active panels.
    pub fn panel_count(&self) -> usize {
        self.panels.values().filter(|p| p.is_alive()).count()
    }

    /// Get all panels for a plugin.
    pub fn plugin_panels(&self, plugin_id: Uuid) -> Vec<&UiPanel> {
        self.plugin_panels
            .get(&plugin_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.panels.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get a panel by ID.
    pub fn get_panel(&self, panel_id: Uuid) -> Option<&UiPanel> {
        self.panels.get(&panel_id)
    }

    /// Get a mutable reference to a panel.
    pub fn get_panel_mut(&mut self, panel_id: Uuid) -> Option<&mut UiPanel> {
        self.panels.get_mut(&panel_id)
    }

    /// Create a new panel for a plugin.
    ///
    /// Checks UiPermission::Render before creating.
    /// Returns the panel ID on success.
    pub fn create_panel(
        &mut self,
        plugin_id: Uuid,
        title: impl Into<String>,
        dock: DockPosition,
    ) -> Result<Uuid, String> {
        // Permission check
        let perms = self.permissions.get(&plugin_id)
            .ok_or("plugin has no UI permissions registered")?;
        perms.check(UiPermission::Render)?;

        // Create panel
        let mut panel = UiPanel::new(plugin_id, title, dock);
        panel.activate();
        let panel_id = panel.id;

        self.panels.insert(panel_id, panel);
        self.plugin_panels
            .entry(plugin_id)
            .or_default()
            .push(panel_id);

        Ok(panel_id)
    }

    /// Create a panel with size and components.
    pub fn create_panel_full(
        &mut self,
        plugin_id: Uuid,
        title: impl Into<String>,
        dock: DockPosition,
        size: PanelSize,
        components: Vec<UiComponent>,
    ) -> Result<Uuid, String> {
        let perms = self.permissions.get(&plugin_id)
            .ok_or("plugin has no UI permissions registered")?;
        perms.check(UiPermission::Render)?;

        let mut panel = UiPanel::new(plugin_id, title, dock)
            .with_size(size)
            .with_components(components);
        panel.activate();
        let panel_id = panel.id;

        self.panels.insert(panel_id, panel);
        self.plugin_panels
            .entry(plugin_id)
            .or_default()
            .push(panel_id);

        Ok(panel_id)
    }

    /// Close a panel.
    ///
    /// Only the owning plugin can close its panels.
    pub fn close_panel(&mut self, plugin_id: Uuid, panel_id: Uuid) -> Result<(), String> {
        let panel = self.panels.get_mut(&panel_id)
            .ok_or("panel not found")?;
        if panel.plugin_id != plugin_id {
            return Err("permission denied: not panel owner".to_string());
        }
        panel.close();
        Ok(())
    }

    /// Update a panel's components.
    pub fn update_panel_components(
        &mut self,
        plugin_id: Uuid,
        panel_id: Uuid,
        components: Vec<UiComponent>,
    ) -> Result<(), String> {
        let panel = self.panels.get_mut(&panel_id)
            .ok_or("panel not found")?;
        if panel.plugin_id != plugin_id {
            return Err("permission denied: not panel owner".to_string());
        }
        if !panel.is_alive() {
            return Err("panel is closed".to_string());
        }
        panel.components = components;

        // Queue a SetComponents message to the panel
        self.outbox.push(QueuedMessage {
            panel_id,
            message: UiMessage::SetComponents {
                components: panel.components.clone(),
            },
            direction: MessageDirection::ToPanel,
        });

        Ok(())
    }

    /// Send a message from plugin to panel.
    ///
    /// Rate-limited to `min_interval` per panel.
    pub fn send_to_panel(
        &mut self,
        plugin_id: Uuid,
        panel_id: Uuid,
        message: UiMessage,
    ) -> Result<(), String> {
        // Ownership check
        let panel = self.panels.get(&panel_id)
            .ok_or("panel not found")?;
        if panel.plugin_id != plugin_id {
            return Err("permission denied: not panel owner".to_string());
        }
        if !panel.is_active() {
            return Err("panel is not active".to_string());
        }

        // Rate limiting
        if let Some(last) = self.last_send.get(&panel_id) {
            if last.elapsed() < self.min_interval {
                self.messages_dropped += 1;
                return Ok(()); // Silently drop (rate limited)
            }
        }

        self.last_send.insert(panel_id, Instant::now());
        self.outbox.push(QueuedMessage {
            panel_id,
            message,
            direction: MessageDirection::ToPanel,
        });
        self.messages_sent += 1;

        Ok(())
    }

    /// Receive a message from panel to plugin (UI event).
    ///
    /// This is called by the host renderer when the user interacts
    /// with a panel (button click, value change, etc.).
    pub fn receive_from_panel(
        &mut self,
        panel_id: Uuid,
        message: UiMessage,
    ) -> Result<(), String> {
        if !self.panels.contains_key(&panel_id) {
            return Err("panel not found".to_string());
        }

        // If this is a Response, match it to a pending request
        if let UiMessage::Response { id, .. } = &message {
            self.pending.remove(id);
        }

        self.inbox.push(QueuedMessage {
            panel_id,
            message,
            direction: MessageDirection::ToPlugin,
        });
        self.messages_received += 1;

        Ok(())
    }

    /// Send a request and track it for response correlation.
    pub fn send_request(
        &mut self,
        plugin_id: Uuid,
        panel_id: Uuid,
        method: impl Into<String>,
        params: HashMap<String, UiValue>,
    ) -> Result<MessageId, String> {
        let id = self.next_message_id;
        self.next_message_id += 1;

        let message = UiMessage::Request {
            id,
            method: method.into(),
            params,
        };

        self.send_to_panel(plugin_id, panel_id, message)?;
        self.pending.insert(id, PendingResponse::new(self.request_timeout));

        Ok(id)
    }

    /// Drain the outbox (messages waiting to be delivered to panels).
    ///
    /// The host renderer calls this to collect messages for rendering.
    pub fn drain_outbox(&mut self) -> Vec<(Uuid, UiMessage)> {
        self.outbox
            .drain(..)
            .map(|q| (q.panel_id, q.message))
            .collect()
    }

    /// Drain the inbox (messages waiting to be delivered to plugins).
    ///
    /// The plugin runtime calls this to process UI events.
    pub fn drain_inbox(&mut self) -> Vec<(Uuid, UiMessage)> {
        self.inbox
            .drain(..)
            .map(|q| (q.panel_id, q.message))
            .collect()
    }

    /// Clean up expired pending requests.
    ///
    /// Returns the number of expired requests removed.
    pub fn cleanup_expired(&mut self) -> usize {
        let before = self.pending.len();
        self.pending.retain(|_, pr| !pr.is_expired());
        before - self.pending.len()
    }

    /// Clean up closed panels.
    ///
    /// Returns the number of panels removed.
    pub fn cleanup_closed(&mut self) -> usize {
        let closed: Vec<Uuid> = self.panels.iter()
            .filter(|(_, p)| p.state == PanelState::Closed)
            .map(|(id, _)| *id)
            .collect();
        let count = closed.len();

        for id in &closed {
            self.panels.remove(id);
            self.last_send.remove(id);
        }

        // Clean up plugin_panels index
        for ids in self.plugin_panels.values_mut() {
            ids.retain(|id| !closed.contains(id));
        }

        count
    }

    /// Number of pending requests.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Number of messages in outbox.
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }

    /// Number of messages in inbox.
    pub fn inbox_len(&self) -> usize {
        self.inbox.len()
    }
}

// ═══════════════════════════════════════════════════════════════
// Serialization helpers
// ═══════════════════════════════════════════════════════════════

impl UiMessage {
    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }

    /// Deserialize from JSON string.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}

impl UiValue {
    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ─── DockPosition tests ───

    #[test]
    fn test_dock_position_from_str() {
        assert_eq!(DockPosition::from_str("left"), Some(DockPosition::Left));
        assert_eq!(DockPosition::from_str("RIGHT"), Some(DockPosition::Right));
        assert_eq!(DockPosition::from_str("Bottom"), Some(DockPosition::Bottom));
        assert_eq!(DockPosition::from_str("float"), Some(DockPosition::Float));
        assert_eq!(DockPosition::from_str("invalid"), None);
    }

    #[test]
    fn test_dock_position_as_str() {
        assert_eq!(DockPosition::Left.as_str(), "left");
        assert_eq!(DockPosition::Right.as_str(), "right");
        assert_eq!(DockPosition::Bottom.as_str(), "bottom");
        assert_eq!(DockPosition::Float.as_str(), "float");
    }

    // ─── PanelSize tests ───

    #[test]
    fn test_panel_size_default() {
        let s = PanelSize::default();
        assert_eq!(s.preferred_width, 280);
        assert_eq!(s.preferred_height, 400);
        assert!(s.is_valid());
    }

    #[test]
    fn test_panel_size_fixed() {
        let s = PanelSize::fixed(300, 500);
        assert_eq!(s.min_width, 300);
        assert_eq!(s.max_width, 300);
        assert!(s.is_valid());
    }

    #[test]
    fn test_panel_size_invalid_min_exceeds_preferred() {
        let s = PanelSize {
            preferred_width: 100,
            preferred_height: 100,
            min_width: 200, // exceeds preferred
            min_height: 50,
            max_width: 0,
            max_height: 0,
        };
        assert!(!s.is_valid());
    }

    #[test]
    fn test_panel_size_invalid_max_below_preferred() {
        let s = PanelSize {
            preferred_width: 300,
            preferred_height: 300,
            min_width: 100,
            min_height: 100,
            max_width: 200, // below preferred
            max_height: 0,
        };
        assert!(!s.is_valid());
    }

    // ─── PanelState tests ───

    #[test]
    fn test_panel_state_as_str() {
        assert_eq!(PanelState::Created.as_str(), "created");
        assert_eq!(PanelState::Active.as_str(), "active");
        assert_eq!(PanelState::Hidden.as_str(), "hidden");
        assert_eq!(PanelState::Closed.as_str(), "closed");
    }

    // ─── UiPanel tests ───

    #[test]
    fn test_panel_creation() {
        let plugin_id = Uuid::new_v4();
        let panel = UiPanel::new(plugin_id, "Test Panel", DockPosition::Right);
        assert_eq!(panel.plugin_id, plugin_id);
        assert_eq!(panel.title, "Test Panel");
        assert_eq!(panel.dock, DockPosition::Right);
        assert_eq!(panel.state, PanelState::Created);
        assert!(!panel.is_active());
        assert!(panel.is_alive());
    }

    #[test]
    fn test_panel_lifecycle() {
        let plugin_id = Uuid::new_v4();
        let mut panel = UiPanel::new(plugin_id, "Test", DockPosition::Left);

        // Created → Active
        panel.activate();
        assert!(panel.is_active());
        assert!(panel.is_alive());

        // Active → Hidden
        panel.hide();
        assert!(!panel.is_active());
        assert!(panel.is_alive());
        assert_eq!(panel.state, PanelState::Hidden);

        // Hidden → Active (re-show)
        panel.activate();
        assert!(panel.is_active());

        // Active → Closed
        panel.close();
        assert!(!panel.is_active());
        assert!(!panel.is_alive());

        // Cannot activate after closed
        panel.activate();
        assert!(!panel.is_active()); // still closed
    }

    #[test]
    fn test_panel_with_size() {
        let panel = UiPanel::new(Uuid::new_v4(), "Test", DockPosition::Float)
            .with_size(PanelSize::fixed(400, 300));
        assert_eq!(panel.size.preferred_width, 400);
        assert_eq!(panel.size.preferred_height, 300);
    }

    #[test]
    fn test_panel_with_components() {
        let components = vec![
            UiComponent::Label { text: "Hello".into() },
            UiComponent::Button { label: "Click".into(), action: "do_thing".into() },
        ];
        let panel = UiPanel::new(Uuid::new_v4(), "Test", DockPosition::Right)
            .with_components(components);
        assert_eq!(panel.components.len(), 2);
    }

    // ─── UiComponent tests ───

    #[test]
    fn test_component_key() {
        let num = UiComponent::NumberInput {
            label: "X".into(), key: "x_pos".into(),
            value: 0.0, min: 0.0, max: 1000.0, step: 1.0,
        };
        assert_eq!(num.key(), Some("x_pos"));

        let label = UiComponent::Label { text: "Hello".into() };
        assert_eq!(label.key(), None);

        let sep = UiComponent::Separator;
        assert_eq!(sep.key(), None);
    }

    #[test]
    fn test_component_is_interactive() {
        assert!(UiComponent::Button { label: "X".into(), action: "a".into() }.is_interactive());
        assert!(UiComponent::NumberInput {
            label: "X".into(), key: "x".into(),
            value: 0.0, min: 0.0, max: 100.0, step: 1.0,
        }.is_interactive());
        assert!(UiComponent::Toggle { label: "X".into(), key: "x".into(), value: false }.is_interactive());
        assert!(!UiComponent::Label { text: "X".into() }.is_interactive());
        assert!(!UiComponent::Separator.is_interactive());
        assert!(!UiComponent::PropertyEditor.is_interactive());
    }

    #[test]
    fn test_component_group() {
        let group = UiComponent::Group {
            label: "Position".into(),
            collapsed: false,
            children: vec![
                UiComponent::NumberInput {
                    label: "X".into(), key: "x".into(),
                    value: 0.0, min: -1000.0, max: 1000.0, step: 1.0,
                },
                UiComponent::NumberInput {
                    label: "Y".into(), key: "y".into(),
                    value: 0.0, min: -1000.0, max: 1000.0, step: 1.0,
                },
            ],
        };
        assert!(!group.is_interactive());
        assert_eq!(group.key(), None);
    }

    // ─── UiValue tests ───

    #[test]
    fn test_ui_value_types() {
        assert!(UiValue::Null.is_null());
        assert_eq!(UiValue::Bool(true).as_bool(), Some(true));
        assert_eq!(UiValue::Number(3.14).as_f64(), Some(3.14));
        assert_eq!(UiValue::String("hello".into()).as_str(), Some("hello"));
        assert_eq!(UiValue::Number(42.0).as_str(), None);
        assert_eq!(UiValue::String("hi".into()).as_f64(), None);
    }

    // ─── UiPermission tests ───

    #[test]
    fn test_ui_permission_from_str() {
        assert_eq!(UiPermission::from_str("ui:render"), Some(UiPermission::Render));
        assert_eq!(UiPermission::from_str("render"), Some(UiPermission::Render));
        assert_eq!(UiPermission::from_str("ui:read"), Some(UiPermission::ReadDocument));
        assert_eq!(UiPermission::from_str("readDocument"), Some(UiPermission::ReadDocument));
        assert_eq!(UiPermission::from_str("invalid"), None);
    }

    #[test]
    fn test_ui_permission_as_str() {
        assert_eq!(UiPermission::Render.as_str(), "ui:render");
        assert_eq!(UiPermission::ReadDocument.as_str(), "ui:read");
        assert_eq!(UiPermission::WriteDocument.as_str(), "ui:write");
        assert_eq!(UiPermission::Network.as_str(), "ui:network");
    }

    // ─── UiPermissionSet tests ───

    #[test]
    fn test_permission_set_none() {
        let ps = UiPermissionSet::none();
        assert!(!ps.has(UiPermission::Render));
        assert!(ps.check(UiPermission::Render).is_err());
    }

    #[test]
    fn test_permission_set_render_only() {
        let ps = UiPermissionSet::render_only();
        assert!(ps.has(UiPermission::Render));
        assert!(!ps.has(UiPermission::ReadDocument));
        assert!(ps.check(UiPermission::Render).is_ok());
        assert!(ps.check(UiPermission::ReadDocument).is_err());
    }

    #[test]
    fn test_permission_set_full() {
        let ps = UiPermissionSet::full();
        assert!(ps.has(UiPermission::Render));
        assert!(ps.has(UiPermission::ReadDocument));
        assert!(ps.has(UiPermission::WriteDocument));
        assert!(!ps.has(UiPermission::Network));
    }

    #[test]
    fn test_permission_set_grant_revoke() {
        let mut ps = UiPermissionSet::none();
        ps.grant(UiPermission::Render);
        assert!(ps.has(UiPermission::Render));

        ps.grant(UiPermission::Render); // duplicate grant
        assert_eq!(ps.granted().len(), 1);

        ps.revoke(UiPermission::Render);
        assert!(!ps.has(UiPermission::Render));
    }

    // ─── UiMessage serialization tests ───

    #[test]
    fn test_message_set_components_json() {
        let msg = UiMessage::SetComponents {
            components: vec![
                UiComponent::Label { text: "Hello".into() },
            ],
        };
        let json = msg.to_json().unwrap();
        let parsed = UiMessage::from_json(&json).unwrap();
        if let UiMessage::SetComponents { components } = parsed {
            assert_eq!(components.len(), 1);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_message_button_clicked_json() {
        let msg = UiMessage::ButtonClicked { action: "submit".into() };
        let json = msg.to_json().unwrap();
        assert!(json.contains("submit"));
        let parsed = UiMessage::from_json(&json).unwrap();
        if let UiMessage::ButtonClicked { action } = parsed {
            assert_eq!(action, "submit");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_message_value_changed_json() {
        let msg = UiMessage::ValueChanged {
            key: "width".into(),
            value: UiValue::Number(42.0),
        };
        let json = msg.to_json().unwrap();
        let parsed = UiMessage::from_json(&json).unwrap();
        if let UiMessage::ValueChanged { key, value } = parsed {
            assert_eq!(key, "width");
            assert_eq!(value.as_f64(), Some(42.0));
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_message_request_response_json() {
        let mut params = HashMap::new();
        params.insert("layer_id".into(), UiValue::String("abc".into()));
        let req = UiMessage::Request {
            id: 42,
            method: "getProperties".into(),
            params,
        };
        let json = req.to_json().unwrap();
        let parsed = UiMessage::from_json(&json).unwrap();
        if let UiMessage::Request { id, method, params } = parsed {
            assert_eq!(id, 42);
            assert_eq!(method, "getProperties");
            assert_eq!(params["layer_id"].as_str(), Some("abc"));
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_message_custom_json() {
        let mut data = HashMap::new();
        data.insert("count".into(), UiValue::Number(5.0));
        let msg = UiMessage::Custom {
            kind: "myEvent".into(),
            data,
        };
        let json = msg.to_json().unwrap();
        let parsed = UiMessage::from_json(&json).unwrap();
        if let UiMessage::Custom { kind, data } = parsed {
            assert_eq!(kind, "myEvent");
            assert_eq!(data["count"].as_f64(), Some(5.0));
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_message_notification_json() {
        let msg = UiMessage::ShowNotification {
            message: "Saved!".into(),
            level: NotificationLevel::Success,
        };
        let json = msg.to_json().unwrap();
        let parsed = UiMessage::from_json(&json).unwrap();
        if let UiMessage::ShowNotification { message, level } = parsed {
            assert_eq!(message, "Saved!");
            assert_eq!(level, NotificationLevel::Success);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_ui_value_json() {
        let v = UiValue::Object({
            let mut m = HashMap::new();
            m.insert("x".into(), UiValue::Number(10.0));
            m.insert("y".into(), UiValue::Number(20.0));
            m
        });
        let json = v.to_json().unwrap();
        assert!(json.contains("10.0") || json.contains("10"));
    }

    // ─── UiBridge tests ───

    #[test]
    fn test_bridge_create_panel() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::render_only());

        let panel_id = bridge.create_panel(plugin_id, "Test Panel", DockPosition::Right).unwrap();
        assert_eq!(bridge.panel_count(), 1);

        let panel = bridge.get_panel(panel_id).unwrap();
        assert_eq!(panel.title, "Test Panel");
        assert!(panel.is_active());
    }

    #[test]
    fn test_bridge_create_panel_no_permission() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::none());

        let result = bridge.create_panel(plugin_id, "Test", DockPosition::Right);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("denied"));
    }

    #[test]
    fn test_bridge_create_panel_no_registration() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        // No permissions registered at all

        let result = bridge.create_panel(plugin_id, "Test", DockPosition::Right);
        assert!(result.is_err());
    }

    #[test]
    fn test_bridge_close_panel() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::render_only());

        let panel_id = bridge.create_panel(plugin_id, "Test", DockPosition::Right).unwrap();
        assert_eq!(bridge.panel_count(), 1);

        bridge.close_panel(plugin_id, panel_id).unwrap();
        let panel = bridge.get_panel(panel_id).unwrap();
        assert!(!panel.is_alive());
    }

    #[test]
    fn test_bridge_close_panel_wrong_owner() {
        let mut bridge = UiBridge::new();
        let plugin_a = Uuid::new_v4();
        let plugin_b = Uuid::new_v4();
        bridge.set_permissions(plugin_a, UiPermissionSet::render_only());
        bridge.set_permissions(plugin_b, UiPermissionSet::render_only());

        let panel_id = bridge.create_panel(plugin_a, "A's Panel", DockPosition::Right).unwrap();

        // Plugin B cannot close Plugin A's panel
        let result = bridge.close_panel(plugin_b, panel_id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not panel owner"));
    }

    #[test]
    fn test_bridge_send_to_panel() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::render_only());

        let panel_id = bridge.create_panel(plugin_id, "Test", DockPosition::Right).unwrap();

        let msg = UiMessage::UpdateValue {
            key: "x".into(),
            value: UiValue::Number(42.0),
        };
        bridge.send_to_panel(plugin_id, panel_id, msg).unwrap();

        assert_eq!(bridge.outbox_len(), 1);
        assert_eq!(bridge.messages_sent, 1);

        let outbox = bridge.drain_outbox();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].0, panel_id);
    }

    #[test]
    fn test_bridge_send_wrong_owner() {
        let mut bridge = UiBridge::new();
        let plugin_a = Uuid::new_v4();
        let plugin_b = Uuid::new_v4();
        bridge.set_permissions(plugin_a, UiPermissionSet::render_only());
        bridge.set_permissions(plugin_b, UiPermissionSet::render_only());

        let panel_id = bridge.create_panel(plugin_a, "A's Panel", DockPosition::Right).unwrap();

        let msg = UiMessage::UpdateValue {
            key: "x".into(),
            value: UiValue::Number(42.0),
        };
        let result = bridge.send_to_panel(plugin_b, panel_id, msg);
        assert!(result.is_err());
    }

    #[test]
    fn test_bridge_receive_from_panel() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::render_only());

        let panel_id = bridge.create_panel(plugin_id, "Test", DockPosition::Right).unwrap();

        let msg = UiMessage::ButtonClicked { action: "submit".into() };
        bridge.receive_from_panel(panel_id, msg).unwrap();

        assert_eq!(bridge.inbox_len(), 1);
        assert_eq!(bridge.messages_received, 1);

        let inbox = bridge.drain_inbox();
        assert_eq!(inbox.len(), 1);
    }

    #[test]
    fn test_bridge_request_response() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::render_only());

        let panel_id = bridge.create_panel(plugin_id, "Test", DockPosition::Right).unwrap();

        // Send request
        let req_id = bridge.send_request(
            plugin_id, panel_id,
            "getColor",
            HashMap::new(),
        ).unwrap();
        assert_eq!(bridge.pending_count(), 1);

        // Receive response
        let response = UiMessage::Response {
            id: req_id,
            result: Some(UiValue::String("#FF0000".into())),
            error: None,
        };
        bridge.receive_from_panel(panel_id, response).unwrap();
        assert_eq!(bridge.pending_count(), 0); // auto-matched
    }

    #[test]
    fn test_bridge_multiple_panels() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::render_only());

        let _p1 = bridge.create_panel(plugin_id, "Panel 1", DockPosition::Left).unwrap();
        let p2 = bridge.create_panel(plugin_id, "Panel 2", DockPosition::Right).unwrap();
        let _p3 = bridge.create_panel(plugin_id, "Panel 3", DockPosition::Bottom).unwrap();

        assert_eq!(bridge.panel_count(), 3);

        let panels = bridge.plugin_panels(plugin_id);
        assert_eq!(panels.len(), 3);

        // Close one
        bridge.close_panel(plugin_id, p2).unwrap();
        assert_eq!(bridge.panel_count(), 2); // p2 is closed but not cleaned up yet
    }

    #[test]
    fn test_bridge_cleanup_closed() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::render_only());

        let p1 = bridge.create_panel(plugin_id, "Panel 1", DockPosition::Left).unwrap();
        let _p2 = bridge.create_panel(plugin_id, "Panel 2", DockPosition::Right).unwrap();

        bridge.close_panel(plugin_id, p1).unwrap();
        let cleaned = bridge.cleanup_closed();
        assert_eq!(cleaned, 1);
        assert!(bridge.get_panel(p1).is_none());
    }

    #[test]
    fn test_bridge_update_panel_components() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::render_only());

        let panel_id = bridge.create_panel(plugin_id, "Test", DockPosition::Right).unwrap();

        let components = vec![
            UiComponent::Label { text: "Width".into() },
            UiComponent::NumberInput {
                label: "W".into(), key: "width".into(),
                value: 100.0, min: 0.0, max: 10000.0, step: 1.0,
            },
        ];
        bridge.update_panel_components(plugin_id, panel_id, components).unwrap();

        let panel = bridge.get_panel(panel_id).unwrap();
        assert_eq!(panel.components.len(), 2);

        // Should have queued a SetComponents message
        assert_eq!(bridge.outbox_len(), 1);
    }

    #[test]
    fn test_bridge_update_closed_panel_fails() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::render_only());

        let panel_id = bridge.create_panel(plugin_id, "Test", DockPosition::Right).unwrap();
        bridge.close_panel(plugin_id, panel_id).unwrap();

        let result = bridge.update_panel_components(plugin_id, panel_id, vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("closed"));
    }

    #[test]
    fn test_bridge_send_to_inactive_panel_fails() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::render_only());

        let panel_id = bridge.create_panel(plugin_id, "Test", DockPosition::Right).unwrap();

        // Hide the panel
        bridge.get_panel_mut(panel_id).unwrap().hide();

        let msg = UiMessage::UpdateValue {
            key: "x".into(),
            value: UiValue::Number(1.0),
        };
        let result = bridge.send_to_panel(plugin_id, panel_id, msg);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not active"));
    }

    #[test]
    fn test_bridge_create_panel_full() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::full());

        let panel_id = bridge.create_panel_full(
            plugin_id,
            "Properties",
            DockPosition::Right,
            PanelSize::fixed(300, 400),
            vec![
                UiComponent::PropertyEditor,
                UiComponent::Separator,
                UiComponent::Button { label: "Apply".into(), action: "apply".into() },
            ],
        ).unwrap();

        let panel = bridge.get_panel(panel_id).unwrap();
        assert_eq!(panel.components.len(), 3);
        assert_eq!(panel.size.preferred_width, 300);
    }

    // ─── Rate limiting test ───

    #[test]
    fn test_bridge_rate_limiting() {
        let mut bridge = UiBridge::new();
        bridge.min_interval = Duration::from_millis(100); // Make it obvious
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::render_only());

        let panel_id = bridge.create_panel(plugin_id, "Test", DockPosition::Right).unwrap();

        // First message goes through
        let msg1 = UiMessage::UpdateValue {
            key: "x".into(), value: UiValue::Number(1.0),
        };
        bridge.send_to_panel(plugin_id, panel_id, msg1).unwrap();
        assert_eq!(bridge.messages_sent, 1);
        assert_eq!(bridge.messages_dropped, 0);

        // Second message (within 100ms) gets dropped
        let msg2 = UiMessage::UpdateValue {
            key: "x".into(), value: UiValue::Number(2.0),
        };
        bridge.send_to_panel(plugin_id, panel_id, msg2).unwrap();
        assert_eq!(bridge.messages_sent, 1); // still 1
        assert_eq!(bridge.messages_dropped, 1);
    }

    // ─── Component schema tests ───

    #[test]
    fn test_component_select() {
        let sel = UiComponent::Select {
            label: "Font".into(),
            key: "font_family".into(),
            value: "Arial".into(),
            options: vec!["Arial".into(), "Helvetica".into(), "Times".into()],
        };
        assert_eq!(sel.key(), Some("font_family"));
        assert!(sel.is_interactive());
    }

    #[test]
    fn test_component_color_picker() {
        let cp = UiComponent::ColorPicker {
            label: "Fill".into(),
            key: "fill_color".into(),
            value: "#FF0000FF".into(),
        };
        assert_eq!(cp.key(), Some("fill_color"));
        assert!(cp.is_interactive());
    }

    #[test]
    fn test_component_text_input() {
        let ti = UiComponent::TextInput {
            label: "Name".into(),
            key: "layer_name".into(),
            value: "Rectangle 1".into(),
            placeholder: "Enter name...".into(),
        };
        assert_eq!(ti.key(), Some("layer_name"));
        assert!(ti.is_interactive());
    }

    #[test]
    fn test_component_layer_list() {
        let ll = UiComponent::LayerList { sync_selection: true };
        assert_eq!(ll.key(), None);
        assert!(ll.is_interactive());
    }

    // ─── PanelEventKind tests ───

    #[test]
    fn test_panel_event_kind_serialization() {
        let msg = UiMessage::PanelEvent { kind: PanelEventKind::Shown };
        let json = msg.to_json().unwrap();
        let parsed = UiMessage::from_json(&json).unwrap();
        if let UiMessage::PanelEvent { kind } = parsed {
            assert_eq!(kind, PanelEventKind::Shown);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_message_layer_selected_json() {
        let msg = UiMessage::LayerSelected {
            ids: vec!["abc".into(), "def".into()],
        };
        let json = msg.to_json().unwrap();
        let parsed = UiMessage::from_json(&json).unwrap();
        if let UiMessage::LayerSelected { ids } = parsed {
            assert_eq!(ids, vec!["abc", "def"]);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_message_set_title_json() {
        let msg = UiMessage::SetTitle { title: "New Title".into() };
        let json = msg.to_json().unwrap();
        let parsed = UiMessage::from_json(&json).unwrap();
        if let UiMessage::SetTitle { title } = parsed {
            assert_eq!(title, "New Title");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_message_response_with_error() {
        let msg = UiMessage::Response {
            id: 99,
            result: None,
            error: Some("not found".into()),
        };
        let json = msg.to_json().unwrap();
        let parsed = UiMessage::from_json(&json).unwrap();
        if let UiMessage::Response { id, result, error } = parsed {
            assert_eq!(id, 99);
            assert!(result.is_none());
            assert_eq!(error, Some("not found".into()));
        } else {
            panic!("wrong variant");
        }
    }

    // ─── Integration: property panel scenario ───

    #[test]
    fn test_property_panel_scenario() {
        let mut bridge = UiBridge::new();
        let plugin_id = Uuid::new_v4();
        bridge.set_permissions(plugin_id, UiPermissionSet::full());

        // 1. Plugin creates a property panel
        let panel_id = bridge.create_panel_full(
            plugin_id,
            "Properties",
            DockPosition::Right,
            PanelSize::default(),
            vec![
                UiComponent::Group {
                    label: "Position".into(),
                    collapsed: false,
                    children: vec![
                        UiComponent::NumberInput {
                            label: "X".into(), key: "x".into(),
                            value: 0.0, min: -10000.0, max: 10000.0, step: 1.0,
                        },
                        UiComponent::NumberInput {
                            label: "Y".into(), key: "y".into(),
                            value: 0.0, min: -10000.0, max: 10000.0, step: 1.0,
                        },
                    ],
                },
                UiComponent::Group {
                    label: "Size".into(),
                    collapsed: false,
                    children: vec![
                        UiComponent::NumberInput {
                            label: "W".into(), key: "width".into(),
                            value: 100.0, min: 0.0, max: 10000.0, step: 1.0,
                        },
                        UiComponent::NumberInput {
                            label: "H".into(), key: "height".into(),
                            value: 100.0, min: 0.0, max: 10000.0, step: 1.0,
                        },
                    ],
                },
                UiComponent::Separator,
                UiComponent::ColorPicker {
                    label: "Fill".into(),
                    key: "fill".into(),
                    value: "#336699FF".into(),
                },
                UiComponent::Separator,
                UiComponent::Button { label: "Delete".into(), action: "delete_layer".into() },
            ],
        ).unwrap();

        let panel = bridge.get_panel(panel_id).unwrap();
        assert!(panel.is_active());
        assert_eq!(panel.components.len(), 6); // 2 groups + 2 separators + 1 color + 1 button

        // 2. User changes width
        bridge.receive_from_panel(panel_id, UiMessage::ValueChanged {
            key: "width".into(),
            value: UiValue::Number(200.0),
        }).unwrap();

        // 3. Plugin processes the change
        let inbox = bridge.drain_inbox();
        assert_eq!(inbox.len(), 1);
        if let UiMessage::ValueChanged { key, value } = &inbox[0].1 {
            assert_eq!(key, "width");
            assert_eq!(value.as_f64(), Some(200.0));
        }

        // 4. Plugin updates the UI (echoes back confirmation)
        bridge.send_to_panel(plugin_id, panel_id, UiMessage::UpdateValue {
            key: "width".into(),
            value: UiValue::Number(200.0),
        }).unwrap();

        // 5. User clicks delete
        bridge.receive_from_panel(panel_id, UiMessage::ButtonClicked {
            action: "delete_layer".into(),
        }).unwrap();

        let inbox = bridge.drain_inbox();
        assert_eq!(inbox.len(), 1);
        if let UiMessage::ButtonClicked { action } = &inbox[0].1 {
            assert_eq!(action, "delete_layer");
        }
    }
}
