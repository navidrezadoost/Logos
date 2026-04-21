// logos-desktop/src/company_hub.rs
//
//! Pure-data state for the Company Hub screen — lists all companies the user
//! belongs to and supports creating / joining a company.
//!
//! No `desktop-ui` deps required.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── CompanyEntry ──────────────────────────────────────────────────────────────

/// A company listing as returned by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyEntry {
    pub id:           Uuid,
    pub name:         String,
    /// Human-readable role the current user holds in this company.
    pub role:         String,
    pub member_count: usize,
    /// Number of projects in this company accessible to the current user.
    pub project_count: usize,
}

// ── CreateCompanyForm ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct CreateCompanyForm {
    pub name:         String,
    pub name_error:   Option<String>,
    pub server_error: Option<String>,
    pub submitting:   bool,
    pub success:      Option<Uuid>,  // id of the newly created company
}

impl CreateCompanyForm {
    pub fn validate(&mut self) -> bool {
        self.name_error = None;
        if self.name.trim().is_empty() {
            self.name_error = Some("Company name is required".into());
            return false;
        }
        if self.name.trim().len() < 2 {
            self.name_error = Some("Company name must be at least 2 characters".into());
            return false;
        }
        true
    }

    pub fn begin_submit(&mut self) {
        self.submitting   = true;
        self.server_error = None;
    }

    pub fn on_ok(&mut self, new_id: Uuid) {
        self.submitting = false;
        self.success    = Some(new_id);
    }

    pub fn on_error(&mut self, msg: impl Into<String>) {
        self.submitting   = false;
        self.server_error = Some(msg.into());
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

// ── CompanyHubState ───────────────────────────────────────────────────────────

/// All state for the company hub screen.
#[derive(Debug, Clone, Default)]
pub struct CompanyHubState {
    /// Companies loaded from the server.
    pub companies:    Vec<CompanyEntry>,
    /// `true` while the list is loading.
    pub loading:      bool,
    /// Error from loading the list.
    pub load_error:   Option<String>,
    /// Search/filter text.
    pub filter:       String,
    /// Whether the "create company" form is open.
    pub show_create:  bool,
    /// Create company form state.
    pub create_form:  CreateCompanyForm,
}

impl CompanyHubState {
    pub fn new() -> Self { Self::default() }

    pub fn begin_load(&mut self) {
        self.loading    = true;
        self.load_error = None;
    }

    pub fn on_loaded(&mut self, companies: Vec<CompanyEntry>) {
        self.loading   = false;
        self.companies = companies;
    }

    pub fn on_load_error(&mut self, msg: impl Into<String>) {
        self.loading    = false;
        self.load_error = Some(msg.into());
    }

    /// Filtered + sorted company list.
    pub fn visible_companies(&self) -> Vec<&CompanyEntry> {
        let f = self.filter.to_lowercase();
        let mut v: Vec<&CompanyEntry> = self.companies.iter()
            .filter(|c| f.is_empty() || c.name.to_lowercase().contains(&f))
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub fn open_create(&mut self) {
        self.create_form.reset();
        self.show_create = true;
    }

    pub fn close_create(&mut self) {
        self.show_create = false;
    }

    /// Called after a company is successfully created.
    pub fn add_company(&mut self, entry: CompanyEntry) {
        self.companies.push(entry);
        self.show_create = false;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (CH-01 … CH-15)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str) -> CompanyEntry {
        CompanyEntry {
            id:            Uuid::new_v4(),
            name:          name.into(),
            role:          "Member".into(),
            member_count:  3,
            project_count: 2,
        }
    }

    // CH-01: starts empty and not loading
    #[test]
    fn ch_01_starts_empty() {
        let h = CompanyHubState::new();
        assert!(h.companies.is_empty());
        assert!(!h.loading);
    }

    // CH-02: begin_load sets loading flag
    #[test]
    fn ch_02_begin_load() {
        let mut h = CompanyHubState::new();
        h.begin_load();
        assert!(h.loading);
        assert!(h.load_error.is_none());
    }

    // CH-03: on_loaded populates companies
    #[test]
    fn ch_03_on_loaded() {
        let mut h = CompanyHubState::new();
        h.on_loaded(vec![make_entry("Acme"), make_entry("Globex")]);
        assert_eq!(h.companies.len(), 2);
        assert!(!h.loading);
    }

    // CH-04: on_load_error sets error and clears loading
    #[test]
    fn ch_04_on_load_error() {
        let mut h = CompanyHubState::new();
        h.begin_load();
        h.on_load_error("timeout");
        assert!(!h.loading);
        assert!(h.load_error.is_some());
    }

    // CH-05: visible_companies filters by name
    #[test]
    fn ch_05_filter() {
        let mut h = CompanyHubState::new();
        h.on_loaded(vec![make_entry("Acme"), make_entry("Globex")]);
        h.filter = "acm".into();
        let v = h.visible_companies();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "Acme");
    }

    // CH-06: visible_companies is sorted alphabetically
    #[test]
    fn ch_06_sorted() {
        let mut h = CompanyHubState::new();
        h.on_loaded(vec![make_entry("Zebra"), make_entry("Alpha")]);
        let v = h.visible_companies();
        assert_eq!(v[0].name, "Alpha");
        assert_eq!(v[1].name, "Zebra");
    }

    // CH-07: filter is case-insensitive
    #[test]
    fn ch_07_filter_case_insensitive() {
        let mut h = CompanyHubState::new();
        h.on_loaded(vec![make_entry("Acme")]);
        h.filter = "ACME".into();
        assert_eq!(h.visible_companies().len(), 1);
    }

    // CH-08: open_create resets the form and sets show_create
    #[test]
    fn ch_08_open_create() {
        let mut h = CompanyHubState::new();
        h.create_form.name = "leftover".into();
        h.open_create();
        assert!(h.show_create);
        assert!(h.create_form.name.is_empty());
    }

    // CH-09: close_create hides the form
    #[test]
    fn ch_09_close_create() {
        let mut h = CompanyHubState::new();
        h.open_create();
        h.close_create();
        assert!(!h.show_create);
    }

    // CH-10: CreateCompanyForm::validate rejects empty name
    #[test]
    fn ch_10_form_empty_name() {
        let mut f = CreateCompanyForm::default();
        assert!(!f.validate());
        assert!(f.name_error.is_some());
    }

    // CH-11: CreateCompanyForm::validate rejects name too short
    #[test]
    fn ch_11_form_name_too_short() {
        let mut f = CreateCompanyForm { name: "A".into(), ..Default::default() };
        assert!(!f.validate());
    }

    // CH-12: CreateCompanyForm::validate passes valid name
    #[test]
    fn ch_12_form_valid_name() {
        let mut f = CreateCompanyForm { name: "Acme Corp".into(), ..Default::default() };
        assert!(f.validate());
    }

    // CH-13: on_ok sets success id and clears submitting
    #[test]
    fn ch_13_form_on_ok() {
        let mut f = CreateCompanyForm { name: "Acme".into(), ..Default::default() };
        let id = Uuid::new_v4();
        f.begin_submit();
        f.on_ok(id);
        assert_eq!(f.success, Some(id));
        assert!(!f.submitting);
    }

    // CH-14: add_company appends entry and closes form
    #[test]
    fn ch_14_add_company() {
        let mut h = CompanyHubState::new();
        h.show_create = true;
        h.add_company(make_entry("NewCo"));
        assert_eq!(h.companies.len(), 1);
        assert!(!h.show_create);
    }

    // CH-15: empty filter shows all companies
    #[test]
    fn ch_15_no_filter_shows_all() {
        let mut h = CompanyHubState::new();
        h.on_loaded(vec![make_entry("A"), make_entry("B"), make_entry("C")]);
        assert_eq!(h.visible_companies().len(), 3);
    }
}
