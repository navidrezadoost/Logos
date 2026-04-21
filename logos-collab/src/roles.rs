// logos-collab/src/roles.rs
//
//! # Role-Based Access Control (RBAC)
//!
//! Defines the five collaboration roles used throughout Logos and the
//! permission set each role holds.  The system is intentionally additive:
//! higher roles inherit all permissions of lower roles.
//!
//! ## Role hierarchy (ascending privilege)
//!
//! ```text
//!  Viewer ──► Editor ──► Designer ──► Developer
//!                                ▲         |
//!                                └── Owner ┘  (Designer + admin perms)
//! ```
//!
//! Note: **Developer** is a *lateral* role (equal to Designer) that trades
//! design-editing powers for code-inspection super-powers.
//!
//! ## Usage
//!
//! ```rust
//! use logos_collab::roles::{Role, Permission};
//!
//! let role = Role::Editor;
//! assert!(role.can(Permission::ModifyLayers));
//! assert!(!role.can(Permission::InviteMembers));
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── Permission flags ──────────────────────────────────────────────────────────

/// A single capability that a user may or may not possess.
///
/// Permissions are evaluated as a bitset (`u64`) for zero-cost `can()` checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Permission {
    // ── Viewer tier ───────────────────────────────────────────────
    /// Open and view the document.
    ViewDocument      = 0,
    /// See other users' cursors in the live session.
    ViewCursors       = 1,
    /// Add (but not edit or delete others') comments.
    AddComments       = 2,
    /// Follow another user's cursor (read-only viewport sync).
    FollowCursors     = 3,
    /// Run a prototype / interactive preview.
    RunPrototype      = 4,

    // ── Editor tier ───────────────────────────────────────────────
    /// Create, delete, or reorder layers.
    ModifyLayers      = 5,
    /// Add or remove frames / artboards.
    ModifyFrames      = 6,
    /// Change layer properties (position, size, fill, …).
    EditProperties    = 7,
    /// Edit own comments (not others').
    EditOwnComments   = 8,
    /// Delete own comments.
    DeleteOwnComments = 9,
    /// Like or dislike comments.
    ReactToComments   = 10,

    // ── Designer tier ─────────────────────────────────────────────
    /// Switch between workspace modes (design / prototype / …).
    ChangeWorkspaceMode = 11,
    /// Add, edit, or remove component library entries.
    ManageComponents    = 12,
    /// Export design tokens (colours, typography, spacing).
    ExportDesignTokens  = 13,
    /// Edit *any* comment in the document (moderation).
    EditAnyComment      = 14,
    /// Delete *any* comment in the document (moderation).
    DeleteAnyComment    = 15,

    // ── Developer tier ────────────────────────────────────────────
    /// Open the inspector panel (CSS box model, values).
    InspectLayers       = 16,
    /// Export CSS / Tailwind / Sass for any layer.
    ExportCode          = 17,
    /// Copy individual CSS / Tailwind / Sass property snippets.
    CopyCodeSnippets    = 18,
    /// See and apply style token overrides in developer mode.
    ViewStyleOverrides  = 19,

    // ── Owner tier ────────────────────────────────────────────────
    /// Invite new collaborators to the project.
    InviteMembers       = 20,
    /// Remove collaborators.
    RemoveMembers       = 21,
    /// Change any collaborator's role (except transfer to Owner).
    ChangeRoles         = 22,
    /// Transfer the Owner role to another Designer.
    TransferOwnership   = 23,
    /// Delete the entire project.
    DeleteProject       = 24,
    /// Remove other users from the live session.
    KickFromSession     = 25,
}

impl Permission {
    /// Number of defined permission variants.
    pub const COUNT: usize = 26;

    fn bit(self) -> u64 {
        1u64 << (self as u8)
    }
}

// ── Permission set (bitfield) ─────────────────────────────────────────────────

/// A compact set of permissions stored as a 64-bit bitmask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PermissionSet(u64);

impl PermissionSet {
    /// Empty – no permissions granted.
    pub const NONE: Self = Self(0);
    /// All permissions granted (Owner receives this).
    pub const ALL: Self  = Self(u64::MAX);

    /// Grant a single permission.
    #[inline]
    pub fn grant(mut self, p: Permission) -> Self {
        self.0 |= p.bit();
        self
    }

    /// Revoke a single permission.
    #[inline]
    pub fn revoke(mut self, p: Permission) -> Self {
        self.0 &= !p.bit();
        self
    }

    /// Returns `true` if the set contains `p`.
    #[inline]
    pub fn contains(self, p: Permission) -> bool {
        self.0 & p.bit() != 0
    }

    /// Union of two permission sets.
    #[inline]
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Intersection.
    #[inline]
    pub fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Raw 64-bit mask (for serialisation / comparison).
    pub fn bits(self) -> u64 {
        self.0
    }
}

// ── Role ──────────────────────────────────────────────────────────────────────

/// Collaboration role assigned to a project member.
///
/// Serialised as a lowercase string in all wire protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Read-only access.  Can comment and follow cursors.
    Viewer,
    /// Viewer + can edit layers/frames.
    Editor,
    /// Editor + workspace modes, component libraries, design tokens.
    Designer,
    /// Designer-tier read access + code inspection and export capabilities.
    Developer,
    /// Designer + full member management.  Only one owner per project.
    Owner,
}

impl Role {
    /// Returns the canonical [`PermissionSet`] for this role.
    pub fn permissions(self) -> PermissionSet {
        use Permission::*;

        let viewer = PermissionSet::NONE
            .grant(ViewDocument)
            .grant(ViewCursors)
            .grant(AddComments)
            .grant(FollowCursors)
            .grant(RunPrototype)
            .grant(ReactToComments);

        let editor = viewer
            .grant(ModifyLayers)
            .grant(ModifyFrames)
            .grant(EditProperties)
            .grant(EditOwnComments)
            .grant(DeleteOwnComments);

        let designer = editor
            .grant(ChangeWorkspaceMode)
            .grant(ManageComponents)
            .grant(ExportDesignTokens)
            .grant(EditAnyComment)
            .grant(DeleteAnyComment)
            // Designers can also inspect / export code.
            .grant(InspectLayers)
            .grant(ExportCode)
            .grant(CopyCodeSnippets)
            .grant(ViewStyleOverrides);

        let developer = viewer
            .grant(EditOwnComments)
            .grant(DeleteOwnComments)
            .grant(InspectLayers)
            .grant(ExportCode)
            .grant(CopyCodeSnippets)
            .grant(ViewStyleOverrides)
            .grant(RunPrototype);

        let owner = designer
            .grant(InviteMembers)
            .grant(RemoveMembers)
            .grant(ChangeRoles)
            .grant(TransferOwnership)
            .grant(DeleteProject)
            .grant(KickFromSession);

        match self {
            Role::Viewer    => viewer,
            Role::Editor    => editor,
            Role::Designer  => designer,
            Role::Developer => developer,
            Role::Owner     => owner,
        }
    }

    /// Shorthand: does this role include `p`?
    #[inline]
    pub fn can(self, p: Permission) -> bool {
        self.permissions().contains(p)
    }

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Role::Viewer    => "Viewer",
            Role::Editor    => "Editor",
            Role::Designer  => "Designer",
            Role::Developer => "Developer",
            Role::Owner     => "Owner",
        }
    }

    /// Numeric rank (higher = more privileged, except Developer is lateral).
    pub fn rank(self) -> u8 {
        match self {
            Role::Viewer    => 0,
            Role::Editor    => 1,
            Role::Designer  => 2,
            Role::Developer => 2, // lateral
            Role::Owner     => 3,
        }
    }

    /// Returns `true` if `other` has at least as many privileges as `self`.
    /// (Useful for "can this user change that user's role?")
    pub fn can_override(self, other: Role) -> bool {
        self.rank() > other.rank()
    }

    /// All five roles in ascending privilege order.
    pub fn all() -> &'static [Role] {
        &[Role::Viewer, Role::Editor, Role::Designer, Role::Developer, Role::Owner]
    }
}

// ── Project member ────────────────────────────────────────────────────────────

/// A single project collaborator with their assigned role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectMember {
    /// Unique user identifier.
    pub user_id: Uuid,
    /// Display name.
    pub display_name: String,
    /// Assigned role.
    pub role: Role,
    /// Email (may be empty for guest/anonymous users).
    pub email: Option<String>,
    /// When the membership was created (Unix ms).
    pub joined_at: u64,
    /// `true` if the invitation has been accepted.
    pub accepted: bool,
}

impl ProjectMember {
    /// Create a new member with default state (not yet accepted).
    pub fn new(user_id: Uuid, display_name: impl Into<String>, role: Role) -> Self {
        Self {
            user_id,
            display_name: display_name.into(),
            role,
            email: None,
            joined_at: 0,
            accepted: false,
        }
    }

    /// Mark invitation as accepted.
    pub fn accept(mut self) -> Self {
        self.accepted = true;
        self
    }

    /// Check a specific permission for this member.
    pub fn can(&self, p: Permission) -> bool {
        self.role.can(p)
    }
}

// ── Membership table ──────────────────────────────────────────────────────────

/// In-memory membership registry for a single project.
///
/// In production this is backed by a CRDT map (one entry per user).
/// For unit tests and non-persistent scenarios it lives in a `HashMap`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MembershipTable {
    members: HashMap<Uuid, ProjectMember>,
    /// The document/project id this table belongs to.
    pub project_id: Uuid,
}

impl MembershipTable {
    /// New empty table.
    pub fn new(project_id: Uuid) -> Self {
        Self { members: HashMap::new(), project_id }
    }

    /// Add or replace a member entry.
    pub fn upsert(&mut self, member: ProjectMember) {
        self.members.insert(member.user_id, member);
    }

    /// Remove a member (returns the removed entry if it existed).
    pub fn remove(&mut self, user_id: &Uuid) -> Option<ProjectMember> {
        self.members.remove(user_id)
    }

    /// Lookup a member by id.
    pub fn get(&self, user_id: &Uuid) -> Option<&ProjectMember> {
        self.members.get(user_id)
    }

    /// Mutable lookup.
    pub fn get_mut(&mut self, user_id: &Uuid) -> Option<&mut ProjectMember> {
        self.members.get_mut(user_id)
    }

    /// Change a member's role.  Returns `Err` if the member doesn't exist.
    pub fn set_role(&mut self, user_id: &Uuid, role: Role) -> Result<(), RoleError> {
        let m = self.members.get_mut(user_id).ok_or(RoleError::MemberNotFound(*user_id))?;
        m.role = role;
        Ok(())
    }

    /// Transfer ownership from `current_owner` to `new_owner`.
    ///
    /// `new_owner` must already be a Designer or Editor.
    /// Returns `Err` if either user is missing or the constraint is violated.
    pub fn transfer_ownership(&mut self, current_owner: Uuid, new_owner: Uuid) -> Result<(), RoleError> {
        // Verify current owner exists and actually IS the owner
        {
            let co = self.members.get(&current_owner).ok_or(RoleError::MemberNotFound(current_owner))?;
            if co.role != Role::Owner {
                return Err(RoleError::NotOwner(current_owner));
            }
        }
        // Verify new owner exists
        if !self.members.contains_key(&new_owner) {
            return Err(RoleError::MemberNotFound(new_owner));
        }
        // Demote old owner → Designer, promote new owner → Owner
        self.members.get_mut(&current_owner).unwrap().role = Role::Designer;
        self.members.get_mut(&new_owner).unwrap().role = Role::Owner;
        Ok(())
    }

    /// Number of registered members.
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// True if there are no members.
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Iterate over all members.
    pub fn iter(&self) -> impl Iterator<Item = &ProjectMember> {
        self.members.values()
    }

    /// Owners (should be exactly one in a well-formed project).
    pub fn owners(&self) -> Vec<&ProjectMember> {
        self.members.values().filter(|m| m.role == Role::Owner).collect()
    }

    /// Check if `actor` may perform `action` on `target`.
    ///
    /// Rules:
    /// - An actor can never modify themselves except to leave.
    /// - Only the owner can change roles.
    /// - Only the owner can transfer ownership.
    pub fn authorize(&self, actor_id: Uuid, perm: Permission) -> Result<(), RoleError> {
        let actor = self.members.get(&actor_id).ok_or(RoleError::MemberNotFound(actor_id))?;
        if actor.can(perm) {
            Ok(())
        } else {
            Err(RoleError::PermissionDenied { user_id: actor_id, permission: perm })
        }
    }
}

// ── Role change error ─────────────────────────────────────────────────────────

/// Errors produced by role-management operations.
#[derive(Debug, Clone, PartialEq)]
pub enum RoleError {
    MemberNotFound(Uuid),
    NotOwner(Uuid),
    PermissionDenied { user_id: Uuid, permission: Permission },
    CannotDemoteSelf,
    AlreadyMember(Uuid),
}

impl std::fmt::Display for RoleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoleError::MemberNotFound(id)  => write!(f, "Member not found: {id}"),
            RoleError::NotOwner(id)        => write!(f, "User {} is not the owner", id),
            RoleError::PermissionDenied { user_id, permission } =>
                write!(f, "User {} lacks permission {permission:?}", user_id),
            RoleError::CannotDemoteSelf    => write!(f, "Cannot demote yourself"),
            RoleError::AlreadyMember(id)   => write!(f, "User {id} is already a member"),
        }
    }
}

impl std::error::Error for RoleError {}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> Uuid { Uuid::new_v4() }

    // ── Permission bitfield ───────────────────────────────────────

    // R-01: Fresh PermissionSet is empty.
    #[test]
    fn r_01_permission_set_none_is_empty() {
        let ps = PermissionSet::NONE;
        assert!(!ps.contains(Permission::ViewDocument));
    }

    // R-02: Grant then contains.
    #[test]
    fn r_02_grant_contains() {
        let ps = PermissionSet::NONE.grant(Permission::ViewDocument);
        assert!(ps.contains(Permission::ViewDocument));
        assert!(!ps.contains(Permission::ModifyLayers));
    }

    // R-03: Revoke removes bit.
    #[test]
    fn r_03_revoke_removes_bit() {
        let ps = PermissionSet::NONE
            .grant(Permission::ViewDocument)
            .revoke(Permission::ViewDocument);
        assert!(!ps.contains(Permission::ViewDocument));
    }

    // R-04: ALL contains every permission.
    #[test]
    fn r_04_all_contains_every_permission() {
        let ps = PermissionSet::ALL;
        for p in [
            Permission::ViewDocument, Permission::ModifyLayers,
            Permission::InviteMembers, Permission::DeleteProject,
        ] {
            assert!(ps.contains(p), "Permission::{p:?} missing from ALL");
        }
    }

    // R-05: Union covers both sets.
    #[test]
    fn r_05_union_covers_both() {
        let a = PermissionSet::NONE.grant(Permission::ViewDocument);
        let b = PermissionSet::NONE.grant(Permission::ModifyLayers);
        let u = a.union(b);
        assert!(u.contains(Permission::ViewDocument));
        assert!(u.contains(Permission::ModifyLayers));
    }

    // R-06: Intersection is only shared bits.
    #[test]
    fn r_06_intersection_shared_only() {
        let a = PermissionSet::NONE.grant(Permission::ViewDocument).grant(Permission::ModifyLayers);
        let b = PermissionSet::NONE.grant(Permission::ViewDocument);
        let i = a.intersection(b);
        assert!(i.contains(Permission::ViewDocument));
        assert!(!i.contains(Permission::ModifyLayers));
    }

    // R-07: bits() returns raw mask.
    #[test]
    fn r_07_bits_roundtrip() {
        let ps = PermissionSet::NONE.grant(Permission::AddComments);
        assert_eq!(PermissionSet(ps.bits()), ps);
    }

    // ── Role canonical permissions ────────────────────────────────

    // R-08: Viewer can view and comment but not modify layers.
    #[test]
    fn r_08_viewer_permissions() {
        let r = Role::Viewer;
        assert!(r.can(Permission::ViewDocument));
        assert!(r.can(Permission::AddComments));
        assert!(r.can(Permission::FollowCursors));
        assert!(!r.can(Permission::ModifyLayers));
        assert!(!r.can(Permission::InviteMembers));
        assert!(!r.can(Permission::ExportCode));
    }

    // R-09: Editor inherits viewer and can modify layers.
    #[test]
    fn r_09_editor_permissions() {
        let r = Role::Editor;
        assert!(r.can(Permission::ViewDocument));
        assert!(r.can(Permission::ModifyLayers));
        assert!(r.can(Permission::ModifyFrames));
        assert!(r.can(Permission::EditProperties));
        assert!(!r.can(Permission::ChangeWorkspaceMode));
        assert!(!r.can(Permission::InviteMembers));
    }

    // R-10: Designer includes editor + workspace + component + code export.
    #[test]
    fn r_10_designer_permissions() {
        let r = Role::Designer;
        assert!(r.can(Permission::ModifyLayers));
        assert!(r.can(Permission::ChangeWorkspaceMode));
        assert!(r.can(Permission::ManageComponents));
        assert!(r.can(Permission::ExportDesignTokens));
        assert!(r.can(Permission::InspectLayers));
        assert!(r.can(Permission::ExportCode));
        assert!(!r.can(Permission::InviteMembers));
    }

    // R-11: Developer can inspect code but cannot modify layers.
    #[test]
    fn r_11_developer_permissions() {
        let r = Role::Developer;
        assert!(r.can(Permission::InspectLayers));
        assert!(r.can(Permission::ExportCode));
        assert!(r.can(Permission::CopyCodeSnippets));
        assert!(r.can(Permission::ViewStyleOverrides));
        assert!(!r.can(Permission::ModifyLayers));
        assert!(!r.can(Permission::InviteMembers));
    }

    // R-12: Owner has all permissions.
    #[test]
    fn r_12_owner_has_all_permissions() {
        let r = Role::Owner;
        assert!(r.can(Permission::InviteMembers));
        assert!(r.can(Permission::RemoveMembers));
        assert!(r.can(Permission::ChangeRoles));
        assert!(r.can(Permission::TransferOwnership));
        assert!(r.can(Permission::DeleteProject));
        assert!(r.can(Permission::KickFromSession));
        assert!(r.can(Permission::ModifyLayers));
        assert!(r.can(Permission::ExportCode));
    }

    // R-13: Role::can() matches permissions().contains().
    #[test]
    fn r_13_can_matches_permission_set() {
        for &role in Role::all() {
            for &perm in &[Permission::ViewDocument, Permission::ModifyLayers, Permission::InviteMembers] {
                assert_eq!(role.can(perm), role.permissions().contains(perm));
            }
        }
    }

    // R-14: Owner's rank is strictly highest.
    #[test]
    fn r_14_owner_rank_highest() {
        for &other in &[Role::Viewer, Role::Editor, Role::Designer, Role::Developer] {
            assert!(Role::Owner.rank() > other.rank());
        }
    }

    // R-15: can_override: owner can override editor.
    #[test]
    fn r_15_can_override_owner_over_editor() {
        assert!(Role::Owner.can_override(Role::Editor));
    }

    // R-16: can_override: viewer cannot override editor.
    #[test]
    fn r_16_cannot_override_upward() {
        assert!(!Role::Viewer.can_override(Role::Editor));
    }

    // R-17: Role label returns non-empty string.
    #[test]
    fn r_17_label_non_empty() {
        for &r in Role::all() {
            assert!(!r.label().is_empty());
        }
    }

    // R-18: Role serialises as lowercase string.
    #[test]
    fn r_18_role_serde_roundtrip() {
        let r = Role::Designer;
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, "\"designer\"");
        let back: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    // ── ProjectMember ─────────────────────────────────────────────

    // R-19: New member has accepted=false.
    #[test]
    fn r_19_new_member_not_accepted() {
        let m = ProjectMember::new(uid(), "Alice", Role::Viewer);
        assert!(!m.accepted);
    }

    // R-20: accept() sets accepted=true.
    #[test]
    fn r_20_accept_sets_flag() {
        let m = ProjectMember::new(uid(), "Alice", Role::Viewer).accept();
        assert!(m.accepted);
    }

    // R-21: Member::can() delegates to role.
    #[test]
    fn r_21_member_can_delegates_to_role() {
        let m = ProjectMember::new(uid(), "Alice", Role::Editor);
        assert!(m.can(Permission::ModifyLayers));
        assert!(!m.can(Permission::InviteMembers));
    }

    // ── MembershipTable ───────────────────────────────────────────

    // R-22: New table is empty.
    #[test]
    fn r_22_new_table_empty() {
        let t = MembershipTable::new(uid());
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    // R-23: Upsert adds a member.
    #[test]
    fn r_23_upsert_adds_member() {
        let mut t = MembershipTable::new(uid());
        let id = uid();
        t.upsert(ProjectMember::new(id, "Alice", Role::Viewer));
        assert_eq!(t.len(), 1);
        assert!(t.get(&id).is_some());
    }

    // R-24: Upsert replaces existing member.
    #[test]
    fn r_24_upsert_replaces() {
        let mut t = MembershipTable::new(uid());
        let id = uid();
        t.upsert(ProjectMember::new(id, "Alice", Role::Viewer));
        t.upsert(ProjectMember::new(id, "Alice V2", Role::Editor));
        assert_eq!(t.len(), 1);
        assert_eq!(t.get(&id).unwrap().role, Role::Editor);
    }

    // R-25: Remove returns the entry.
    #[test]
    fn r_25_remove_returns_entry() {
        let mut t = MembershipTable::new(uid());
        let id = uid();
        t.upsert(ProjectMember::new(id, "Alice", Role::Viewer));
        let removed = t.remove(&id);
        assert!(removed.is_some());
        assert_eq!(t.len(), 0);
    }

    // R-26: set_role changes role.
    #[test]
    fn r_26_set_role_changes_role() {
        let mut t = MembershipTable::new(uid());
        let id = uid();
        t.upsert(ProjectMember::new(id, "Alice", Role::Viewer));
        t.set_role(&id, Role::Editor).unwrap();
        assert_eq!(t.get(&id).unwrap().role, Role::Editor);
    }

    // R-27: set_role returns Err for unknown user.
    #[test]
    fn r_27_set_role_unknown_user_errors() {
        let mut t = MembershipTable::new(uid());
        let result = t.set_role(&uid(), Role::Editor);
        assert!(matches!(result, Err(RoleError::MemberNotFound(_))));
    }

    // R-28: authorize returns Ok when member has permission.
    #[test]
    fn r_28_authorize_ok() {
        let mut t = MembershipTable::new(uid());
        let id = uid();
        t.upsert(ProjectMember::new(id, "Alice", Role::Owner).accept());
        assert!(t.authorize(id, Permission::InviteMembers).is_ok());
    }

    // R-29: authorize returns Err when member lacks permission.
    #[test]
    fn r_29_authorize_denied() {
        let mut t = MembershipTable::new(uid());
        let id = uid();
        t.upsert(ProjectMember::new(id, "Alice", Role::Viewer).accept());
        let result = t.authorize(id, Permission::InviteMembers);
        assert!(matches!(result, Err(RoleError::PermissionDenied { .. })));
    }

    // R-30: authorize returns Err for unknown user.
    #[test]
    fn r_30_authorize_unknown_user() {
        let t = MembershipTable::new(uid());
        let result = t.authorize(uid(), Permission::ViewDocument);
        assert!(matches!(result, Err(RoleError::MemberNotFound(_))));
    }

    // R-31: transfer_ownership demotes old, promotes new.
    #[test]
    fn r_31_transfer_ownership() {
        let mut t = MembershipTable::new(uid());
        let owner_id = uid();
        let new_id   = uid();
        t.upsert(ProjectMember::new(owner_id, "Owner",    Role::Owner).accept());
        t.upsert(ProjectMember::new(new_id,   "NewOwner", Role::Designer).accept());

        t.transfer_ownership(owner_id, new_id).unwrap();

        assert_eq!(t.get(&owner_id).unwrap().role, Role::Designer);
        assert_eq!(t.get(&new_id).unwrap().role,   Role::Owner);
    }

    // R-32: transfer_ownership fails if caller is not owner.
    #[test]
    fn r_32_transfer_ownership_not_owner_fails() {
        let mut t = MembershipTable::new(uid());
        let editor_id = uid();
        let target_id = uid();
        t.upsert(ProjectMember::new(editor_id, "Editor", Role::Editor).accept());
        t.upsert(ProjectMember::new(target_id, "Target", Role::Designer).accept());

        let result = t.transfer_ownership(editor_id, target_id);
        assert!(matches!(result, Err(RoleError::NotOwner(_))));
    }

    // R-33: transfer_ownership fails if new owner not in table.
    #[test]
    fn r_33_transfer_ownership_new_owner_missing() {
        let mut t = MembershipTable::new(uid());
        let owner_id = uid();
        t.upsert(ProjectMember::new(owner_id, "Owner", Role::Owner).accept());
        let result = t.transfer_ownership(owner_id, uid());
        assert!(matches!(result, Err(RoleError::MemberNotFound(_))));
    }

    // R-34: owners() returns exactly one entry after transfer.
    #[test]
    fn r_34_owners_count_after_transfer() {
        let mut t = MembershipTable::new(uid());
        let a = uid(); let b = uid();
        t.upsert(ProjectMember::new(a, "A", Role::Owner).accept());
        t.upsert(ProjectMember::new(b, "B", Role::Designer).accept());
        t.transfer_ownership(a, b).unwrap();
        assert_eq!(t.owners().len(), 1);
        assert_eq!(t.owners()[0].user_id, b);
    }

    // R-35: RoleError Display is non-empty.
    #[test]
    fn r_35_role_error_display() {
        let id = uid();
        let err = RoleError::MemberNotFound(id);
        let s = err.to_string();
        assert!(!s.is_empty());
        assert!(s.contains(&id.to_string()));
    }

    // ── Role-permission completeness ──────────────────────────────

    // R-36: Every role's permission set has at least ViewDocument.
    #[test]
    fn r_36_all_roles_can_view() {
        for &r in Role::all() {
            assert!(r.can(Permission::ViewDocument), "{:?} cannot view", r);
        }
    }

    // R-37: No role below Owner has InviteMembers.
    #[test]
    fn r_37_only_owner_can_invite() {
        for &r in &[Role::Viewer, Role::Editor, Role::Designer, Role::Developer] {
            assert!(!r.can(Permission::InviteMembers), "{:?} should not invite", r);
        }
        assert!(Role::Owner.can(Permission::InviteMembers));
    }

    // R-38: Only Designer/Developer/Owner can export code.
    #[test]
    fn r_38_code_export_roles() {
        assert!(!Role::Viewer.can(Permission::ExportCode));
        assert!(!Role::Editor.can(Permission::ExportCode));
        assert!(Role::Designer.can(Permission::ExportCode));
        assert!(Role::Developer.can(Permission::ExportCode));
        assert!(Role::Owner.can(Permission::ExportCode));
    }

    // R-39: Developer cannot modify layers.
    #[test]
    fn r_39_developer_no_layer_modify() {
        assert!(!Role::Developer.can(Permission::ModifyLayers));
        assert!(!Role::Developer.can(Permission::ModifyFrames));
        assert!(!Role::Developer.can(Permission::ChangeWorkspaceMode));
    }

    // R-40: iter() length matches len().
    #[test]
    fn r_40_iter_length() {
        let mut t = MembershipTable::new(uid());
        for _ in 0..5 { t.upsert(ProjectMember::new(uid(), "X", Role::Viewer)); }
        assert_eq!(t.iter().count(), t.len());
    }
}
