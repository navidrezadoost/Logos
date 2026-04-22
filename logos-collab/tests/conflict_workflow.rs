// logos-collab/tests/conflict_workflow.rs
//
//! Integration tests for the full offline conflict resolution workflow.

use logos_collab::{
    admin::AdminEngine,
    conflict::{ConflictStore, ElementVersion, ResolutionStrategy},
    offline_tracker::{EditType, LocalEdit, OfflineTracker},
    org::CompanyStore,
    project_scope::ProjectStore,
    sync_status::{SyncState, SyncStatusStore},
};
use uuid::Uuid;

fn sample_props(val: &str) -> serde_json::Value {
    serde_json::json!({"content": val})
}

// CW-01: Full workflow — offline edit → conflict detection → resolution.
#[tokio::test]
async fn cw_01_full_conflict_workflow() {
    let mut admin = AdminEngine::new();
    let admin_id = admin
        .initialize("admin", "admin@test.com", "pass", "Admin", "User")
        .unwrap();

    let mut orgs = CompanyStore::new();
    let company = logos_collab::org::Company::new("TestCo", admin_id);
    let company_id = orgs.create_company(company);

    let mut projects = ProjectStore::new();
    let project = logos_collab::project_scope::Project::new("TestProj", "desc", company_id, admin_id);
    let project_id = projects.create_project(project);

    let mut offline_tracker = OfflineTracker::new();
    let mut conflicts = ConflictStore::new();
    let mut sync_status = SyncStatusStore::new();

    // 1. User goes offline
    offline_tracker.set_offline(true);

    // 2. User makes a local edit
    let element_id = Uuid::new_v4();
    let local_edit = LocalEdit::new(
        element_id,
        project_id,
        EditType::Update,
        sample_props("local version"),
        1,
    );
    offline_tracker.track_edit(local_edit);
    sync_status.mark_pending(element_id, project_id);

    assert_eq!(offline_tracker.pending_count(), 1);
    assert_eq!(sync_status.get(element_id).unwrap().state, SyncState::Pending);

    // 3. User comes back online, sync attempt detects conflict
    offline_tracker.set_offline(false);

    // Simulate remote edit happened during offline period
    let local_version = ElementVersion::new(
        element_id,
        admin_id,
        "Admin".into(),
        "rectangle".into(),
        sample_props("local version"),
        None,
    );

    let remote_version = ElementVersion::new(
        element_id,
        Uuid::new_v4(),
        "RemoteUser".into(),
        "rectangle".into(),
        sample_props("remote version"),
        None,
    );

    // 4. Create conflict
    let conflict_id = conflicts
        .create_conflict(
            project_id,
            element_id,
            vec![local_version.clone(), remote_version.clone()],
            admin_id, // admin must review
        )
        .unwrap();

    sync_status.mark_conflicted(element_id, project_id, conflict_id);

    assert_eq!(
        sync_status.get(element_id).unwrap().state,
        SyncState::Conflicted
    );

    // 5. Admin reviews conflict
    conflicts.mark_under_review(conflict_id, admin_id).unwrap();

    // 6. Admin chooses resolution (accept local)
    conflicts
        .resolve_conflict(
            conflict_id,
            admin_id,
            ResolutionStrategy::AcceptLocal,
            vec![local_version.version_id],
        )
        .unwrap();

    // 7. Mark element as synced
    sync_status.mark_synced(element_id, project_id);
    offline_tracker.clear_element(element_id);

    assert_eq!(sync_status.get(element_id).unwrap().state, SyncState::Synced);
    assert_eq!(offline_tracker.pending_count(), 0);
}

// CW-02: Accept both versions creates side-by-side elements.
#[tokio::test]
async fn cw_02_accept_both_versions() {
    let mut conflicts = ConflictStore::new();
    let project_id = Uuid::new_v4();
    let element_id = Uuid::new_v4();
    let reviewer_id = Uuid::new_v4();

    let v1 = ElementVersion::new(
        element_id,
        Uuid::new_v4(),
        "User1".into(),
        "text".into(),
        sample_props("version 1"),
        None,
    );
    let v2 = ElementVersion::new(
        element_id,
        Uuid::new_v4(),
        "User2".into(),
        "text".into(),
        sample_props("version 2"),
        None,
    );

    let v1_id = v1.version_id;
    let v2_id = v2.version_id;

    let conflict_id = conflicts
        .create_conflict(project_id, element_id, vec![v1, v2], reviewer_id)
        .unwrap();

    conflicts
        .resolve_conflict(
            conflict_id,
            reviewer_id,
            ResolutionStrategy::AcceptBoth,
            vec![v1_id, v2_id],
        )
        .unwrap();

    let record = conflicts.get_conflict(conflict_id).unwrap();
    assert_eq!(record.accepted_versions.len(), 2);
    assert!(record.accepted_versions.contains(&v1_id));
    assert!(record.accepted_versions.contains(&v2_id));
}

// CW-03: Reject all versions marks element as rejected.
#[tokio::test]
async fn cw_03_reject_all_versions() {
    let mut conflicts = ConflictStore::new();
    let mut sync_status = SyncStatusStore::new();

    let project_id = Uuid::new_v4();
    let element_id = Uuid::new_v4();
    let reviewer_id = Uuid::new_v4();

    let v1 = ElementVersion::new(
        element_id,
        Uuid::new_v4(),
        "User1".into(),
        "shape".into(),
        sample_props("v1"),
        None,
    );
    let v2 = ElementVersion::new(
        element_id,
        Uuid::new_v4(),
        "User2".into(),
        "shape".into(),
        sample_props("v2"),
        None,
    );

    let conflict_id = conflicts
        .create_conflict(project_id, element_id, vec![v1, v2], reviewer_id)
        .unwrap();

    conflicts.reject_conflict(conflict_id, reviewer_id).unwrap();
    sync_status.mark_rejected(element_id, project_id, Some("Reviewer rejected all versions".into()));

    let status = sync_status.get(element_id).unwrap();
    assert_eq!(status.state, SyncState::Rejected);
    assert!(status.error_message.is_some());
}

// CW-04: Multiple conflicts in same project.
#[tokio::test]
async fn cw_04_multiple_conflicts_in_project() {
    let mut conflicts = ConflictStore::new();
    let project_id = Uuid::new_v4();
    let reviewer_id = Uuid::new_v4();

    let e1 = Uuid::new_v4();
    let e2 = Uuid::new_v4();
    let e3 = Uuid::new_v4();

    conflicts
        .create_conflict(
            project_id,
            e1,
            vec![
                ElementVersion::new(e1, Uuid::new_v4(), "A".into(), "rect".into(), sample_props("a"), None),
                ElementVersion::new(e1, Uuid::new_v4(), "B".into(), "rect".into(), sample_props("b"), None),
            ],
            reviewer_id,
        )
        .unwrap();

    conflicts
        .create_conflict(
            project_id,
            e2,
            vec![
                ElementVersion::new(e2, Uuid::new_v4(), "C".into(), "text".into(), sample_props("c"), None),
                ElementVersion::new(e2, Uuid::new_v4(), "D".into(), "text".into(), sample_props("d"), None),
            ],
            reviewer_id,
        )
        .unwrap();

    conflicts
        .create_conflict(
            project_id,
            e3,
            vec![
                ElementVersion::new(e3, Uuid::new_v4(), "E".into(), "ellipse".into(), sample_props("e"), None),
                ElementVersion::new(e3, Uuid::new_v4(), "F".into(), "ellipse".into(), sample_props("f"), None),
            ],
            reviewer_id,
        )
        .unwrap();

    let pending = conflicts.pending_conflicts_for_project(project_id);
    assert_eq!(pending.len(), 3);
}

// CW-05: Sync status transitions through states.
#[tokio::test]
async fn cw_05_sync_status_transitions() {
    let mut sync_status = SyncStatusStore::new();
    let element_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();

    // Initial: synced
    sync_status.mark_synced(element_id, project_id);
    assert_eq!(sync_status.get(element_id).unwrap().state, SyncState::Synced);

    // User edits: pending
    sync_status.mark_pending(element_id, project_id);
    assert_eq!(sync_status.get(element_id).unwrap().state, SyncState::Pending);

    // Syncing
    sync_status.mark_syncing(element_id, project_id);
    assert_eq!(sync_status.get(element_id).unwrap().state, SyncState::Syncing);

    // Conflict detected
    let conflict_id = Uuid::new_v4();
    sync_status.mark_conflicted(element_id, project_id, conflict_id);
    assert_eq!(sync_status.get(element_id).unwrap().state, SyncState::Conflicted);
    assert_eq!(sync_status.get(element_id).unwrap().conflict_id, Some(conflict_id));

    // Resolved
    sync_status.mark_synced(element_id, project_id);
    assert_eq!(sync_status.get(element_id).unwrap().state, SyncState::Synced);
    assert_eq!(sync_status.get(element_id).unwrap().conflict_id, None);
}

// CW-06: Offline tracker bulk clear.
#[tokio::test]
async fn cw_06_offline_tracker_bulk_clear() {
    let mut offline_tracker = OfflineTracker::new();
    let project_id = Uuid::new_v4();

    for i in 0..10 {
        offline_tracker.track_edit(LocalEdit::new(
            Uuid::new_v4(),
            project_id,
            EditType::Update,
            sample_props(&format!("edit{}", i)),
            i,
        ));
    }

    assert_eq!(offline_tracker.pending_count(), 10);

    offline_tracker.clear_project(project_id);
    assert_eq!(offline_tracker.pending_count(), 0);
}

// CW-07: Clear rejected items from sync status.
#[tokio::test]
async fn cw_07_clear_rejected_items() {
    let mut sync_status = SyncStatusStore::new();
    let project_id = Uuid::new_v4();

    let e1 = Uuid::new_v4();
    let e2 = Uuid::new_v4();
    let e3 = Uuid::new_v4();

    sync_status.mark_rejected(e1, project_id, None);
    sync_status.mark_rejected(e2, project_id, None);
    sync_status.mark_pending(e3, project_id);

    let count = sync_status.clear_rejected(project_id);
    assert_eq!(count, 2);
    assert!(sync_status.get(e1).is_none());
    assert!(sync_status.get(e2).is_none());
    assert!(sync_status.get(e3).is_some());
}

// CW-08: Pending edits by project.
#[tokio::test]
async fn cw_08_pending_edits_by_project() {
    let mut offline_tracker = OfflineTracker::new();
    let p1 = Uuid::new_v4();
    let p2 = Uuid::new_v4();

    offline_tracker.track_edit(LocalEdit::new(
        Uuid::new_v4(),
        p1,
        EditType::Create,
        sample_props("p1_e1"),
        1,
    ));
    offline_tracker.track_edit(LocalEdit::new(
        Uuid::new_v4(),
        p1,
        EditType::Update,
        sample_props("p1_e2"),
        1,
    ));
    offline_tracker.track_edit(LocalEdit::new(
        Uuid::new_v4(),
        p2,
        EditType::Delete,
        sample_props("p2_e1"),
        1,
    ));

    let p1_edits = offline_tracker.pending_edits_for_project(p1);
    assert_eq!(p1_edits.len(), 2);

    let p2_edits = offline_tracker.pending_edits_for_project(p2);
    assert_eq!(p2_edits.len(), 1);
}

// CW-09: Conflict with 3+ versions (e.g., 3-way merge scenario).
#[tokio::test]
async fn cw_09_three_way_conflict() {
    let mut conflicts = ConflictStore::new();
    let project_id = Uuid::new_v4();
    let element_id = Uuid::new_v4();
    let reviewer_id = Uuid::new_v4();

    let v1 = ElementVersion::new(
        element_id,
        Uuid::new_v4(),
        "User1".into(),
        "rect".into(),
        sample_props("v1"),
        None,
    );
    let v2 = ElementVersion::new(
        element_id,
        Uuid::new_v4(),
        "User2".into(),
        "rect".into(),
        sample_props("v2"),
        None,
    );
    let v3 = ElementVersion::new(
        element_id,
        Uuid::new_v4(),
        "User3".into(),
        "rect".into(),
        sample_props("v3"),
        None,
    );

    let conflict_id = conflicts
        .create_conflict(project_id, element_id, vec![v1, v2, v3], reviewer_id)
        .unwrap();

    let record = conflicts.get_conflict(conflict_id).unwrap();
    assert_eq!(record.versions.len(), 3);
}

// CW-10: Retry count increments on failed sync attempts.
#[tokio::test]
async fn cw_10_retry_count() {
    let mut sync_status = SyncStatusStore::new();
    let element_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();

    sync_status.mark_pending(element_id, project_id);

    for _ in 0..5 {
        let record = sync_status.get_mut(element_id).unwrap();
        record.increment_retry();
    }

    let record = sync_status.get(element_id).unwrap();
    assert_eq!(record.retry_count, 5);
}
