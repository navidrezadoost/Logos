//! logos-agent-ui — UI Integration layer for certified AI agents in Logos
//!
//! Phase 15.1: Chat panel, command palette, agent dispatcher, status badges,
//! and editor context bridge. Agents certified in logos-ai-agent can now be
//! invoked directly from the Logos UI.

pub mod chat_model;
pub mod command_palette;
pub mod agent_dispatcher;
pub mod status_badge;
pub mod context_bridge;
pub mod ui_events;

// ── Re-exports ────────────────────────────────────────────────────────────────

// Chat
pub use chat_model::{
    ChatSession, ChatMessage, MessageRole, MessageStatus,
    ConversationHistory, ChatConfig, TypingIndicator, TextChunk,
};

// Command palette
pub use command_palette::{
    CommandRegistry, CommandEntry, CommandCategory, PaletteState,
    PaletteAction, AgentCommandShortcut, PaletteFilter,
    CommandMatch, CommandSuggestion,
};

// Agent dispatcher
pub use agent_dispatcher::{
    AgentDispatcher, DispatchRequest, DispatchResponse, DispatchStatus,
    RoutingPolicy, AgentSlot, DispatchPriority,
    DispatcherConfig, DispatchMetrics,
};

// Status badges
pub use status_badge::{
    AgentBadge, BadgeVariant, BadgeState, PresenceState,
    BadgeConfig, BadgeRenderer, AgentCard,
};

// Context bridge
pub use context_bridge::{
    EditorContext, ContextBridge, SelectionInfo, ViewportInfo,
    PageInfo, ContextSnapshot, ContextDiff,
};

// UI events
pub use ui_events::{
    UiEvent, EventBus, EventSubscriber, EventHandler,
    PanelEvent, PaletteEvent, AgentEvent, UiEventKind,
};

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error("no certified agent available")]
    NoAgentAvailable,

    #[error("agent session expired: {0}")]
    SessionExpired(String),

    #[error("dispatch failed: {0}")]
    DispatchFailed(String),

    #[error("command not found: {0}")]
    CommandNotFound(String),

    #[error("context capture failed: {0}")]
    ContextError(String),

    #[error("chat error: {0}")]
    ChatError(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}
