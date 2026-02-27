//! Phase 9 integration tests — Version History UI data layer.
//!
//! Tests cross-module interactions: timeline + bookmarks, activity + changeset,
//! restore + branches, and end-to-end workflows.

use logos_history::{
    ActivityFeed, Bookmark, BookmarkStore, Branch, BranchStore, BranchStatus,
    Changeset, ChangeCategory, HistoryError, InMemoryBookmarkStore, InMemoryBranchStore,
    RestoreEngine, RestoreRequest, RestoreStrategy, SessionGrouper, Timeline, TimelineFilter,
};
use logos_identity::UserId;
use logos_replay::{DiffEntry, DiffKind, FieldChange, HistoryEntry, VersionDiff};
use serde_json::json;
use uuid::Uuid;

// ── Helpers ──────────────────────────────────────────────────────────

fn make_entry(version: u64, user: &str, domain: &str, ts: u64, desc: &str) -> HistoryEntry {
    HistoryEntry {
        version,
        user_id: user.to_string(),
        timestamp: ts,
        domain: domain.to_string(),
        description: Some(desc.to_string()),
        acknowledged: true,
    }
}

fn design_history() -> Vec<HistoryEntry> {
    vec![
        make_entry(1, "alice", "design", 1000, "Create canvas"),
        make_entry(2, "alice", "design", 1060, "Add background layer"),
        make_entry(3, "alice", "design", 1120, "Set fill color"),
        make_entry(4, "bob", "comment", 1300, "Leave feedback"),
        make_entry(5, "alice", "design", 1400, "Add rectangle"),
        make_entry(6, "alice", "design", 1460, "Add circle"),
        make_entry(7, "charlie", "design", 1600, "Add text layer"),
        make_entry(8, "alice", "design", 1700, "Resize canvas"),
        make_entry(9, "bob", "design", 1800, "Move element"),
        make_entry(10, "alice", "design", 1900, "Final touches"),
    ]
}

// ── Timeline + Bookmark integration ─────────────────────────────────

#[test]
fn timeline_with_bookmarks() {
    let doc_id = Uuid::new_v4();
    let entries = design_history();

    // Create timeline.
    let tl = Timeline::new(entries, doc_id);
    assert_eq!(tl.len(), 10);

    // Create bookmarks at key versions.
    let mut bookmarks = InMemoryBookmarkStore::new();
    bookmarks
        .save(Bookmark::new("Draft", 3, doc_id, UserId::new()))
        .unwrap();
    bookmarks
        .save(Bookmark::new("Final", 10, doc_id, UserId::new()).pin())
        .unwrap();

    // Verify bookmark presence.
    assert!(bookmarks.get_at_version(&doc_id, 3).is_some());
    assert!(bookmarks.get_at_version(&doc_id, 10).is_some());
    assert!(bookmarks.get_at_version(&doc_id, 5).is_none());

    // Pinned bookmarks.
    let pinned = bookmarks.pinned(&doc_id);
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].name, "Final");

    // Timeline pagination respects version order.
    let page = tl.page(0, 5, &TimelineFilter::new(), 2000);
    assert_eq!(page.entries.len(), 5);
    assert!(page.has_next);
}

// ── Activity + Changeset integration ────────────────────────────────

#[test]
fn activity_sessions_produce_changesets() {
    let entries = design_history();
    let grouper = SessionGrouper::new();
    let feed = ActivityFeed::from_entries(&entries, &grouper);

    // Should have multiple sessions.
    assert!(feed.len() >= 3);
    assert_eq!(feed.total_ops(), 10);

    // Create a diff for one of the sessions (simulating what replay would produce).
    let diff = VersionDiff {
        from_version: 1,
        to_version: 3,
        entries: vec![
            DiffEntry {
                path: "data.layers[0]".to_string(),
                kind: DiffKind::Added,
                change: FieldChange {
                    old: None,
                    new: Some(json!({"name": "Background"})),
                },
            },
            DiffEntry {
                path: "data.fill_color".to_string(),
                kind: DiffKind::Changed,
                change: FieldChange {
                    old: Some(json!("white")),
                    new: Some(json!("blue")),
                },
            },
        ],
    };

    let changeset = Changeset::from_diff(&diff);
    assert_eq!(changeset.len(), 2);
    assert_eq!(changeset.from_version, 1);
    assert_eq!(changeset.to_version, 3);

    // Structural + Style categories.
    let cats = changeset.categories();
    assert_eq!(cats.len(), 2);
    assert!(cats.contains(&ChangeCategory::Structural));
    assert!(cats.contains(&ChangeCategory::Style));
}

// ── Restore + Branch integration ────────────────────────────────────

#[test]
fn restore_then_branch() {
    let doc = Uuid::new_v4();

    // Set up engine with document at version 10.
    let mut engine = RestoreEngine::new();
    engine.register_document(doc, 10);

    // Fork to version 5.
    let fork_req = RestoreRequest::new(doc, 5, RestoreStrategy::Fork, "alice")
        .with_reason("Try alternative layout");
    let result = engine.execute(&fork_req).unwrap();
    assert!(result.branch_id.is_some());

    // Create branch from the fork.
    let mut branches = InMemoryBranchStore::new();
    let mut branch = Branch::new("alt-layout", doc, 5, UserId::new());
    branch.advance().unwrap(); // v6
    branch.advance().unwrap(); // v7
    let branch_id = branches.save(branch).unwrap();

    let b = branches.get(&branch_id).unwrap();
    assert_eq!(b.ops_since_fork(), 2);
    assert!(b.is_active());
}

// ── End-to-end workflow ─────────────────────────────────────────────

#[test]
fn full_version_history_workflow() {
    let doc = Uuid::new_v4();
    let entries = design_history();

    // 1. Build timeline.
    let tl = Timeline::new(entries.clone(), doc);
    let all = tl.all_entries(2000);
    assert_eq!(all.len(), 10);

    // 2. Group into activity sessions.
    let grouper = SessionGrouper::new().with_max_gap(200);
    let feed = ActivityFeed::from_entries(&entries, &grouper);
    assert!(feed.len() >= 2);

    // 3. Create bookmarks.
    let mut bookmarks = InMemoryBookmarkStore::new();
    bookmarks
        .save(Bookmark::new("Initial", 1, doc, UserId::new()))
        .unwrap();
    bookmarks
        .save(Bookmark::new("Mid-progress", 5, doc, UserId::new()))
        .unwrap();
    bookmarks
        .save(Bookmark::new("Complete", 10, doc, UserId::new()).pin())
        .unwrap();
    assert_eq!(bookmarks.count(&doc), 3);

    // 4. Search timeline.
    let results = tl.search("feedback", 2000);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].version, 4);

    // 5. Filter by user.
    let filter = TimelineFilter::new().with_user("alice");
    let page = tl.page(0, 20, &filter, 2000);
    assert_eq!(page.total_entries, 7); // alice has 7

    // 6. Generate changeset.
    let diff = VersionDiff {
        from_version: 5,
        to_version: 10,
        entries: vec![
            DiffEntry {
                path: "data.layers[2].text_content".to_string(),
                kind: DiffKind::Added,
                change: FieldChange {
                    old: None,
                    new: Some(json!("Hello World")),
                },
            },
            DiffEntry {
                path: "data.size.width".to_string(),
                kind: DiffKind::Changed,
                change: FieldChange {
                    old: Some(json!(800)),
                    new: Some(json!(1200)),
                },
            },
        ],
    };
    let cs = Changeset::from_diff(&diff);
    assert_eq!(cs.len(), 2);

    // 7. Restore to bookmark.
    let mut engine = RestoreEngine::new();
    engine.register_document(doc, 10);
    let restore_req = RestoreRequest::new(doc, 5, RestoreStrategy::Overwrite, "alice");
    let restore_result = engine.execute(&restore_req).unwrap();
    assert_eq!(restore_result.new_version, 11);

    // 8. Create an exploratory branch.
    let mut branch_store = InMemoryBranchStore::new();
    let b_id = branch_store
        .save(Branch::new("experiment", doc, 10, UserId::new()))
        .unwrap();
    assert_eq!(branch_store.active(&doc).len(), 1);

    // Advance branch.
    branch_store.get_mut(&b_id).unwrap().advance().unwrap();
    let b = branch_store.get(&b_id).unwrap();
    assert_eq!(b.current_version, 11);
}

// ── Error handling integration ──────────────────────────────────────

#[test]
fn error_propagation_across_modules() {
    // Bookmark duplicate.
    let mut bm = InMemoryBookmarkStore::new();
    let doc = Uuid::new_v4();
    bm.save(Bookmark::new("v1", 1, doc, UserId::new())).unwrap();
    let err = bm.save(Bookmark::new("v1", 2, doc, UserId::new()));
    assert!(matches!(err, Err(HistoryError::DuplicateBookmarkName { .. })));

    // Branch duplicate.
    let mut bs = InMemoryBranchStore::new();
    bs.save(Branch::new("main", doc, 0, UserId::new())).unwrap();
    let err = bs.save(Branch::new("main", doc, 5, UserId::new()));
    assert!(matches!(err, Err(HistoryError::DuplicateBranchName { .. })));

    // Restore validation.
    let engine = RestoreEngine::new();
    let req = RestoreRequest::new(doc, 5, RestoreStrategy::Overwrite, "alice");
    let err = engine.validate(&req);
    assert!(matches!(err, Err(HistoryError::RestoreFailed { .. })));
}

// ── Filtering + grouping combined ───────────────────────────────────

#[test]
fn filter_then_group() {
    let entries = design_history();
    let doc = Uuid::new_v4();

    // Filter to alice only.
    let tl = Timeline::new(entries.clone(), doc);
    let filter = TimelineFilter::new().with_user("alice");
    let page = tl.page(0, 100, &filter, 2000);
    let alice_count = page.total_entries;
    assert_eq!(alice_count, 7);

    // Group alice's entries into sessions.
    let alice_entries: Vec<HistoryEntry> = entries
        .into_iter()
        .filter(|e| e.user_id == "alice")
        .collect();
    let grouper = SessionGrouper::new();
    let feed = ActivityFeed::from_entries(&alice_entries, &grouper);
    assert!(feed.len() >= 1);
    assert_eq!(feed.total_ops(), 7);
}

// ── Bookmark lifecycle ──────────────────────────────────────────────

#[test]
fn bookmark_crud_lifecycle() {
    let mut store = InMemoryBookmarkStore::new();
    let doc = Uuid::new_v4();

    // Create.
    let id = store
        .save(
            Bookmark::new("Draft", 5, doc, UserId::new())
                .with_description("First draft")
                .with_color("#3498db"),
        )
        .unwrap();

    // Read.
    let bm = store.get(&id).unwrap();
    assert_eq!(bm.name, "Draft");
    assert_eq!(bm.description.as_deref(), Some("First draft"));

    // Update.
    store
        .update(&id, Some("Final Draft".into()), Some("Approved".into()))
        .unwrap();
    let bm = store.get(&id).unwrap();
    assert_eq!(bm.name, "Final Draft");
    assert_eq!(bm.description.as_deref(), Some("Approved"));

    // Delete.
    store.delete(&id).unwrap();
    assert_eq!(store.count(&doc), 0);
}

// ── Branch lifecycle ────────────────────────────────────────────────

#[test]
fn branch_lifecycle() {
    let mut store = InMemoryBranchStore::new();
    let doc = Uuid::new_v4();

    // Create.
    let id = store
        .save(
            Branch::new("experiment", doc, 10, UserId::new())
                .with_description("Try dark theme"),
        )
        .unwrap();

    // Work on branch.
    for _ in 0..5 {
        store.get_mut(&id).unwrap().advance().unwrap();
    }
    assert_eq!(store.get(&id).unwrap().current_version, 15);
    assert_eq!(store.get(&id).unwrap().ops_since_fork(), 5);

    // Merge.
    store.get_mut(&id).unwrap().merge().unwrap();
    assert_eq!(store.get(&id).unwrap().status, BranchStatus::Merged);
    assert!(store.active(&doc).is_empty());

    // Archive.
    store.get_mut(&id).unwrap().archive().unwrap();
    assert_eq!(store.get(&id).unwrap().status, BranchStatus::Archived);
}
