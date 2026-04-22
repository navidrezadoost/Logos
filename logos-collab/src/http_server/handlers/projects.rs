// logos-collab/src/http_server/handlers/projects.rs
//
//! Handlers: GET/POST /api/companies/:id/projects, GET /api/projects/:id

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── DTOs ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ProjectResponseBody {
    pub id:           Uuid,
    pub name:         String,
    pub description:  String,
    pub company_id:   Uuid,
    pub creator_name: String,
    pub tools:        Vec<String>,
    pub member_count: usize,
    pub created_at:   u64,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectBody {
    pub name:        String,
    pub description: String,
    pub tools:       Vec<String>,
}

// ── Axum handlers (http-server feature) ──────────────────────────────────────

#[cfg(feature = "http-server")]
pub mod axum_handlers {
    use axum::{
        extract::{Path, State},
        http::StatusCode,
        Json,
    };
    use uuid::Uuid;

    use crate::project_scope::Project;
    use crate::http_server::app_state::AppState;
    use crate::http_server::extract::AuthUser;
    use crate::http_server::handlers::auth::ErrorBody;
    use super::{CreateProjectBody, ProjectResponseBody};

    pub async fn list_projects(
        State(state): State<AppState>,
        AuthUser(user_id): AuthUser,
        Path(company_id): Path<Uuid>,
    ) -> Result<Json<Vec<ProjectResponseBody>>, (StatusCode, Json<ErrorBody>)> {
        // Verify user is a member of the company
        {
            let orgs = state.orgs.read().await;
            if !orgs.is_member(company_id, user_id) {
                return Err((StatusCode::FORBIDDEN,
                            Json(ErrorBody::new("Not a member of this company"))));
            }
        }

        let projects = state.projects.read().await;
        let list = projects.projects_for_company(company_id);
        let body = list.iter().map(|p| {
            let members = projects.get_members(p.id).map(|m| m.len()).unwrap_or(0);
            ProjectResponseBody {
                id:           p.id,
                name:         p.name.clone(),
                description:  p.description.clone(),
                company_id:   p.company_id,
                creator_name: p.creator_user_id.to_string(), // resolved in real impl
                tools:        p.tools.clone(),
                member_count: members,
                created_at:   p.created_at,
            }
        }).collect();

        Ok(Json(body))
    }

    pub async fn create_project(
        State(state): State<AppState>,
        AuthUser(user_id): AuthUser,
        Path(company_id): Path<Uuid>,
        Json(body): Json<CreateProjectBody>,
    ) -> Result<(StatusCode, Json<ProjectResponseBody>), (StatusCode, Json<ErrorBody>)> {
        if body.name.trim().is_empty() {
            return Err((StatusCode::BAD_REQUEST, Json(ErrorBody::new("Name is required"))));
        }
        // Verify membership
        {
            let orgs = state.orgs.read().await;
            if !orgs.is_member(company_id, user_id) {
                return Err((StatusCode::FORBIDDEN,
                            Json(ErrorBody::new("Not a member of this company"))));
            }
        }
        let project = Project::new(
            body.name.trim().to_owned(),
            body.description.clone(),
            company_id,
            user_id,
        ).with_tools(body.tools.iter().cloned());

        let mut projects = state.projects.write().await;
        let id = projects.create_project(project);
        let p  = projects.get_project(id).unwrap();

        Ok((StatusCode::CREATED, Json(ProjectResponseBody {
            id:           p.id,
            name:         p.name.clone(),
            description:  p.description.clone(),
            company_id:   p.company_id,
            creator_name: p.creator_user_id.to_string(),
            tools:        p.tools.clone(),
            member_count: 1,
            created_at:   p.created_at,
        })))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (HP-01 … HP-06)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // HP-01: ProjectResponseBody serializes tools array
    #[test]
    fn hp_01_response_serializes_tools() {
        let r = ProjectResponseBody {
            id: Uuid::nil(), name: "Brand".into(), description: "d".into(),
            company_id: Uuid::nil(), creator_name: "alice".into(),
            tools: vec!["vector".into()], member_count: 1, created_at: 0,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("vector"));
    }

    // HP-02: CreateProjectBody deserializes all fields
    #[test]
    fn hp_02_create_body_deserializes() {
        let json = r#"{"name":"P1","description":"desc","tools":["vector"]}"#;
        let b: CreateProjectBody = serde_json::from_str(json).unwrap();
        assert_eq!(b.name, "P1");
        assert_eq!(b.tools, vec!["vector"]);
    }

    // HP-03: CreateProjectBody tools can be empty
    #[test]
    fn hp_03_empty_tools() {
        let json = r#"{"name":"P2","description":"","tools":[]}"#;
        let b: CreateProjectBody = serde_json::from_str(json).unwrap();
        assert!(b.tools.is_empty());
    }

    // HP-04: ProjectResponseBody member_count is numeric
    #[test]
    fn hp_04_member_count() {
        let r = ProjectResponseBody {
            id: Uuid::nil(), name: "X".into(), description: "".into(),
            company_id: Uuid::nil(), creator_name: "u".into(),
            tools: vec![], member_count: 7, created_at: 0,
        };
        assert_eq!(r.member_count, 7);
    }
}
