//! # logos-agent-sso
//!
//! Enterprise SSO (Single Sign-On) for Logos Agents.
//!
//! Provides SAML 2.0 assertion parsing, OIDC token exchange, JWT-based
//! session management, and Role-Based Access Control (RBAC) for agent
//! operations inside corporate deployments.
//!
//! ## Quick start
//!
//! ```rust
//! use logos_agent_sso::{
//!     IdentityProvider, SamlConfig, OidcConfig,
//!     SessionStore, SsoSession,
//!     Role, RbacPolicy,
//! };
//!
//! // Provision an OIDC provider
//! let cfg = OidcConfig::new(
//!     "https://sso.corp.example/oidc",
//!     "logos-client",
//!     "s3cr3t",
//! );
//! let idp = IdentityProvider::oidc(cfg);
//! assert_eq!(idp.protocol(), "oidc");
//!
//! // Create a session for a successfully-authenticated user
//! let mut store = SessionStore::new();
//! let sess = SsoSession::new("user@corp.example", &["agent:read", "agent:invoke"]);
//! let token = store.issue(sess);
//! assert!(store.validate(&token).is_ok());
//!
//! // Check RBAC
//! let policy = RbacPolicy::default();
//! let role = Role::Publisher;
//! assert!(policy.allows(&role, "agent:publish"));
//! ```

pub mod identity;
pub mod saml;
pub mod oidc;
pub mod session;

pub use identity::{IdentityProvider, SamlConfig, OidcConfig, IdpError};
pub use saml::{SamlAssertion, SamlAttribute, SamlParser, SamlError};
pub use oidc::{OidcToken, OidcClaims, OidcExchange, OidcError};
pub use session::{SessionStore, SsoSession, SessionToken, SessionError, Role, RbacPolicy};
