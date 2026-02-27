//! # Session Management
//!
//! Collaborative session orchestration — tracks users, permissions,
//! activity state, and document-level session lifecycle.
//!
//! Uses `logos_identity::Role` as the canonical role type. The local
//! `SessionUserRole` alias preserves backward compatibility.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── Identifiers ──────────────────────────────────────────────────────

/// Unique identifier for a collaboration session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

// ── User Role & Permissions ──────────────────────────────────────────

/// Role of a user in a collaboration session — now a re-export of the canonical type.
pub type SessionUserRole = logos_identity::Role;

/// Extension trait for session-specific role queries.
pub trait SessionRoleExt {
    fn can_manage(&self) -> bool;
}

impl SessionRoleExt for SessionUserRole {
    fn can_manage(&self) -> bool {
        self.is_owner()
    }
}

/// Granular permission flags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionPermission {
    pub can_edit_components: bool,
    pub can_edit_instances: bool,
    pub can_edit_styles: bool,
    pub can_edit_prototypes: bool,
    pub can_manage_libraries: bool,
    pub can_export: bool,
    pub can_invite: bool,
    pub can_comment: bool,
}

impl SessionPermission {
    pub fn from_role(role: SessionUserRole) -> Self {
        match role {
            SessionUserRole::Owner => Self {
                can_edit_components: true,
                can_edit_instances: true,
                can_edit_styles: true,
                can_edit_prototypes: true,
                can_manage_libraries: true,
                can_export: true,
                can_invite: true,
                can_comment: true,
            },
            SessionUserRole::Admin => Self {
                can_edit_components: true,
                can_edit_instances: true,
                can_edit_styles: true,
                can_edit_prototypes: true,
                can_manage_libraries: true,
                can_export: true,
                can_invite: true,
                can_comment: true,
            },
            SessionUserRole::Editor => Self {
                can_edit_components: true,
                can_edit_instances: true,
                can_edit_styles: true,
                can_edit_prototypes: true,
                can_manage_libraries: false,
                can_export: true,
                can_invite: false,
                can_comment: true,
            },
            SessionUserRole::Commenter => Self {
                can_edit_components: false,
                can_edit_instances: false,
                can_edit_styles: false,
                can_edit_prototypes: false,
                can_manage_libraries: false,
                can_export: false,
                can_invite: false,
                can_comment: true,
            },
            SessionUserRole::Viewer => Self {
                can_edit_components: false,
                can_edit_instances: false,
                can_edit_styles: false,
                can_edit_prototypes: false,
                can_manage_libraries: false,
                can_export: false,
                can_invite: false,
                can_comment: false,
            },
        }
    }
}

// ── Session User ─────────────────────────────────────────────────────

/// A user in a collaboration session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionUser {
    pub user_id: Uuid,
    pub name: String,
    pub color: [f32; 4],
    pub role: SessionUserRole,
    pub permissions: SessionPermission,
    pub joined_at: u64,
    pub last_active: u64,
    pub active_page: Option<Uuid>,
    pub is_online: bool,
}

impl SessionUser {
    pub fn new(
        user_id: Uuid,
        name: impl Into<String>,
        role: SessionUserRole,
        timestamp: u64,
    ) -> Self {
        // Deterministic color from user ID
        let bytes = user_id.as_bytes();
        let hue = (bytes[0] as f32 / 255.0) * 360.0;
        let color = hsl_to_rgba(hue, 0.7, 0.5);

        Self {
            user_id,
            name: name.into(),
            color,
            role,
            permissions: SessionPermission::from_role(role),
            joined_at: timestamp,
            last_active: timestamp,
            active_page: None,
            is_online: true,
        }
    }

    pub fn touch(&mut self, timestamp: u64) {
        self.last_active = timestamp;
    }

    pub fn set_page(&mut self, page_id: Option<Uuid>) {
        self.active_page = page_id;
    }

    pub fn go_offline(&mut self) {
        self.is_online = false;
    }

    pub fn go_online(&mut self, timestamp: u64) {
        self.is_online = true;
        self.last_active = timestamp;
    }

    /// Duration since last activity.
    pub fn idle_duration(&self, now: u64) -> u64 {
        now.saturating_sub(self.last_active)
    }
}

// ── Session Events ───────────────────────────────────────────────────

/// Events emitted by a collaboration session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SessionEvent {
    /// A user joined the session.
    UserJoined {
        user_id: Uuid,
        name: String,
        role: SessionUserRole,
    },
    /// A user left the session.
    UserLeft {
        user_id: Uuid,
    },
    /// A user's role changed.
    RoleChanged {
        user_id: Uuid,
        old_role: SessionUserRole,
        new_role: SessionUserRole,
    },
    /// A user switched pages.
    PageChanged {
        user_id: Uuid,
        page_id: Option<Uuid>,
    },
    /// A user went offline.
    UserOffline {
        user_id: Uuid,
    },
    /// A user came back online.
    UserOnline {
        user_id: Uuid,
    },
    /// Session ended.
    SessionEnded,
}

// ── Session Config ───────────────────────────────────────────────────

/// Configuration for a collaboration session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Maximum concurrent users.
    pub max_users: usize,
    /// Idle timeout (seconds) before marking a user offline.
    pub idle_timeout_secs: u64,
    /// Whether to allow anonymous viewers.
    pub allow_anonymous: bool,
    /// Whether link sharing is enabled.
    pub link_sharing: bool,
    /// Default role for invited users.
    pub default_role: SessionUserRole,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_users: 50,
            idle_timeout_secs: 300,
            allow_anonymous: false,
            link_sharing: false,
            default_role: SessionUserRole::Editor,
        }
    }
}

// ── Collaboration Session ────────────────────────────────────────────

/// A collaboration session for a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabSession {
    pub id: SessionId,
    pub document_id: Uuid,
    pub config: SessionConfig,
    pub users: HashMap<Uuid, SessionUser>,
    pub created_at: u64,
    pub events: Vec<SessionEvent>,
}

impl CollabSession {
    pub fn new(document_id: Uuid, config: SessionConfig, timestamp: u64) -> Self {
        Self {
            id: SessionId::new(),
            document_id,
            config,
            users: HashMap::new(),
            created_at: timestamp,
            events: Vec::new(),
        }
    }

    /// Create a session with default config.
    pub fn with_defaults(document_id: Uuid, timestamp: u64) -> Self {
        Self::new(document_id, SessionConfig::default(), timestamp)
    }

    /// Add a user to the session.
    pub fn join(
        &mut self,
        user_id: Uuid,
        name: impl Into<String>,
        role: SessionUserRole,
        timestamp: u64,
    ) -> Result<(), String> {
        if self.users.len() >= self.config.max_users {
            return Err("Session is full".into());
        }
        if self.users.contains_key(&user_id) {
            return Err("User already in session".into());
        }

        let name = name.into();
        self.events.push(SessionEvent::UserJoined {
            user_id,
            name: name.clone(),
            role,
        });
        self.users
            .insert(user_id, SessionUser::new(user_id, name, role, timestamp));
        Ok(())
    }

    /// Remove a user from the session.
    pub fn leave(&mut self, user_id: Uuid) -> Option<SessionUser> {
        let user = self.users.remove(&user_id);
        if user.is_some() {
            self.events.push(SessionEvent::UserLeft { user_id });
        }
        user
    }

    /// Change a user's role.
    pub fn change_role(
        &mut self,
        user_id: Uuid,
        new_role: SessionUserRole,
    ) -> Result<(), String> {
        let user = self
            .users
            .get_mut(&user_id)
            .ok_or_else(|| "User not found".to_string())?;
        let old_role = user.role;
        user.role = new_role;
        user.permissions = SessionPermission::from_role(new_role);
        self.events.push(SessionEvent::RoleChanged {
            user_id,
            old_role,
            new_role,
        });
        Ok(())
    }

    /// Record that a user switched pages.
    pub fn set_user_page(&mut self, user_id: Uuid, page_id: Option<Uuid>) {
        if let Some(user) = self.users.get_mut(&user_id) {
            user.set_page(page_id);
            self.events.push(SessionEvent::PageChanged {
                user_id,
                page_id,
            });
        }
    }

    /// Mark a user as offline.
    pub fn mark_offline(&mut self, user_id: Uuid) {
        if let Some(user) = self.users.get_mut(&user_id) {
            user.go_offline();
            self.events.push(SessionEvent::UserOffline { user_id });
        }
    }

    /// Mark a user as online.
    pub fn mark_online(&mut self, user_id: Uuid, timestamp: u64) {
        if let Some(user) = self.users.get_mut(&user_id) {
            user.go_online(timestamp);
            self.events.push(SessionEvent::UserOnline { user_id });
        }
    }

    /// Touch a user's activity timestamp.
    pub fn touch_user(&mut self, user_id: Uuid, timestamp: u64) {
        if let Some(user) = self.users.get_mut(&user_id) {
            user.touch(timestamp);
        }
    }

    /// Check for idle users and mark them offline.
    pub fn check_idle(&mut self, now: u64) -> Vec<Uuid> {
        let timeout = self.config.idle_timeout_secs;
        let idle: Vec<Uuid> = self
            .users
            .values()
            .filter(|u| u.is_online && u.idle_duration(now) > timeout)
            .map(|u| u.user_id)
            .collect();

        for uid in &idle {
            self.mark_offline(*uid);
        }
        idle
    }

    /// Get a user.
    pub fn get_user(&self, user_id: Uuid) -> Option<&SessionUser> {
        self.users.get(&user_id)
    }

    /// Get online users.
    pub fn online_users(&self) -> Vec<&SessionUser> {
        self.users.values().filter(|u| u.is_online).collect()
    }

    /// Get users on a specific page.
    pub fn users_on_page(&self, page_id: Uuid) -> Vec<&SessionUser> {
        self.users
            .values()
            .filter(|u| u.active_page == Some(page_id))
            .collect()
    }

    /// Count users.
    pub fn user_count(&self) -> usize {
        self.users.len()
    }

    /// Count online users.
    pub fn online_count(&self) -> usize {
        self.users.values().filter(|u| u.is_online).count()
    }

    /// End the session.
    pub fn end(&mut self) {
        self.events.push(SessionEvent::SessionEnded);
    }

    /// Get recent events (last N).
    pub fn recent_events(&self, count: usize) -> &[SessionEvent] {
        let start = self.events.len().saturating_sub(count);
        &self.events[start..]
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn hsl_to_rgba(h: f32, s: f32, l: f32) -> [f32; 4] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    [r + m, g + m, b + m, 1.0]
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> Uuid {
        Uuid::from_bytes([1; 16])
    }

    fn bob() -> Uuid {
        Uuid::from_bytes([2; 16])
    }

    fn doc_id() -> Uuid {
        Uuid::from_bytes([10; 16])
    }

    #[test]
    fn test_session_creation() {
        let sess = CollabSession::with_defaults(doc_id(), 1000);
        assert_eq!(sess.document_id, doc_id());
        assert_eq!(sess.user_count(), 0);
    }

    #[test]
    fn test_session_join() {
        let mut sess = CollabSession::with_defaults(doc_id(), 1000);
        sess.join(alice(), "Alice", SessionUserRole::Editor, 1000)
            .unwrap();
        assert_eq!(sess.user_count(), 1);
        assert_eq!(sess.online_count(), 1);
    }

    #[test]
    fn test_session_join_duplicate() {
        let mut sess = CollabSession::with_defaults(doc_id(), 1000);
        sess.join(alice(), "Alice", SessionUserRole::Editor, 1000)
            .unwrap();
        let result = sess.join(alice(), "Alice", SessionUserRole::Editor, 1001);
        assert!(result.is_err());
    }

    #[test]
    fn test_session_full() {
        let config = SessionConfig {
            max_users: 1,
            ..Default::default()
        };
        let mut sess = CollabSession::new(doc_id(), config, 1000);
        sess.join(alice(), "Alice", SessionUserRole::Editor, 1000)
            .unwrap();
        let result = sess.join(bob(), "Bob", SessionUserRole::Editor, 1001);
        assert!(result.is_err());
    }

    #[test]
    fn test_session_leave() {
        let mut sess = CollabSession::with_defaults(doc_id(), 1000);
        sess.join(alice(), "Alice", SessionUserRole::Editor, 1000)
            .unwrap();
        let user = sess.leave(alice());
        assert!(user.is_some());
        assert_eq!(sess.user_count(), 0);
    }

    #[test]
    fn test_session_change_role() {
        let mut sess = CollabSession::with_defaults(doc_id(), 1000);
        sess.join(alice(), "Alice", SessionUserRole::Viewer, 1000)
            .unwrap();
        sess.change_role(alice(), SessionUserRole::Editor).unwrap();
        assert_eq!(
            sess.get_user(alice()).unwrap().role,
            SessionUserRole::Editor
        );
        assert!(sess.get_user(alice()).unwrap().permissions.can_edit_components);
    }

    #[test]
    fn test_session_mark_offline_online() {
        let mut sess = CollabSession::with_defaults(doc_id(), 1000);
        sess.join(alice(), "Alice", SessionUserRole::Editor, 1000)
            .unwrap();
        sess.mark_offline(alice());
        assert!(!sess.get_user(alice()).unwrap().is_online);
        assert_eq!(sess.online_count(), 0);

        sess.mark_online(alice(), 1001);
        assert!(sess.get_user(alice()).unwrap().is_online);
    }

    #[test]
    fn test_session_page_tracking() {
        let mut sess = CollabSession::with_defaults(doc_id(), 1000);
        sess.join(alice(), "Alice", SessionUserRole::Editor, 1000)
            .unwrap();
        let page = Uuid::new_v4();
        sess.set_user_page(alice(), Some(page));
        assert_eq!(sess.users_on_page(page).len(), 1);
    }

    #[test]
    fn test_session_idle_check() {
        let mut sess = CollabSession::with_defaults(doc_id(), 1000);
        sess.join(alice(), "Alice", SessionUserRole::Editor, 1000)
            .unwrap();
        sess.join(bob(), "Bob", SessionUserRole::Editor, 1000)
            .unwrap();
        sess.touch_user(bob(), 1200);

        // Check at time 1301 (idle_timeout = 300)
        let idle = sess.check_idle(1301);
        assert_eq!(idle.len(), 1);
        assert_eq!(idle[0], alice());
    }

    #[test]
    fn test_session_events() {
        let mut sess = CollabSession::with_defaults(doc_id(), 1000);
        sess.join(alice(), "Alice", SessionUserRole::Editor, 1000)
            .unwrap();
        sess.leave(alice());
        sess.end();

        assert!(sess.events.len() >= 3);
        assert!(matches!(
            sess.events.last(),
            Some(SessionEvent::SessionEnded)
        ));
    }

    #[test]
    fn test_role_permissions() {
        assert!(SessionUserRole::Owner.can_edit());
        assert!(SessionUserRole::Owner.can_comment());
        assert!(SessionUserRole::Owner.can_manage());

        assert!(SessionUserRole::Editor.can_edit());
        assert!(SessionUserRole::Editor.can_comment());
        assert!(!SessionUserRole::Editor.can_manage());

        assert!(!SessionUserRole::Commenter.can_edit());
        assert!(SessionUserRole::Commenter.can_comment());

        assert!(!SessionUserRole::Viewer.can_edit());
        assert!(!SessionUserRole::Viewer.can_comment());
    }

    #[test]
    fn test_user_idle_duration() {
        let user = SessionUser::new(alice(), "Alice", SessionUserRole::Editor, 1000);
        assert_eq!(user.idle_duration(1050), 50);
    }

    #[test]
    fn test_user_color_deterministic() {
        let u1 = SessionUser::new(alice(), "A", SessionUserRole::Editor, 0);
        let u2 = SessionUser::new(alice(), "A", SessionUserRole::Editor, 0);
        assert_eq!(u1.color, u2.color);
    }

    #[test]
    fn test_session_config_default() {
        let cfg = SessionConfig::default();
        assert_eq!(cfg.max_users, 50);
        assert_eq!(cfg.idle_timeout_secs, 300);
        assert!(!cfg.allow_anonymous);
    }

    #[test]
    fn test_session_recent_events() {
        let mut sess = CollabSession::with_defaults(doc_id(), 1000);
        for i in 0..10 {
            sess.join(
                Uuid::from_bytes([i; 16]),
                format!("User{}", i),
                SessionUserRole::Viewer,
                1000 + i as u64,
            )
            .unwrap();
        }
        let recent = sess.recent_events(3);
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn test_session_serde_roundtrip() {
        let mut sess = CollabSession::with_defaults(doc_id(), 1000);
        sess.join(alice(), "Alice", SessionUserRole::Editor, 1000)
            .unwrap();
        let json = serde_json::to_string(&sess).unwrap();
        let back: CollabSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_count(), 1);
        assert_eq!(back.document_id, doc_id());
    }
}
