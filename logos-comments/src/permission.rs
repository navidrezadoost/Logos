//! Permission system for comment operations.
//!
//! Role-based access control determines who can create, edit, delete,
//! and moderate comments.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::Comment;
use crate::ops::CommentOp;

// ── User Roles ───────────────────────────────────────────────────────

/// User role in the design project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UserRole {
    /// View-only access.
    Viewer,
    /// Can comment but not edit designs.
    Commenter,
    /// Can edit designs and comment.
    Editor,
    /// Full control including moderation.
    Admin,
    /// Project owner — all permissions.
    Owner,
}

impl UserRole {
    pub fn can_view(&self) -> bool {
        true // all roles can view
    }

    pub fn can_comment(&self) -> bool {
        !matches!(self, Self::Viewer)
    }

    pub fn can_edit_designs(&self) -> bool {
        matches!(self, Self::Editor | Self::Admin | Self::Owner)
    }

    pub fn can_moderate(&self) -> bool {
        matches!(self, Self::Admin | Self::Owner)
    }

    pub fn can_delete_any_comment(&self) -> bool {
        matches!(self, Self::Admin | Self::Owner)
    }

    pub fn can_resolve_any_thread(&self) -> bool {
        matches!(self, Self::Admin | Self::Owner)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Viewer => "Viewer",
            Self::Commenter => "Commenter",
            Self::Editor => "Editor",
            Self::Admin => "Admin",
            Self::Owner => "Owner",
        }
    }
}

impl Default for UserRole {
    fn default() -> Self {
        Self::Viewer
    }
}

// ── Permissions ──────────────────────────────────────────────────────

/// Fine-grained permission for a specific comment operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommentPermission {
    Allowed,
    Denied,
    /// Allowed only on own comments.
    OwnOnly,
}

// ── Permission Checker ───────────────────────────────────────────────

/// Checks whether a user is authorized to perform a comment operation.
#[derive(Debug, Clone)]
pub struct PermissionChecker {
    /// Map of user_id → role.
    roles: std::collections::HashMap<Uuid, UserRole>,
}

impl PermissionChecker {
    pub fn new() -> Self {
        Self {
            roles: std::collections::HashMap::new(),
        }
    }

    /// Register a user's role.
    pub fn set_role(&mut self, user_id: Uuid, role: UserRole) {
        self.roles.insert(user_id, role);
    }

    /// Get a user's role (defaults to Viewer if not registered).
    pub fn get_role(&self, user_id: Uuid) -> UserRole {
        self.roles.get(&user_id).copied().unwrap_or(UserRole::Viewer)
    }

    /// Check if a user can perform a specific operation.
    pub fn check(&self, user_id: Uuid, op: &CommentOp) -> CommentPermission {
        let role = self.get_role(user_id);

        match op {
            // Creating threads and replying require commenter+
            CommentOp::StartThread { .. } | CommentOp::Reply { .. } => {
                if role.can_comment() {
                    CommentPermission::Allowed
                } else {
                    CommentPermission::Denied
                }
            }

            // Editing comments: own only for commenter/editor, any for admin+
            CommentOp::EditComment { .. } => {
                if role.can_delete_any_comment() {
                    CommentPermission::Allowed
                } else if role.can_comment() {
                    CommentPermission::OwnOnly
                } else {
                    CommentPermission::Denied
                }
            }

            // Deleting comments: own only for commenter/editor, any for admin+
            CommentOp::DeleteComment { .. } => {
                if role.can_delete_any_comment() {
                    CommentPermission::Allowed
                } else if role.can_comment() {
                    CommentPermission::OwnOnly
                } else {
                    CommentPermission::Denied
                }
            }

            // Reactions: any commenter+
            CommentOp::AddReaction { .. } | CommentOp::RemoveReaction { .. } => {
                if role.can_comment() {
                    CommentPermission::Allowed
                } else {
                    CommentPermission::Denied
                }
            }

            // Resolution: admin+ or thread participants
            CommentOp::SetResolution { .. } => {
                if role.can_resolve_any_thread() {
                    CommentPermission::Allowed
                } else if role.can_comment() {
                    CommentPermission::OwnOnly // participant check done at application layer
                } else {
                    CommentPermission::Denied
                }
            }

            // Priority and assignment: editor+
            CommentOp::SetPriority { .. }
            | CommentOp::AssignThread { .. }
            | CommentOp::UnassignThread { .. } => {
                if role.can_edit_designs() {
                    CommentPermission::Allowed
                } else {
                    CommentPermission::Denied
                }
            }

            // Tags: commenter+
            CommentOp::AddTag { .. } | CommentOp::RemoveTag { .. } => {
                if role.can_comment() {
                    CommentPermission::Allowed
                } else {
                    CommentPermission::Denied
                }
            }

            // Annotations: commenter+
            CommentOp::AddAnnotation { .. } => {
                if role.can_comment() {
                    CommentPermission::Allowed
                } else {
                    CommentPermission::Denied
                }
            }

            // Removing annotations: own only for commenter, any for admin+
            CommentOp::RemoveAnnotation { .. } => {
                if role.can_delete_any_comment() {
                    CommentPermission::Allowed
                } else if role.can_comment() {
                    CommentPermission::OwnOnly
                } else {
                    CommentPermission::Denied
                }
            }

            // Deleting threads: admin+ only
            CommentOp::DeleteThread { .. } => {
                if role.can_delete_any_comment() {
                    CommentPermission::Allowed
                } else {
                    CommentPermission::Denied
                }
            }

            // Toggle visibility: commenter+
            CommentOp::ToggleAnnotationVisibility { .. } => {
                if role.can_comment() {
                    CommentPermission::Allowed
                } else {
                    CommentPermission::Denied
                }
            }
        }
    }

    /// Convenience: check if a user can edit a specific comment (ownership check).
    pub fn can_edit_comment(
        &self,
        user_id: Uuid,
        comment: &Comment,
        op: &CommentOp,
    ) -> bool {
        match self.check(user_id, op) {
            CommentPermission::Allowed => true,
            CommentPermission::OwnOnly => comment.author_id == user_id,
            CommentPermission::Denied => false,
        }
    }
}

impl Default for PermissionChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CommentAnchor, CommentId, Comment, ThreadId};

    fn alice() -> Uuid {
        Uuid::from_bytes([1; 16])
    }
    fn bob() -> Uuid {
        Uuid::from_bytes([2; 16])
    }
    fn layer_id() -> Uuid {
        Uuid::from_bytes([10; 16])
    }

    #[test]
    fn viewer_cannot_comment() {
        let mut checker = PermissionChecker::new();
        checker.set_role(alice(), UserRole::Viewer);

        let op = CommentOp::StartThread {
            thread_id: ThreadId::new(),
            anchor: CommentAnchor::layer(layer_id()),
            comment_id: CommentId::new(),
            content: "test".into(),
        };
        assert_eq!(checker.check(alice(), &op), CommentPermission::Denied);
    }

    #[test]
    fn commenter_can_start_thread() {
        let mut checker = PermissionChecker::new();
        checker.set_role(alice(), UserRole::Commenter);

        let op = CommentOp::StartThread {
            thread_id: ThreadId::new(),
            anchor: CommentAnchor::layer(layer_id()),
            comment_id: CommentId::new(),
            content: "test".into(),
        };
        assert_eq!(checker.check(alice(), &op), CommentPermission::Allowed);
    }

    #[test]
    fn commenter_can_only_edit_own() {
        let mut checker = PermissionChecker::new();
        checker.set_role(alice(), UserRole::Commenter);

        let op = CommentOp::EditComment {
            thread_id: ThreadId::new(),
            comment_id: CommentId::new(),
            new_content: "edited".into(),
        };
        assert_eq!(checker.check(alice(), &op), CommentPermission::OwnOnly);

        let alice_comment = Comment::new(alice(), "Alice", "text", 1000);
        assert!(checker.can_edit_comment(alice(), &alice_comment, &op));

        let bob_comment = Comment::new(bob(), "Bob", "text", 1000);
        assert!(!checker.can_edit_comment(alice(), &bob_comment, &op));
    }

    #[test]
    fn admin_can_delete_any() {
        let mut checker = PermissionChecker::new();
        checker.set_role(alice(), UserRole::Admin);

        let op = CommentOp::DeleteComment {
            thread_id: ThreadId::new(),
            comment_id: CommentId::new(),
        };
        assert_eq!(checker.check(alice(), &op), CommentPermission::Allowed);
    }

    #[test]
    fn editor_can_assign() {
        let mut checker = PermissionChecker::new();
        checker.set_role(alice(), UserRole::Editor);

        let op = CommentOp::AssignThread {
            thread_id: ThreadId::new(),
            assignee_id: bob(),
        };
        assert_eq!(checker.check(alice(), &op), CommentPermission::Allowed);
    }

    #[test]
    fn commenter_cannot_assign() {
        let mut checker = PermissionChecker::new();
        checker.set_role(alice(), UserRole::Commenter);

        let op = CommentOp::AssignThread {
            thread_id: ThreadId::new(),
            assignee_id: bob(),
        };
        assert_eq!(checker.check(alice(), &op), CommentPermission::Denied);
    }

    #[test]
    fn role_hierarchy() {
        assert!(UserRole::Owner > UserRole::Admin);
        assert!(UserRole::Admin > UserRole::Editor);
        assert!(UserRole::Editor > UserRole::Commenter);
        assert!(UserRole::Commenter > UserRole::Viewer);
    }

    #[test]
    fn role_labels() {
        assert_eq!(UserRole::Viewer.label(), "Viewer");
        assert_eq!(UserRole::Owner.label(), "Owner");
    }

    #[test]
    fn only_admin_can_delete_thread() {
        let mut checker = PermissionChecker::new();
        checker.set_role(alice(), UserRole::Editor);
        checker.set_role(bob(), UserRole::Admin);

        let op = CommentOp::DeleteThread {
            thread_id: ThreadId::new(),
        };
        assert_eq!(checker.check(alice(), &op), CommentPermission::Denied);
        assert_eq!(checker.check(bob(), &op), CommentPermission::Allowed);
    }
}
