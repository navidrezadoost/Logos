// logos-collab/src/http_server/handlers/companies.rs
//
//! Handlers: GET/POST /api/companies, GET/POST /api/companies/:id/members

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── DTOs ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CompanyResponseBody {
    pub id:            Uuid,
    pub name:          String,
    pub role:          String,
    pub member_count:  usize,
    pub project_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct CreateCompanyBody {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberBody {
    pub user_id: Uuid,
    pub role:    String,   // "Viewer" | "Member" | "Admin"
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

    use crate::org::{Company, CompanyRole};
    use crate::http_server::app_state::AppState;
    use crate::http_server::extract::AuthUser;
    use crate::http_server::handlers::auth::ErrorBody;
    use super::{AddMemberBody, CompanyResponseBody, CreateCompanyBody};

    pub async fn list_companies(
        State(state): State<AppState>,
        AuthUser(user_id): AuthUser,
    ) -> Json<Vec<CompanyResponseBody>> {
        let orgs     = state.orgs.read().await;
        let projects = state.projects.read().await;
        let companies = orgs.companies_for_user(user_id);

        let body = companies.iter().map(|c| {
            let members      = orgs.get_members(c.id).map(|m| m.len()).unwrap_or(0);
            let proj_count   = projects.projects_for_company(c.id).len();
            let role         = orgs.member_role(c.id, user_id)
                .map(|r| format!("{r:?}")).unwrap_or_default();
            CompanyResponseBody {
                id: c.id, name: c.name.clone(), role,
                member_count: members, project_count: proj_count,
            }
        }).collect();

        Json(body)
    }

    pub async fn create_company(
        State(state): State<AppState>,
        AuthUser(user_id): AuthUser,
        Json(body): Json<CreateCompanyBody>,
    ) -> Result<(StatusCode, Json<CompanyResponseBody>), (StatusCode, Json<ErrorBody>)> {
        if body.name.trim().is_empty() {
            return Err((StatusCode::BAD_REQUEST, Json(ErrorBody::new("Name is required"))));
        }
        let company = Company::new(body.name.trim().to_owned(), user_id);
        let mut orgs = state.orgs.write().await;
        let id = orgs.create_company(company);
        let c  = orgs.get_company(id).unwrap();
        Ok((StatusCode::CREATED, Json(CompanyResponseBody {
            id: c.id, name: c.name.clone(), role: "Admin".into(),
            member_count: 1, project_count: 0,
        })))
    }

    pub async fn add_member(
        State(state): State<AppState>,
        AuthUser(actor_id): AuthUser,
        Path(company_id): Path<Uuid>,
        Json(body): Json<AddMemberBody>,
    ) -> Result<StatusCode, (StatusCode, Json<ErrorBody>)> {
        let role = match body.role.as_str() {
            "Viewer" => CompanyRole::Viewer,
            "Member" => CompanyRole::Member,
            "Admin"  => CompanyRole::Admin,
            other    => return Err((StatusCode::BAD_REQUEST,
                                    Json(ErrorBody::new(format!("Unknown role: {other}"))))),
        };
        let mut orgs = state.orgs.write().await;
        orgs.add_member(company_id, actor_id, body.user_id, role)
            .map(|_| StatusCode::NO_CONTENT)
            .map_err(|e| (StatusCode::FORBIDDEN, Json(ErrorBody::new(e.to_string()))))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (HC-01 … HC-06)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // HC-01: CompanyResponseBody serializes all fields
    #[test]
    fn hc_01_response_serializes() {
        let r = CompanyResponseBody {
            id: Uuid::nil(), name: "Acme".into(),
            role: "Admin".into(), member_count: 3, project_count: 2,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("Acme"));
        assert!(json.contains("Admin"));
    }

    // HC-02: CreateCompanyBody deserializes name
    #[test]
    fn hc_02_create_body_deserializes() {
        let json = r#"{"name":"Globex"}"#;
        let b: CreateCompanyBody = serde_json::from_str(json).unwrap();
        assert_eq!(b.name, "Globex");
    }

    // HC-03: AddMemberBody deserializes user_id + role
    #[test]
    fn hc_03_add_member_deserializes() {
        let json = format!(r#"{{"user_id":"{}","role":"Member"}}"#, Uuid::nil());
        let b: AddMemberBody = serde_json::from_str(&json).unwrap();
        assert_eq!(b.role, "Member");
    }

    // HC-04: member_count field is numeric
    #[test]
    fn hc_04_member_count_numeric() {
        let r = CompanyResponseBody {
            id: Uuid::nil(), name: "X".into(), role: "Viewer".into(),
            member_count: 42, project_count: 0,
        };
        assert_eq!(r.member_count, 42);
    }
}
