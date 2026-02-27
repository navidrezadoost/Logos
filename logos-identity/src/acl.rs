//! Access Control Lists (ACLs) for document-level permissions.
//!
//! Each document has an `AccessControlList` that maps users to roles.
//! The owner always has full permissions. Additional entries can be
//! added with custom permission overrides.

use crate::error::IdentityError;
use crate::permission::{Permission, PermissionSet};
use crate::role::Role;
use crate::user::UserId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single entry in an access control list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlEntry {
    /// The user this entry applies to.
    pub user_id: UserId,
    /// The role granted to this user.
    pub role: Role,
    /// Who granted this access.
    pub granted_by: UserId,
    /// When access was granted (Unix timestamp).
    pub granted_at: u64,
    /// Custom permissions that override the role defaults (optional).
    pub custom_permissions: Option<PermissionSet>,
}

impl AccessControlEntry {
    /// Create a new ACL entry.
    pub fn new(user_id: UserId, role: Role, granted_by: UserId) -> Self {
        Self {
            user_id,
            role,
            granted_by,
            granted_at: crate::user::current_timestamp(),
            custom_permissions: None,
        }
    }

    /// Create an entry with custom permissions override.
    pub fn with_custom_permissions(mut self, permissions: PermissionSet) -> Self {
        self.custom_permissions = Some(permissions);
        self
    }

    /// Effective permissions for this entry.
    pub fn effective_permissions(&self) -> PermissionSet {
        self.custom_permissions.unwrap_or_else(|| PermissionSet::for_role(self.role))
    }
}

/// Access control list for a resource (typically a document).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlList {
    /// The resource this ACL protects.
    pub resource_id: Uuid,
    /// The owner of the resource (always has full permissions).
    pub owner: UserId,
    /// Explicit access entries.
    entries: Vec<AccessControlEntry>,
    /// Default role for "anyone with link" (None = no link sharing).
    pub default_role: Option<Role>,
    /// Whether the resource is publicly accessible.
    pub is_public: bool,
    /// When the ACL was created.
    pub created_at: u64,
    /// Last modification.
    pub updated_at: u64,
}

impl AccessControlList {
    /// Create a new ACL for a resource.
    pub fn new(resource_id: Uuid, owner: UserId) -> Self {
        let now = crate::user::current_timestamp();
        Self {
            resource_id,
            owner,
            entries: Vec::new(),
            default_role: None,
            is_public: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Grant access to a user.
    pub fn grant(
        &mut self,
        user_id: UserId,
        role: Role,
        granted_by: UserId,
    ) -> Result<(), IdentityError> {
        // Can't change owner via grant
        if user_id == self.owner {
            return Err(IdentityError::InvalidInput(
                "Cannot change owner role via grant".into(),
            ));
        }
        // Remove existing entry if any
        self.entries.retain(|e| e.user_id != user_id);
        self.entries.push(AccessControlEntry::new(user_id, role, granted_by));
        self.updated_at = crate::user::current_timestamp();
        Ok(())
    }

    /// Grant access with custom permissions.
    pub fn grant_custom(
        &mut self,
        user_id: UserId,
        role: Role,
        permissions: PermissionSet,
        granted_by: UserId,
    ) -> Result<(), IdentityError> {
        if user_id == self.owner {
            return Err(IdentityError::InvalidInput(
                "Cannot change owner role via grant".into(),
            ));
        }
        self.entries.retain(|e| e.user_id != user_id);
        self.entries.push(
            AccessControlEntry::new(user_id, role, granted_by)
                .with_custom_permissions(permissions),
        );
        self.updated_at = crate::user::current_timestamp();
        Ok(())
    }

    /// Revoke access for a user.
    pub fn revoke(&mut self, user_id: UserId) -> Result<bool, IdentityError> {
        if user_id == self.owner {
            return Err(IdentityError::InvalidInput(
                "Cannot revoke owner access".into(),
            ));
        }
        let before = self.entries.len();
        self.entries.retain(|e| e.user_id != user_id);
        let removed = self.entries.len() < before;
        if removed {
            self.updated_at = crate::user::current_timestamp();
        }
        Ok(removed)
    }

    /// Get a user's role (None if no access).
    pub fn get_role(&self, user_id: &UserId) -> Option<Role> {
        if *user_id == self.owner {
            return Some(Role::Owner);
        }
        self.entries.iter()
            .find(|e| e.user_id == *user_id)
            .map(|e| e.role)
            .or_else(|| {
                if self.is_public || self.default_role.is_some() {
                    self.default_role.or(Some(Role::Viewer))
                } else {
                    None
                }
            })
    }

    /// Effective permissions for a user.
    pub fn effective_permissions(&self, user_id: &UserId) -> PermissionSet {
        if *user_id == self.owner {
            return PermissionSet::for_role(Role::Owner);
        }
        if let Some(entry) = self.entries.iter().find(|e| e.user_id == *user_id) {
            return entry.effective_permissions();
        }
        if let Some(default) = self.default_role {
            return PermissionSet::for_role(default);
        }
        if self.is_public {
            return PermissionSet::for_role(Role::Viewer);
        }
        PermissionSet::EMPTY
    }

    /// Check if a user has a specific permission.
    pub fn check(&self, user_id: &UserId, permission: Permission) -> bool {
        self.effective_permissions(user_id).has(permission)
    }

    /// All access entries (excludes owner, who is implicit).
    pub fn entries(&self) -> &[AccessControlEntry] {
        &self.entries
    }

    /// List all users with explicit access (including owner).
    pub fn list_users_with_access(&self) -> Vec<(UserId, Role)> {
        let mut result = vec![(self.owner, Role::Owner)];
        for entry in &self.entries {
            result.push((entry.user_id, entry.role));
        }
        result
    }

    /// Number of users with explicit access (including owner).
    pub fn user_count(&self) -> usize {
        1 + self.entries.len() // +1 for owner
    }

    /// Transfer ownership to another user.
    pub fn transfer_ownership(&mut self, new_owner: UserId) -> Result<(), IdentityError> {
        let old_owner = self.owner;
        if old_owner == new_owner {
            return Ok(());
        }
        // Remove new owner from entries if they have an explicit entry
        self.entries.retain(|e| e.user_id != new_owner);
        // Add old owner as Admin
        self.entries.push(AccessControlEntry::new(old_owner, Role::Admin, new_owner));
        self.owner = new_owner;
        self.updated_at = crate::user::current_timestamp();
        Ok(())
    }

    /// Enable link sharing with a specific role.
    pub fn set_link_sharing(&mut self, role: Option<Role>) {
        self.default_role = role;
        self.updated_at = crate::user::current_timestamp();
    }

    /// Make the resource public (viewable by anyone).
    pub fn set_public(&mut self, is_public: bool) {
        self.is_public = is_public;
        self.updated_at = crate::user::current_timestamp();
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> UserId { UserId::from_uuid(Uuid::from_bytes([1; 16])) }
    fn alice() -> UserId { UserId::from_uuid(Uuid::from_bytes([2; 16])) }
    fn bob() -> UserId { UserId::from_uuid(Uuid::from_bytes([3; 16])) }
    fn doc_id() -> Uuid { Uuid::from_bytes([10; 16]) }

    #[test]
    fn new_acl() {
        let acl = AccessControlList::new(doc_id(), owner());
        assert_eq!(acl.owner, owner());
        assert_eq!(acl.user_count(), 1);
        assert!(!acl.is_public);
        assert!(acl.default_role.is_none());
    }

    #[test]
    fn owner_has_full_permissions() {
        let acl = AccessControlList::new(doc_id(), owner());
        assert_eq!(acl.get_role(&owner()), Some(Role::Owner));
        assert!(acl.check(&owner(), Permission::DeleteDocument));
        assert!(acl.check(&owner(), Permission::TransferOwnership));
    }

    #[test]
    fn grant_access() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        acl.grant(alice(), Role::Editor, owner()).unwrap();
        assert_eq!(acl.get_role(&alice()), Some(Role::Editor));
        assert!(acl.check(&alice(), Permission::EditDocument));
        assert!(!acl.check(&alice(), Permission::DeleteDocument));
    }

    #[test]
    fn grant_replaces_existing() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        acl.grant(alice(), Role::Viewer, owner()).unwrap();
        acl.grant(alice(), Role::Editor, owner()).unwrap();
        assert_eq!(acl.get_role(&alice()), Some(Role::Editor));
        assert_eq!(acl.entries().len(), 1);
    }

    #[test]
    fn cannot_grant_to_owner() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        let result = acl.grant(owner(), Role::Editor, owner());
        assert!(matches!(result, Err(IdentityError::InvalidInput(_))));
    }

    #[test]
    fn grant_custom_permissions() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        let mut custom = PermissionSet::for_role(Role::Editor);
        custom.grant(Permission::ManageLibraries);
        acl.grant_custom(alice(), Role::Editor, custom, owner()).unwrap();
        assert!(acl.check(&alice(), Permission::ManageLibraries));
    }

    #[test]
    fn revoke_access() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        acl.grant(alice(), Role::Editor, owner()).unwrap();
        assert!(acl.revoke(alice()).unwrap());
        assert!(acl.get_role(&alice()).is_none());
    }

    #[test]
    fn cannot_revoke_owner() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        let result = acl.revoke(owner());
        assert!(matches!(result, Err(IdentityError::InvalidInput(_))));
    }

    #[test]
    fn no_access_by_default() {
        let acl = AccessControlList::new(doc_id(), owner());
        assert!(acl.get_role(&alice()).is_none());
        assert!(!acl.check(&alice(), Permission::ViewDocument));
    }

    #[test]
    fn public_access() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        acl.set_public(true);
        assert_eq!(acl.get_role(&alice()), Some(Role::Viewer));
        assert!(acl.check(&alice(), Permission::ViewDocument));
        assert!(!acl.check(&alice(), Permission::EditDocument));
    }

    #[test]
    fn link_sharing() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        acl.set_link_sharing(Some(Role::Commenter));
        assert_eq!(acl.get_role(&alice()), Some(Role::Commenter));
        assert!(acl.check(&alice(), Permission::CreateComment));
        assert!(!acl.check(&alice(), Permission::EditDocument));
    }

    #[test]
    fn explicit_trumps_default() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        acl.set_link_sharing(Some(Role::Viewer));
        acl.grant(alice(), Role::Editor, owner()).unwrap();
        assert_eq!(acl.get_role(&alice()), Some(Role::Editor));
    }

    #[test]
    fn transfer_ownership() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        acl.grant(alice(), Role::Editor, owner()).unwrap();
        acl.transfer_ownership(alice()).unwrap();
        assert_eq!(acl.owner, alice());
        assert_eq!(acl.get_role(&alice()), Some(Role::Owner));
        // Old owner becomes admin
        assert_eq!(acl.get_role(&owner()), Some(Role::Admin));
    }

    #[test]
    fn transfer_to_self() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        acl.transfer_ownership(owner()).unwrap(); // No-op
        assert_eq!(acl.owner, owner());
    }

    #[test]
    fn list_users() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        acl.grant(alice(), Role::Editor, owner()).unwrap();
        acl.grant(bob(), Role::Viewer, owner()).unwrap();
        let users = acl.list_users_with_access();
        assert_eq!(users.len(), 3);
        assert!(users.iter().any(|(uid, r)| *uid == owner() && *r == Role::Owner));
        assert!(users.iter().any(|(uid, r)| *uid == alice() && *r == Role::Editor));
    }

    #[test]
    fn acl_serde_roundtrip() {
        let mut acl = AccessControlList::new(doc_id(), owner());
        acl.grant(alice(), Role::Editor, owner()).unwrap();
        acl.set_link_sharing(Some(Role::Viewer));
        let json = serde_json::to_string(&acl).unwrap();
        let back: AccessControlList = serde_json::from_str(&json).unwrap();
        assert_eq!(back.owner, owner());
        assert_eq!(back.entries().len(), 1);
        assert_eq!(back.default_role, Some(Role::Viewer));
    }

    #[test]
    fn entry_effective_permissions() {
        let entry = AccessControlEntry::new(alice(), Role::Editor, owner());
        let perms = entry.effective_permissions();
        assert!(perms.has(Permission::EditDocument));

        let custom = PermissionSet::from_bits(1); // Only ViewDocument
        let entry_custom = entry.with_custom_permissions(custom);
        let perms = entry_custom.effective_permissions();
        assert!(perms.has(Permission::ViewDocument));
        assert!(!perms.has(Permission::EditDocument));
    }
}
