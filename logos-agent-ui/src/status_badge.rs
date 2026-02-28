//! Status Badge — visual representation of agent state in the UI
//!
//! Each active agent session is represented by a badge/card that is rendered
//! inline in the Logos toolbar or agent panel. The badge shows level, provider,
//! availability, and a usage/health indicator.

use serde::{Deserialize, Serialize};

// ── Badge variant ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BadgeVariant {
    /// Single colored dot (toolbar status indicator).
    Dot,
    /// Compact chip: icon + level label.
    Chip,
    /// Full pill with provider name + level.
    Pill,
    /// Expanded card with usage bar and capability list.
    Card,
}

impl BadgeVariant {
    pub fn width_hint_px(&self) -> u32 {
        match self {
            BadgeVariant::Dot => 12,
            BadgeVariant::Chip => 64,
            BadgeVariant::Pill => 120,
            BadgeVariant::Card => 240,
        }
    }
}

// ── Badge state ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BadgeState {
    /// Agent is initializing or connecting.
    Loading,
    /// Agent is available and ready.
    Ready,
    /// Agent is currently processing a request.
    Busy,
    /// Agent is connected but no recent activity.
    Idle,
    /// Agent session has expired or disconnected.
    Offline,
    /// Agent encountered an error.
    Error { message: String },
}

impl BadgeState {
    pub fn color_hex(&self) -> &str {
        match self {
            BadgeState::Loading => "#94a3b8",
            BadgeState::Ready   => "#22c55e",
            BadgeState::Busy    => "#f59e0b",
            BadgeState::Idle    => "#64748b",
            BadgeState::Offline => "#475569",
            BadgeState::Error { .. } => "#ef4444",
        }
    }

    pub fn label(&self) -> &str {
        match self {
            BadgeState::Loading => "Loading…",
            BadgeState::Ready   => "Ready",
            BadgeState::Busy    => "Busy",
            BadgeState::Idle    => "Idle",
            BadgeState::Offline => "Offline",
            BadgeState::Error { .. } => "Error",
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, BadgeState::Ready | BadgeState::Idle)
    }
}

// ── Presence state ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresenceState {
    Online,
    Away,
    DoNotDisturb,
    Offline,
}

impl PresenceState {
    pub fn dot_color(&self) -> &str {
        match self {
            PresenceState::Online        => "#22c55e",
            PresenceState::Away          => "#f59e0b",
            PresenceState::DoNotDisturb  => "#ef4444",
            PresenceState::Offline       => "#6b7280",
        }
    }

    pub fn display(&self) -> &str {
        match self {
            PresenceState::Online       => "Online",
            PresenceState::Away         => "Away",
            PresenceState::DoNotDisturb => "Do Not Disturb",
            PresenceState::Offline      => "Offline",
        }
    }
}

// ── Badge config ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BadgeConfig {
    pub show_level: bool,
    pub show_provider: bool,
    pub show_presence: bool,
    pub show_usage_bar: bool,
    pub show_request_count: bool,
    pub animate_on_busy: bool,
}

impl Default for BadgeConfig {
    fn default() -> Self {
        BadgeConfig {
            show_level: true,
            show_provider: true,
            show_presence: true,
            show_usage_bar: false,
            show_request_count: false,
            animate_on_busy: true,
        }
    }
}

// ── Agent badge ───────────────────────────────────────────────────────────────

/// The primary badge attached to an agent session.
#[derive(Debug, Clone)]
pub struct AgentBadge {
    pub session_id: String,
    pub display_name: String,
    pub level: String,
    pub provider: String,
    pub state: BadgeState,
    pub presence: PresenceState,
    pub config: BadgeConfig,
    /// 0.0–100.0% of token/budget usage.
    pub usage_pct: f32,
    /// Total requests served.
    pub request_count: u32,
}

impl AgentBadge {
    pub fn new(
        session_id: impl Into<String>,
        display_name: impl Into<String>,
        level: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        AgentBadge {
            session_id: session_id.into(),
            display_name: display_name.into(),
            level: level.into(),
            provider: provider.into(),
            state: BadgeState::Loading,
            presence: PresenceState::Online,
            config: BadgeConfig::default(),
            usage_pct: 0.0,
            request_count: 0,
        }
    }

    pub fn with_config(mut self, config: BadgeConfig) -> Self {
        self.config = config;
        self
    }

    pub fn set_ready(&mut self) { self.state = BadgeState::Ready; }
    pub fn set_busy(&mut self) { self.state = BadgeState::Busy; }
    pub fn set_idle(&mut self) { self.state = BadgeState::Idle; }
    pub fn set_offline(&mut self) { self.state = BadgeState::Offline; self.presence = PresenceState::Offline; }
    pub fn set_error(&mut self, msg: impl Into<String>) { self.state = BadgeState::Error { message: msg.into() }; }

    pub fn increment_request(&mut self) { self.request_count += 1; }
    pub fn set_usage(&mut self, pct: f32) { self.usage_pct = pct.clamp(0.0, 100.0); }

    pub fn is_available(&self) -> bool { self.state.is_available() }

    pub fn level_emoji(&self) -> &str {
        match self.level.to_lowercase().as_str() {
            "junior" => "🌱",
            "mid" | "mid-level" => "⚡",
            "senior" => "🏆",
            "principal" => "🌟",
            _ => "🤖",
        }
    }
}

// ── Agent card ────────────────────────────────────────────────────────────────

/// Expanded card view shown in the agent panel drawer.
#[derive(Debug, Clone)]
pub struct AgentCard {
    pub badge: AgentBadge,
    pub capabilities: Vec<String>,
    pub tests_passed: u32,
    pub score_pct: f32,
    pub avg_response_ms: f64,
    pub session_duration_secs: u64,
}

impl AgentCard {
    pub fn from_badge(badge: AgentBadge) -> Self {
        AgentCard {
            badge,
            capabilities: Vec::new(),
            tests_passed: 0,
            score_pct: 0.0,
            avg_response_ms: 0.0,
            session_duration_secs: 0,
        }
    }

    pub fn with_capabilities(mut self, caps: Vec<&str>) -> Self {
        self.capabilities = caps.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_score(mut self, passed: u32, score: f32) -> Self {
        self.tests_passed = passed;
        self.score_pct = score.clamp(0.0, 100.0);
        self
    }

    pub fn quality_tier(&self) -> &str {
        match self.score_pct as u32 {
            90..=100 => "Exceptional",
            75..=89  => "High",
            60..=74  => "Adequate",
            _        => "Developing",
        }
    }
}

// ── Badge renderer ────────────────────────────────────────────────────────────

pub struct BadgeRenderer;

impl BadgeRenderer {
    /// Render a dot (single colored circle representation).
    pub fn render_dot(badge: &AgentBadge) -> String {
        format!(
            r#"<span class="agent-dot" style="background:{}" title="{}"></span>"#,
            badge.state.color_hex(),
            badge.state.label(),
        )
    }

    /// Render a compact chip for the toolbar.
    pub fn render_chip(badge: &AgentBadge) -> String {
        format!(
            "{} {} {}",
            badge.level_emoji(),
            badge.level,
            badge.state.label(),
        )
    }

    /// Render a full pill with provider + level + state.
    pub fn render_pill(badge: &AgentBadge) -> String {
        let mut parts = Vec::new();
        parts.push(badge.level_emoji().to_string());
        if badge.config.show_level { parts.push(badge.level.clone()); }
        if badge.config.show_provider { parts.push(format!("({})", badge.provider)); }
        parts.push(format!("[{}]", badge.state.label()));
        if badge.config.show_presence { parts.push(badge.presence.display().to_string()); }
        parts.join(" ")
    }

    /// Render a detailed card as plain text.
    pub fn render_card(card: &AgentCard) -> String {
        let mut buf = String::new();
        buf.push_str(&format!("╔═ {} ══════════════════\n", card.badge.display_name));
        buf.push_str(&format!("║ Level    : {} {}\n", card.badge.level_emoji(), card.badge.level));
        buf.push_str(&format!("║ Provider : {}\n", card.badge.provider));
        buf.push_str(&format!("║ Status   : {}\n", card.badge.state.label()));
        buf.push_str(&format!("║ Presence : {}\n", card.badge.presence.display()));
        if !card.capabilities.is_empty() {
            buf.push_str(&format!("║ Skills   : {}\n", card.capabilities.join(", ")));
        }
        buf.push_str(&format!("║ Tests    : {} passed ({:.0}%)\n", card.tests_passed, card.score_pct));
        buf.push_str(&format!("║ Requests : {}\n", card.badge.request_count));
        buf.push_str(&format!("║ Quality  : {}\n", card.quality_tier()));
        if card.badge.config.show_usage_bar {
            let filled = (card.badge.usage_pct / 10.0) as usize;
            let bar: String = "█".repeat(filled) + &"░".repeat(10 - filled);
            buf.push_str(&format!("║ Usage    : [{}] {:.0}%\n", bar, card.badge.usage_pct));
        }
        buf.push_str("╚══════════════════════════");
        buf
    }

    /// Generate accessible aria-label for the badge.
    pub fn aria_label(badge: &AgentBadge) -> String {
        format!(
            "AI Agent: {} level, {}, {}",
            badge.level,
            badge.provider,
            badge.state.label(),
        )
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn badge() -> AgentBadge {
        AgentBadge::new("session-1", "Senior Agent", "Senior", "builtin")
    }

    #[test]
    fn new_badge_is_loading() {
        let b = badge();
        assert_eq!(b.state, BadgeState::Loading);
    }

    #[test]
    fn badge_state_transitions() {
        let mut b = badge();
        b.set_ready();
        assert!(b.is_available());
        b.set_busy();
        assert!(!b.is_available());
        b.set_idle();
        assert!(b.is_available());
        b.set_offline();
        assert!(!b.is_available());
    }

    #[test]
    fn badge_error_state() {
        let mut b = badge();
        b.set_error("timeout");
        assert!(matches!(b.state, BadgeState::Error { .. }));
        assert!(!b.is_available());
    }

    #[test]
    fn usage_clamped() {
        let mut b = badge();
        b.set_usage(150.0);
        assert_eq!(b.usage_pct, 100.0);
        b.set_usage(-10.0);
        assert_eq!(b.usage_pct, 0.0);
    }

    #[test]
    fn level_emoji_mapping() {
        let mut b = badge();
        assert_eq!(b.level_emoji(), "🏆");
        b.level = "Junior".into();
        assert_eq!(b.level_emoji(), "🌱");
        b.level = "Mid".into();
        assert_eq!(b.level_emoji(), "⚡");
    }

    #[test]
    fn state_colors() {
        assert_eq!(BadgeState::Ready.color_hex(), "#22c55e");
        assert_eq!(BadgeState::Error { message: "".into() }.color_hex(), "#ef4444");
        assert_eq!(BadgeState::Offline.color_hex(), "#475569");
    }

    #[test]
    fn render_chip_contains_level() {
        let mut b = badge();
        b.set_ready();
        let chip = BadgeRenderer::render_chip(&b);
        assert!(chip.contains("Senior"), "Chip: {}", chip);
    }

    #[test]
    fn render_pill_contains_provider() {
        let mut b = badge();
        b.set_ready();
        let pill = BadgeRenderer::render_pill(&b);
        assert!(pill.contains("builtin"), "Pill: {}", pill);
    }

    #[test]
    fn render_dot_is_html() {
        let b = badge();
        let dot = BadgeRenderer::render_dot(&b);
        assert!(dot.contains("<span"), "Dot: {}", dot);
        assert!(dot.contains("agent-dot"));
    }

    #[test]
    fn agent_card_quality_tier() {
        let badge = badge();
        let card = AgentCard::from_badge(badge).with_score(90, 92.0);
        assert_eq!(card.quality_tier(), "Exceptional");

        let badge2 = AgentBadge::new("s2", "Intern", "Junior", "builtin");
        let card2 = AgentCard::from_badge(badge2).with_score(10, 45.0);
        assert_eq!(card2.quality_tier(), "Developing");
    }

    #[test]
    fn render_card_includes_capabilities() {
        let b = badge();
        let card = AgentCard::from_badge(b)
            .with_capabilities(vec!["layer-ops", "accessibility", "color-gen"])
            .with_score(45, 88.0);
        let rendered = BadgeRenderer::render_card(&card);
        assert!(rendered.contains("layer-ops"), "Card: {}", rendered);
        assert!(rendered.contains("88%") || rendered.contains("88"), "Card: {}", rendered);
    }

    #[test]
    fn aria_label_format() {
        let b = badge();
        let label = BadgeRenderer::aria_label(&b);
        assert!(label.contains("Senior"));
        assert!(label.contains("builtin"));
    }

    #[test]
    fn presence_state_colors() {
        assert_eq!(PresenceState::Online.dot_color(), "#22c55e");
        assert_eq!(PresenceState::Away.dot_color(), "#f59e0b");
        assert_eq!(PresenceState::DoNotDisturb.dot_color(), "#ef4444");
        assert_eq!(PresenceState::Offline.dot_color(), "#6b7280");
    }
}
