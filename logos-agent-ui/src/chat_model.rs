//! Chat Model — message structures, conversation history, session state
//!
//! Represents the chat panel's data layer: sessions, messages (user/assistant/
//! system), delivery status, typing indicators, and conversation trimming for
//! context-window management.

use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

// ── Message role ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Agent,
    System,
    Error,
}

impl MessageRole {
    pub fn display_label(&self) -> &str {
        match self {
            MessageRole::User => "You",
            MessageRole::Agent => "Agent",
            MessageRole::System => "Logos",
            MessageRole::Error => "Error",
        }
    }

    pub fn is_agent_authored(&self) -> bool {
        matches!(self, MessageRole::Agent | MessageRole::System)
    }
}

// ── Message status ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageStatus {
    /// Sent by user, waiting for agent.
    Pending,
    /// Agent is generating a response.
    Streaming,
    /// Fully delivered.
    Delivered,
    /// Failed to deliver.
    Failed(String),
    /// Message was edited.
    Edited,
}

impl MessageStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, MessageStatus::Delivered | MessageStatus::Failed(_) | MessageStatus::Edited)
    }
}

// ── Text chunk (streaming token) ──────────────────────────────────────────────

/// A single streamed token from the agent (for progressive rendering).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextChunk {
    pub text: String,
    pub is_final: bool,
}

// ── Chat message ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: MessageRole,
    pub content: String,
    /// If the message triggered a Logos command, the command JSON.
    pub command_json: Option<String>,
    pub status: MessageStatus,
    pub timestamp_secs: u64,
    /// Token count (estimated).
    pub token_estimate: usize,
}

impl ChatMessage {
    pub fn user(session_id: impl Into<String>, content: impl Into<String>, ts: u64) -> Self {
        let content = content.into();
        let token_estimate = estimate_tokens(&content);
        ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            role: MessageRole::User,
            content,
            command_json: None,
            status: MessageStatus::Pending,
            timestamp_secs: ts,
            token_estimate,
        }
    }

    pub fn agent(session_id: impl Into<String>, content: impl Into<String>, ts: u64) -> Self {
        let content = content.into();
        let token_estimate = estimate_tokens(&content);
        ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            role: MessageRole::Agent,
            content,
            command_json: None,
            status: MessageStatus::Delivered,
            timestamp_secs: ts,
            token_estimate,
        }
    }

    pub fn system(session_id: impl Into<String>, content: impl Into<String>, ts: u64) -> Self {
        let content = content.into();
        let token_estimate = estimate_tokens(&content);
        ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            role: MessageRole::System,
            content,
            command_json: None,
            status: MessageStatus::Delivered,
            timestamp_secs: ts,
            token_estimate,
        }
    }

    pub fn error(session_id: impl Into<String>, content: impl Into<String>, ts: u64) -> Self {
        let content = content.into();
        ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            role: MessageRole::Error,
            content,
            command_json: None,
            status: MessageStatus::Failed("error".into()),
            timestamp_secs: ts,
            token_estimate: 0,
        }
    }

    pub fn with_command(mut self, cmd_json: impl Into<String>) -> Self {
        self.command_json = Some(cmd_json.into());
        self
    }

    pub fn mark_delivered(&mut self) {
        self.status = MessageStatus::Delivered;
    }

    /// Whether this message contains an executable command.
    pub fn has_command(&self) -> bool {
        self.command_json.is_some()
    }
}

fn estimate_tokens(text: &str) -> usize {
    // Rough estimate: 1 token ≈ 4 characters
    (text.len() / 4).max(1)
}

// ── Typing indicator ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypingIndicator {
    pub session_id: String,
    pub started_at: u64,
    pub is_visible: bool,
}

impl TypingIndicator {
    pub fn show(session_id: impl Into<String>, ts: u64) -> Self {
        TypingIndicator { session_id: session_id.into(), started_at: ts, is_visible: true }
    }

    pub fn hide(session_id: impl Into<String>, ts: u64) -> Self {
        TypingIndicator { session_id: session_id.into(), started_at: ts, is_visible: false }
    }
}

// ── Chat config ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ChatConfig {
    /// Maximum messages to keep in history before trimming (oldest first).
    pub max_messages: usize,
    /// Maximum token budget for context window.
    pub max_context_tokens: usize,
    /// Whether to include system messages in display.
    pub show_system_messages: bool,
    /// Auto-execute commands suggested by the agent.
    pub auto_execute_commands: bool,
}

impl Default for ChatConfig {
    fn default() -> Self {
        ChatConfig {
            max_messages: 100,
            max_context_tokens: 4096,
            show_system_messages: false,
            auto_execute_commands: false,
        }
    }
}

// ── Conversation history ──────────────────────────────────────────────────────

/// Sliding window of messages with token budget tracking.
#[derive(Debug, Clone)]
pub struct ConversationHistory {
    messages: VecDeque<ChatMessage>,
    config: ChatConfig,
    total_tokens: usize,
}

impl ConversationHistory {
    pub fn new(config: ChatConfig) -> Self {
        ConversationHistory {
            messages: VecDeque::new(),
            config,
            total_tokens: 0,
        }
    }

    pub fn push(&mut self, msg: ChatMessage) {
        self.total_tokens += msg.token_estimate;

        // Trim oldest messages when over token budget or message limit
        while (self.total_tokens > self.config.max_context_tokens
            || self.messages.len() >= self.config.max_messages)
            && !self.messages.is_empty()
        {
            if let Some(removed) = self.messages.pop_front() {
                self.total_tokens = self.total_tokens.saturating_sub(removed.token_estimate);
            }
        }

        self.messages.push_back(msg);
    }

    pub fn len(&self) -> usize { self.messages.len() }
    pub fn is_empty(&self) -> bool { self.messages.is_empty() }
    pub fn total_tokens(&self) -> usize { self.total_tokens }

    pub fn messages(&self) -> impl Iterator<Item = &ChatMessage> {
        self.messages.iter()
    }

    /// Last N messages for context window.
    pub fn last_n(&self, n: usize) -> Vec<&ChatMessage> {
        self.messages.iter().rev().take(n).collect::<Vec<_>>()
            .into_iter().rev().collect()
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.total_tokens = 0;
    }

    /// All messages that contain commands.
    pub fn command_messages(&self) -> Vec<&ChatMessage> {
        self.messages.iter().filter(|m| m.has_command()).collect()
    }

    /// Get the latest user message text.
    pub fn last_user_message(&self) -> Option<&ChatMessage> {
        self.messages.iter().rev().find(|m| m.role == MessageRole::User)
    }
}

// ── Chat session ──────────────────────────────────────────────────────────────

/// Active chat session between a user and a certified agent.
#[derive(Debug)]
pub struct ChatSession {
    pub id: String,
    pub agent_session_id: String,
    pub history: ConversationHistory,
    pub typing: bool,
    pub created_at: u64,
    pub last_message_at: u64,
    pub message_count: usize,
}

impl ChatSession {
    pub fn new(
        agent_session_id: impl Into<String>,
        config: ChatConfig,
        now: u64,
    ) -> Self {
        ChatSession {
            id: uuid::Uuid::new_v4().to_string(),
            agent_session_id: agent_session_id.into(),
            history: ConversationHistory::new(config),
            typing: false,
            created_at: now,
            last_message_at: now,
            message_count: 0,
        }
    }

    pub fn add_message(&mut self, msg: ChatMessage) {
        self.last_message_at = msg.timestamp_secs;
        self.message_count += 1;
        self.history.push(msg);
    }

    pub fn set_typing(&mut self, typing: bool) {
        self.typing = typing;
    }

    pub fn is_idle(&self, now: u64, idle_timeout_secs: u64) -> bool {
        now.saturating_sub(self.last_message_at) > idle_timeout_secs
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> ChatSession {
        ChatSession::new("agent-1", ChatConfig::default(), 0)
    }

    #[test]
    fn new_session_is_empty() {
        let s = session();
        assert_eq!(s.history.len(), 0);
        assert!(!s.typing);
    }

    #[test]
    fn add_user_message() {
        let mut s = session();
        s.add_message(ChatMessage::user(&s.id.clone(), "Create a rectangle", 100));
        assert_eq!(s.history.len(), 1);
        assert_eq!(s.message_count, 1);
        assert_eq!(s.last_message_at, 100);
    }

    #[test]
    fn message_roles_correct() {
        let mut s = session();
        let id = s.id.clone();
        s.add_message(ChatMessage::user(&id, "hello", 0));
        s.add_message(ChatMessage::agent(&id, "Hi there!", 1));
        s.add_message(ChatMessage::system(&id, "Session started", 2));
        assert_eq!(s.history.len(), 3);
    }

    #[test]
    fn history_trims_on_message_limit() {
        let config = ChatConfig { max_messages: 3, max_context_tokens: 999999, ..Default::default() };
        let mut h = ConversationHistory::new(config);
        for i in 0..5u64 {
            h.push(ChatMessage::user("s", format!("msg {}", i), i));
        }
        assert_eq!(h.len(), 3);
    }

    #[test]
    fn history_trims_on_token_budget() {
        let config = ChatConfig { max_messages: 999, max_context_tokens: 10, ..Default::default() };
        let mut h = ConversationHistory::new(config);
        // Each message "twelve chars" ≈ 3 tokens
        for i in 0..10u64 {
            h.push(ChatMessage::user("s", format!("a{}", i), i));
        }
        assert!(h.total_tokens() <= 10, "Tokens: {}", h.total_tokens());
    }

    #[test]
    fn last_n_messages() {
        let mut h = ConversationHistory::new(ChatConfig::default());
        for i in 0..5u64 {
            h.push(ChatMessage::user("s", format!("msg {}", i), i));
        }
        let last3 = h.last_n(3);
        assert_eq!(last3.len(), 3);
        assert_eq!(last3[2].content, "msg 4");
    }

    #[test]
    fn message_with_command() {
        let msg = ChatMessage::agent("s", "Done!", 0)
            .with_command(r#"{"cmd":"create_layer"}"#);
        assert!(msg.has_command());
        assert!(msg.command_json.as_deref().unwrap().contains("create_layer"));
    }

    #[test]
    fn history_clear_resets_tokens() {
        let mut h = ConversationHistory::new(ChatConfig::default());
        h.push(ChatMessage::user("s", "hello world", 0));
        h.clear();
        assert_eq!(h.len(), 0);
        assert_eq!(h.total_tokens(), 0);
    }

    #[test]
    fn typing_indicator_toggle() {
        let mut s = session();
        s.set_typing(true);
        assert!(s.typing);
        s.set_typing(false);
        assert!(!s.typing);
    }

    #[test]
    fn session_idle_detection() {
        let mut s = session();
        let id = s.id.clone();
        s.add_message(ChatMessage::user(&id, "hi", 50));
        assert!(s.is_idle(200, 100));   // 150s elapsed > 100s timeout
        assert!(!s.is_idle(100, 100));  // 50s elapsed, not idle
    }

    #[test]
    fn message_status_terminal() {
        assert!(MessageStatus::Delivered.is_terminal());
        assert!(MessageStatus::Failed("oops".into()).is_terminal());
        assert!(!MessageStatus::Pending.is_terminal());
        assert!(!MessageStatus::Streaming.is_terminal());
    }

    #[test]
    fn error_message_has_failed_status() {
        let msg = ChatMessage::error("s", "Something went wrong", 0);
        assert!(matches!(msg.status, MessageStatus::Failed(_)));
    }

    #[test]
    fn last_user_message() {
        let mut h = ConversationHistory::new(ChatConfig::default());
        h.push(ChatMessage::user("s", "first", 0));
        h.push(ChatMessage::agent("s", "response", 1));
        h.push(ChatMessage::user("s", "second", 2));
        assert_eq!(h.last_user_message().unwrap().content, "second");
    }

    #[test]
    fn role_display_labels() {
        assert_eq!(MessageRole::User.display_label(), "You");
        assert_eq!(MessageRole::Agent.display_label(), "Agent");
        assert_eq!(MessageRole::System.display_label(), "Logos");
    }

    #[test]
    fn token_estimation_nonzero() {
        let msg = ChatMessage::user("s", "Create a blue rectangle at position 100 200", 0);
        assert!(msg.token_estimate > 0);
    }
}
