//! Fine-grained permission system with bitflag-based `PermissionSet`.
//!
//! Replaces:
//! - `logos_comments::CommentPermission` — now derived from `PermissionSet`
//! - `logos_sync::SessionPermission` — 8 boolean flags → `PermissionSet`
//! - `logos_plugins::PermissionKind` — maps to `Permission` variants
//!
//! Each `Permission` maps to a single bit in a `u64`, supporting up to
//! 64 distinct permissions. `PermissionSet` provides fast bitwise ops.

use crate::role::Role;
use serde::{Deserialize, Serialize};

/// A single permission that can be granted or denied.
///
/// Permissions are organized by domain (document, comment, design,
/// spreadsheet, marketplace, admin). Each variant maps to a unique
/// bit position in a `PermissionSet`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    // ── Document (bits 0–7) ──────────────────────────
    ViewDocument,
    EditDocument,
    DeleteDocument,
    ShareDocument,
    ManageDocPermissions,
    ExportDocument,
    TransferOwnership,

    // ── Comment (bits 8–15) ──────────────────────────
    CreateComment,
    EditOwnComment,
    DeleteOwnComment,
    ResolveOwnThread,
    EditAnyComment,
    DeleteAnyComment,
    ResolveAnyThread,
    DeleteThread,

    // ── Design / Component (bits 16–23) ──────────────
    EditComponents,
    EditInstances,
    EditStyles,
    EditPrototypes,
    ManageLibraries,

    // ── Spreadsheet (bits 24–31) ─────────────────────
    EditCells,
    InsertRowsCols,
    DeleteRowsCols,
    ResizeSheet,
    EditFormulas,

    // ── Collaboration (bits 32–35) ───────────────────
    ManageSession,
    InviteUsers,
    KickUsers,
    ChangeRoles,

    // ── Marketplace (bits 36–39) ─────────────────────
    PublishPlugin,
    ApprovePlugin,
    ManagePublishers,

    // ── Admin (bits 40–47) ───────────────────────────
    ViewAuditLogs,
    ManageUsers,
    SystemConfig,
    ViewAnalytics,
}

impl Permission {
    /// Bit position in the `PermissionSet` (0–63).
    pub fn bit_position(&self) -> u32 {
        match self {
            // Document
            Self::ViewDocument => 0,
            Self::EditDocument => 1,
            Self::DeleteDocument => 2,
            Self::ShareDocument => 3,
            Self::ManageDocPermissions => 4,
            Self::ExportDocument => 5,
            Self::TransferOwnership => 6,
            // Comment
            Self::CreateComment => 8,
            Self::EditOwnComment => 9,
            Self::DeleteOwnComment => 10,
            Self::ResolveOwnThread => 11,
            Self::EditAnyComment => 12,
            Self::DeleteAnyComment => 13,
            Self::ResolveAnyThread => 14,
            Self::DeleteThread => 15,
            // Design
            Self::EditComponents => 16,
            Self::EditInstances => 17,
            Self::EditStyles => 18,
            Self::EditPrototypes => 19,
            Self::ManageLibraries => 20,
            // Spreadsheet
            Self::EditCells => 24,
            Self::InsertRowsCols => 25,
            Self::DeleteRowsCols => 26,
            Self::ResizeSheet => 27,
            Self::EditFormulas => 28,
            // Collaboration
            Self::ManageSession => 32,
            Self::InviteUsers => 33,
            Self::KickUsers => 34,
            Self::ChangeRoles => 35,
            // Marketplace
            Self::PublishPlugin => 36,
            Self::ApprovePlugin => 37,
            Self::ManagePublishers => 38,
            // Admin
            Self::ViewAuditLogs => 40,
            Self::ManageUsers => 41,
            Self::SystemConfig => 42,
            Self::ViewAnalytics => 43,
        }
    }

    /// Bitmask for this permission.
    pub fn mask(&self) -> u64 {
        1u64 << self.bit_position()
    }

    /// All permission variants.
    pub fn all_variants() -> &'static [Permission] {
        use Permission::*;
        &[
            ViewDocument, EditDocument, DeleteDocument, ShareDocument,
            ManageDocPermissions, ExportDocument, TransferOwnership,
            CreateComment, EditOwnComment, DeleteOwnComment, ResolveOwnThread,
            EditAnyComment, DeleteAnyComment, ResolveAnyThread, DeleteThread,
            EditComponents, EditInstances, EditStyles, EditPrototypes, ManageLibraries,
            EditCells, InsertRowsCols, DeleteRowsCols, ResizeSheet, EditFormulas,
            ManageSession, InviteUsers, KickUsers, ChangeRoles,
            PublishPlugin, ApprovePlugin, ManagePublishers,
            ViewAuditLogs, ManageUsers, SystemConfig, ViewAnalytics,
        ]
    }
}

// ── PermissionSet ────────────────────────────────────────────────────

/// A compact set of permissions stored as a 64-bit bitmask.
///
/// Supports fast grant/revoke/check operations and set algebra
/// (union, intersection, difference).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionSet {
    bits: u64,
}

impl PermissionSet {
    /// Empty permission set.
    pub const EMPTY: Self = Self { bits: 0 };

    /// Create an empty permission set.
    pub fn new() -> Self {
        Self::EMPTY
    }

    /// Create from raw bits.
    pub fn from_bits(bits: u64) -> Self {
        Self { bits }
    }

    /// Get raw bits.
    pub fn bits(&self) -> u64 {
        self.bits
    }

    /// Check if a permission is granted.
    pub fn has(&self, perm: Permission) -> bool {
        self.bits & perm.mask() != 0
    }

    /// Grant a permission.
    pub fn grant(&mut self, perm: Permission) {
        self.bits |= perm.mask();
    }

    /// Revoke a permission.
    pub fn revoke(&mut self, perm: Permission) {
        self.bits &= !perm.mask();
    }

    /// Toggle a permission.
    pub fn toggle(&mut self, perm: Permission) {
        self.bits ^= perm.mask();
    }

    /// Union of two sets (all permissions in either set).
    pub fn union(&self, other: &Self) -> Self {
        Self { bits: self.bits | other.bits }
    }

    /// Intersection of two sets (only permissions in both sets).
    pub fn intersection(&self, other: &Self) -> Self {
        Self { bits: self.bits & other.bits }
    }

    /// Difference: permissions in self but not in other.
    pub fn difference(&self, other: &Self) -> Self {
        Self { bits: self.bits & !other.bits }
    }

    /// Whether this set is a superset (contains all permissions of other).
    pub fn contains_all(&self, other: &Self) -> bool {
        self.bits & other.bits == other.bits
    }

    /// Number of granted permissions.
    pub fn count(&self) -> u32 {
        self.bits.count_ones()
    }

    /// Whether no permissions are granted.
    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// List all granted permissions.
    pub fn granted_permissions(&self) -> Vec<Permission> {
        Permission::all_variants()
            .iter()
            .filter(|p| self.has(**p))
            .copied()
            .collect()
    }

    /// Default permission set for a given role.
    ///
    /// Role permissions are cumulative — each role inherits from the
    /// level below it.
    pub fn for_role(role: Role) -> Self {
        match role {
            Role::Viewer => VIEWER_PERMS,
            Role::Commenter => COMMENTER_PERMS,
            Role::Editor => EDITOR_PERMS,
            Role::Admin => ADMIN_PERMS,
            Role::Owner => OWNER_PERMS,
        }
    }
}

impl Default for PermissionSet {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl std::fmt::Display for PermissionSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PermissionSet({} perms, 0x{:016x})", self.count(), self.bits)
    }
}

// ── Pre-computed role permission sets ────────────────────────────────

const fn role_bits(permissions: &[Permission]) -> u64 {
    let mut bits = 0u64;
    let mut i = 0;
    while i < permissions.len() {
        bits |= 1u64 << permissions[i].bit_position_const();
        i += 1;
    }
    bits
}

// Const-compatible bit_position (mirrors the match in Permission::bit_position)
impl Permission {
    const fn bit_position_const(&self) -> u32 {
        match self {
            Self::ViewDocument => 0,
            Self::EditDocument => 1,
            Self::DeleteDocument => 2,
            Self::ShareDocument => 3,
            Self::ManageDocPermissions => 4,
            Self::ExportDocument => 5,
            Self::TransferOwnership => 6,
            Self::CreateComment => 8,
            Self::EditOwnComment => 9,
            Self::DeleteOwnComment => 10,
            Self::ResolveOwnThread => 11,
            Self::EditAnyComment => 12,
            Self::DeleteAnyComment => 13,
            Self::ResolveAnyThread => 14,
            Self::DeleteThread => 15,
            Self::EditComponents => 16,
            Self::EditInstances => 17,
            Self::EditStyles => 18,
            Self::EditPrototypes => 19,
            Self::ManageLibraries => 20,
            Self::EditCells => 24,
            Self::InsertRowsCols => 25,
            Self::DeleteRowsCols => 26,
            Self::ResizeSheet => 27,
            Self::EditFormulas => 28,
            Self::ManageSession => 32,
            Self::InviteUsers => 33,
            Self::KickUsers => 34,
            Self::ChangeRoles => 35,
            Self::PublishPlugin => 36,
            Self::ApprovePlugin => 37,
            Self::ManagePublishers => 38,
            Self::ViewAuditLogs => 40,
            Self::ManageUsers => 41,
            Self::SystemConfig => 42,
            Self::ViewAnalytics => 43,
        }
    }
}

/// Viewer: view only
const VIEWER_PERMS: PermissionSet = PermissionSet {
    bits: role_bits(&[Permission::ViewDocument]),
};

/// Commenter: view + comment operations
const COMMENTER_PERMS: PermissionSet = PermissionSet {
    bits: role_bits(&[
        Permission::ViewDocument,
        Permission::CreateComment,
        Permission::EditOwnComment,
        Permission::DeleteOwnComment,
        Permission::ResolveOwnThread,
    ]),
};

/// Editor: commenter + edit content + export
const EDITOR_PERMS: PermissionSet = PermissionSet {
    bits: role_bits(&[
        // Inherit commenter
        Permission::ViewDocument,
        Permission::CreateComment,
        Permission::EditOwnComment,
        Permission::DeleteOwnComment,
        Permission::ResolveOwnThread,
        // Edit
        Permission::EditDocument,
        Permission::ExportDocument,
        Permission::ShareDocument,
        // Design
        Permission::EditComponents,
        Permission::EditInstances,
        Permission::EditStyles,
        Permission::EditPrototypes,
        // Spreadsheet
        Permission::EditCells,
        Permission::InsertRowsCols,
        Permission::DeleteRowsCols,
        Permission::ResizeSheet,
        Permission::EditFormulas,
    ]),
};

/// Admin: editor + moderation + management
const ADMIN_PERMS: PermissionSet = PermissionSet {
    bits: role_bits(&[
        // Inherit editor
        Permission::ViewDocument,
        Permission::CreateComment,
        Permission::EditOwnComment,
        Permission::DeleteOwnComment,
        Permission::ResolveOwnThread,
        Permission::EditDocument,
        Permission::ExportDocument,
        Permission::ShareDocument,
        Permission::EditComponents,
        Permission::EditInstances,
        Permission::EditStyles,
        Permission::EditPrototypes,
        Permission::EditCells,
        Permission::InsertRowsCols,
        Permission::DeleteRowsCols,
        Permission::ResizeSheet,
        Permission::EditFormulas,
        // Admin-specific
        Permission::ManageDocPermissions,
        Permission::EditAnyComment,
        Permission::DeleteAnyComment,
        Permission::ResolveAnyThread,
        Permission::DeleteThread,
        Permission::ManageLibraries,
        Permission::ManageSession,
        Permission::InviteUsers,
        Permission::KickUsers,
        Permission::ChangeRoles,
        Permission::ViewAuditLogs,
        Permission::ManageUsers,
        Permission::ViewAnalytics,
    ]),
};

/// Owner: admin + ownership operations
const OWNER_PERMS: PermissionSet = PermissionSet {
    bits: role_bits(&[
        // Inherit admin
        Permission::ViewDocument,
        Permission::CreateComment,
        Permission::EditOwnComment,
        Permission::DeleteOwnComment,
        Permission::ResolveOwnThread,
        Permission::EditDocument,
        Permission::ExportDocument,
        Permission::ShareDocument,
        Permission::EditComponents,
        Permission::EditInstances,
        Permission::EditStyles,
        Permission::EditPrototypes,
        Permission::EditCells,
        Permission::InsertRowsCols,
        Permission::DeleteRowsCols,
        Permission::ResizeSheet,
        Permission::EditFormulas,
        Permission::ManageDocPermissions,
        Permission::EditAnyComment,
        Permission::DeleteAnyComment,
        Permission::ResolveAnyThread,
        Permission::DeleteThread,
        Permission::ManageLibraries,
        Permission::ManageSession,
        Permission::InviteUsers,
        Permission::KickUsers,
        Permission::ChangeRoles,
        Permission::ViewAuditLogs,
        Permission::ManageUsers,
        Permission::ViewAnalytics,
        // Owner-only
        Permission::DeleteDocument,
        Permission::TransferOwnership,
        Permission::PublishPlugin,
        Permission::ApprovePlugin,
        Permission::ManagePublishers,
        Permission::SystemConfig,
    ]),
};

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_bit_positions_unique() {
        let variants = Permission::all_variants();
        let mut positions = std::collections::HashSet::new();
        for p in variants {
            let pos = p.bit_position();
            assert!(positions.insert(pos), "Duplicate bit position {} for {:?}", pos, p);
        }
    }

    #[test]
    fn permission_mask_is_single_bit() {
        for p in Permission::all_variants() {
            assert_eq!(p.mask().count_ones(), 1);
        }
    }

    #[test]
    fn permission_set_empty() {
        let ps = PermissionSet::new();
        assert!(ps.is_empty());
        assert_eq!(ps.count(), 0);
        assert!(!ps.has(Permission::ViewDocument));
    }

    #[test]
    fn permission_set_grant_revoke() {
        let mut ps = PermissionSet::new();
        ps.grant(Permission::ViewDocument);
        assert!(ps.has(Permission::ViewDocument));
        assert_eq!(ps.count(), 1);

        ps.grant(Permission::EditDocument);
        assert_eq!(ps.count(), 2);

        ps.revoke(Permission::ViewDocument);
        assert!(!ps.has(Permission::ViewDocument));
        assert!(ps.has(Permission::EditDocument));
        assert_eq!(ps.count(), 1);
    }

    #[test]
    fn permission_set_toggle() {
        let mut ps = PermissionSet::new();
        ps.toggle(Permission::EditCells);
        assert!(ps.has(Permission::EditCells));
        ps.toggle(Permission::EditCells);
        assert!(!ps.has(Permission::EditCells));
    }

    #[test]
    fn permission_set_union() {
        let mut a = PermissionSet::new();
        a.grant(Permission::ViewDocument);
        let mut b = PermissionSet::new();
        b.grant(Permission::EditDocument);
        let c = a.union(&b);
        assert!(c.has(Permission::ViewDocument));
        assert!(c.has(Permission::EditDocument));
        assert_eq!(c.count(), 2);
    }

    #[test]
    fn permission_set_intersection() {
        let mut a = PermissionSet::new();
        a.grant(Permission::ViewDocument);
        a.grant(Permission::EditDocument);
        let mut b = PermissionSet::new();
        b.grant(Permission::EditDocument);
        b.grant(Permission::DeleteDocument);
        let c = a.intersection(&b);
        assert!(!c.has(Permission::ViewDocument));
        assert!(c.has(Permission::EditDocument));
        assert!(!c.has(Permission::DeleteDocument));
    }

    #[test]
    fn permission_set_difference() {
        let mut a = PermissionSet::new();
        a.grant(Permission::ViewDocument);
        a.grant(Permission::EditDocument);
        let mut b = PermissionSet::new();
        b.grant(Permission::EditDocument);
        let c = a.difference(&b);
        assert!(c.has(Permission::ViewDocument));
        assert!(!c.has(Permission::EditDocument));
    }

    #[test]
    fn permission_set_contains_all() {
        let editor = PermissionSet::for_role(Role::Editor);
        let viewer = PermissionSet::for_role(Role::Viewer);
        assert!(editor.contains_all(&viewer));
        assert!(!viewer.contains_all(&editor));
    }

    #[test]
    fn viewer_permissions() {
        let ps = PermissionSet::for_role(Role::Viewer);
        assert!(ps.has(Permission::ViewDocument));
        assert!(!ps.has(Permission::EditDocument));
        assert!(!ps.has(Permission::CreateComment));
        assert_eq!(ps.count(), 1);
    }

    #[test]
    fn commenter_permissions() {
        let ps = PermissionSet::for_role(Role::Commenter);
        assert!(ps.has(Permission::ViewDocument));
        assert!(ps.has(Permission::CreateComment));
        assert!(ps.has(Permission::EditOwnComment));
        assert!(ps.has(Permission::DeleteOwnComment));
        assert!(ps.has(Permission::ResolveOwnThread));
        assert!(!ps.has(Permission::EditDocument));
        assert!(!ps.has(Permission::EditAnyComment));
    }

    #[test]
    fn editor_permissions() {
        let ps = PermissionSet::for_role(Role::Editor);
        // Inherits commenter
        assert!(ps.has(Permission::CreateComment));
        // Editor-specific
        assert!(ps.has(Permission::EditDocument));
        assert!(ps.has(Permission::ExportDocument));
        assert!(ps.has(Permission::EditComponents));
        assert!(ps.has(Permission::EditCells));
        assert!(ps.has(Permission::InsertRowsCols));
        // Not admin
        assert!(!ps.has(Permission::DeleteAnyComment));
        assert!(!ps.has(Permission::ManageUsers));
    }

    #[test]
    fn admin_permissions() {
        let ps = PermissionSet::for_role(Role::Admin);
        // Inherits editor
        assert!(ps.has(Permission::EditDocument));
        assert!(ps.has(Permission::EditCells));
        // Admin-specific
        assert!(ps.has(Permission::DeleteAnyComment));
        assert!(ps.has(Permission::ManageSession));
        assert!(ps.has(Permission::InviteUsers));
        assert!(ps.has(Permission::ManageUsers));
        // Not owner
        assert!(!ps.has(Permission::DeleteDocument));
        assert!(!ps.has(Permission::TransferOwnership));
    }

    #[test]
    fn owner_permissions() {
        let ps = PermissionSet::for_role(Role::Owner);
        // Owner has everything
        assert!(ps.has(Permission::DeleteDocument));
        assert!(ps.has(Permission::TransferOwnership));
        assert!(ps.has(Permission::SystemConfig));
        assert!(ps.has(Permission::ManagePublishers));
    }

    #[test]
    fn role_hierarchy_superset() {
        let roles = Role::all();
        for i in 1..roles.len() {
            let higher = PermissionSet::for_role(roles[i]);
            let lower = PermissionSet::for_role(roles[i - 1]);
            assert!(
                higher.contains_all(&lower),
                "{:?} should contain all permissions of {:?}",
                roles[i],
                roles[i - 1]
            );
        }
    }

    #[test]
    fn permission_set_granted_permissions() {
        let ps = PermissionSet::for_role(Role::Commenter);
        let granted = ps.granted_permissions();
        assert_eq!(granted.len() as u32, ps.count());
        assert!(granted.contains(&Permission::ViewDocument));
        assert!(granted.contains(&Permission::CreateComment));
    }

    #[test]
    fn permission_set_serde_roundtrip() {
        let ps = PermissionSet::for_role(Role::Editor);
        let json = serde_json::to_string(&ps).unwrap();
        let back: PermissionSet = serde_json::from_str(&json).unwrap();
        assert_eq!(ps, back);
    }

    #[test]
    fn permission_set_display() {
        let ps = PermissionSet::for_role(Role::Viewer);
        let s = ps.to_string();
        assert!(s.contains("1 perms"));
    }

    #[test]
    fn permission_set_from_bits() {
        let ps = PermissionSet::for_role(Role::Editor);
        let ps2 = PermissionSet::from_bits(ps.bits());
        assert_eq!(ps, ps2);
    }

    #[test]
    fn maps_session_permission_flags() {
        // Verify the editor PermissionSet covers all SessionPermission flags
        let editor = PermissionSet::for_role(Role::Editor);
        assert!(editor.has(Permission::EditComponents));   // can_edit_components
        assert!(editor.has(Permission::EditInstances));    // can_edit_instances
        assert!(editor.has(Permission::EditStyles));       // can_edit_styles
        assert!(editor.has(Permission::EditPrototypes));   // can_edit_prototypes
        assert!(editor.has(Permission::ExportDocument));   // can_export
        assert!(editor.has(Permission::CreateComment));    // can_comment
        // These are admin-only in the new model
        assert!(!editor.has(Permission::ManageLibraries)); // can_manage_libraries → admin
        assert!(!editor.has(Permission::InviteUsers));     // can_invite → admin
    }
}
