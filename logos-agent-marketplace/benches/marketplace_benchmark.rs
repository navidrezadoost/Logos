use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logos_agent_marketplace::{
    certification::{CertificationRegistry, CertificationRequest, Certifier},
    install::{InstallRegistry, InstallRequest},
    manifest::{AgentCategory, AgentManifest, AgentVersion, PricingModel},
    ratings::{Rating, Review, ReviewStore},
    registry::{MarketplaceRegistry, SearchQuery, SortOrder},
};

fn v(a: u16, b: u16, c: u16) -> AgentVersion { AgentVersion::new(a, b, c) }

fn make_manifest(id: &str) -> AgentManifest {
    AgentManifest::new(
        id, format!("Agent {}", id), "A detailed agent description for the marketplace.",
        "Benchmark Author", "bench-author",
        v(1, 0, 0), AgentCategory::Productivity, PricingModel::Free, v(1, 0, 0), 0,
    )
    .with_tags(&["perf", "bench"])
    .with_tagline("Benchmark test agent")
    .with_icon("https://cdn.example.com/icon.png")
    .with_docs("https://docs.example.com")
}

fn bench_registry_publish_search(c: &mut Criterion) {
    let mut reg = MarketplaceRegistry::new();
    for i in 0..100 {
        reg.publish(make_manifest(&format!("agent-{}", i)));
    }
    c.bench_function("registry_search_text", |b| {
        let q = SearchQuery::new().with_text(black_box("agent-5"));
        b.iter(|| black_box(reg.search(&q).len()))
    });
}

fn bench_registry_sort_trending(c: &mut Criterion) {
    let mut reg = MarketplaceRegistry::new();
    for i in 0..200 {
        reg.publish_with_stats(
            make_manifest(&format!("agent-{}", i)),
            (i as u64) * 100,
            3.5 + (i % 5) as f32 * 0.3,
            i as u32 * 2,
            (i as u64) * 10,
            7.0,
            i % 3 == 0,
            i % 10 == 0,
        );
    }
    c.bench_function("registry_sort_trending_200", |b| {
        let q = SearchQuery::new().with_sort(SortOrder::Trending);
        b.iter(|| black_box(reg.search(&q).len()))
    });
}

fn bench_certification(c: &mut Criterion) {
    let certifier = Certifier::new();
    c.bench_function("certify_manifest", |b| {
        b.iter(|| {
            let m = make_manifest(black_box("bench-agent"));
            let req = CertificationRequest::new(m, "bench-author", v(1, 5, 0), 0)
                .with_bundle_hash("sha256:deadbeef");
            black_box(certifier.certify(&req).level.badge_label())
        })
    });
}

fn bench_install(c: &mut Criterion) {
    let mut reg = InstallRegistry::new();
    for i in 0..50 {
        reg.register_agent(make_manifest(&format!("tool-{}", i)));
    }
    c.bench_function("install_agent", |b| {
        let mut uid = 0u64;
        b.iter(|| {
            let req = InstallRequest::new(
                black_box("tool-0"),
                format!("user-{}", uid),
                v(1, 5, 0),
                uid,
            );
            uid += 1;
            black_box(reg.install(&req).success)
        })
    });
}

fn bench_review_store(c: &mut Criterion) {
    let mut store = ReviewStore::new();
    // Pre-populate 200 approved reviews
    for i in 0..200u32 {
        let rev = Review::new("agent-bench", format!("user-{}", i), "User", (i % 5 + 1) as u8, "Review text here.", i as u64)
            .approve();
        store.submit_review(rev);
    }
    c.bench_function("visible_reviews_200", |b| {
        b.iter(|| black_box(store.visible_reviews("agent-bench").len()))
    });
}

criterion_group!(
    benches,
    bench_registry_publish_search,
    bench_registry_sort_trending,
    bench_certification,
    bench_install,
    bench_review_store,
);
criterion_main!(benches);
