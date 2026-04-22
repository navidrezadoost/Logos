// logos-collab/src/http_server/handlers/conflicts.rs
//
//! HTTP handlers for conflict resolution workflow.

#[cfg(feature = "http-server")]
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::conflict::{ConflictRecord, ConflictStatus, ElementVersion, ResolutionStrategy};
use crate::http_server::app_state::AppState;
#[cfg(feature = "http-server")]
use crate::http_server::extract::AuthUser;

// ── DTOs ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementVersionBody {
    pub version_id:   Uuid,
    pub element_id:   Uuid,
    pub editor_id:    Uuid,
    pub editor_name:  String,
    pub modified_at:  u64,
    pub element_type: String,
    pub properties:   serde_json::Value,
}

impl From<&ElementVersion> for ElementVersionBody {
    fn from(v: &ElementVersion) -> Self {
        Self {
            version_id:   v.version_id,
            element_id:   v.element_id,
            editor_id:    v.editor_id,
            editor_name:  v.editor_name.clone(),
            modified_at:  v.modified_at,
            element_type: v.element_type.clone(),
            properties:   v.properties.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictResponseBody {
    pub conflict_id:  Uuid,
    pub project_id:   Uuid,
    pub element_id:   Uuid,
    pub status:       String, // "pending", "under_review", "resolved", "rejected"
    pub versions:     Vec<ElementVersionBody>,
    pub reviewer_id:  Uuid,
    pub created_at:   u64,
    pub resolved_at:  Option<u64>,
    pub resolution:   Option<String>, // "accept_local", "accept_remote", "accept_both", "reject_all"
    pub accepted_versions: Vec<Uuid>,
}

impl From<&ConflictRecord> for ConflictResponseBody {
    fn from(c: &ConflictRecord) -> Self {
        Self {
            conflict_id: c.conflict_id,
            project_id: c.project_id,
            element_id: c.element_id,
            status: match c.status {
                ConflictStatus::Pending => "pending".into(),
                ConflictStatus::UnderReview => "under_review".into(),
                ConflictStatus::Resolved => "resolved".into(),
                ConflictStatus::Rejected => "rejected".into(),
            },
            versions: c.versions.iter().map(ElementVersionBody::from).collect(),
            reviewer_id: c.reviewer_id,
            created_at: c.created_at,
            resolved_at: c.resolved_at,
            resolution: c.resolution.map(|r| match r {
                ResolutionStrategy::AcceptLocal => "accept_local".into(),
                ResolutionStrategy::AcceptRemote => "accept_remote".into(),
                ResolutionStrategy::AcceptBoth => "accept_both".into(),
                ResolutionStrategy::RejectAll => "reject_all".into(),
            }),
            accepted_versions: c.accepted_versions.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConflictBody {
    pub project_id: Uuid,
    pub element_id: Uuid,
    pub versions:   Vec<CreateElementVersionBody>,
    pub reviewer_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateElementVersionBody {
    pub editor_id:    Uuid,
    pub editor_name:  String,
    pub element_type: String,
    pub properties:   serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveConflictBody {
    pub strategy: String, // "accept_local", "accept_remote", "accept_both", "reject_all"
    pub accepted_versions: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
}

// ── Axum Handlers ─────────────────────────────────────────────────────────────

#[cfg(feature = "http-server")]
pub mod axum_handlers {
    use super::*;

    /// GET /api/projects/:project_id/conflicts — List conflicts for a project.
    pub async fn list_conflicts(
        State(state): State<AppState>,
        Path(project_id): Path<Uuid>,
        AuthUser(user_id): AuthUser,
    ) -> Result<Json<Vec<ConflictResponseBody>>, (StatusCode, Json<ErrorBody>)> {
        // Check project membership
        let orgs = state.orgs.read().await;
        let projects = state.projects.read().await;

        let project = projects
            .get_project(project_id)
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorBody {
                        error: "project not found".into(),
                    }),
                )
            })?;

        if !orgs.is_member(project.company_id, user_id) {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorBody {
                    error: "not a member of this company".into(),
                }),
            ));
        }

        let conflicts = state.conflicts.read().await;
        let records = conflicts.list_conflicts_for_project(project_id);

        Ok(Json(records.iter().map(|r| r.into()).collect()))
    }

    /// POST /api/conflicts — Create a new conflict.
    pub async fn create_conflict(
        State(state): State<AppState>,
        AuthUser(user_id): AuthUser,
        Json(body): Json<CreateConflictBody>,
    ) -> Result<Json<ConflictResponseBody>, (StatusCode, Json<ErrorBody>)> {
        // Check project access
        let orgs = state.orgs.read().await;
        let projects = state.projects.read().await;

        let project = projects
            .get_project(body.project_id)
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorBody {
                        error: "project not found".into(),
                    }),
                )
            })?;

        if !orgs.is_member(project.company_id, user_id) {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorBody {
                    error: "not a member of this company".into(),
                }),
            ));
        }

        // Build ElementVersions
        let versions: Vec<crate::conflict::ElementVersion> = body
            .versions
            .into_iter()
            .map(|v| {
                crate::conflict::ElementVersion::new(
                    body.element_id,
                    v.editor_id,
                    v.editor_name,
                    v.element_type,
                    v.properties,
                    None,
                )
            })
            .collect();

        let mut conflicts = state.conflicts.write().await;
        let conflict_id = conflicts
            .create_conflict(body.project_id, body.element_id, versions, body.reviewer_id)
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorBody {
                        error: e.to_string(),
                    }),
                )
            })?;

        let record = conflicts.get_conflict(conflict_id).unwrap();
        Ok(Json(ConflictResponseBody::from(record)))
    }

    /// GET /api/conflicts/:conflict_id — Get conflict details.
    pub async fn get_conflict(
        State(state): State<AppState>,
        Path(conflict_id): Path<Uuid>,
        AuthUser(user_id): AuthUser,
    ) -> Result<Json<ConflictResponseBody>, (StatusCode, Json<ErrorBody>)> {
        let conflicts = state.conflicts.read().await;
        let record = conflicts.get_conflict(conflict_id).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorBody {
                    error: "conflict not found".into(),
                }),
            )
        })?;

        // Check user has access to this project
        let orgs = state.orgs.read().await;
        let projects = state.projects.read().await;
        let project = projects.get_project(record.project_id).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorBody {
                    error: "project not found".into(),
                }),
            )
        })?;

        if !orgs.is_member(project.company_id, user_id) {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorBody {
                    error: "not a member of this company".into(),
                }),
            ));
        }

        Ok(Json(ConflictResponseBody::from(record)))
    }

    /// POST /api/conflicts/:conflict_id/review — Mark conflict as under review.
    pub async fn mark_under_review(
        State(state): State<AppState>,
        Path(conflict_id): Path<Uuid>,
        AuthUser(user_id): AuthUser,
    ) -> Result<Json<ConflictResponseBody>, (StatusCode, Json<ErrorBody>)> {
        let mut conflicts = state.conflicts.write().await;
        conflicts
            .mark_under_review(conflict_id, user_id)
            .map_err(|e| {
                (
                    StatusCode::FORBIDDEN,
                    Json(ErrorBody {
                        error: e.to_string(),
                    }),
                )
            })?;

        let record = conflicts.get_conflict(conflict_id).unwrap();
        Ok(Json(ConflictResponseBody::from(record)))
    }

    /// POST /api/conflicts/:conflict_id/resolve — Resolve a conflict.
    pub async fn resolve_conflict(
        State(state): State<AppState>,
        Path(conflict_id): Path<Uuid>,
        AuthUser(user_id): AuthUser,
        Json(body): Json<ResolveConflictBody>,
    ) -> Result<Json<ConflictResponseBody>, (StatusCode, Json<ErrorBody>)> {
        let strategy = match body.strategy.as_str() {
            "accept_local" => ResolutionStrategy::AcceptLocal,
            "accept_remote" => ResolutionStrategy::AcceptRemote,
            "accept_both" => ResolutionStrategy::AcceptBoth,
            "reject_all" => ResolutionStrategy::RejectAll,
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorBody {
                        error: "invalid strategy".into(),
                    }),
                ))
            }
        };

        let mut conflicts = state.conflicts.write().await;
        conflicts
            .resolve_conflict(conflict_id, user_id, strategy, body.accepted_versions)
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorBody {
                        error: e.to_string(),
                    }),
                )
            })?;

        let record = conflicts.get_conflict(conflict_id).unwrap();

        // Update sync status
        drop(conflicts);
        let mut sync_status = state.sync_status.write().await;
        let projects = state.projects.read().await;
        if let Some(project_id) = projects.get_project(record.project_id).map(|p| p.id) {
            sync_status.mark_synced(record.element_id, project_id);
        }

        Ok(Json(ConflictResponseBody::from(record)))
    }

    /// POST /api/conflicts/:conflict_id/reject — Reject a conflict (discard all versions).
    pub async fn reject_conflict(
        State(state): State<AppState>,
        Path(conflict_id): Path<Uuid>,
        AuthUser(user_id): AuthUser,
    ) -> Result<Json<ConflictResponseBody>, (StatusCode, Json<ErrorBody>)> {
        let mut conflicts = state.conflicts.write().await;
        conflicts.reject_conflict(conflict_id, user_id).map_err(|e| {
            (
                StatusCode::FORBIDDEN,
                Json(ErrorBody {
                    error: e.to_string(),
                }),
            )
        })?;

        let record = conflicts.get_conflict(conflict_id).unwrap();

        // Update sync status
        drop(conflicts);
        let mut sync_status = state.sync_status.write().await;
        let projects = state.projects.read().await;
        if let Some(project_id) = projects.get_project(record.project_id).map(|p| p.id) {
            sync_status.mark_rejected(record.element_id, project_id, Some("Reviewer rejected all versions".into()));
        }

        Ok(Json(ConflictResponseBody::from(record)))
    }

    /// GET /api/projects/:project_id/sync-status — Get sync status for project elements.
    pub async fn get_sync_status(
        State(state): State<AppState>,
        Path(project_id): Path<Uuid>,
        AuthUser(user_id): AuthUser,
    ) -> Result<Json<Vec<crate::sync_status::SyncStatusRecord>>, (StatusCode, Json<ErrorBody>)> {
        // Check project access
        let orgs = state.orgs.read().await;
        let projects = state.projects.read().await;

        let project = projects
            .get_project(project_id)
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorBody {
                        error: "project not found".into(),
                    }),
                )
            })?;

        if !orgs.is_member(project.company_id, user_id) {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorBody {
                    error: "not a member of this company".into(),
                }),
            ));
        }

        let sync_status = state.sync_status.read().await;
        let statuses = sync_status.list_for_project(project_id);

        Ok(Json(statuses.into_iter().cloned().collect()))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::AdminEngine;
    use crate::conflict::ConflictStore;
    use crate::org::CompanyStore;
    use crate::project_scope::ProjectStore;
    use crate::sync_status::SyncStatusStore;
    use crate::TokenEngine;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[cfg(feature = "http-server")]
    use axum::{
        extract::{Path, State},
        Json,
    };
    #[cfg(feature = "http-server")]
    use crate::http_server::extract::AuthUser;

    async fn setup() -> (AppState, Uuid, Uuid, Uuid) {
        let mut admin = AdminEngine::new();
        let admin_id = admin
            .initialize("admin", "admin@test.com", "pass", "A", "U")
            .unwrap();

        let token_engine = Arc::new(TokenEngine::new(*b"test-secret-key-32bytes-padded00"));
        let mut orgs = CompanyStore::new();
        let company = crate::org::Company::new("TestCo", admin_id);
        let company_id = orgs.create_company(company);

        let mut projects = ProjectStore::new();
        let project = crate::project_scope::Project::new("TestProj", "desc", company_id, admin_id);
        let project_id = projects.create_project(project);

        let state = AppState {
            admin: Arc::new(RwLock::new(admin)),
            orgs: Arc::new(RwLock::new(orgs)),
            projects: Arc::new(RwLock::new(projects)),
            tokens: token_engine,
            conflicts: Arc::new(RwLock::new(ConflictStore::new())),
            sync_status: Arc::new(RwLock::new(SyncStatusStore::new())),
            server_name: "test".into(),
            version: "1.0.0".into(),
        };

        (state, admin_id, company_id, project_id)
    }

    fn sample_props(val: &str) -> serde_json::Value {
        serde_json::json!({ "content": val })
    }

    // HCON-01: Create conflict via HTTP handler simulation.
    #[tokio::test]
    async fn hcon_01_create_conflict() {
        let (state, admin_id, _company_id, project_id) = setup().await;
        let element_id = Uuid::new_v4();

        let body = CreateConflictBody {
            project_id,
            element_id,
            versions: vec![
                CreateElementVersionBody {
                    editor_id: admin_id,
                    editor_name: "Admin".into(),
                    element_type: "rectangle".into(),
                    properties: sample_props("v1"),
                },
                CreateElementVersionBody {
                    editor_id: Uuid::new_v4(),
                    editor_name: "Bob".into(),
                    element_type: "rectangle".into(),
                    properties: sample_props("v2"),
                },
            ],
            reviewer_id: admin_id,
        };

        #[cfg(feature = "http-server")]
        {
            let result = axum_handlers::create_conflict(
                State(state.clone()),
                AuthUser(admin_id),
                Json(body),
            )
            .await;
            assert!(result.is_ok());
        }

        #[cfg(not(feature = "http-server"))]
        {
            // Non-axum test: call ConflictStore directly
            let versions: Vec<crate::conflict::ElementVersion> = body
                .versions
                .into_iter()
                .map(|v| {
                    crate::conflict::ElementVersion::new(
                        element_id,
                        v.editor_id,
                        v.editor_name,
                        v.element_type,
                        v.properties,
                        None,
                    )
                })
                .collect();

            let mut conflicts = state.conflicts.write().await;
            let cid = conflicts
                .create_conflict(project_id, element_id, versions, admin_id)
                .unwrap();
            assert!(conflicts.get_conflict(cid).is_some());
        }
    }

    // HCON-02: List conflicts for project.
    #[tokio::test]
    async fn hcon_02_list_conflicts() {
        let (state, admin_id, _company_id, project_id) = setup().await;
        let element_id = Uuid::new_v4();

        // Create conflict
        let mut conflicts = state.conflicts.write().await;
        let v1 = crate::conflict::ElementVersion::new(
            element_id,
            admin_id,
            "Admin".into(),
            "rect".into(),
            sample_props("x"),
            None,
        );
        let v2 = crate::conflict::ElementVersion::new(
            element_id,
            Uuid::new_v4(),
            "Bob".into(),
            "rect".into(),
            sample_props("y"),
            None,
        );
        conflicts
            .create_conflict(project_id, element_id, vec![v1, v2], admin_id)
            .unwrap();
        drop(conflicts);

        #[cfg(feature = "http-server")]
        {
            let result = axum_handlers::list_conflicts(
                State(state),
                Path(project_id),
                AuthUser(admin_id),
            )
            .await;
            assert!(result.is_ok());
            let list = result.unwrap().0;
            assert_eq!(list.len(), 1);
        }

        #[cfg(not(feature = "http-server"))]
        {
            let conflicts = state.conflicts.read().await;
            let list = conflicts.list_conflicts_for_project(project_id);
            assert_eq!(list.len(), 1);
        }
    }

    // HCON-03: Resolve conflict.
    #[tokio::test]
    async fn hcon_03_resolve_conflict() {
        let (state, admin_id, _company_id, project_id) = setup().await;
        let element_id = Uuid::new_v4();

        let v1 = crate::conflict::ElementVersion::new(
            element_id,
            admin_id,
            "Admin".into(),
            "rect".into(),
            sample_props("local"),
            None,
        );
        let v1_id = v1.version_id;
        let v2 = crate::conflict::ElementVersion::new(
            element_id,
            Uuid::new_v4(),
            "Bob".into(),
            "rect".into(),
            sample_props("remote"),
            None,
        );

        let mut conflicts = state.conflicts.write().await;
        let cid = conflicts
            .create_conflict(project_id, element_id, vec![v1, v2], admin_id)
            .unwrap();
        drop(conflicts);

        let body = ResolveConflictBody {
            strategy: "accept_local".into(),
            accepted_versions: vec![v1_id],
        };

        #[cfg(feature = "http-server")]
        {
            let result = axum_handlers::resolve_conflict(
                State(state.clone()),
                Path(cid),
                AuthUser(admin_id),
                Json(body),
            )
            .await;
            assert!(result.is_ok());
        }

        #[cfg(not(feature = "http-server"))]
        {
            let mut conflicts = state.conflicts.write().await;
            conflicts
                .resolve_conflict(cid, admin_id, ResolutionStrategy::AcceptLocal, vec![v1_id])
                .unwrap();
        }

        let conflicts = state.conflicts.read().await;
        let record = conflicts.get_conflict(cid).unwrap();
        assert_eq!(record.status, ConflictStatus::Resolved);
    }

    // HCON-04: Reject conflict.
    #[tokio::test]
    async fn hcon_04_reject_conflict() {
        let (state, admin_id, _company_id, project_id) = setup().await;
        let element_id = Uuid::new_v4();

        let v1 = crate::conflict::ElementVersion::new(
            element_id,
            admin_id,
            "Admin".into(),
            "rect".into(),
            sample_props("a"),
            None,
        );
        let v2 = crate::conflict::ElementVersion::new(
            element_id,
            Uuid::new_v4(),
            "Bob".into(),
            "rect".into(),
            sample_props("b"),
            None,
        );

        let mut conflicts = state.conflicts.write().await;
        let cid = conflicts
            .create_conflict(project_id, element_id, vec![v1, v2], admin_id)
            .unwrap();
        drop(conflicts);

        #[cfg(feature = "http-server")]
        {
            let result = axum_handlers::reject_conflict(
                State(state.clone()),
                Path(cid),
                AuthUser(admin_id),
            )
            .await;
            assert!(result.is_ok());
        }

        #[cfg(not(feature = "http-server"))]
        {
            let mut conflicts = state.conflicts.write().await;
            conflicts.reject_conflict(cid, admin_id).unwrap();
        }

        let conflicts = state.conflicts.read().await;
        let record = conflicts.get_conflict(cid).unwrap();
        assert_eq!(record.status, ConflictStatus::Rejected);
    }
}
