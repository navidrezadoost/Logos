//! Publisher onboarding — guided registration flow.
//!
//! Walks new publishers through:
//! 1. Name & profile creation
//! 2. Ed25519 key generation
//! 3. Email/domain verification
//! 4. First plugin checklist
//! 5. Analytics setup

use logos_marketplace_auth::crypto::Ed25519KeyPair;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Steps in the publisher onboarding flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OnboardingStep {
    /// Create publisher profile (name, website, bio)
    Registration,
    /// Generate Ed25519 keypair
    KeyGeneration,
    /// Verify email address
    EmailVerification,
    /// Read and accept developer guidelines
    AcceptGuidelines,
    /// Submit first plugin
    FirstPlugin,
    /// Configure analytics preferences
    AnalyticsSetup,
}

impl OnboardingStep {
    /// Human-readable label for the step.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Registration => "Create Your Profile",
            Self::KeyGeneration => "Generate Signing Key",
            Self::EmailVerification => "Verify Email",
            Self::AcceptGuidelines => "Accept Developer Guidelines",
            Self::FirstPlugin => "Submit Your First Plugin",
            Self::AnalyticsSetup => "Configure Analytics",
        }
    }

    /// Description text for the step.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Registration => "Choose your publisher name and set up your public profile.",
            Self::KeyGeneration => "Generate an Ed25519 keypair to sign your plugins.",
            Self::EmailVerification => "Verify your email to increase trust level.",
            Self::AcceptGuidelines => "Read and accept the Logos developer guidelines.",
            Self::FirstPlugin => "Package and submit your first plugin for review.",
            Self::AnalyticsSetup => "Choose what analytics data to collect.",
        }
    }

    /// All steps in order.
    pub fn all() -> Vec<OnboardingStep> {
        vec![
            Self::Registration,
            Self::KeyGeneration,
            Self::EmailVerification,
            Self::AcceptGuidelines,
            Self::FirstPlugin,
            Self::AnalyticsSetup,
        ]
    }

    /// Step number (1-indexed).
    pub fn number(&self) -> usize {
        match self {
            Self::Registration => 1,
            Self::KeyGeneration => 2,
            Self::EmailVerification => 3,
            Self::AcceptGuidelines => 4,
            Self::FirstPlugin => 5,
            Self::AnalyticsSetup => 6,
        }
    }
}

impl std::fmt::Display for OnboardingStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Step {}: {}", self.number(), self.label())
    }
}

/// Registration form data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistrationForm {
    pub name: String,
    pub display_name: String,
    pub email: String,
    pub website: String,
    pub bio: String,
    pub github: String,
    pub organization: String,
}

impl RegistrationForm {
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate the form.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.name.is_empty() {
            errors.push("Publisher name is required".into());
        }
        if self.name.len() < 3 {
            errors.push("Publisher name must be at least 3 characters".into());
        }
        if self.name.len() > 64 {
            errors.push("Publisher name must be at most 64 characters".into());
        }
        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            errors.push("Publisher name may only contain letters, numbers, hyphens, and underscores".into());
        }
        if !self.email.is_empty() && !self.email.contains('@') {
            errors.push("Invalid email address".into());
        }
        if !self.website.is_empty()
            && !self.website.starts_with("http://")
            && !self.website.starts_with("https://")
        {
            errors.push("Website must start with http:// or https://".into());
        }

        errors
    }

    /// Check if the form is valid.
    pub fn is_valid(&self) -> bool {
        self.validate().is_empty()
    }
}

/// Key backup info displayed to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBackupInfo {
    pub public_key_hex: String,
    pub fingerprint: String,
    pub generated_at: u64,
    /// Mnemonic-style display of the key (groups of 4 hex chars)
    pub display_groups: Vec<String>,
}

impl KeyBackupInfo {
    /// Create from a keypair.
    pub fn from_keypair(kp: &Ed25519KeyPair) -> Self {
        let public_hex = kp.public_key().to_hex();
        let fingerprint = kp.public_key().fingerprint();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs();

        let groups: Vec<String> = public_hex
            .as_bytes()
            .chunks(4)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or("????").to_string())
            .collect();

        Self {
            public_key_hex: public_hex,
            fingerprint,
            generated_at: now,
            display_groups: groups,
        }
    }
}

/// Developer guidelines content.
pub struct DeveloperGuidelines;

impl DeveloperGuidelines {
    /// Get guideline sections.
    pub fn sections() -> Vec<(&'static str, &'static str)> {
        vec![
            ("Code Quality", "All plugins must pass automated code quality checks. No minified or obfuscated code."),
            ("Security", "Plugins must not access user data without consent. All network requests must be documented."),
            ("Performance", "Plugins must not significantly impact application performance. Maximum 100ms initialization time."),
            ("User Experience", "Plugins must follow Logos design guidelines. Provide clear documentation."),
            ("Licensing", "All plugins must declare their license. Open source is encouraged but not required."),
            ("Content Policy", "No malicious, deceptive, or illegal content. Respect intellectual property rights."),
            ("Updates", "Publishers are responsible for maintaining their plugins. Abandoned plugins may be archived."),
            ("Revenue", "Revenue sharing follows the standard 70/30 split. Free plugins are always welcome."),
        ]
    }
}

/// Current state of the onboarding flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingState {
    pub current_step: OnboardingStep,
    pub completed_steps: HashSet<OnboardingStep>,
    pub form: RegistrationForm,
    pub key_info: Option<KeyBackupInfo>,
    pub guidelines_accepted: bool,
}

/// The publisher onboarding flow controller.
pub struct PublisherOnboarding {
    state: OnboardingState,
}

impl PublisherOnboarding {
    /// Create a new onboarding flow.
    pub fn new() -> Self {
        Self {
            state: OnboardingState {
                current_step: OnboardingStep::Registration,
                completed_steps: HashSet::new(),
                form: RegistrationForm::new(),
                key_info: None,
                guidelines_accepted: false,
            },
        }
    }

    /// Get the current state.
    pub fn state(&self) -> &OnboardingState {
        &self.state
    }

    /// Get the current step.
    pub fn current_step(&self) -> OnboardingStep {
        self.state.current_step
    }

    /// Check if a step is completed.
    pub fn is_completed(&self, step: OnboardingStep) -> bool {
        self.state.completed_steps.contains(&step)
    }

    /// Overall progress (0.0 to 1.0).
    pub fn progress(&self) -> f32 {
        self.state.completed_steps.len() as f32 / OnboardingStep::all().len() as f32
    }

    /// Mark a step as completed and advance to the next.
    pub fn complete_step(&mut self, step: OnboardingStep) {
        self.state.completed_steps.insert(step);

        // Advance to next uncompleted step
        for s in OnboardingStep::all() {
            if !self.state.completed_steps.contains(&s) {
                self.state.current_step = s;
                return;
            }
        }

        // All steps complete — stay on last
        self.state.current_step = OnboardingStep::AnalyticsSetup;
    }

    /// Go back to a previous step.
    pub fn go_to_step(&mut self, step: OnboardingStep) {
        self.state.current_step = step;
    }

    /// Update the registration form.
    pub fn update_form(&mut self, form: RegistrationForm) {
        self.state.form = form;
    }

    /// Generate a keypair and store the backup info.
    pub fn generate_key(&mut self) -> KeyBackupInfo {
        let kp = Ed25519KeyPair::generate();
        let info = KeyBackupInfo::from_keypair(&kp);
        self.state.key_info = Some(info.clone());
        info
    }

    /// Accept developer guidelines.
    pub fn accept_guidelines(&mut self) {
        self.state.guidelines_accepted = true;
        self.complete_step(OnboardingStep::AcceptGuidelines);
    }

    /// Check if onboarding is fully complete.
    pub fn is_complete(&self) -> bool {
        self.state.completed_steps.len() == OnboardingStep::all().len()
    }

    /// Get completion summary.
    pub fn summary(&self) -> Vec<(OnboardingStep, bool)> {
        OnboardingStep::all()
            .into_iter()
            .map(|s| (s, self.is_completed(s)))
            .collect()
    }

    /// Reset the onboarding flow.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for PublisherOnboarding {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onboarding_steps() {
        let steps = OnboardingStep::all();
        assert_eq!(steps.len(), 6);
        assert_eq!(steps[0], OnboardingStep::Registration);
        assert_eq!(steps[5], OnboardingStep::AnalyticsSetup);
    }

    #[test]
    fn test_step_labels() {
        assert_eq!(OnboardingStep::Registration.label(), "Create Your Profile");
        assert_eq!(OnboardingStep::KeyGeneration.number(), 2);
    }

    #[test]
    fn test_registration_form_validate() {
        let mut form = RegistrationForm::new();
        assert!(!form.is_valid()); // empty name

        form.name = "al".into();
        assert!(!form.is_valid()); // too short

        form.name = "alice-dev".into();
        assert!(form.is_valid());

        form.email = "bad-email".into();
        assert!(!form.is_valid());

        form.email = "alice@example.com".into();
        assert!(form.is_valid());
    }

    #[test]
    fn test_form_website_validation() {
        let mut form = RegistrationForm::new();
        form.name = "test-dev".into();
        form.website = "example.com".into();
        assert!(!form.is_valid());

        form.website = "https://example.com".into();
        assert!(form.is_valid());
    }

    #[test]
    fn test_onboarding_flow() {
        let mut ob = PublisherOnboarding::new();
        assert_eq!(ob.current_step(), OnboardingStep::Registration);
        assert_eq!(ob.progress(), 0.0);
        assert!(!ob.is_complete());

        ob.complete_step(OnboardingStep::Registration);
        assert_eq!(ob.current_step(), OnboardingStep::KeyGeneration);

        ob.complete_step(OnboardingStep::KeyGeneration);
        ob.complete_step(OnboardingStep::EmailVerification);
        ob.accept_guidelines();

        assert!(ob.progress() > 0.5);
    }

    #[test]
    fn test_generate_key() {
        let mut ob = PublisherOnboarding::new();
        let info = ob.generate_key();
        assert_eq!(info.public_key_hex.len(), 64);
        assert_eq!(info.display_groups.len(), 16); // 64 hex chars / 4
        assert!(ob.state().key_info.is_some());
    }

    #[test]
    fn test_summary() {
        let mut ob = PublisherOnboarding::new();
        let summary = ob.summary();
        assert_eq!(summary.len(), 6);
        assert!(!summary[0].1); // not completed

        ob.complete_step(OnboardingStep::Registration);
        let summary = ob.summary();
        assert!(summary[0].1); // completed
    }

    #[test]
    fn test_go_back() {
        let mut ob = PublisherOnboarding::new();
        ob.complete_step(OnboardingStep::Registration);
        assert_eq!(ob.current_step(), OnboardingStep::KeyGeneration);

        ob.go_to_step(OnboardingStep::Registration);
        assert_eq!(ob.current_step(), OnboardingStep::Registration);
    }

    #[test]
    fn test_developer_guidelines() {
        let sections = DeveloperGuidelines::sections();
        assert!(sections.len() >= 8);
        assert_eq!(sections[0].0, "Code Quality");
    }

    #[test]
    fn test_reset() {
        let mut ob = PublisherOnboarding::new();
        ob.complete_step(OnboardingStep::Registration);
        ob.complete_step(OnboardingStep::KeyGeneration);
        ob.reset();
        assert_eq!(ob.current_step(), OnboardingStep::Registration);
        assert_eq!(ob.progress(), 0.0);
    }
}
