//! Install subsystem — dependency resolution, install registry, and receipts.
//!
//! Handles the full lifecycle of installing an agent: dependency graph checks,
//! compatibility verification, install receipt recording, and uninstall.

use crate::manifest::{AgentManifest, AgentVersion};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Install status ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallStatus {
    Installed,
    Pending,
    Failed { reason: String },
    Removed,
}

impl InstallStatus {
    pub fn is_active(&self) -> bool { matches!(self, Self::Installed) }
}

// ── Installed agent record ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledAgent {
    pub agent_id: String,
    pub version: AgentVersion,
    pub user_id: String,
    pub installed_ts: u64,
    pub status: InstallStatus,
    pub auto_update: bool,
    /// IDs of agents that were installed as dependencies of this one
    pub installed_deps: Vec<String>,
}

impl InstalledAgent {
    pub fn new(
        agent_id: impl Into<String>,
        version: AgentVersion,
        user_id: impl Into<String>,
        ts: u64,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            version,
            user_id: user_id.into(),
            installed_ts: ts,
            status: InstallStatus::Installed,
            auto_update: true,
            installed_deps: Vec::new(),
        }
    }

    pub fn with_deps(mut self, deps: Vec<String>) -> Self {
        self.installed_deps = deps; self
    }

    pub fn disable_auto_update(mut self) -> Self {
        self.auto_update = false; self
    }
}

// ── Install request & result ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    pub agent_id: String,
    pub user_id: String,
    pub logos_version: AgentVersion,
    pub timestamp_secs: u64,
    pub install_deps_automatically: bool,
}

impl InstallRequest {
    pub fn new(
        agent_id: impl Into<String>,
        user_id: impl Into<String>,
        logos_version: AgentVersion,
        ts: u64,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            user_id: user_id.into(),
            logos_version,
            timestamp_secs: ts,
            install_deps_automatically: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResult {
    pub agent_id: String,
    pub success: bool,
    pub installed_version: Option<AgentVersion>,
    pub deps_installed: Vec<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

impl InstallResult {
    pub fn success(agent_id: impl Into<String>, version: AgentVersion, deps: Vec<String>, ms: u64) -> Self {
        Self {
            agent_id: agent_id.into(),
            success: true,
            installed_version: Some(version),
            deps_installed: deps,
            error: None,
            duration_ms: ms,
        }
    }

    pub fn failure(agent_id: impl Into<String>, reason: impl Into<String>, ms: u64) -> Self {
        Self {
            agent_id: agent_id.into(),
            success: false,
            installed_version: None,
            deps_installed: Vec::new(),
            error: Some(reason.into()),
            duration_ms: ms,
        }
    }
}

// ── Dependency resolver ───────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct DependencyResolver {
    /// All available manifests by agent_id
    available: HashMap<String, AgentManifest>,
}

impl DependencyResolver {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, manifest: AgentManifest) {
        self.available.insert(manifest.id.clone(), manifest);
    }

    /// Returns ordered list of agent IDs to install (dependencies first).
    /// Returns Err if any dep is missing or circular.
    pub fn resolve(
        &self,
        agent_id: &str,
        logos_version: &AgentVersion,
        already_installed: &[String],
    ) -> Result<Vec<String>, String> {
        let mut order = Vec::new();
        let mut visited = std::collections::HashSet::new();
        self.resolve_inner(agent_id, logos_version, already_installed, &mut order, &mut visited, 0)?;
        Ok(order)
    }

    fn resolve_inner(
        &self,
        id: &str,
        logos_version: &AgentVersion,
        already_installed: &[String],
        order: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 20 {
            return Err(format!("Circular dependency detected near '{}'", id));
        }
        if already_installed.contains(&id.to_string()) { return Ok(()); }
        if visited.contains(id) { return Ok(()); }
        visited.insert(id.to_string());

        let manifest = self.available.get(id)
            .ok_or_else(|| format!("Agent '{}' not found in marketplace", id))?;

        if !manifest.compatibility.is_compatible(logos_version) {
            return Err(format!(
                "Agent '{}' requires Logos >= {} (have {})",
                id, manifest.compatibility.min_logos_version, logos_version
            ));
        }

        for dep_id in &manifest.compatibility.agent_dependencies {
            self.resolve_inner(dep_id, logos_version, already_installed, order, visited, depth + 1)?;
        }
        order.push(id.to_string());
        Ok(())
    }
}

// ── Install registry ──────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct InstallRegistry {
    /// (user_id, agent_id) → InstalledAgent
    installs: HashMap<(String, String), InstalledAgent>,
    resolver: DependencyResolver,
    /// Simulated install duration (ms) — always fast in tests
    simulated_install_ms: u64,
}

impl InstallRegistry {
    pub fn new() -> Self { Self { simulated_install_ms: 50, ..Default::default() } }

    pub fn register_agent(&mut self, manifest: AgentManifest) {
        self.resolver.register(manifest);
    }

    /// Install an agent for a user. Resolves dependencies and records installs.
    pub fn install(&mut self, req: &InstallRequest) -> InstallResult {
        let already: Vec<String> = self.installs.iter()
            .filter(|((uid, _), r)| uid == &req.user_id && r.status.is_active())
            .map(|((_, aid), _)| aid.clone())
            .collect();

        let order = match self.resolver.resolve(&req.agent_id, &req.logos_version, &already) {
            Ok(o) => o,
            Err(e) => return InstallResult::failure(&req.agent_id, e, self.simulated_install_ms),
        };

        // Install all in order
        let mut installed_deps = Vec::new();
        let mut main_version = None;

        for id in &order {
            let version = self.resolver.available.get(id.as_str())
                .map(|m| m.version.clone())
                .unwrap_or_else(|| AgentVersion::new(0, 0, 0));

            if id == &req.agent_id {
                main_version = Some(version.clone());
            } else {
                installed_deps.push(id.clone());
            }

            let record = InstalledAgent::new(id, version, &req.user_id, req.timestamp_secs);
            self.installs.insert((req.user_id.clone(), id.clone()), record);
        }

        InstallResult::success(
            &req.agent_id,
            main_version.unwrap_or_else(|| AgentVersion::new(1, 0, 0)),
            installed_deps,
            self.simulated_install_ms,
        )
    }

    pub fn uninstall(&mut self, user_id: &str, agent_id: &str) -> bool {
        if let Some(rec) = self.installs.get_mut(&(user_id.to_string(), agent_id.to_string())) {
            rec.status = InstallStatus::Removed;
            return true;
        }
        false
    }

    pub fn is_installed(&self, user_id: &str, agent_id: &str) -> bool {
        self.installs.get(&(user_id.to_string(), agent_id.to_string()))
            .map(|r| r.status.is_active())
            .unwrap_or(false)
    }

    pub fn installed_agents(&self, user_id: &str) -> Vec<&InstalledAgent> {
        self.installs.iter()
            .filter(|((uid, _), r)| uid == user_id && r.status.is_active())
            .map(|(_, r)| r)
            .collect()
    }

    pub fn total_installs_for_agent(&self, agent_id: &str) -> u64 {
        self.installs.iter()
            .filter(|((_, aid), r)| aid == agent_id && r.status.is_active())
            .count() as u64
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{AgentCategory, AgentManifest, CompatibilityMatrix, PricingModel};

    fn v(a: u16, b: u16, c: u16) -> AgentVersion { AgentVersion::new(a, b, c) }

    fn make_manifest(id: &str, deps: &[&str]) -> AgentManifest {
        let mut compat = CompatibilityMatrix::new(v(1, 0, 0));
        for dep in deps { compat = compat.with_dependency(dep); }
        let mut m = AgentManifest::new(
            id, id, "desc", "author", "author",
            v(1, 2, 0), AgentCategory::Productivity,
            PricingModel::Free, v(1, 0, 0), 0,
        );
        m.compatibility = compat;
        m
    }

    #[test]
    fn basic_install_succeeds() {
        let mut reg = InstallRegistry::new();
        reg.register_agent(make_manifest("wcag-checker", &[]));
        let req = InstallRequest::new("wcag-checker", "user-1", v(1, 5, 0), 100);
        let result = reg.install(&req);
        assert!(result.success, "Install should succeed");
        assert!(reg.is_installed("user-1", "wcag-checker"));
    }

    #[test]
    fn install_with_dependency_installs_dep_first() {
        let mut reg = InstallRegistry::new();
        reg.register_agent(make_manifest("color-core", &[]));
        reg.register_agent(make_manifest("color-pro", &["color-core"]));

        let req = InstallRequest::new("color-pro", "user-1", v(1, 5, 0), 100);
        let result = reg.install(&req);
        assert!(result.success);
        assert_eq!(result.deps_installed, vec!["color-core"]);
        assert!(reg.is_installed("user-1", "color-core"));
        assert!(reg.is_installed("user-1", "color-pro"));
    }

    #[test]
    fn install_missing_agent_fails() {
        let mut reg = InstallRegistry::new();
        let req = InstallRequest::new("nonexistent", "user-1", v(1, 5, 0), 0);
        let result = reg.install(&req);
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn install_incompatible_logos_fails() {
        let mut reg = InstallRegistry::new();
        // Agent requires Logos 2.0
        let mut m = make_manifest("futuristic-agent", &[]);
        m.compatibility = CompatibilityMatrix::new(v(2, 0, 0));
        reg.register_agent(m);

        let req = InstallRequest::new("futuristic-agent", "user-1", v(1, 9, 9), 0);
        let result = reg.install(&req);
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("Logos >="));
    }

    #[test]
    fn uninstall_deactivates_record() {
        let mut reg = InstallRegistry::new();
        reg.register_agent(make_manifest("agent-x", &[]));
        let req = InstallRequest::new("agent-x", "user-1", v(1, 0, 0), 0);
        reg.install(&req);
        assert!(reg.is_installed("user-1", "agent-x"));
        assert!(reg.uninstall("user-1", "agent-x"));
        assert!(!reg.is_installed("user-1", "agent-x"));
    }

    #[test]
    fn installed_agents_listing() {
        let mut reg = InstallRegistry::new();
        reg.register_agent(make_manifest("a1", &[]));
        reg.register_agent(make_manifest("a2", &[]));
        reg.install(&InstallRequest::new("a1", "user-1", v(1, 0, 0), 0));
        reg.install(&InstallRequest::new("a2", "user-1", v(1, 0, 0), 0));
        assert_eq!(reg.installed_agents("user-1").len(), 2);
        assert_eq!(reg.installed_agents("user-2").len(), 0);
    }

    #[test]
    fn total_installs_per_agent() {
        let mut reg = InstallRegistry::new();
        reg.register_agent(make_manifest("popular", &[]));
        for i in 0..5 {
            reg.install(&InstallRequest::new("popular", format!("user-{}", i), v(1, 0, 0), i as u64));
        }
        assert_eq!(reg.total_installs_for_agent("popular"), 5);
    }

    #[test]
    fn dep_already_installed_is_skipped() {
        let mut reg = InstallRegistry::new();
        reg.register_agent(make_manifest("base", &[]));
        reg.register_agent(make_manifest("tool", &["base"]));

        // Pre-install base
        reg.install(&InstallRequest::new("base", "user-1", v(1, 0, 0), 0));
        // Install tool — base should not be reinstalled
        let result = reg.install(&InstallRequest::new("tool", "user-1", v(1, 0, 0), 0));
        assert!(result.success);
        assert!(result.deps_installed.is_empty(), "base already installed, should skip");
    }

    #[test]
    fn resolver_detects_missing_dep() {
        let resolver = DependencyResolver::new(); // empty
        let result = resolver.resolve("missing-agent", &v(1, 0, 0), &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
