//! Plugin submission — upload and version management.
//!
//! Guides publishers through:
//! 1. Plugin metadata (name, description, category)
//! 2. Package upload + content hashing
//! 3. Version management
//! 4. Pre-submission validation
//! 5. Submission history tracking

use serde::{Deserialize, Serialize};

/// Plugin categories.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginCategory {
    Utility,
    Design,
    Export,
    Import,
    Collaboration,
    Accessibility,
    Animation,
    DataViz,
    DevTools,
    Custom(String),
}

impl PluginCategory {
    /// All standard categories.
    pub fn all_standard() -> Vec<Self> {
        vec![
            Self::Utility,
            Self::Design,
            Self::Export,
            Self::Import,
            Self::Collaboration,
            Self::Accessibility,
            Self::Animation,
            Self::DataViz,
            Self::DevTools,
        ]
    }

    /// Human-readable label.
    pub fn label(&self) -> &str {
        match self {
            Self::Utility => "Utility",
            Self::Design => "Design Tools",
            Self::Export => "Export",
            Self::Import => "Import",
            Self::Collaboration => "Collaboration",
            Self::Accessibility => "Accessibility",
            Self::Animation => "Animation",
            Self::DataViz => "Data Visualization",
            Self::DevTools => "Developer Tools",
            Self::Custom(name) => name,
        }
    }

    /// URL-safe slug.
    pub fn slug(&self) -> String {
        match self {
            Self::Utility => "utility".into(),
            Self::Design => "design".into(),
            Self::Export => "export".into(),
            Self::Import => "import".into(),
            Self::Collaboration => "collaboration".into(),
            Self::Accessibility => "accessibility".into(),
            Self::Animation => "animation".into(),
            Self::DataViz => "data_viz".into(),
            Self::DevTools => "dev_tools".into(),
            Self::Custom(name) => name.to_lowercase().replace(' ', "_"),
        }
    }

    /// Parse from slug string.
    pub fn from_slug(s: &str) -> Self {
        match s {
            "utility" => Self::Utility,
            "design" => Self::Design,
            "export" => Self::Export,
            "import" => Self::Import,
            "collaboration" => Self::Collaboration,
            "accessibility" => Self::Accessibility,
            "animation" => Self::Animation,
            "data_viz" => Self::DataViz,
            "dev_tools" => Self::DevTools,
            other => Self::Custom(other.to_string()),
        }
    }
}

impl std::fmt::Display for PluginCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Plugin submission form data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginForm {
    pub name: String,
    pub description: String,
    pub category: String,
    pub version: String,
    pub tags: Vec<String>,
    pub min_logos_version: String,
    pub changelog: String,
    pub readme: String,
    pub license: String,
    pub source_url: String,
    pub icon_path: String,
    pub screenshots: Vec<String>,
}

impl PluginForm {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Validation result for a plugin submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub score: u32, // 0-100
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

/// A validation error (blocks submission).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

/// A validation warning (advisory).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub field: String,
    pub message: String,
}

/// Pre-submission validator.
pub struct SubmissionValidator;

impl SubmissionValidator {
    /// Validate a plugin form.
    pub fn validate(form: &PluginForm) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut score: i32 = 100;

        // Required fields
        if form.name.is_empty() {
            errors.push(ValidationError {
                field: "name".into(),
                message: "Plugin name is required".into(),
            });
            score -= 25;
        } else if form.name.len() < 3 {
            errors.push(ValidationError {
                field: "name".into(),
                message: "Plugin name must be at least 3 characters".into(),
            });
            score -= 25;
        }

        if form.description.is_empty() {
            errors.push(ValidationError {
                field: "description".into(),
                message: "Description is required".into(),
            });
            score -= 20;
        } else if form.description.len() < 20 {
            warnings.push(ValidationWarning {
                field: "description".into(),
                message: "Description should be at least 20 characters for discoverability".into(),
            });
            score -= 5;
        }

        if form.category.is_empty() {
            errors.push(ValidationError {
                field: "category".into(),
                message: "Category is required".into(),
            });
            score -= 15;
        }

        // Version format (semver-like)
        if form.version.is_empty() {
            errors.push(ValidationError {
                field: "version".into(),
                message: "Version is required".into(),
            });
            score -= 15;
        } else if !Self::is_valid_semver(&form.version) {
            errors.push(ValidationError {
                field: "version".into(),
                message: "Version must be in semver format (e.g., 1.0.0)".into(),
            });
            score -= 10;
        }

        // Optional but recommended
        if form.tags.is_empty() {
            warnings.push(ValidationWarning {
                field: "tags".into(),
                message: "Adding tags improves discoverability".into(),
            });
            score -= 5;
        }

        if form.readme.is_empty() {
            warnings.push(ValidationWarning {
                field: "readme".into(),
                message: "A README helps users understand your plugin".into(),
            });
            score -= 10;
        }

        if form.license.is_empty() {
            warnings.push(ValidationWarning {
                field: "license".into(),
                message: "Specifying a license builds trust".into(),
            });
            score -= 5;
        }

        if form.icon_path.is_empty() {
            warnings.push(ValidationWarning {
                field: "icon".into(),
                message: "An icon makes your plugin stand out".into(),
            });
            score -= 5;
        }

        if form.screenshots.is_empty() {
            warnings.push(ValidationWarning {
                field: "screenshots".into(),
                message: "Screenshots help users decide to install".into(),
            });
            score -= 5;
        }

        ValidationResult {
            errors,
            warnings,
            score: score.max(0) as u32,
        }
    }

    /// Check if a version string is valid semver.
    fn is_valid_semver(version: &str) -> bool {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return false;
        }
        parts.iter().all(|p| p.parse::<u32>().is_ok())
    }
}

/// Submission history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionRecord {
    pub plugin_id: String,
    pub plugin_name: String,
    pub submitted_at: u64,
    pub status: SubmissionStatus,
}

/// Status of a submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmissionStatus {
    Draft,
    Submitted,
    InReview,
    Approved,
    Rejected,
    NeedsChanges,
}

impl std::fmt::Display for SubmissionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draft => write!(f, "Draft"),
            Self::Submitted => write!(f, "Submitted"),
            Self::InReview => write!(f, "In Review"),
            Self::Approved => write!(f, "Approved"),
            Self::Rejected => write!(f, "Rejected"),
            Self::NeedsChanges => write!(f, "Needs Changes"),
        }
    }
}

/// Current state of the plugin submission flow.
pub struct SubmissionState {
    pub form: PluginForm,
    pub validation: Option<ValidationResult>,
    pub package_path: Option<String>,
    pub content_hash: Option<String>,
    pub package_size: Option<u64>,
}

impl SubmissionState {
    pub fn new() -> Self {
        Self {
            form: PluginForm::new(),
            validation: None,
            package_path: None,
            content_hash: None,
            package_size: None,
        }
    }

    /// Check if ready to submit.
    pub fn is_ready(&self) -> bool {
        self.validation.as_ref().map_or(false, |v| v.is_valid())
            && self.content_hash.is_some()
            && self.package_size.is_some()
    }
}

impl Default for SubmissionState {
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin submission flow controller.
pub struct PluginSubmission {
    state: SubmissionState,
    history: Vec<SubmissionRecord>,
}

impl PluginSubmission {
    /// Create new submission flow.
    pub fn new() -> Self {
        Self {
            state: SubmissionState::new(),
            history: Vec::new(),
        }
    }

    /// Get current state.
    pub fn state(&self) -> &SubmissionState {
        &self.state
    }

    /// Get mutable state.
    pub fn state_mut(&mut self) -> &mut SubmissionState {
        &mut self.state
    }

    /// Update the form.
    pub fn update_form(&mut self, form: PluginForm) {
        self.state.form = form;
        self.state.validation = None; // invalidate on change
    }

    /// Validate the current form.
    pub fn validate(&mut self) -> &ValidationResult {
        let result = SubmissionValidator::validate(&self.state.form);
        self.state.validation = Some(result);
        self.state.validation.as_ref().unwrap()
    }

    /// Set the package info (after upload/hashing).
    pub fn set_package(&mut self, path: &str, content_hash: &str, size: u64) {
        self.state.package_path = Some(path.to_string());
        self.state.content_hash = Some(content_hash.to_string());
        self.state.package_size = Some(size);
    }

    /// Record a submission.
    pub fn record_submission(&mut self, plugin_id: &str, name: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs();

        self.history.push(SubmissionRecord {
            plugin_id: plugin_id.to_string(),
            plugin_name: name.to_string(),
            submitted_at: now,
            status: SubmissionStatus::Submitted,
        });

        // Reset form for next submission
        self.state = SubmissionState::new();
    }

    /// Get submission history.
    pub fn history(&self) -> &[SubmissionRecord] {
        &self.history
    }

    /// Get submission count.
    pub fn submission_count(&self) -> usize {
        self.history.len()
    }

    /// Reset the form.
    pub fn reset(&mut self) {
        self.state = SubmissionState::new();
    }
}

impl Default for PluginSubmission {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_categories() {
        let cats = PluginCategory::all_standard();
        assert_eq!(cats.len(), 9);
        assert_eq!(PluginCategory::Utility.slug(), "utility");
        assert_eq!(PluginCategory::from_slug("design"), PluginCategory::Design);
    }

    #[test]
    fn test_custom_category() {
        let cat = PluginCategory::Custom("My Category".into());
        assert_eq!(cat.slug(), "my_category");
        assert_eq!(cat.label(), "My Category");
    }

    #[test]
    fn test_plugin_form_default() {
        let form = PluginForm::new();
        assert!(form.name.is_empty());
        assert!(form.tags.is_empty());
    }

    #[test]
    fn test_validation_empty_form() {
        let form = PluginForm::new();
        let result = SubmissionValidator::validate(&form);
        assert!(!result.is_valid());
        assert!(result.errors.len() >= 3); // name, description, category, version
    }

    #[test]
    fn test_validation_minimal_valid() {
        let form = PluginForm {
            name: "My Plugin".into(),
            description: "A great plugin that does things".into(),
            category: "utility".into(),
            version: "1.0.0".into(),
            ..Default::default()
        };
        let result = SubmissionValidator::validate(&form);
        assert!(result.is_valid());
        assert!(result.has_warnings()); // missing optional fields
    }

    #[test]
    fn test_validation_full_form() {
        let form = PluginForm {
            name: "My Plugin".into(),
            description: "A great plugin for designers".into(),
            category: "utility".into(),
            version: "1.0.0".into(),
            tags: vec!["design".into(), "tools".into()],
            readme: "# My Plugin\n\nDoes things.".into(),
            license: "MIT".into(),
            icon_path: "/icons/my-plugin.png".into(),
            screenshots: vec!["/screenshots/main.png".into()],
            ..Default::default()
        };
        let result = SubmissionValidator::validate(&form);
        assert!(result.is_valid());
        assert_eq!(result.score, 100);
    }

    #[test]
    fn test_semver_validation() {
        assert!(SubmissionValidator::is_valid_semver("1.0.0"));
        assert!(SubmissionValidator::is_valid_semver("0.1.0"));
        assert!(SubmissionValidator::is_valid_semver("10.20.30"));
        assert!(!SubmissionValidator::is_valid_semver("1.0"));
        assert!(!SubmissionValidator::is_valid_semver("latest"));
        assert!(!SubmissionValidator::is_valid_semver("1.0.0-beta"));
    }

    #[test]
    fn test_submission_flow() {
        let mut sub = PluginSubmission::new();
        assert_eq!(sub.submission_count(), 0);

        let form = PluginForm {
            name: "Test Plugin".into(),
            description: "A test plugin description".into(),
            category: "utility".into(),
            version: "1.0.0".into(),
            ..Default::default()
        };
        sub.update_form(form);
        let result = sub.validate();
        assert!(result.is_valid());

        sub.set_package("/tmp/plugin.zip", "abc123", 2048);
        assert!(sub.state().is_ready());

        sub.record_submission("plugin-123", "Test Plugin");
        assert_eq!(sub.submission_count(), 1);
        assert_eq!(sub.history()[0].status, SubmissionStatus::Submitted);
    }

    #[test]
    fn test_submission_reset() {
        let mut sub = PluginSubmission::new();
        sub.state_mut().form.name = "Test".into();
        sub.reset();
        assert!(sub.state().form.name.is_empty());
    }

    #[test]
    fn test_submission_status_display() {
        assert_eq!(SubmissionStatus::Draft.to_string(), "Draft");
        assert_eq!(SubmissionStatus::InReview.to_string(), "In Review");
        assert_eq!(SubmissionStatus::Approved.to_string(), "Approved");
    }

    #[test]
    fn test_not_ready_without_package() {
        let mut sub = PluginSubmission::new();
        sub.state_mut().form = PluginForm {
            name: "Ready Test".into(),
            description: "Testing readiness check".into(),
            category: "utility".into(),
            version: "1.0.0".into(),
            ..Default::default()
        };
        sub.validate();
        assert!(!sub.state().is_ready()); // no package
    }
}
