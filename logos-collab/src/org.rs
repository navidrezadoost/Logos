// logos-collab/src/org.rs
//
//! Companies (organizations) and their membership tables.
//!
//! A `Company` groups users and projects.  Each member has one of three
//! company-level roles; project-level roles are managed separately in
//! [`crate::project_scope`].

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

type Timestamp = u64;

fn now_ms() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── CompanyRole ───────────────────────────────────────────────────────────────

/// Company-level role.  Project-level roles are finer-grained (see
/// [`crate::roles::Role`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompanyRole {
    /// Can view and open projects but cannot create or modify them.
    Viewer,
    /// Can create projects, manage own content, and invite others.
    Member,
    /// Full company control: add/remove members, alter roles, delete company.
    Admin,
}

impl CompanyRole {
    pub fn can_manage_members(self) -> bool {
        matches!(self, CompanyRole::Admin)
    }
    pub fn can_create_projects(self) -> bool {
        matches!(self, CompanyRole::Member | CompanyRole::Admin)
    }
}

// ── OrgError ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrgError {
    CompanyNotFound,
    UserNotMember,
    AlreadyMember,
    PermissionDenied,
    CannotRemoveOwner,
}

impl std::fmt::Display for OrgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CompanyNotFound  => write!(f, "company not found"),
            Self::UserNotMember    => write!(f, "user is not a member of this company"),
            Self::AlreadyMember    => write!(f, "user is already a member"),
            Self::PermissionDenied => write!(f, "permission denied"),
            Self::CannotRemoveOwner=> write!(f, "cannot remove the company owner"),
        }
    }
}

// ── CompanyMember ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyMember {
    pub company_id: Uuid,
    pub user_id:    Uuid,
    pub role:       CompanyRole,
    pub joined_at:  Timestamp,
}

// ── Company ───────────────────────────────────────────────────────────────────

/// An organization that owns projects and has a list of members.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Company {
    pub id:            Uuid,
    pub name:          String,
    /// The user who created the company (always Admin role).
    pub owner_user_id: Uuid,
    pub created_at:    Timestamp,
    /// Arbitrary JSON settings (stored as `serde_json::Value` but modelled as
    /// `String` here to keep the crate JSON-free in non-serde paths).
    pub settings:      serde_json::Value,
}

impl Company {
    pub fn new(name: impl Into<String>, owner_user_id: Uuid) -> Self {
        Self {
            id:            Uuid::new_v4(),
            name:          name.into(),
            owner_user_id,
            created_at:    now_ms(),
            settings:      serde_json::Value::Object(Default::default()),
        }
    }
}

// ── CompanyStore ──────────────────────────────────────────────────────────────

/// In-memory store for companies and their memberships.
#[derive(Debug, Default)]
pub struct CompanyStore {
    companies:  HashMap<Uuid, Company>,
    /// company_id → list of members
    memberships: HashMap<Uuid, Vec<CompanyMember>>,
}

impl CompanyStore {
    pub fn new() -> Self { Self::default() }

    // ── Company CRUD ──────────────────────────────────────────────────────

    /// Create a company and automatically add the owner as an Admin member.
    pub fn create_company(&mut self, company: Company) -> Uuid {
        let id = company.id;
        let owner = company.owner_user_id;
        self.companies.insert(id, company);
        self.memberships.insert(id, vec![CompanyMember {
            company_id: id,
            user_id:    owner,
            role:       CompanyRole::Admin,
            joined_at:  now_ms(),
        }]);
        id
    }

    pub fn get_company(&self, id: Uuid) -> Option<&Company> {
        self.companies.get(&id)
    }

    pub fn delete_company(&mut self, id: Uuid) -> Result<(), OrgError> {
        self.companies.remove(&id).ok_or(OrgError::CompanyNotFound)?;
        self.memberships.remove(&id);
        Ok(())
    }

    pub fn company_count(&self) -> usize { self.companies.len() }

    /// List all companies a user belongs to.
    pub fn companies_for_user(&self, user_id: Uuid) -> Vec<&Company> {
        self.memberships.values()
            .filter_map(|members| {
                if members.iter().any(|m| m.user_id == user_id) {
                    members.first().and_then(|m| self.companies.get(&m.company_id))
                } else {
                    None
                }
            })
            .collect()
    }

    // ── Membership ────────────────────────────────────────────────────────

    /// Add a member to a company.  The `actor` must be an Admin.
    pub fn add_member(
        &mut self,
        company_id: Uuid,
        actor_id:   Uuid,
        new_user:   Uuid,
        role:       CompanyRole,
    ) -> Result<(), OrgError> {
        self.require_admin(company_id, actor_id)?;
        let members = self.memberships
            .get_mut(&company_id)
            .ok_or(OrgError::CompanyNotFound)?;
        if members.iter().any(|m| m.user_id == new_user) {
            return Err(OrgError::AlreadyMember);
        }
        members.push(CompanyMember { company_id, user_id: new_user, role, joined_at: now_ms() });
        Ok(())
    }

    /// Remove a member.  Cannot remove the owner.
    pub fn remove_member(
        &mut self,
        company_id: Uuid,
        actor_id:   Uuid,
        target_id:  Uuid,
    ) -> Result<(), OrgError> {
        self.require_admin(company_id, actor_id)?;
        let company = self.companies.get(&company_id).ok_or(OrgError::CompanyNotFound)?;
        if target_id == company.owner_user_id {
            return Err(OrgError::CannotRemoveOwner);
        }
        let members = self.memberships.get_mut(&company_id).ok_or(OrgError::CompanyNotFound)?;
        let before = members.len();
        members.retain(|m| m.user_id != target_id);
        if members.len() == before { return Err(OrgError::UserNotMember); }
        Ok(())
    }

    /// Change a member's company role.
    pub fn set_member_role(
        &mut self,
        company_id: Uuid,
        actor_id:   Uuid,
        target_id:  Uuid,
        new_role:   CompanyRole,
    ) -> Result<(), OrgError> {
        self.require_admin(company_id, actor_id)?;
        let members = self.memberships.get_mut(&company_id).ok_or(OrgError::CompanyNotFound)?;
        let m = members.iter_mut().find(|m| m.user_id == target_id).ok_or(OrgError::UserNotMember)?;
        m.role = new_role;
        Ok(())
    }

    pub fn get_members(&self, company_id: Uuid) -> Option<&Vec<CompanyMember>> {
        self.memberships.get(&company_id)
    }

    pub fn member_role(&self, company_id: Uuid, user_id: Uuid) -> Option<CompanyRole> {
        self.memberships.get(&company_id)?
            .iter()
            .find(|m| m.user_id == user_id)
            .map(|m| m.role)
    }

    pub fn is_member(&self, company_id: Uuid, user_id: Uuid) -> bool {
        self.member_role(company_id, user_id).is_some()
    }

    fn require_admin(&self, company_id: Uuid, actor_id: Uuid) -> Result<(), OrgError> {
        match self.member_role(company_id, actor_id) {
            Some(CompanyRole::Admin) => Ok(()),
            Some(_) => Err(OrgError::PermissionDenied),
            None    => Err(OrgError::UserNotMember),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> Uuid { Uuid::new_v4() }

    fn make_company(store: &mut CompanyStore, owner: Uuid) -> Uuid {
        store.create_company(Company::new("Acme Corp", owner))
    }

    // O-01: Creating a company auto-adds owner as Admin.
    #[test]
    fn o_01_creator_is_admin() {
        let mut s = CompanyStore::new();
        let owner = owner();
        let cid   = make_company(&mut s, owner);
        assert_eq!(s.member_role(cid, owner), Some(CompanyRole::Admin));
    }

    // O-02: company_count increments on creation.
    #[test]
    fn o_02_company_count() {
        let mut s = CompanyStore::new();
        let o = owner(); make_company(&mut s, o);
        let o2 = owner(); make_company(&mut s, o2);
        assert_eq!(s.company_count(), 2);
    }

    // O-03: delete_company removes it.
    #[test]
    fn o_03_delete_company() {
        let mut s = CompanyStore::new();
        let o = owner();
        let cid = make_company(&mut s, o);
        s.delete_company(cid).unwrap();
        assert!(s.get_company(cid).is_none());
    }

    // O-04: Admin can add a Member.
    #[test]
    fn o_04_admin_adds_member() {
        let mut s = CompanyStore::new();
        let o = owner(); let cid = make_company(&mut s, o);
        let new_user = Uuid::new_v4();
        s.add_member(cid, o, new_user, CompanyRole::Member).unwrap();
        assert_eq!(s.member_role(cid, new_user), Some(CompanyRole::Member));
    }

    // O-05: Non-Admin cannot add a member.
    #[test]
    fn o_05_nonadmin_cannot_add_member() {
        let mut s = CompanyStore::new();
        let o = owner(); let cid = make_company(&mut s, o);
        let member = Uuid::new_v4();
        s.add_member(cid, o, member, CompanyRole::Member).unwrap();
        let outsider = Uuid::new_v4();
        let err = s.add_member(cid, member, outsider, CompanyRole::Viewer).unwrap_err();
        assert_eq!(err, OrgError::PermissionDenied);
    }

    // O-06: Adding already-existing member returns AlreadyMember.
    #[test]
    fn o_06_duplicate_member_rejected() {
        let mut s = CompanyStore::new();
        let o = owner(); let cid = make_company(&mut s, o);
        let u = Uuid::new_v4();
        s.add_member(cid, o, u, CompanyRole::Member).unwrap();
        let err = s.add_member(cid, o, u, CompanyRole::Member).unwrap_err();
        assert_eq!(err, OrgError::AlreadyMember);
    }

    // O-07: Admin can remove a Member.
    #[test]
    fn o_07_admin_removes_member() {
        let mut s = CompanyStore::new();
        let o = owner(); let cid = make_company(&mut s, o);
        let u = Uuid::new_v4();
        s.add_member(cid, o, u, CompanyRole::Member).unwrap();
        s.remove_member(cid, o, u).unwrap();
        assert!(!s.is_member(cid, u));
    }

    // O-08: Cannot remove the company owner.
    #[test]
    fn o_08_cannot_remove_owner() {
        let mut s = CompanyStore::new();
        let o = owner(); let cid = make_company(&mut s, o);
        let err = s.remove_member(cid, o, o).unwrap_err();
        assert_eq!(err, OrgError::CannotRemoveOwner);
    }

    // O-09: set_member_role changes role.
    #[test]
    fn o_09_set_member_role() {
        let mut s = CompanyStore::new();
        let o = owner(); let cid = make_company(&mut s, o);
        let u = Uuid::new_v4();
        s.add_member(cid, o, u, CompanyRole::Viewer).unwrap();
        s.set_member_role(cid, o, u, CompanyRole::Member).unwrap();
        assert_eq!(s.member_role(cid, u), Some(CompanyRole::Member));
    }

    // O-10: companies_for_user returns all companies the user belongs to.
    #[test]
    fn o_10_companies_for_user() {
        let mut s = CompanyStore::new();
        let o = owner(); let o2 = owner();
        let cid1 = make_company(&mut s, o);
        let cid2 = make_company(&mut s, o2);
        let u = Uuid::new_v4();
        s.add_member(cid1, o, u, CompanyRole::Member).unwrap();
        s.add_member(cid2, o2, u, CompanyRole::Viewer).unwrap();
        let list = s.companies_for_user(u);
        assert_eq!(list.len(), 2);
    }

    // O-11: CompanyRole::can_manage_members only for Admin.
    #[test]
    fn o_11_role_permissions() {
        assert!(CompanyRole::Admin.can_manage_members());
        assert!(!CompanyRole::Member.can_manage_members());
        assert!(!CompanyRole::Viewer.can_manage_members());
    }

    // O-12: CompanyRole::can_create_projects for Member and Admin only.
    #[test]
    fn o_12_create_projects_permission() {
        assert!(CompanyRole::Admin.can_create_projects());
        assert!(CompanyRole::Member.can_create_projects());
        assert!(!CompanyRole::Viewer.can_create_projects());
    }

    // O-13: Non-member actor gets UserNotMember error on admin operations.
    #[test]
    fn o_13_nonmember_actor_rejected() {
        let mut s = CompanyStore::new();
        let o = owner(); let cid = make_company(&mut s, o);
        let stranger = Uuid::new_v4();
        let err = s.add_member(cid, stranger, Uuid::new_v4(), CompanyRole::Member).unwrap_err();
        assert_eq!(err, OrgError::UserNotMember);
    }

    // O-14: delete_company on unknown id returns CompanyNotFound.
    #[test]
    fn o_14_delete_nonexistent_company() {
        let mut s = CompanyStore::new();
        let err = s.delete_company(Uuid::new_v4()).unwrap_err();
        assert_eq!(err, OrgError::CompanyNotFound);
    }

    // O-15: Company name is stored correctly.
    #[test]
    fn o_15_company_name_stored() {
        let mut s = CompanyStore::new();
        let o = owner();
        let c = Company::new("Globals Inc", o);
        let id = c.id;
        s.create_company(c);
        assert_eq!(s.get_company(id).unwrap().name, "Globals Inc");
    }
}
