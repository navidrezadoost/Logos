//! Unified role hierarchy.
//!
//! Replaces:
//! - `logos_comments::permission::UserRole` — {Viewer, Commenter, Editor, Admin, Owner}
//! - `logos_sync::session::SessionUserRole` — {Editor, Commenter, Viewer, Owner}
//!
//! The unified `Role` enum preserves the same five-tier hierarchy and
//! is `Ord` so roles can be compared by privilege level.

use serde::{Deserialize, Serialize};

/// User role in a document or workspace.
///
/// Roles form a strict hierarchy: `Viewer < Commenter < Editor < Admin < Owner`.
/// Each level inherits all permissions of the levels below it.
///
/// ## Mapping from legacy types
///
/// | `logos_comments::UserRole` | `logos_sync::SessionUserRole` | `Role` |
/// |---------------------------|------------------------------|--------|
/// | `Viewer` | `Viewer` | `Viewer` |
/// | `Commenter` | `Commenter` | `Commenter` |
/// | `Editor` | `Editor` | `Editor` |
/// | `Admin` | — | `Admin` |
/// | `Owner` | `Owner` | `Owner` |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Role {
    /// View-only access. Can see content but cannot modify or comment.
    Viewer = 0,
    /// Can view and add comments, reactions, annotations.
    Commenter = 1,
    /// Full editing privileges on content, plus commenting.
    Editor = 2,
    /// Editor privileges + moderation (delete others' comments, manage users).
    Admin = 3,
    /// Full control — manage permissions, transfer ownership, delete.
    Owner = 4,
}

impl Role {
    /// Whether this role can view content (always true).
    pub fn can_view(&self) -> bool {
        true
    }

    /// Whether this role can create comments, reactions, annotations.
    pub fn can_comment(&self) -> bool {
        *self >= Role::Commenter
    }

    /// Whether this role can edit content (design elements, cells, etc).
    pub fn can_edit(&self) -> bool {
        *self >= Role::Editor
    }

    /// Whether this role can moderate (delete others' content, manage perms).
    pub fn can_moderate(&self) -> bool {
        *self >= Role::Admin
    }

    /// Whether this role is the owner.
    pub fn is_owner(&self) -> bool {
        *self == Role::Owner
    }

    /// Whether this role can manage other users' roles.
    pub fn can_manage_users(&self) -> bool {
        *self >= Role::Admin
    }

    /// Whether this role can export content.
    pub fn can_export(&self) -> bool {
        *self >= Role::Editor
    }

    /// Whether this role can invite new users.
    pub fn can_invite(&self) -> bool {
        *self >= Role::Admin
    }

    /// Whether this role can delete any comment (not just own).
    pub fn can_delete_any_comment(&self) -> bool {
        *self >= Role::Admin
    }

    /// Whether this role can resolve any thread (not just own).
    pub fn can_resolve_any_thread(&self) -> bool {
        *self >= Role::Admin
    }

    /// Numeric rank (0–4) for serialization and comparison.
    pub fn rank(&self) -> u8 {
        *self as u8
    }

    /// Create from numeric rank. Returns None for invalid values.
    pub fn from_rank(rank: u8) -> Option<Self> {
        match rank {
            0 => Some(Role::Viewer),
            1 => Some(Role::Commenter),
            2 => Some(Role::Editor),
            3 => Some(Role::Admin),
            4 => Some(Role::Owner),
            _ => None,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Viewer => "Viewer",
            Self::Commenter => "Commenter",
            Self::Editor => "Editor",
            Self::Admin => "Admin",
            Self::Owner => "Owner",
        }
    }

    /// All role variants in ascending order.
    pub fn all() -> &'static [Role] {
        &[Role::Viewer, Role::Commenter, Role::Editor, Role::Admin, Role::Owner]
    }
}

impl Default for Role {
    fn default() -> Self {
        Role::Viewer
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_hierarchy_ordering() {
        assert!(Role::Viewer < Role::Commenter);
        assert!(Role::Commenter < Role::Editor);
        assert!(Role::Editor < Role::Admin);
        assert!(Role::Admin < Role::Owner);
    }

    #[test]
    fn role_can_view() {
        for role in Role::all() {
            assert!(role.can_view());
        }
    }

    #[test]
    fn role_can_comment() {
        assert!(!Role::Viewer.can_comment());
        assert!(Role::Commenter.can_comment());
        assert!(Role::Editor.can_comment());
        assert!(Role::Admin.can_comment());
        assert!(Role::Owner.can_comment());
    }

    #[test]
    fn role_can_edit() {
        assert!(!Role::Viewer.can_edit());
        assert!(!Role::Commenter.can_edit());
        assert!(Role::Editor.can_edit());
        assert!(Role::Admin.can_edit());
        assert!(Role::Owner.can_edit());
    }

    #[test]
    fn role_can_moderate() {
        assert!(!Role::Viewer.can_moderate());
        assert!(!Role::Commenter.can_moderate());
        assert!(!Role::Editor.can_moderate());
        assert!(Role::Admin.can_moderate());
        assert!(Role::Owner.can_moderate());
    }

    #[test]
    fn role_is_owner() {
        assert!(!Role::Admin.is_owner());
        assert!(Role::Owner.is_owner());
    }

    #[test]
    fn role_rank_roundtrip() {
        for role in Role::all() {
            assert_eq!(Role::from_rank(role.rank()), Some(*role));
        }
        assert_eq!(Role::from_rank(5), None);
        assert_eq!(Role::from_rank(255), None);
    }

    #[test]
    fn role_labels() {
        assert_eq!(Role::Viewer.label(), "Viewer");
        assert_eq!(Role::Commenter.label(), "Commenter");
        assert_eq!(Role::Editor.label(), "Editor");
        assert_eq!(Role::Admin.label(), "Admin");
        assert_eq!(Role::Owner.label(), "Owner");
    }

    #[test]
    fn role_display() {
        assert_eq!(format!("{}", Role::Editor), "Editor");
    }

    #[test]
    fn role_default_is_viewer() {
        assert_eq!(Role::default(), Role::Viewer);
    }

    #[test]
    fn role_serde_roundtrip() {
        for role in Role::all() {
            let json = serde_json::to_string(role).unwrap();
            let back: Role = serde_json::from_str(&json).unwrap();
            assert_eq!(*role, back);
        }
    }

    #[test]
    fn role_all_has_five_variants() {
        assert_eq!(Role::all().len(), 5);
    }

    #[test]
    fn role_can_delete_any_comment() {
        assert!(!Role::Editor.can_delete_any_comment());
        assert!(Role::Admin.can_delete_any_comment());
        assert!(Role::Owner.can_delete_any_comment());
    }

    #[test]
    fn role_can_export() {
        assert!(!Role::Viewer.can_export());
        assert!(!Role::Commenter.can_export());
        assert!(Role::Editor.can_export());
    }

    #[test]
    fn role_can_invite() {
        assert!(!Role::Editor.can_invite());
        assert!(Role::Admin.can_invite());
    }
}
