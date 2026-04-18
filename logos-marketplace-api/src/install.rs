//! One-click plugin install/uninstall handler and install tracking.
//!
//! `InstallRepo` tracks which plugins are installed per user, and notifies the
//! analytics layer of install events. `InstallHandlers` provides the public
//! API surface that the router calls.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logos_marketplace_db::{AnalyticsEvent, AnalyticsRepo, MarketplaceStore};

use crate::response::{ApiResponse, StatusCode};

// ── InstallRecord ─────────────────────────────────────────────────────────────

/// A record of a user having installed a specific plugin (any version).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallRecord {
    pub user_id: Uuid,
    pub plugin_id: Uuid,
    /// Semver string of the installed version, e.g. `"1.2.0"`.
    pub version: String,
    /// Unix-ms timestamp of installation.
    pub installed_at: u64,
}

// ── InstallRepo ───────────────────────────────────────────────────────────────

/// In-process install registry (production would be a DB table).
#[derive(Default, Debug)]
pub struct InstallRepo {
    records: Vec<InstallRecord>,
}

impl InstallRepo {
    pub fn new() -> Self { Self::default() }

    /// Record an install. Only one record per (user, plugin) is kept; updates
    /// the version if the user re-installs.
    pub fn install(&mut self, record: InstallRecord) {
        if let Some(existing) = self.records.iter_mut()
            .find(|r| r.user_id == record.user_id && r.plugin_id == record.plugin_id)
        {
            existing.version     = record.version;
            existing.installed_at = record.installed_at;
        } else {
            self.records.push(record);
        }
    }

    /// Remove an install record. Returns `true` if one was found.
    pub fn uninstall(&mut self, user_id: &Uuid, plugin_id: &Uuid) -> bool {
        let before = self.records.len();
        self.records.retain(|r| !(r.user_id == *user_id && r.plugin_id == *plugin_id));
        self.records.len() < before
    }

    /// `true` if `user_id` currently has `plugin_id` installed.
    pub fn is_installed(&self, user_id: &Uuid, plugin_id: &Uuid) -> bool {
        self.records.iter()
            .any(|r| r.user_id == *user_id && r.plugin_id == *plugin_id)
    }

    /// All plugins installed by a given user.
    pub fn installed_for_user(&self, user_id: &Uuid) -> Vec<&InstallRecord> {
        self.records.iter().filter(|r| r.user_id == *user_id).collect()
    }

    /// All users who have `plugin_id` installed.
    pub fn users_with_plugin(&self, plugin_id: &Uuid) -> Vec<Uuid> {
        self.records.iter()
            .filter(|r| r.plugin_id == *plugin_id)
            .map(|r| r.user_id)
            .collect()
    }

    /// Total number of distinct (user, plugin) install pairs.
    pub fn total_installs(&self) -> usize {
        self.records.len()
    }
}

// ── InstallHandlers ───────────────────────────────────────────────────────────

pub struct InstallHandlers;

impl InstallHandlers {
    /// Install a plugin for a user.
    ///
    /// Expects the `MarketplaceStore` to contain the plugin, records the install
    /// in `repo`, fires an analytics event, and increments the plugin's download
    /// counter.
    pub fn install(
        store: &mut MarketplaceStore,
        repo: &mut InstallRepo,
        analytics: &mut AnalyticsRepo,
        user_id: Uuid,
        plugin_id_str: &str,
        version: &str,
        now: u64,
    ) -> ApiResponse {
        let plugin_id = match Uuid::parse_str(plugin_id_str) {
            Ok(id) => id,
            Err(_) => return ApiResponse::error(StatusCode::BadRequest, "invalid plugin id"),
        };

        if store.plugins_ref().get(&plugin_id).is_err() {
            return ApiResponse::error(StatusCode::NotFound, "plugin not found");
        }

        let record = InstallRecord { user_id, plugin_id, version: version.to_string(), installed_at: now };
        repo.install(record);

        analytics.record(AnalyticsEvent::install(plugin_id));
        let _ = store.plugins().increment_downloads(&plugin_id);

        let body = serde_json::json!({
            "status": "installed",
            "plugin_id": plugin_id.to_string(),
            "version": version,
        });
        ApiResponse::ok(body)
    }

    /// Uninstall a plugin for a user.
    pub fn uninstall(
        repo: &mut InstallRepo,
        user_id: Uuid,
        plugin_id_str: &str,
    ) -> ApiResponse {
        let plugin_id = match Uuid::parse_str(plugin_id_str) {
            Ok(id) => id,
            Err(_) => return ApiResponse::error(StatusCode::BadRequest, "invalid plugin id"),
        };

        if repo.uninstall(&user_id, &plugin_id) {
            ApiResponse::ok(serde_json::json!({"status": "uninstalled"}))
        } else {
            ApiResponse::error(StatusCode::NotFound, "install record not found")
        }
    }

    /// List all plugins installed by a user.
    pub fn list_for_user(repo: &InstallRepo, user_id: Uuid) -> ApiResponse {
        let installs: Vec<serde_json::Value> = repo.installed_for_user(&user_id)
            .iter()
            .map(|r| serde_json::json!({
                "plugin_id":    r.plugin_id.to_string(),
                "version":      r.version,
                "installed_at": r.installed_at,
            }))
            .collect();
        ApiResponse::ok(serde_json::json!({ "installs": installs }))
    }

    /// Check whether a user has a plugin installed and what version.
    pub fn check(repo: &InstallRepo, user_id: Uuid, plugin_id_str: &str) -> ApiResponse {
        let plugin_id = match Uuid::parse_str(plugin_id_str) {
            Ok(id) => id,
            Err(_) => return ApiResponse::error(StatusCode::BadRequest, "invalid plugin id"),
        };

        let installed = repo.is_installed(&user_id, &plugin_id);
        let version = repo.installed_for_user(&user_id)
            .iter()
            .find(|r| r.plugin_id == plugin_id)
            .map(|r| r.version.clone());

        ApiResponse::ok(serde_json::json!({
            "installed": installed,
            "version":   version,
        }))
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> Uuid { Uuid::new_v4() }
    fn pid() -> Uuid { Uuid::new_v4() }

    fn record(user_id: Uuid, plugin_id: Uuid, version: &str) -> InstallRecord {
        InstallRecord { user_id, plugin_id, version: version.to_string(), installed_at: 1000 }
    }

    // InstallRepo -------------------------------------------------------------

    #[test]
    fn inst001_repo_starts_empty() {
        let repo = InstallRepo::new();
        assert_eq!(repo.total_installs(), 0);
    }

    #[test]
    fn inst002_install_one() {
        let mut repo = InstallRepo::new();
        let u = uid(); let p = pid();
        repo.install(record(u, p, "1.0.0"));
        assert_eq!(repo.total_installs(), 1);
        assert!(repo.is_installed(&u, &p));
    }

    #[test]
    fn inst003_install_idempotent_updates_version() {
        let mut repo = InstallRepo::new();
        let u = uid(); let p = pid();
        repo.install(record(u, p, "1.0.0"));
        repo.install(record(u, p, "1.1.0"));
        assert_eq!(repo.total_installs(), 1);
        assert_eq!(repo.installed_for_user(&u)[0].version, "1.1.0");
    }

    #[test]
    fn inst004_uninstall_existing() {
        let mut repo = InstallRepo::new();
        let u = uid(); let p = pid();
        repo.install(record(u, p, "1.0.0"));
        assert!(repo.uninstall(&u, &p));
        assert!(!repo.is_installed(&u, &p));
    }

    #[test]
    fn inst005_uninstall_nonexistent_returns_false() {
        let mut repo = InstallRepo::new();
        assert!(!repo.uninstall(&uid(), &pid()));
    }

    #[test]
    fn inst006_installed_for_user_multiple_plugins() {
        let mut repo = InstallRepo::new();
        let u = uid();
        let p1 = pid(); let p2 = pid();
        repo.install(record(u, p1, "1.0.0"));
        repo.install(record(u, p2, "2.0.0"));
        assert_eq!(repo.installed_for_user(&u).len(), 2);
    }

    #[test]
    fn inst007_installed_for_user_empty_for_new_user() {
        let repo = InstallRepo::new();
        assert!(repo.installed_for_user(&uid()).is_empty());
    }

    #[test]
    fn inst008_users_with_plugin() {
        let mut repo = InstallRepo::new();
        let u1 = uid(); let u2 = uid(); let p = pid();
        repo.install(record(u1, p, "1.0.0"));
        repo.install(record(u2, p, "1.0.0"));
        let users = repo.users_with_plugin(&p);
        assert_eq!(users.len(), 2);
        assert!(users.contains(&u1));
        assert!(users.contains(&u2));
    }

    #[test]
    fn inst009_not_installed_is_false() {
        let repo = InstallRepo::new();
        assert!(!repo.is_installed(&uid(), &pid()));
    }

    #[test]
    fn inst010_total_installs_after_multiple_users() {
        let mut repo = InstallRepo::new();
        let p = pid();
        for _ in 0..5 {
            repo.install(record(uid(), p, "1.0.0"));
        }
        assert_eq!(repo.total_installs(), 5);
    }

    // InstallHandlers ---------------------------------------------------------

    fn make_store_with_plugin() -> (MarketplaceStore, Uuid) {
        use logos_marketplace_db::{PluginRecord, PublisherRecord, PublisherStatus, SubmissionStatus};
        let mut store = MarketplaceStore::new();
        let publisher_id = Uuid::new_v4();
        let pub_rec = PublisherRecord {
            id: publisher_id,
            name: "Test Publisher".to_string(),
            public_key_hex: "abcdef1234567890".to_string(),
            status: PublisherStatus::Active,
            registered_at: 0,
            plugin_count: 0,
            total_downloads: 0,
        };
        store.publishers().insert(pub_rec).unwrap();

        let plugin_id = Uuid::new_v4();
        let plugin = PluginRecord {
            id: plugin_id,
            name: "TestPlugin".to_string(),
            publisher_id,
            description: "desc".to_string(),
            current_version: "1.0.0".to_string(),
            category: "utility".to_string(),
            tags: vec![],
            downloads: 0,
            rating: 0.0,
            rating_count: 0,
            status: SubmissionStatus::Approved,
            created_at: 0,
            updated_at: 0,
            content_hash: "hash".to_string(),
            package_size: 0,
            verified: true,
        };
        store.plugins().insert(plugin).unwrap();
        (store, plugin_id)
    }

    #[test]
    fn inst011_handler_install_success() {
        let (mut store, plugin_id) = make_store_with_plugin();
        let mut repo = InstallRepo::new();
        let mut analytics = AnalyticsRepo::new();
        let user_id = uid();

        let resp = InstallHandlers::install(
            &mut store, &mut repo, &mut analytics,
            user_id, &plugin_id.to_string(), "1.0.0", 1000,
        );
        assert_eq!(resp.status, StatusCode::Ok);
        assert!(repo.is_installed(&user_id, &plugin_id));
    }

    #[test]
    fn inst012_handler_install_bad_uuid() {
        let (mut store, _) = make_store_with_plugin();
        let mut repo = InstallRepo::new();
        let mut analytics = AnalyticsRepo::new();
        let resp = InstallHandlers::install(
            &mut store, &mut repo, &mut analytics,
            uid(), "not-a-uuid", "1.0.0", 0,
        );
        assert_eq!(resp.status, StatusCode::BadRequest);
    }

    #[test]
    fn inst013_handler_install_not_found() {
        let (mut store, _) = make_store_with_plugin();
        let mut repo = InstallRepo::new();
        let mut analytics = AnalyticsRepo::new();
        let resp = InstallHandlers::install(
            &mut store, &mut repo, &mut analytics,
            uid(), &Uuid::new_v4().to_string(), "1.0.0", 0,
        );
        assert_eq!(resp.status, StatusCode::NotFound);
    }

    #[test]
    fn inst014_handler_install_records_analytics() {
        let (mut store, plugin_id) = make_store_with_plugin();
        let mut repo = InstallRepo::new();
        let mut analytics = AnalyticsRepo::new();
        InstallHandlers::install(
            &mut store, &mut repo, &mut analytics,
            uid(), &plugin_id.to_string(), "1.0.0", 0,
        );
        use logos_marketplace_db::EventType;
        assert_eq!(analytics.count_by_type(&EventType::Install), 1);
    }

    #[test]
    fn inst015_handler_uninstall_success() {
        let (mut store, plugin_id) = make_store_with_plugin();
        let mut repo = InstallRepo::new();
        let mut analytics = AnalyticsRepo::new();
        let user_id = uid();
        InstallHandlers::install(
            &mut store, &mut repo, &mut analytics,
            user_id, &plugin_id.to_string(), "1.0.0", 0,
        );
        let resp = InstallHandlers::uninstall(&mut repo, user_id, &plugin_id.to_string());
        assert_eq!(resp.status, StatusCode::Ok);
        assert!(!repo.is_installed(&user_id, &plugin_id));
    }

    #[test]
    fn inst016_handler_uninstall_not_found() {
        let mut repo = InstallRepo::new();
        let resp = InstallHandlers::uninstall(&mut repo, uid(), &Uuid::new_v4().to_string());
        assert_eq!(resp.status, StatusCode::NotFound);
    }

    #[test]
    fn inst017_handler_list_for_user() {
        let (mut store, plugin_id) = make_store_with_plugin();
        let mut repo = InstallRepo::new();
        let mut analytics = AnalyticsRepo::new();
        let user_id = uid();
        InstallHandlers::install(
            &mut store, &mut repo, &mut analytics,
            user_id, &plugin_id.to_string(), "1.0.0", 0,
        );
        let resp = InstallHandlers::list_for_user(&repo, user_id);
        assert_eq!(resp.status, StatusCode::Ok);
        let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        assert_eq!(data["installs"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn inst018_handler_check_installed() {
        let (mut store, plugin_id) = make_store_with_plugin();
        let mut repo = InstallRepo::new();
        let mut analytics = AnalyticsRepo::new();
        let user_id = uid();
        InstallHandlers::install(
            &mut store, &mut repo, &mut analytics,
            user_id, &plugin_id.to_string(), "1.0.0", 0,
        );
        let resp = InstallHandlers::check(&repo, user_id, &plugin_id.to_string());
        let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        assert_eq!(data["installed"], true);
        assert_eq!(data["version"], "1.0.0");
    }

    #[test]
    fn inst019_handler_check_not_installed() {
        let repo = InstallRepo::new();
        let resp = InstallHandlers::check(&repo, uid(), &Uuid::new_v4().to_string());
        let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        assert_eq!(data["installed"], false);
    }

    #[test]
    fn inst020_install_increments_downloads() {
        let (mut store, plugin_id) = make_store_with_plugin();
        let mut repo = InstallRepo::new();
        let mut analytics = AnalyticsRepo::new();
        InstallHandlers::install(
            &mut store, &mut repo, &mut analytics,
            uid(), &plugin_id.to_string(), "1.0.0", 0,
        );
        let downloads = store.plugins_ref().get(&plugin_id).unwrap().downloads;
        assert_eq!(downloads, 1);
    }
}
