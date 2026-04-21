// logos-collab/src/project_scope.rs
//
//! Projects scoped to a company, with per-project membership and tool lists.
//!
//! A `Project` contains a CRDT document (not stored here — referenced by ID)
//! and a free-form `tools` JSON array listing the external tools/versions in
//! use (e.g. `["Figma 2.4", "React 18", "Tailwind 3"]`).
//!
//! Per-project roles are the finer-grained `Role` defined in
//! [`crate::roles`].

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::roles::Role;

type Timestamp = u64;

fn now_ms() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── ProjectError ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    ProjectNotFound,
    UserNotMember,
    AlreadyMember,
    PermissionDenied,
    CannotRemoveOwner,
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProjectNotFound  => write!(f, "project not found"),
            Self::UserNotMember    => write!(f, "user is not a project member"),
            Self::AlreadyMember    => write!(f, "user is already a project member"),
            Self::PermissionDenied => write!(f, "permission denied"),
            Self::CannotRemoveOwner=> write!(f, "cannot remove the project owner"),
        }
    }
}

// ── ProjectMember ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMember {
    pub project_id: Uuid,
    pub user_id:    Uuid,
    pub role:       Role,
    pub joined_at:  Timestamp,
}

// ── Project ───────────────────────────────────────────────────────────────────

/// A design project belonging to a company.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id:              Uuid,
    pub name:            String,
    pub description:     String,
    /// Company this project belongs to.
    pub company_id:      Uuid,
    /// User who created the project (always Owner role in the project).
    pub creator_user_id: Uuid,
    pub created_at:      Timestamp,
    pub updated_at:      Timestamp,
    /// Free-form list of tool names/versions used in this project.
    pub tools:           Vec<String>,
}

impl Project {
    pub fn new(
        name:            impl Into<String>,
        description:     impl Into<String>,
        company_id:      Uuid,
        creator_user_id: Uuid,
    ) -> Self {
        let now = now_ms();
        Self {
            id:              Uuid::new_v4(),
            name:            name.into(),
            description:     description.into(),
            company_id,
            creator_user_id,
            created_at:      now,
            updated_at:      now,
            tools:           Vec::new(),
        }
    }

    pub fn with_tools(mut self, tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tools = tools.into_iter().map(|t| t.into()).collect();
        self
    }

    pub fn update_meta(
        &mut self,
        name:        impl Into<String>,
        description: impl Into<String>,
    ) {
        self.name        = name.into();
        self.description = description.into();
        self.updated_at  = now_ms();
    }
}

// ── ProjectStore ──────────────────────────────────────────────────────────────

/// In-memory store for projects and their per-project memberships.
#[derive(Debug, Default)]
pub struct ProjectStore {
    projects:    HashMap<Uuid, Project>,
    memberships: HashMap<Uuid, Vec<ProjectMember>>,
}

impl ProjectStore {
    pub fn new() -> Self { Self::default() }

    // ── Project CRUD ──────────────────────────────────────────────────────

    /// Create a project.  The creator is automatically added as Owner.
    pub fn create_project(&mut self, project: Project) -> Uuid {
        let id      = project.id;
        let creator = project.creator_user_id;
        self.projects.insert(id, project);
        self.memberships.insert(id, vec![ProjectMember {
            project_id: id,
            user_id:    creator,
            role:       Role::Owner,
            joined_at:  now_ms(),
        }]);
        id
    }

    pub fn get_project(&self, id: Uuid) -> Option<&Project> {
        self.projects.get(&id)
    }

    pub fn get_project_mut(&mut self, id: Uuid) -> Option<&mut Project> {
        self.projects.get_mut(&id)
    }

    pub fn delete_project(&mut self, id: Uuid) -> Result<(), ProjectError> {
        self.projects.remove(&id).ok_or(ProjectError::ProjectNotFound)?;
        self.memberships.remove(&id);
        Ok(())
    }

    pub fn project_count(&self) -> usize { self.projects.len() }

    /// List all projects in a company.
    pub fn projects_for_company(&self, company_id: Uuid) -> Vec<&Project> {
        self.projects.values()
            .filter(|p| p.company_id == company_id)
            .collect()
    }

    /// List all projects a user is a member of.
    pub fn projects_for_user(&self, user_id: Uuid) -> Vec<&Project> {
        self.memberships.iter()
            .filter(|(_, members)| members.iter().any(|m| m.user_id == user_id))
            .filter_map(|(pid, _)| self.projects.get(pid))
            .collect()
    }

    // ── Membership ────────────────────────────────────────────────────────

    /// Add a member.  `actor` must be an Owner of the project.
    pub fn add_member(
        &mut self,
        project_id: Uuid,
        actor_id:   Uuid,
        new_user:   Uuid,
        role:       Role,
    ) -> Result<(), ProjectError> {
        self.require_owner(project_id, actor_id)?;
        let members = self.memberships
            .get_mut(&project_id)
            .ok_or(ProjectError::ProjectNotFound)?;
        if members.iter().any(|m| m.user_id == new_user) {
            return Err(ProjectError::AlreadyMember);
        }
        members.push(ProjectMember { project_id, user_id: new_user, role, joined_at: now_ms() });
        Ok(())
    }

    /// Remove a member.  Cannot remove the project creator.
    pub fn remove_member(
        &mut self,
        project_id: Uuid,
        actor_id:   Uuid,
        target_id:  Uuid,
    ) -> Result<(), ProjectError> {
        self.require_owner(project_id, actor_id)?;
        let project = self.projects.get(&project_id).ok_or(ProjectError::ProjectNotFound)?;
        if target_id == project.creator_user_id {
            return Err(ProjectError::CannotRemoveOwner);
        }
        let members = self.memberships.get_mut(&project_id).ok_or(ProjectError::ProjectNotFound)?;
        let before  = members.len();
        members.retain(|m| m.user_id != target_id);
        if members.len() == before { return Err(ProjectError::UserNotMember); }
        Ok(())
    }

    pub fn set_member_role(
        &mut self,
        project_id: Uuid,
        actor_id:   Uuid,
        target_id:  Uuid,
        new_role:   Role,
    ) -> Result<(), ProjectError> {
        self.require_owner(project_id, actor_id)?;
        let members = self.memberships.get_mut(&project_id).ok_or(ProjectError::ProjectNotFound)?;
        let m = members.iter_mut().find(|m| m.user_id == target_id).ok_or(ProjectError::UserNotMember)?;
        m.role = new_role;
        Ok(())
    }

    pub fn member_role(&self, project_id: Uuid, user_id: Uuid) -> Option<Role> {
        self.memberships.get(&project_id)?
            .iter()
            .find(|m| m.user_id == user_id)
            .map(|m| m.role)
    }

    pub fn is_member(&self, project_id: Uuid, user_id: Uuid) -> bool {
        self.member_role(project_id, user_id).is_some()
    }

    pub fn get_members(&self, project_id: Uuid) -> Option<&Vec<ProjectMember>> {
        self.memberships.get(&project_id)
    }

    fn require_owner(&self, project_id: Uuid, actor_id: Uuid) -> Result<(), ProjectError> {
        match self.member_role(project_id, actor_id) {
            Some(Role::Owner) => Ok(()),
            Some(_)           => Err(ProjectError::PermissionDenied),
            None              => Err(ProjectError::UserNotMember),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_project(store: &mut ProjectStore, creator: Uuid) -> Uuid {
        store.create_project(
            Project::new("My Project", "desc", Uuid::new_v4(), creator)
        )
    }

    // PS-01: Creator is automatically the Owner.
    #[test]
    fn ps_01_creator_is_owner() {
        let mut s = ProjectStore::new();
        let c  = Uuid::new_v4();
        let pid = make_project(&mut s, c);
        assert_eq!(s.member_role(pid, c), Some(Role::Owner));
    }

    // PS-02: project_count increments.
    #[test]
    fn ps_02_project_count() {
        let mut s = ProjectStore::new();
        make_project(&mut s, Uuid::new_v4());
        make_project(&mut s, Uuid::new_v4());
        assert_eq!(s.project_count(), 2);
    }

    // PS-03: delete_project removes it.
    #[test]
    fn ps_03_delete_project() {
        let mut s = ProjectStore::new();
        let pid = make_project(&mut s, Uuid::new_v4());
        s.delete_project(pid).unwrap();
        assert!(s.get_project(pid).is_none());
    }

    // PS-04: Owner can add a member.
    #[test]
    fn ps_04_owner_adds_member() {
        let mut s = ProjectStore::new();
        let creator = Uuid::new_v4();
        let pid = make_project(&mut s, creator);
        let new_user = Uuid::new_v4();
        s.add_member(pid, creator, new_user, Role::Editor).unwrap();
        assert_eq!(s.member_role(pid, new_user), Some(Role::Editor));
    }

    // PS-05: Non-Owner cannot add members.
    #[test]
    fn ps_05_non_owner_cant_add() {
        let mut s = ProjectStore::new();
        let creator = Uuid::new_v4();
        let pid = make_project(&mut s, creator);
        let editor = Uuid::new_v4();
        s.add_member(pid, creator, editor, Role::Editor).unwrap();
        let err = s.add_member(pid, editor, Uuid::new_v4(), Role::Viewer).unwrap_err();
        assert_eq!(err, ProjectError::PermissionDenied);
    }

    // PS-06: Duplicate add returns AlreadyMember.
    #[test]
    fn ps_06_duplicate_member() {
        let mut s = ProjectStore::new();
        let c = Uuid::new_v4(); let pid = make_project(&mut s, c);
        let u = Uuid::new_v4();
        s.add_member(pid, c, u, Role::Viewer).unwrap();
        let err = s.add_member(pid, c, u, Role::Viewer).unwrap_err();
        assert_eq!(err, ProjectError::AlreadyMember);
    }

    // PS-07: Owner can remove a member.
    #[test]
    fn ps_07_remove_member() {
        let mut s = ProjectStore::new();
        let c = Uuid::new_v4(); let pid = make_project(&mut s, c);
        let u = Uuid::new_v4();
        s.add_member(pid, c, u, Role::Designer).unwrap();
        s.remove_member(pid, c, u).unwrap();
        assert!(!s.is_member(pid, u));
    }

    // PS-08: Cannot remove the project creator.
    #[test]
    fn ps_08_cannot_remove_creator() {
        let mut s = ProjectStore::new();
        let c = Uuid::new_v4(); let pid = make_project(&mut s, c);
        let err = s.remove_member(pid, c, c).unwrap_err();
        assert_eq!(err, ProjectError::CannotRemoveOwner);
    }

    // PS-09: set_member_role changes role.
    #[test]
    fn ps_09_set_role() {
        let mut s = ProjectStore::new();
        let c = Uuid::new_v4(); let pid = make_project(&mut s, c);
        let u = Uuid::new_v4();
        s.add_member(pid, c, u, Role::Viewer).unwrap();
        s.set_member_role(pid, c, u, Role::Developer).unwrap();
        assert_eq!(s.member_role(pid, u), Some(Role::Developer));
    }

    // PS-10: projects_for_company filters correctly.
    #[test]
    fn ps_10_projects_for_company() {
        let mut s   = ProjectStore::new();
        let cid1    = Uuid::new_v4();
        let cid2    = Uuid::new_v4();
        let creator = Uuid::new_v4();
        s.create_project(Project::new("P1", "", cid1, creator));
        s.create_project(Project::new("P2", "", cid1, creator));
        s.create_project(Project::new("P3", "", cid2, creator));
        assert_eq!(s.projects_for_company(cid1).len(), 2);
        assert_eq!(s.projects_for_company(cid2).len(), 1);
    }

    // PS-11: projects_for_user returns correct list.
    #[test]
    fn ps_11_projects_for_user() {
        let mut s = ProjectStore::new();
        let creator = Uuid::new_v4();
        let p1 = make_project(&mut s, creator);
        let _p2 = make_project(&mut s, Uuid::new_v4());
        let u = Uuid::new_v4();
        s.add_member(p1, creator, u, Role::Editor).unwrap();
        let projs = s.projects_for_user(u);
        assert_eq!(projs.len(), 1);
    }

    // PS-12: Project::with_tools stores tools.
    #[test]
    fn ps_12_tools_stored() {
        let p = Project::new("X", "d", Uuid::new_v4(), Uuid::new_v4())
            .with_tools(["React 18", "Tailwind 3"]);
        assert_eq!(p.tools.len(), 2);
        assert!(p.tools.contains(&"React 18".to_owned()));
    }

    // PS-13: Project::update_meta changes name and description.
    #[test]
    fn ps_13_update_meta() {
        let mut s = ProjectStore::new();
        let c = Uuid::new_v4();
        let pid = make_project(&mut s, c);
        s.get_project_mut(pid).unwrap().update_meta("New Name", "New Desc");
        assert_eq!(s.get_project(pid).unwrap().name, "New Name");
    }

    // PS-14: delete non-existent project returns ProjectNotFound.
    #[test]
    fn ps_14_delete_nonexistent() {
        let mut s = ProjectStore::new();
        let err = s.delete_project(Uuid::new_v4()).unwrap_err();
        assert_eq!(err, ProjectError::ProjectNotFound);
    }

    // PS-15: Non-member actor gets UserNotMember.
    #[test]
    fn ps_15_nonmember_actor() {
        let mut s = ProjectStore::new();
        let c = Uuid::new_v4(); let pid = make_project(&mut s, c);
        let stranger = Uuid::new_v4();
        let err = s.add_member(pid, stranger, Uuid::new_v4(), Role::Viewer).unwrap_err();
        assert_eq!(err, ProjectError::UserNotMember);
    }
}
