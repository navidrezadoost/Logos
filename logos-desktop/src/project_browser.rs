// logos-desktop/src/project_browser.rs
//
//! Pure-data state for the Project Browser screen.
//!
//! No `desktop-ui` deps required.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── ProjectEntry ──────────────────────────────────────────────────────────────

/// A project listing as returned by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub id:           Uuid,
    pub name:         String,
    pub description:  String,
    pub company_id:   Uuid,
    pub creator_name: String,
    /// Tools enabled for this project (free-form strings, e.g. "vector", "prototype").
    pub tools:        Vec<String>,
    pub member_count: usize,
    pub created_at:   u64,
}

// ── SortBy ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortBy {
    #[default]
    Name,
    CreatedAt,
    MemberCount,
}

// ── CreateProjectForm ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct CreateProjectForm {
    pub name:         String,
    pub description:  String,
    pub tools_input:  String,   // comma-separated raw input
    pub name_error:   Option<String>,
    pub server_error: Option<String>,
    pub submitting:   bool,
    pub success:      Option<Uuid>,
}

impl CreateProjectForm {
    pub fn validate(&mut self) -> bool {
        self.name_error = None;
        if self.name.trim().is_empty() {
            self.name_error = Some("Project name is required".into());
            return false;
        }
        true
    }

    /// Parse tools from comma-separated input.
    pub fn parsed_tools(&self) -> Vec<String> {
        self.tools_input.split(',')
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect()
    }

    pub fn begin_submit(&mut self) {
        self.submitting   = true;
        self.server_error = None;
    }

    pub fn on_ok(&mut self, id: Uuid) {
        self.submitting = false;
        self.success    = Some(id);
    }

    pub fn on_error(&mut self, msg: impl Into<String>) {
        self.submitting   = false;
        self.server_error = Some(msg.into());
    }

    pub fn reset(&mut self) { *self = Self::default(); }
}

// ── ProjectBrowserState ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ProjectBrowserState {
    pub company_id:   Option<Uuid>,
    pub company_name: String,
    pub projects:     Vec<ProjectEntry>,
    pub loading:      bool,
    pub load_error:   Option<String>,
    pub filter:       String,
    pub sort_by:      SortBy,
    pub show_create:  bool,
    pub create_form:  CreateProjectForm,
}

impl ProjectBrowserState {
    pub fn new() -> Self { Self::default() }

    pub fn for_company(company_id: Uuid, company_name: impl Into<String>) -> Self {
        Self {
            company_id:   Some(company_id),
            company_name: company_name.into(),
            ..Default::default()
        }
    }

    pub fn begin_load(&mut self) {
        self.loading    = true;
        self.load_error = None;
    }

    pub fn on_loaded(&mut self, projects: Vec<ProjectEntry>) {
        self.loading  = false;
        self.projects = projects;
    }

    pub fn on_load_error(&mut self, msg: impl Into<String>) {
        self.loading    = false;
        self.load_error = Some(msg.into());
    }

    /// Filtered and sorted project list.
    pub fn visible_projects(&self) -> Vec<&ProjectEntry> {
        let f = self.filter.to_lowercase();
        let mut v: Vec<&ProjectEntry> = self.projects.iter()
            .filter(|p| {
                f.is_empty()
                || p.name.to_lowercase().contains(&f)
                || p.description.to_lowercase().contains(&f)
            })
            .collect();
        match self.sort_by {
            SortBy::Name        => v.sort_by(|a, b| a.name.cmp(&b.name)),
            SortBy::CreatedAt   => v.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
            SortBy::MemberCount => v.sort_by(|a, b| b.member_count.cmp(&a.member_count)),
        }
        v
    }

    pub fn open_create(&mut self) {
        self.create_form.reset();
        self.show_create = true;
    }

    pub fn close_create(&mut self) {
        self.show_create = false;
    }

    pub fn add_project(&mut self, entry: ProjectEntry) {
        self.projects.push(entry);
        self.show_create = false;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (PB-01 … PB-15)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_project(name: &str, created_at: u64) -> ProjectEntry {
        ProjectEntry {
            id:           Uuid::new_v4(),
            name:         name.into(),
            description:  format!("{name} desc"),
            company_id:   Uuid::nil(),
            creator_name: "alice".into(),
            tools:        vec!["vector".into()],
            member_count: 2,
            created_at,
        }
    }

    // PB-01: starts empty
    #[test]
    fn pb_01_starts_empty() {
        let b = ProjectBrowserState::new();
        assert!(b.projects.is_empty());
    }

    // PB-02: on_loaded populates projects
    #[test]
    fn pb_02_on_loaded() {
        let mut b = ProjectBrowserState::new();
        b.on_loaded(vec![make_project("Alpha", 100), make_project("Beta", 200)]);
        assert_eq!(b.projects.len(), 2);
    }

    // PB-03: filter by name
    #[test]
    fn pb_03_filter_name() {
        let mut b = ProjectBrowserState::new();
        b.on_loaded(vec![make_project("Alpha", 1), make_project("Beta", 2)]);
        b.filter = "alp".into();
        assert_eq!(b.visible_projects().len(), 1);
    }

    // PB-04: filter by description
    #[test]
    fn pb_04_filter_description() {
        let mut b = ProjectBrowserState::new();
        b.on_loaded(vec![make_project("Alpha", 1)]);
        b.filter = "Alpha desc".into();
        assert_eq!(b.visible_projects().len(), 1);
    }

    // PB-05: sort by name
    #[test]
    fn pb_05_sort_name() {
        let mut b = ProjectBrowserState::new();
        b.on_loaded(vec![make_project("Zebra", 1), make_project("Alpha", 2)]);
        b.sort_by = SortBy::Name;
        let v = b.visible_projects();
        assert_eq!(v[0].name, "Alpha");
    }

    // PB-06: sort by created_at (newest first)
    #[test]
    fn pb_06_sort_newest_first() {
        let mut b = ProjectBrowserState::new();
        b.on_loaded(vec![make_project("Old", 100), make_project("New", 999)]);
        b.sort_by = SortBy::CreatedAt;
        let v = b.visible_projects();
        assert_eq!(v[0].name, "New");
    }

    // PB-07: sort by member_count (most first)
    #[test]
    fn pb_07_sort_member_count() {
        let mut b = ProjectBrowserState::new();
        let mut p1 = make_project("Small", 1); p1.member_count = 1;
        let mut p2 = make_project("Large", 2); p2.member_count = 99;
        b.on_loaded(vec![p1, p2]);
        b.sort_by = SortBy::MemberCount;
        assert_eq!(b.visible_projects()[0].name, "Large");
    }

    // PB-08: CreateProjectForm rejects empty name
    #[test]
    fn pb_08_form_empty_name() {
        let mut f = CreateProjectForm::default();
        assert!(!f.validate());
        assert!(f.name_error.is_some());
    }

    // PB-09: parsed_tools splits comma-separated input
    #[test]
    fn pb_09_parsed_tools() {
        let f = CreateProjectForm { tools_input: "Vector, Prototype , ".into(), ..Default::default() };
        let tools = f.parsed_tools();
        assert_eq!(tools, vec!["vector", "prototype"]);
    }

    // PB-10: on_ok sets success
    #[test]
    fn pb_10_form_on_ok() {
        let mut f = CreateProjectForm { name: "P1".into(), ..Default::default() };
        let id = Uuid::new_v4();
        f.begin_submit();
        f.on_ok(id);
        assert_eq!(f.success, Some(id));
        assert!(!f.submitting);
    }

    // PB-11: add_project appends and closes form
    #[test]
    fn pb_11_add_project() {
        let mut b = ProjectBrowserState::new();
        b.show_create = true;
        b.add_project(make_project("New", 1));
        assert_eq!(b.projects.len(), 1);
        assert!(!b.show_create);
    }

    // PB-12: open_create resets form
    #[test]
    fn pb_12_open_create_resets() {
        let mut b = ProjectBrowserState::new();
        b.create_form.name = "stale".into();
        b.open_create();
        assert!(b.show_create);
        assert!(b.create_form.name.is_empty());
    }

    // PB-13: close_create hides form
    #[test]
    fn pb_13_close_create() {
        let mut b = ProjectBrowserState::new();
        b.open_create();
        b.close_create();
        assert!(!b.show_create);
    }

    // PB-14: on_load_error sets error and clears loading
    #[test]
    fn pb_14_on_load_error() {
        let mut b = ProjectBrowserState::new();
        b.begin_load();
        b.on_load_error("net error");
        assert!(!b.loading);
        assert!(b.load_error.is_some());
    }

    // PB-15: for_company sets company_id and name
    #[test]
    fn pb_15_for_company() {
        let id = Uuid::new_v4();
        let b  = ProjectBrowserState::for_company(id, "Acme");
        assert_eq!(b.company_id, Some(id));
        assert_eq!(b.company_name, "Acme");
    }
}
