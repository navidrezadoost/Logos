//! Phase 15.9 — Enterprise SSO for Agents integration tests.
//!
//! 40 tests covering: identity (10), saml (10), oidc (10), session/rbac (10).

use logos_agent_sso::{
    // identity
    IdentityProvider, SamlConfig, OidcConfig, IdpError,
    // saml
    SamlAssertion, SamlAttribute, SamlParser, SamlError,
    // oidc
    OidcToken, OidcClaims, OidcExchange, OidcError,
    // session
    SessionStore, SsoSession, SessionToken, SessionError, Role, RbacPolicy,
};

// ════════════════════════════════════════════════════════════════════════════
// Helpers
// ════════════════════════════════════════════════════════════════════════════

fn saml_cfg() -> SamlConfig {
    SamlConfig::new(
        "https://idp.corp.example/saml2",
        "https://idp.corp.example/sso",
        "MIIC_FAKE_CERT",
        "https://logos.example/sp",
        "https://logos.example/acs",
    )
}

fn oidc_cfg() -> OidcConfig {
    OidcConfig::new("https://sso.corp.example/oidc", "logos-client", "s3cr3t")
}

fn assertion_json(not_on_or_after: u64) -> String {
    serde_json::json!({
        "subject": "user@corp.example",
        "issuer": "https://idp.corp.example/saml2",
        "audience": "https://logos.example/sp",
        "not_before": 1_000_000u64,
        "not_on_or_after": not_on_or_after,
        "session_index": "idx-001",
        "attributes": {
            "email": "user@corp.example",
            "roles": ["publisher", "viewer"],
            "department": "Engineering"
        }
    }).to_string()
}

fn valid_oidc_claims(exp: u64) -> OidcClaims {
    OidcClaims {
        sub: "sub-001".into(),
        iss: "https://sso.corp.example/oidc".into(),
        aud: "logos-client".into(),
        exp,
        iat: 1_000_000,
        nonce: None,
        email: Some("alice@corp.example".into()),
        email_verified: Some(true),
        given_name: Some("Alice".into()),
        family_name: Some("Smith".into()),
        roles: Some(vec!["publisher".into()]),
    }
}

fn make_session(user: &str) -> SsoSession {
    SsoSession::new(user, &["agent:read", "agent:invoke", "metrics:read"])
}

// ════════════════════════════════════════════════════════════════════════════
// §1 Identity provider (10 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn idp_saml_protocol_label() {
    let idp = IdentityProvider::saml("corp", saml_cfg());
    assert_eq!(idp.protocol(), "saml2");
}

#[test]
fn idp_oidc_protocol_label() {
    let idp = IdentityProvider::oidc(oidc_cfg());
    assert_eq!(idp.protocol(), "oidc");
}

#[test]
fn idp_saml_validate_ok() {
    let idp = IdentityProvider::saml("corp", saml_cfg());
    assert!(idp.validate().is_ok());
}

#[test]
fn idp_oidc_validate_ok() {
    let idp = IdentityProvider::oidc(oidc_cfg());
    assert!(idp.validate().is_ok());
}

#[test]
fn idp_saml_missing_cert_fails() {
    let mut cfg = saml_cfg();
    cfg.certificate_pem = String::new();
    let idp = IdentityProvider::saml("corp", cfg);
    assert!(idp.validate().is_err());
}

#[test]
fn idp_oidc_missing_client_secret_fails() {
    let cfg = OidcConfig::new("https://sso.corp.example/oidc", "client", "");
    let idp = IdentityProvider::oidc(cfg);
    assert_eq!(idp.validate().unwrap_err(), IdpError::MissingField("client_secret".into()));
}

#[test]
fn idp_registry_register_two_protocols() {
    use logos_agent_sso::identity::IdpRegistry;
    let mut reg = IdpRegistry::new();
    reg.register(IdentityProvider::saml("s1", saml_cfg())).unwrap();
    reg.register(IdentityProvider::oidc(oidc_cfg())).unwrap();
    assert_eq!(reg.count(), 2);
}

#[test]
fn idp_registry_find_by_name() {
    use logos_agent_sso::identity::IdpRegistry;
    let mut reg = IdpRegistry::new();
    reg.register(IdentityProvider::saml("my-saml", saml_cfg())).unwrap();
    assert!(reg.find("my-saml").is_some());
    assert!(reg.find("nonexistent").is_none());
}

#[test]
fn idp_registry_enabled_filter() {
    use logos_agent_sso::identity::IdpRegistry;
    let mut reg = IdpRegistry::new();
    reg.register(IdentityProvider::oidc(oidc_cfg()).with_enabled(true)).unwrap();
    let disabled = OidcConfig::new("https://disabled.example/oidc", "c", "s");
    reg.register(IdentityProvider::oidc(disabled).with_enabled(false)).unwrap();
    assert_eq!(reg.enabled_providers().len(), 1);
}

#[test]
fn oidc_discovery_url_format() {
    let cfg = oidc_cfg();
    assert_eq!(
        cfg.discovery_url(),
        "https://sso.corp.example/oidc/.well-known/openid-configuration"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// §2 SAML (10 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn saml_parse_subject() {
    let parser = SamlParser::new("https://logos.example/sp");
    let a = parser.parse(&assertion_json(9_999_999_999)).unwrap();
    assert_eq!(a.subject, "user@corp.example");
}

#[test]
fn saml_parse_issuer() {
    let parser = SamlParser::new("https://logos.example/sp");
    let a = parser.parse(&assertion_json(9_999_999_999)).unwrap();
    assert_eq!(a.issuer, "https://idp.corp.example/saml2");
}

#[test]
fn saml_parse_session_index() {
    let parser = SamlParser::new("https://logos.example/sp");
    let a = parser.parse(&assertion_json(9_999_999_999)).unwrap();
    assert_eq!(a.session_index.as_deref(), Some("idx-001"));
}

#[test]
fn saml_email_from_attribute() {
    let parser = SamlParser::new("https://logos.example/sp");
    let a = parser.parse(&assertion_json(9_999_999_999)).unwrap();
    assert_eq!(a.email(), "user@corp.example");
}

#[test]
fn saml_roles_extracted() {
    let parser = SamlParser::new("https://logos.example/sp");
    let a = parser.parse(&assertion_json(9_999_999_999)).unwrap();
    assert!(a.roles().contains(&"publisher"));
}

#[test]
fn saml_validate_ok() {
    let parser = SamlParser::new("https://logos.example/sp");
    let a = parser.parse(&assertion_json(9_999_999_999)).unwrap();
    assert!(a.validate(1_000_001, "https://logos.example/sp").is_ok());
}

#[test]
fn saml_expired_fails() {
    let parser = SamlParser::new("https://logos.example/sp");
    let a = parser.parse(&assertion_json(50)).unwrap();
    assert_eq!(a.validate(100, "https://logos.example/sp").unwrap_err(), SamlError::Expired);
}

#[test]
fn saml_audience_mismatch_fails() {
    let parser = SamlParser::new("https://logos.example/sp");
    let a = parser.parse(&assertion_json(9_999_999_999)).unwrap();
    let err = a.validate(1_000_001, "https://other.example/sp").unwrap_err();
    assert!(matches!(err, SamlError::AudienceMismatch { .. }));
}

#[test]
fn saml_parse_and_validate_integrated() {
    let parser = SamlParser::new("https://logos.example/sp");
    let result = parser.parse_and_validate(&assertion_json(9_999_999_999), 1_000_001);
    assert!(result.is_ok());
}

#[test]
fn saml_malformed_payload_err() {
    let parser = SamlParser::new("https://logos.example/sp");
    assert!(matches!(parser.parse("<!xml garbage>"), Err(SamlError::MalformedAssertion(_))));
}

// ════════════════════════════════════════════════════════════════════════════
// §3 OIDC (10 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn oidc_claims_validate_ok() {
    let c = valid_oidc_claims(9_999_999_999);
    assert!(c.validate(1_000_001, "https://sso.corp.example/oidc", "logos-client", None).is_ok());
}

#[test]
fn oidc_claims_expired_err() {
    let c = valid_oidc_claims(100);
    assert_eq!(
        c.validate(200, "https://sso.corp.example/oidc", "logos-client", None).unwrap_err(),
        OidcError::TokenExpired
    );
}

#[test]
fn oidc_claims_wrong_issuer_err() {
    let c = valid_oidc_claims(9_999_999_999);
    let err = c.validate(1_000_001, "https://evil.example", "logos-client", None).unwrap_err();
    assert!(matches!(err, OidcError::InvalidIssuer { .. }));
}

#[test]
fn oidc_claims_wrong_audience_err() {
    let c = valid_oidc_claims(9_999_999_999);
    let err = c.validate(1_000_001, "https://sso.corp.example/oidc", "other-client", None).unwrap_err();
    assert!(matches!(err, OidcError::InvalidAudience(_)));
}

#[test]
fn oidc_claims_nonce_mismatch_err() {
    let mut c = valid_oidc_claims(9_999_999_999);
    c.nonce = Some("expected-nonce".into());
    let err = c.validate(1_000_001, "https://sso.corp.example/oidc", "logos-client", Some("wrong-nonce")).unwrap_err();
    assert_eq!(err, OidcError::NonceMismatch);
}

#[test]
fn oidc_claims_display_name_from_given() {
    let c = valid_oidc_claims(9_999_999_999);
    assert_eq!(c.display_name(), "Alice");
}

#[test]
fn oidc_claims_display_name_fallback_email() {
    let mut c = valid_oidc_claims(9_999_999_999);
    c.given_name = None;
    c.family_name = None;
    assert_eq!(c.display_name(), "alice@corp.example");
}

#[test]
fn oidc_exchange_parse_valid_token() {
    let exch = OidcExchange::new("https://sso.corp.example/oidc", "logos-client");
    let payload = serde_json::to_string(&valid_oidc_claims(9_999_999_999)).unwrap();
    let claims = exch.parse_id_token(&payload, 1_000_001, None).unwrap();
    assert_eq!(claims.sub, "sub-001");
    assert_eq!(claims.email.as_deref(), Some("alice@corp.example"));
}

#[test]
fn oidc_token_bearer_type() {
    let tok = OidcToken::new("access", "{}", 3600);
    assert_eq!(tok.token_type, "Bearer");
}

#[test]
fn oidc_exchange_bad_json_err() {
    let exch = OidcExchange::new("iss", "aud");
    assert!(matches!(
        exch.parse_id_token("not-json", 0, None),
        Err(OidcError::InvalidFormat(_))
    ));
}

// ════════════════════════════════════════════════════════════════════════════
// §4 Session & RBAC (10 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn session_issue_and_validate() {
    let mut store = SessionStore::new();
    let tok = store.issue(make_session("alice@corp.example"));
    let sess = store.validate(&tok).unwrap();
    assert_eq!(sess.user_id, "alice@corp.example");
}

#[test]
fn session_unknown_token_not_found() {
    let store = SessionStore::new();
    let err = store.validate(&SessionToken("bad-token".into())).unwrap_err();
    assert_eq!(err, SessionError::NotFound);
}

#[test]
fn session_revoke_then_validate_revoked() {
    let mut store = SessionStore::new();
    let tok = store.issue(make_session("bob@corp.example"));
    store.revoke(&tok).unwrap();
    assert_eq!(store.validate(&tok).unwrap_err(), SessionError::Revoked);
}

#[test]
fn session_expired_at_explicit_ts() {
    let mut store = SessionStore::new();
    let sess = SsoSession::new("carol@corp.example", &[]).with_expiry(500);
    let tok = store.issue(sess);
    assert_eq!(store.validate_at(&tok, 600).unwrap_err(), SessionError::Expired);
}

#[test]
fn session_has_permission_check() {
    let sess = make_session("dave@corp.example");
    assert!(sess.has_permission("agent:read"));
    assert!(!sess.has_permission("agent:publish"));
}

#[test]
fn session_active_count_after_revoke() {
    let mut store = SessionStore::new();
    let t1 = store.issue(make_session("u1@x.example"));
    let _t2 = store.issue(make_session("u2@x.example"));
    store.revoke(&t1).unwrap();
    assert_eq!(store.active_count(), 1);
    assert_eq!(store.total_issued(), 2);
}

#[test]
fn rbac_admin_all_actions_allowed() {
    let policy = RbacPolicy::default();
    for act in ["agent:read","agent:invoke","agent:publish","agent:delete","session:revoke"] {
        assert!(policy.allows(&Role::Admin, act), "admin should allow {act}");
    }
}

#[test]
fn rbac_viewer_only_read_and_invoke() {
    let policy = RbacPolicy::default();
    assert!(policy.allows(&Role::Viewer, "agent:read"));
    assert!(policy.allows(&Role::Viewer, "agent:invoke"));
    assert!(!policy.allows(&Role::Viewer, "agent:publish"));
    assert!(!policy.allows(&Role::Viewer, "agent:delete"));
}

#[test]
fn rbac_check_permission_denied_error() {
    let policy = RbacPolicy::default();
    let err = policy.check(&Role::Developer, "agent:delete").unwrap_err();
    assert!(matches!(err, SessionError::PermissionDenied(_, _)));
}

#[test]
fn rbac_custom_role_add_rule() {
    let mut policy = RbacPolicy::new();
    policy.add_rule(Role::Custom("auditor".into()), vec!["metrics:read", "feedback:read"]);
    assert!(policy.allows(&Role::Custom("auditor".into()), "metrics:read"));
    assert!(!policy.allows(&Role::Custom("auditor".into()), "agent:delete"));
}
