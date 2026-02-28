//! Core prompt builder — messages, templates, variable interpolation, and payload assembly.
//!
//! Use `Prompt::new()` for a fluent builder. Use `TemplateRegistry` to store and
//! render named templates with `{{variable}}` slots.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Role ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn label(&self) -> &str {
        match self {
            Self::System    => "system",
            Self::User      => "user",
            Self::Assistant => "assistant",
        }
    }
}

// ── Message ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self { Self { role: Role::System,    content: content.into() } }
    pub fn user(content: impl Into<String>) -> Self   { Self { role: Role::User,      content: content.into() } }
    pub fn assistant(content: impl Into<String>) -> Self { Self { role: Role::Assistant, content: content.into() } }
    pub fn is_empty(&self) -> bool { self.content.trim().is_empty() }
}

// ── Prompt variables ──────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct PromptVariables(HashMap<String, String>);

impl PromptVariables {
    pub fn new() -> Self { Self::default() }

    pub fn set(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.0.insert(key.into(), value.into()); self
    }

    pub fn get(&self, key: &str) -> Option<&str> { self.0.get(key).map(|s| s.as_str()) }

    /// Replace all `{{key}}` slots in `template` with their values.
    /// Unknown slots are left as-is.
    pub fn render(&self, template: &str) -> String {
        let mut result = template.to_owned();
        for (k, v) in &self.0 {
            result = result.replace(&format!("{{{{{}}}}}", k), v);
        }
        result
    }

    pub fn len(&self) -> usize { self.0.len() }
    pub fn is_empty(&self) -> bool { self.0.is_empty() }
}

// ── Prompt configuration ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PromptConfig {
    pub temperature: f32,
    pub max_tokens: u32,
    pub stop_sequences: Vec<String>,
    pub top_p: f32,
    pub frequency_penalty: f32,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            max_tokens: 2048,
            stop_sequences: Vec::new(),
            top_p: 1.0,
            frequency_penalty: 0.0,
        }
    }
}

// ── Prompt builder ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Prompt {
    pub messages: Vec<Message>,
    pub config: PromptConfig,
    pub metadata: HashMap<String, String>,
}

impl Prompt {
    pub fn new() -> Self { Self::default() }

    pub fn system(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message::system(content)); self
    }

    pub fn user(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message::user(content)); self
    }

    pub fn assistant(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message::assistant(content)); self
    }

    pub fn add_message(mut self, msg: Message) -> Self {
        self.messages.push(msg); self
    }

    pub fn prepend_message(mut self, msg: Message) -> Self {
        self.messages.insert(0, msg); self
    }

    pub fn with_temperature(mut self, t: f32) -> Self {
        self.config.temperature = t.clamp(0.0, 2.0); self
    }

    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.config.max_tokens = n; self
    }

    pub fn with_stop(mut self, seq: impl Into<String>) -> Self {
        self.config.stop_sequences.push(seq.into()); self
    }

    pub fn with_top_p(mut self, v: f32) -> Self {
        self.config.top_p = v.clamp(0.0, 1.0); self
    }

    pub fn with_meta(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.metadata.insert(k.into(), v.into()); self
    }

    pub fn message_count(&self) -> usize { self.messages.len() }

    pub fn last_user_message(&self) -> Option<&str> {
        self.messages.iter().rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.as_str())
    }

    pub fn system_messages(&self) -> Vec<&Message> {
        self.messages.iter().filter(|m| m.role == Role::System).collect()
    }

    pub fn user_messages(&self) -> Vec<&Message> {
        self.messages.iter().filter(|m| m.role == Role::User).collect()
    }

    pub fn to_text_concat(&self) -> String {
        self.messages.iter()
            .map(|m| format!("[{}]: {}", m.role.label(), m.content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Approximate token count (4 chars ≈ 1 token).
    pub fn estimated_tokens(&self) -> usize {
        let chars: usize = self.messages.iter().map(|m| m.content.len()).sum();
        chars / 4 + 1
    }

    pub fn build(self) -> PromptPayload {
        PromptPayload {
            messages: self.messages,
            config: self.config,
            metadata: self.metadata,
        }
    }
}

// ── Prompt payload ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptPayload {
    pub messages: Vec<Message>,
    #[serde(skip)]
    pub config: PromptConfig,
    pub metadata: HashMap<String, String>,
}

impl PromptPayload {
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

// ── Template registry ─────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct TemplateRegistry {
    templates: HashMap<String, String>,
}

impl TemplateRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, name: impl Into<String>, template: impl Into<String>) {
        self.templates.insert(name.into(), template.into());
    }

    pub fn render(&self, name: &str, vars: &PromptVariables) -> Option<String> {
        self.templates.get(name).map(|t| vars.render(t))
    }

    pub fn get_raw(&self, name: &str) -> Option<&str> {
        self.templates.get(name).map(|s| s.as_str())
    }

    pub fn list(&self) -> Vec<&str> {
        self.templates.keys().map(|k| k.as_str()).collect()
    }

    pub fn count(&self) -> usize { self.templates.len() }
    pub fn contains(&self, name: &str) -> bool { self.templates.contains_key(name) }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_labels() {
        assert_eq!(Role::System.label(), "system");
        assert_eq!(Role::User.label(), "user");
        assert_eq!(Role::Assistant.label(), "assistant");
    }

    #[test]
    fn prompt_builder_message_count() {
        let p = Prompt::new()
            .system("You are a helpful design agent.")
            .user("Create a login screen.")
            .assistant("Sure, here's the layout…")
            .user("Make the button blue.");
        assert_eq!(p.message_count(), 4);
        assert_eq!(p.user_messages().len(), 2);
        assert_eq!(p.system_messages().len(), 1);
    }

    #[test]
    fn prompt_last_user_message() {
        let p = Prompt::new()
            .user("first")
            .assistant("ok")
            .user("second");
        assert_eq!(p.last_user_message(), Some("second"));
    }

    #[test]
    fn prompt_temperature_clamping() {
        let p = Prompt::new().with_temperature(5.0);
        assert!((p.config.temperature - 2.0).abs() < 0.001);
        let p2 = Prompt::new().with_temperature(-1.0);
        assert!((p2.config.temperature - 0.0).abs() < 0.001);
    }

    #[test]
    fn prompt_estimated_tokens() {
        // "hello world" = 11 chars → ~3 tokens
        let p = Prompt::new().user("hello world");
        assert!(p.estimated_tokens() > 0);
    }

    #[test]
    fn prompt_to_text_concat() {
        let text = Prompt::new().system("sys").user("ask").to_text_concat();
        assert!(text.contains("[system]: sys"));
        assert!(text.contains("[user]: ask"));
    }

    #[test]
    fn prompt_variables_render() {
        let vars = PromptVariables::new()
            .set("task", "design a button")
            .set("agent", "Alice");
        let rendered = vars.render("{{agent}} is asked to {{task}}.");
        assert_eq!(rendered, "Alice is asked to design a button.");
    }

    #[test]
    fn prompt_variables_unknown_slot_preserved() {
        let vars = PromptVariables::new().set("known", "X");
        let rendered = vars.render("{{known}} and {{unknown}}");
        assert_eq!(rendered, "X and {{unknown}}");
    }

    #[test]
    fn template_registry_register_and_render() {
        let mut reg = TemplateRegistry::new();
        reg.register("greet", "Hello {{name}}, your task is {{task}}.");
        let vars = PromptVariables::new().set("name", "Bob").set("task", "layout");
        let result = reg.render("greet", &vars).unwrap();
        assert_eq!(result, "Hello Bob, your task is layout.");
    }

    #[test]
    fn template_registry_missing_returns_none() {
        let reg = TemplateRegistry::new();
        assert!(reg.render("nonexistent", &PromptVariables::new()).is_none());
    }

    #[test]
    fn template_registry_list_and_count() {
        let mut reg = TemplateRegistry::new();
        reg.register("t1", "foo");
        reg.register("t2", "bar");
        assert_eq!(reg.count(), 2);
        assert!(reg.contains("t1"));
        assert!(!reg.contains("t3"));
    }

    #[test]
    fn prompt_build_to_payload() {
        let payload = Prompt::new()
            .system("You are a Logos AI agent.")
            .user("Analyse the design.")
            .with_max_tokens(512)
            .build();
        assert_eq!(payload.messages.len(), 2);
    }

    #[test]
    fn prompt_prepend_message() {
        let p = Prompt::new()
            .user("second")
            .prepend_message(Message::system("first"));
        assert_eq!(p.messages[0].role, Role::System);
    }
}
