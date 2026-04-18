//! Marketplace integration tests — end-to-end flows across all subsystems.

use logos_marketplace_api::{
    InstallHandlers, InstallRepo,
    ModerationHandlers, PluginHandlers, PublisherHandlers, ReviewHandlers, TemplateHandlers,
};
use logos_marketplace_api::request::{
    CreateTemplateRequest, ModerationActionRequest, RegisterPublisherRequest,
    SubmitPluginRequest, SubmitReviewRequest,
};
use logos_marketplace_db::{
    AnalyticsRepo, BadgeLevel, CertificationRepo, SandboxResult,
    SubmissionStatus, VersionEntry, VersionRegistry,
    versioning::SemVer,
    MarketplaceStore,
};
use uuid::Uuid;

// ── helpers ───────────────────────────────────────────────────────────────────

fn register_publisher(store: &mut MarketplaceStore, name: &str, key_hex: &str) -> Uuid {
    let req = RegisterPublisherRequest {
        name: name.to_string(),
        public_key_hex: key_hex.to_string(),
        website: None,
        email: None,
    };
    let resp = PublisherHandlers::register(store, &req);
    assert!(resp.is_success(), "register failed: {}", resp.body);
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    Uuid::parse_str(data["id"].as_str().unwrap()).unwrap()
}

fn submit_plugin(store: &mut MarketplaceStore, key_hex: &str, name: &str) -> Uuid {
    let req = SubmitPluginRequest {
        name: name.to_string(),
        description: format!("{name} description"),
        category: "utility".to_string(),
        version: "1.0.0".to_string(),
        content_hash: "aabbccdd".to_string(),
        package_size: 1024,
        tags: vec![],
        min_logos_version: None,
    };
    let resp = PluginHandlers::submit(store, &req, key_hex);
    assert!(resp.is_success(), "submit failed: {}", resp.body);
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    Uuid::parse_str(data["id"].as_str().unwrap()).unwrap()
}

fn approve_first_pending(store: &mut MarketplaceStore) {
    let resp = ModerationHandlers::list_pending(store);
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    let item_id = data["queue"][0]["id"].as_str().unwrap().to_string();
    let req = ModerationActionRequest { item_id, notes: "approved in test".to_string() };
    ModerationHandlers::approve(store, &req);
}

// ── §1  Publisher registration (int001–int005) ────────────────────────────────

#[test]
fn int001_register_publisher() {
    let mut store = MarketplaceStore::new();
    let req = RegisterPublisherRequest {
        name: "Acme Corp".to_string(),
        public_key_hex: "key001".to_string(),
        website: None, email: None,
    };
    let resp = PublisherHandlers::register(&mut store, &req);
    assert!(resp.is_success());
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    assert!(data["id"].as_str().is_some());
}

#[test]
fn int002_register_duplicate_publisher_fails() {
    let mut store = MarketplaceStore::new();
    let req = RegisterPublisherRequest {
        name: "Acme".to_string(), public_key_hex: "key002a".to_string(),
        website: None, email: None,
    };
    PublisherHandlers::register(&mut store, &req);
    let req2 = RegisterPublisherRequest {
        name: "Acme".to_string(), public_key_hex: "key002b".to_string(),
        website: None, email: None,
    };
    let resp2 = PublisherHandlers::register(&mut store, &req2);
    assert!(!resp2.is_success());
}

#[test]
fn int003_get_publisher_by_id() {
    let mut store = MarketplaceStore::new();
    let pub_id = register_publisher(&mut store, "Widgets Inc", "key003");
    let resp = PublisherHandlers::get_by_id(&store, &pub_id.to_string());
    assert!(resp.is_success());
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    assert_eq!(data["name"].as_str().unwrap(), "Widgets Inc");
}

#[test]
fn int004_get_nonexistent_publisher() {
    let store = MarketplaceStore::new();
    let resp = PublisherHandlers::get_by_id(&store, &Uuid::new_v4().to_string());
    assert!(!resp.is_success());
}

#[test]
fn int005_register_multiple_publishers() {
    let mut store = MarketplaceStore::new();
    for i in 0..5_u32 {
        let req = RegisterPublisherRequest {
            name: format!("Publisher {i}"),
            public_key_hex: format!("uniquekey{i:04}"),
            website: None, email: None,
        };
        let resp = PublisherHandlers::register(&mut store, &req);
        assert!(resp.is_success());
    }
}

// ── §2  Plugin submission → moderation (int006–int012) ───────────────────────

#[test]
fn int006_submit_plugin() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Dev A", "devkey006");
    let resp = PluginHandlers::submit(&mut store, &SubmitPluginRequest {
        name: "AwesomePlugin".to_string(),
        description: "An awesome plugin".to_string(),
        category: "productivity".to_string(),
        version: "1.0.0".to_string(),
        content_hash: "cafebabe".to_string(),
        package_size: 512,
        tags: vec![],
        min_logos_version: None,
    }, "devkey006");
    assert!(resp.is_success());
}

#[test]
fn int007_submitted_plugin_status_is_pending() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Dev B", "devkey007");
    let plugin_id = submit_plugin(&mut store, "devkey007", "PendingPlugin");
    let plugin = store.plugins_ref().get(&plugin_id).unwrap();
    assert_eq!(plugin.status, SubmissionStatus::Pending);
}

#[test]
fn int008_moderate_approve_plugin() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Dev C", "devkey008");
    let plugin_id = submit_plugin(&mut store, "devkey008", "GoodPlugin");

    let pending = ModerationHandlers::list_pending(&store);
    let pending_data: serde_json::Value = serde_json::from_str(&pending.body).unwrap();
    let item_id = pending_data["queue"][0]["id"].as_str().unwrap().to_string();

    let resp = ModerationHandlers::approve(&mut store, &ModerationActionRequest {
        item_id, notes: "looks good".to_string(),
    });
    assert!(resp.is_success());

    let plugin = store.plugins_ref().get(&plugin_id).unwrap();
    assert_eq!(plugin.status, SubmissionStatus::Approved);
}

#[test]
fn int009_moderate_reject_plugin() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Dev D", "devkey009");
    let plugin_id = submit_plugin(&mut store, "devkey009", "BadPlugin");

    let pending = ModerationHandlers::list_pending(&store);
    let data: serde_json::Value = serde_json::from_str(&pending.body).unwrap();
    let item_id = data["queue"][0]["id"].as_str().unwrap().to_string();

    let resp = ModerationHandlers::reject(&mut store, &ModerationActionRequest {
        item_id, notes: "violates policy".to_string(),
    });
    assert!(resp.is_success());

    let plugin = store.plugins_ref().get(&plugin_id).unwrap();
    assert_eq!(plugin.status, SubmissionStatus::Rejected);
}

#[test]
fn int010_moderation_queue_shows_pending() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Dev E", "devkey010");
    submit_plugin(&mut store, "devkey010", "Plugin1");
    submit_plugin(&mut store, "devkey010", "Plugin2");

    let resp = ModerationHandlers::list_pending(&store);
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    assert_eq!(data["queue"].as_array().unwrap().len(), 2);
}

#[test]
fn int011_approved_plugin_disappears_from_pending() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Dev F", "devkey011");
    submit_plugin(&mut store, "devkey011", "ApprovablePlugin");
    approve_first_pending(&mut store);

    let resp = ModerationHandlers::list_pending(&store);
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    assert_eq!(data["stats"]["pending"].as_u64().unwrap_or(0), 0);
}

#[test]
fn int012_submit_then_get_by_id() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Dev G", "devkey012");
    let plugin_id = submit_plugin(&mut store, "devkey012", "FindablePlugin");

    let resp = PluginHandlers::get_by_id(&store, &plugin_id.to_string());
    assert!(resp.is_success());
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    assert_eq!(data["name"].as_str().unwrap(), "FindablePlugin");
}

// ── §3  Discovery (int013–int017) ─────────────────────────────────────────────

#[test]
fn int013_search_by_name() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Search Publisher", "searchkey013");
    submit_plugin(&mut store, "searchkey013", "UniqueXYZ42Plugin");
    approve_first_pending(&mut store);

    let resp = PluginHandlers::search(&store, "UniqueXYZ42");
    assert!(resp.is_success());
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    assert!(!data["results"].as_array().unwrap().is_empty());
}

#[test]
fn int014_search_no_results() {
    let store = MarketplaceStore::new();
    let resp = PluginHandlers::search(&store, "zzznomatch99999xyzabc");
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    assert_eq!(data["total"].as_u64().unwrap_or(0), 0);
}

#[test]
fn int015_featured_plugins_after_approve_and_verify() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Featured Publisher", "featkey015");
    let plugin_id = submit_plugin(&mut store, "featkey015", "FeaturedPlugin");
    approve_first_pending(&mut store);
    store.plugins().set_verified(&plugin_id, true).unwrap();

    let resp = PluginHandlers::featured(&store);
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    assert!(!data["featured"].as_array().unwrap().is_empty());
}

#[test]
fn int016_template_add_and_search() {
    let mut store = MarketplaceStore::new();
    let resp = TemplateHandlers::add(&mut store, &CreateTemplateRequest {
        name: "Business Card".to_string(),
        description: "A professional business card template".to_string(),
        category: "business".to_string(),
        tags: vec![],
    });
    assert!(resp.is_success());

    let search = TemplateHandlers::search(&store, "business card");
    let data: serde_json::Value = serde_json::from_str(&search.body).unwrap();
    assert!(!data["results"].as_array().unwrap().is_empty());
}

#[test]
fn int017_template_featured() {
    let mut store = MarketplaceStore::new();
    TemplateHandlers::add(&mut store, &CreateTemplateRequest {
        name: "Hero Section".to_string(),
        description: "Landing page hero".to_string(),
        category: "marketing".to_string(),
        tags: vec![],
    });
    let resp = TemplateHandlers::featured(&store);
    assert!(resp.is_success());
}

// ── §4  Review flow (int018–int021) ──────────────────────────────────────────

#[test]
fn int018_submit_review() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Review Publisher", "revkey018");
    let plugin_id = submit_plugin(&mut store, "revkey018", "ReviewedPlugin");

    let resp = ReviewHandlers::submit(&mut store, &SubmitReviewRequest {
        plugin_id: plugin_id.to_string(),
        stars: 5,
        body: "Excellent plugin!".to_string(),
        title: Some("Five stars".to_string()),
    });
    assert!(resp.is_success());
}

#[test]
fn int019_list_reviews_for_plugin() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Listed Review Publisher", "revkey019");
    let plugin_id = submit_plugin(&mut store, "revkey019", "MultiReviewPlugin");

    for stars in [5u8, 4, 3] {
        ReviewHandlers::submit(&mut store, &SubmitReviewRequest {
            plugin_id: plugin_id.to_string(),
            stars,
            body: "Great!".to_string(),
            title: None,
        });
    }

    let resp = ReviewHandlers::list_for_plugin(&store, &plugin_id.to_string());
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    assert_eq!(data["reviews"].as_array().unwrap().len(), 3);
}

#[test]
fn int020_rating_updates_after_review() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Rating Publisher", "ratingkey020");
    let plugin_id = submit_plugin(&mut store, "ratingkey020", "RatedPlugin");

    ReviewHandlers::submit(&mut store, &SubmitReviewRequest {
        plugin_id: plugin_id.to_string(), stars: 5,
        body: "5 star!".to_string(), title: None,
    });
    ReviewHandlers::submit(&mut store, &SubmitReviewRequest {
        plugin_id: plugin_id.to_string(), stars: 3,
        body: "3 star.".to_string(), title: None,
    });

    let plugin = store.plugins_ref().get(&plugin_id).unwrap();
    assert!(plugin.rating > 0.0);
}

#[test]
fn int021_reviews_empty_for_new_plugin() {
    let mut store = MarketplaceStore::new();
    register_publisher(&mut store, "Empty Review Publisher", "emptyrevkey021");
    let plugin_id = submit_plugin(&mut store, "emptyrevkey021", "NoReviewPlugin");

    let resp = ReviewHandlers::list_for_plugin(&store, &plugin_id.to_string());
    let data: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    assert_eq!(data["reviews"].as_array().unwrap().len(), 0);
}

// ── §5  Certification (int022–int025) ─────────────────────────────────────────

#[test]
fn int022_certify_and_badge_expert() {
    let mut cert_repo = CertificationRepo::new();
    let plugin_id = Uuid::new_v4();
    let score = cert_repo.certify(plugin_id, SandboxResult::perfect(100), 0);
    assert_eq!(score.badge, BadgeLevel::Expert);
    assert!(score.is_passing());
}

#[test]
fn int023_failing_certification() {
    let mut cert_repo = CertificationRepo::new();
    let plugin_id = Uuid::new_v4();
    let result = SandboxResult {
        passed_tests: 0, total_tests: 100,
        no_forbidden_apis: false, memory_safe: false, deterministic: false,
        notes: "many failures".to_string(),
    };
    let score = cert_repo.certify(plugin_id, result, 0);
    assert!(!score.is_passing());
}

#[test]
fn int024_certification_filtered_by_badge() {
    let mut cert_repo = CertificationRepo::new();
    let expert = Uuid::new_v4();
    let junior = Uuid::new_v4();
    cert_repo.certify(expert, SandboxResult::perfect(50), 0);
    cert_repo.certify(junior, SandboxResult {
        passed_tests: 30, total_tests: 100,
        no_forbidden_apis: false, memory_safe: false, deterministic: false,
        notes: String::new(),
    }, 0);
    let seniors = cert_repo.list_at_least(BadgeLevel::Senior);
    assert_eq!(seniors.len(), 1);
    assert_eq!(seniors[0].plugin_id, expert);
}

#[test]
fn int025_revoke_then_badge_is_none() {
    let mut cert_repo = CertificationRepo::new();
    let id = Uuid::new_v4();
    cert_repo.certify(id, SandboxResult::perfect(10), 0);
    cert_repo.revoke(&id).unwrap();
    assert_eq!(cert_repo.badge_for(&id), BadgeLevel::None);
}

// ── §6  Versioning + install flow (int026–int030) ────────────────────────────

#[test]
fn int026_publish_and_latest_version() {
    let mut reg = VersionRegistry::new();
    let plugin_id = Uuid::new_v4();
    reg.publish(VersionEntry::new(plugin_id, SemVer::new(1, 0, 0), "h1", 1000)).unwrap();
    reg.publish(VersionEntry::new(plugin_id, SemVer::new(1, 1, 0), "h2", 2000)).unwrap();
    assert_eq!(reg.latest(&plugin_id).unwrap().version, SemVer::new(1, 1, 0));
}

#[test]
fn int027_rollback_then_latest_is_old_version() {
    let mut reg = VersionRegistry::new();
    let plugin_id = Uuid::new_v4();
    reg.publish(VersionEntry::new(plugin_id, SemVer::new(1, 0, 0), "h1", 1000)).unwrap();
    reg.publish(VersionEntry::new(plugin_id, SemVer::new(1, 1, 0), "h2", 2000)).unwrap();
    reg.rollback(&plugin_id, SemVer::new(1, 0, 0)).unwrap();
    assert_eq!(reg.latest(&plugin_id).unwrap().version, SemVer::new(1, 0, 0));
}

#[test]
fn int028_install_handler_full_flow() {
    use logos_marketplace_db::{PluginRecord, PublisherRecord, PublisherStatus};
    let mut store = MarketplaceStore::new();
    let pub_id = Uuid::new_v4();
    store.publishers().insert(PublisherRecord {
        id: pub_id, name: "InstallPub".to_string(), public_key_hex: "installkey028".to_string(),
        status: PublisherStatus::Active, registered_at: 0, plugin_count: 0, total_downloads: 0,
    }).unwrap();
    let plugin_id = Uuid::new_v4();
    store.plugins().insert(PluginRecord {
        id: plugin_id, name: "InstallMe".to_string(), publisher_id: pub_id,
        description: "installable".to_string(), current_version: "1.0.0".to_string(),
        category: "utility".to_string(), tags: vec![], downloads: 0, rating: 0.0,
        rating_count: 0, status: SubmissionStatus::Approved, created_at: 0, updated_at: 0,
        content_hash: "abc".to_string(), package_size: 0, verified: true,
    }).unwrap();

    let mut install_repo = InstallRepo::new();
    let mut analytics = AnalyticsRepo::new();
    let user_id = Uuid::new_v4();

    let resp = InstallHandlers::install(
        &mut store, &mut install_repo, &mut analytics,
        user_id, &plugin_id.to_string(), "1.0.0", 1000,
    );
    assert!(resp.is_success());
    assert!(install_repo.is_installed(&user_id, &plugin_id));
    assert_eq!(store.plugins_ref().get(&plugin_id).unwrap().downloads, 1);
}

#[test]
fn int029_install_then_uninstall_flow() {
    use logos_marketplace_db::{PluginRecord, PublisherRecord, PublisherStatus};
    let mut store = MarketplaceStore::new();
    let pub_id = Uuid::new_v4();
    store.publishers().insert(PublisherRecord {
        id: pub_id, name: "P29".to_string(), public_key_hex: "key029".to_string(),
        status: PublisherStatus::Active, registered_at: 0, plugin_count: 0, total_downloads: 0,
    }).unwrap();
    let plugin_id = Uuid::new_v4();
    store.plugins().insert(PluginRecord {
        id: plugin_id, name: "Removable".to_string(), publisher_id: pub_id,
        description: "d".to_string(), current_version: "1.0.0".to_string(),
        category: "c".to_string(), tags: vec![], downloads: 0, rating: 0.0,
        rating_count: 0, status: SubmissionStatus::Approved, created_at: 0, updated_at: 0,
        content_hash: "h".to_string(), package_size: 0, verified: true,
    }).unwrap();

    let mut install_repo = InstallRepo::new();
    let mut analytics = AnalyticsRepo::new();
    let user_id = Uuid::new_v4();

    InstallHandlers::install(
        &mut store, &mut install_repo, &mut analytics,
        user_id, &plugin_id.to_string(), "1.0.0", 0,
    );
    let resp = InstallHandlers::uninstall(&mut install_repo, user_id, &plugin_id.to_string());
    assert!(resp.is_success());
    assert!(!install_repo.is_installed(&user_id, &plugin_id));
}

#[test]
fn int030_version_history_and_compatibility() {
    let mut reg = VersionRegistry::new();
    let id = Uuid::new_v4();
    reg.publish(VersionEntry::new(id, SemVer::new(1, 0, 0), "h1", 1000)).unwrap();
    reg.publish(VersionEntry::new(id, SemVer::new(1, 1, 0), "h2", 2000)).unwrap();
    reg.publish(VersionEntry::new(id, SemVer::new(2, 0, 0), "h3", 3000)).unwrap();

    let history = reg.history(&id);
    assert_eq!(history.len(), 3);
    assert!(SemVer::new(2, 0, 0).is_breaking_from(&SemVer::new(1, 5, 0)));
    assert!(SemVer::new(1, 1, 0).is_compatible_with(&SemVer::new(1, 0, 0)));
}
