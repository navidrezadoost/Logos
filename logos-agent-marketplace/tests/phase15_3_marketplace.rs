//! Phase 15.3 Integration Tests — Agent Marketplace
//!
//! End-to-end scenarios spanning the full marketplace lifecycle:
//! certify → publish → search → install → rate → review.

use logos_agent_marketplace::{
    certification::{CertificationLevel, CertificationRegistry, CertificationRequest},
    install::{InstallRegistry, InstallRequest},
    manifest::{AgentCategory, AgentManifest, AgentVersion, CompatibilityMatrix, PricingModel},
    ratings::{Rating, Review, ReviewStore},
    registry::{MarketplaceRegistry, PublisherProfile, SearchQuery, SortOrder},
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn v(a: u16, b: u16, c: u16) -> AgentVersion { AgentVersion::new(a, b, c) }

fn logos_v() -> AgentVersion { v(1, 5, 0) }

fn make_agent(id: &str, author_id: &str, category: AgentCategory, free: bool) -> AgentManifest {
    let pricing = if free { PricingModel::Free }
        else { PricingModel::OneTime { price_cents: 999, currency: "USD".into() } };
    AgentManifest::new(
        id,
        format!("{} Agent", id),
        "A comprehensive agent that does many helpful things for designers.",
        "Test Author",
        author_id,
        v(1, 0, 0),
        category,
        pricing,
        v(1, 0, 0),
        1000,
    )
    .with_tagline("Does helpful things")
    .with_tags(&["ai", "design"])
    .with_icon("https://cdn.example.com/icon.png")
    .with_docs("https://docs.example.com")
}

fn certify_and_publish(
    cert_reg: &mut CertificationRegistry,
    reg: &mut MarketplaceRegistry,
    agent: AgentManifest,
) -> bool {
    let author_id = agent.author_id.clone();
    let req = CertificationRequest::new(agent.clone(), author_id, logos_v(), 0)
        .with_bundle_hash("sha256:abcdef");
    let result = cert_reg.certify(req);
    if result.level.is_listable() {
        reg.publish(agent);
        true
    } else {
        false
    }
}

// ─── Test 1: full publish lifecycle — certify → list → search → install ───────

#[test]
fn full_publish_and_install_lifecycle() {
    let mut cert_reg = CertificationRegistry::new();
    let mut reg = MarketplaceRegistry::new();
    let mut install_reg = InstallRegistry::new();

    let wcag = make_agent("wcag-checker", "author-1", AgentCategory::Accessibility, true);
    install_reg.register_agent(wcag.clone());

    let published = certify_and_publish(&mut cert_reg, &mut reg, wcag);
    assert!(published, "WCAG checker should pass certification");

    // Search
    let q = SearchQuery::new().with_category(AgentCategory::Accessibility);
    let results = reg.search(&q);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].manifest.id, "wcag-checker");

    // Install
    let req = InstallRequest::new("wcag-checker", "user-alice", logos_v(), 500);
    let result = install_reg.install(&req);
    assert!(result.success, "Install should succeed: {:?}", result.error);
    assert!(install_reg.is_installed("user-alice", "wcag-checker"));
}

// ─── Test 2: certification gate blocks bad agents ─────────────────────────────

#[test]
fn certification_blocks_invalid_agents() {
    let mut cert_reg = CertificationRegistry::new();
    let mut reg = MarketplaceRegistry::new();

    // Bad ID (uppercase)
    let bad = AgentManifest::new(
        "BAD_ID", "Bad Agent",
        "A description that is long enough to meet the minimum length requirement for this check.",
        "Author", "author-x",
        v(1, 0, 0), AgentCategory::Productivity, PricingModel::Free, v(1, 0, 0), 0,
    ).with_tagline("tag").with_tags(&["x"]);

    let req = CertificationRequest::new(bad.clone(), "author-x", logos_v(), 0);
    let result = cert_reg.certify(req);
    assert_eq!(result.level, CertificationLevel::Failed);

    let published = if result.level.is_listable() { reg.publish(bad); true } else { false };
    assert!(!published, "Bad agent should not be published");
    assert_eq!(reg.len(), 0);
}

// ─── Test 3: ratings and reviews lifecycle ────────────────────────────────────

#[test]
fn ratings_and_reviews_lifecycle() {
    let mut reg = MarketplaceRegistry::new();
    let agent = make_agent("color-ai", "author-2", AgentCategory::ColorTheory, true);
    reg.publish(agent);

    let mut reviews = ReviewStore::new();

    // 5 users rate
    for (i, stars) in [4u8, 5, 5, 3, 4].iter().enumerate() {
        reviews.submit_rating(Rating::new("color-ai", format!("user-{}", i), *stars, i as u64));
    }

    // 2 users write reviews
    let r1 = Review::new("color-ai", "user-0", "Alice", 4, "Really helpful color suggestions!", 100)
        .with_title("Great tool")
        .approve();
    let r2_pending = Review::new("color-ai", "user-1", "Bob", 5, "Totally changed how I pick colors for my designs.", 200);
    let r2_id = r2_pending.review_id.clone();

    reviews.submit_review(r1);
    reviews.submit_review(r2_pending);

    // Only 1 approved so far
    assert_eq!(reviews.visible_reviews("color-ai").len(), 1);

    // Admin approves Bob's review
    assert!(reviews.approve_review("color-ai", &r2_id));
    assert_eq!(reviews.visible_reviews("color-ai").len(), 2);

    let summary = reviews.summary("color-ai").unwrap();
    assert!(summary.avg_rating >= 4.0 && summary.avg_rating <= 5.0);

    // Update registry rating
    reg.update_rating("color-ai", summary.avg_rating, summary.review_count);
    let listed = reg.get("color-ai").unwrap();
    assert!((listed.avg_rating - summary.avg_rating).abs() < 0.01);
}

// ─── Test 4: dependency resolution chain ──────────────────────────────────────

#[test]
fn install_with_deep_dependency_chain() {
    let mut install_reg = InstallRegistry::new();

    // Chain: core → extension → pro-tool
    let core = make_agent("core-lib", "auth", AgentCategory::Productivity, true);
    let mut ext = make_agent("extension", "auth", AgentCategory::Productivity, true);
    ext.compatibility = CompatibilityMatrix::new(v(1, 0, 0))
        .with_dependency("core-lib");
    let mut pro = make_agent("pro-tool", "auth", AgentCategory::Productivity, false);
    pro.compatibility = CompatibilityMatrix::new(v(1, 0, 0))
        .with_dependency("extension");

    install_reg.register_agent(core);
    install_reg.register_agent(ext);
    install_reg.register_agent(pro);

    let req = InstallRequest::new("pro-tool", "user-1", logos_v(), 0);
    let result = install_reg.install(&req);
    assert!(result.success, "pro-tool install should succeed: {:?}", result.error);
    assert_eq!(result.deps_installed.len(), 2, "Should install core-lib and extension");
    assert!(install_reg.is_installed("user-1", "core-lib"));
    assert!(install_reg.is_installed("user-1", "extension"));
    assert!(install_reg.is_installed("user-1", "pro-tool"));
}

// ─── Test 5: featured and trending sections ─────────────────────────────────

#[test]
fn featured_and_trending_agents() {
    let mut reg = MarketplaceRegistry::new();

    reg.publish_with_stats(make_agent("a", "auth", AgentCategory::Layout, true),  1000, 4.5, 50, 500, 1.0, true, true);
    reg.publish_with_stats(make_agent("b", "auth", AgentCategory::Export, true),  200,  3.8, 10,  50, 30.0, false, false);
    reg.publish_with_stats(make_agent("c", "auth", AgentCategory::Typography, false),  5000, 4.9, 200, 1000, 7.0, true, true);

    reg.set_featured(vec!["a".into(), "c".into()]);
    let featured = reg.featured();
    assert_eq!(featured.len(), 2);

    let q = SearchQuery::new().with_sort(SortOrder::Trending);
    let trending = reg.search(&q);
    // trending_score = recent_installs / max(days/30, 1.0)
    // "a": 500/max(0.033,1) = 500, "b": 50/max(1,1) = 50, "c": 1000/max(0.23,1) = 1000
    // So "c" is highest, then "a", then "b"
    assert_eq!(trending[0].manifest.id, "c");
    assert_eq!(reg.certified_count(), 2);
}

// ─── Test 6: official publisher gets Official badge end-to-end ────────────────

#[test]
fn official_publisher_full_pipeline() {
    let mut cert_reg = CertificationRegistry::new()
        .with_trusted_publisher("logos-official");
    let mut reg = MarketplaceRegistry::new();

    let official_agent = make_agent("logos-accessibility", "logos-official", AgentCategory::Accessibility, true);
    let req = CertificationRequest::new(
        official_agent.clone(), "logos-official", logos_v(), 500,
    ).with_bundle_hash("sha256:official");

    let result = cert_reg.certify(req);
    assert_eq!(result.level, CertificationLevel::Official);
    assert!(result.level.is_trusted());

    reg.publish(official_agent);
    assert_eq!(reg.len(), 1);
}

// ─── Test 7: search with multiple filters combined ───────────────────────────

#[test]
fn combined_search_filters() {
    let mut reg = MarketplaceRegistry::new();

    let mut m1 = make_agent("free-a11y-tool", "a", AgentCategory::Accessibility, true);
    m1.tags = vec!["wcag".into(), "screen-reader".into()];
    let mut m2 = make_agent("paid-a11y-tool", "b", AgentCategory::Accessibility, false);
    m2.tags = vec!["wcag".into()];
    let m3 = make_agent("free-layout-tool", "c", AgentCategory::Layout, true);

    reg.publish(m1); reg.publish(m2); reg.publish(m3);

    let q = SearchQuery::new()
        .with_category(AgentCategory::Accessibility)
        .free_only()
        .with_tags(&["wcag"]);
    let results = reg.search(&q);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].manifest.id, "free-a11y-tool");
}

// ─── Test 8: uninstall and reinstall ─────────────────────────────────────────

#[test]
fn uninstall_then_reinstall() {
    let mut reg = InstallRegistry::new();
    reg.register_agent(make_agent("toggleable", "a", AgentCategory::Productivity, true));

    let user = "user-toggle";
    reg.install(&InstallRequest::new("toggleable", user, logos_v(), 0));
    assert!(reg.is_installed(user, "toggleable"));

    reg.uninstall(user, "toggleable");
    assert!(!reg.is_installed(user, "toggleable"));

    // Reinstall
    reg.install(&InstallRequest::new("toggleable", user, logos_v(), 100));
    assert!(reg.is_installed(user, "toggleable"));
}

// ─── Test 9: publisher with multiple agents ────────────────────────────────────

#[test]
fn publisher_multi_agent_portfolio() {
    let mut reg = MarketplaceRegistry::new();
    let publisher = PublisherProfile::new("acme-inc", "Acme Inc.", 0);
    reg.register_publisher(publisher);

    for i in 0..5 {
        let cat = match i % 3 {
            0 => AgentCategory::Layout,
            1 => AgentCategory::Export,
            _ => AgentCategory::Typography,
        };
        reg.publish(make_agent(&format!("acme-tool-{}", i), "acme-inc", cat, i % 2 == 0));
    }

    assert_eq!(reg.len(), 5);
    assert_eq!(reg.free_count(), 3);
    let p = reg.get_publisher("acme-inc").unwrap();
    assert_eq!(p.display_name, "Acme Inc.");
}

// ─── Test 10: price sort order ────────────────────────────────────────────────

#[test]
fn price_sort_ascending_puts_free_first() {
    let mut reg = MarketplaceRegistry::new();

    let mut paid_500 = make_agent("premium", "a", AgentCategory::Productivity, false);
    paid_500.pricing = PricingModel::OneTime { price_cents: 500, currency: "USD".into() };
    let free = make_agent("freebie", "b", AgentCategory::Productivity, true);
    let mut paid_999 = make_agent("expensive", "c", AgentCategory::Productivity, false);
    paid_999.pricing = PricingModel::OneTime { price_cents: 999, currency: "USD".into() };

    reg.publish(paid_999); reg.publish(paid_500); reg.publish(free);

    let q = SearchQuery::new().with_sort(SortOrder::PriceAscending);
    let results = reg.search(&q);
    assert_eq!(results[0].manifest.id, "freebie");
    assert_eq!(results[1].manifest.id, "premium");
    assert_eq!(results[2].manifest.id, "expensive");
}
