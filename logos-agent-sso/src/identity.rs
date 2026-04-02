//! Identity provider configuration and protocol selection.
//!
//! An `IdentityProvider` represents a corporate IdP that Logos agents
//! are registered with.  Supports SAML 2.0 and OIDC protocols.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdpError {
    #[error("missing required field: {0}")]
    MissingField(String),
    #[error("unsupported protocol: {0}")]
    UnsupportedProtocol(String),
    #[error("invalid entity ID: {0}")]
    InvalidEntityId(String),
    #[error("metadata fetch failed: {0}")]
    MetadataError(String),
}

// ── SAML configuration ────────────────────────────────────────────────────────

/// Configuration for a SAML 2.0 identity provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlConfig {
    /// SAML entity ID (issuer) of the IdP.
    pub entity_id: String,
    /// HTTP-Redirect or HTTP-POST SSO URL.
    pub sso_url: String,
    /// X.509 certificate (PEM, without headers) used to verify assertions.
    pub certificate_pem: String,
    /// SP entity ID presented to the IdP.
    pub sp_entity_id: String,
    /// ACS (Assertion Consumer Service) callback URL registered with the IdP.
    pub acs_url: String,
    /// Whether to require signed assertions.
    pub require_signed_assertions: bool,
}

impl SamlConfig {
    pub fn new(
        entity_id: impl Into<String>,
        sso_url: impl Into<String>,
        certificate_pem: impl Into<String>,
        sp_entity_id: impl Into<String>,
        acs_url: impl Into<String>,
    ) -> Self {
        Self {
            entity_id: entity_id.into(),
            sso_url: sso_url.into(),
            certificate_pem: certificate_pem.into(),
            sp_entity_id: sp_entity_id.into(),
            acs_url: acs_url.into(),
            require_signed_assertions: true,
        }
    }

    pub fn validate(&self) -> Result<(), IdpError> {
        if self.entity_id.is_empty() {
            return Err(IdpError::MissingField("entity_id".into()));
        }
        if self.sso_url.is_empty() {
            return Err(IdpError::MissingField("sso_url".into()));
        }
        if self.certificate_pem.is_empty() {
            return Err(IdpError::MissingField("certificate_pem".into()));
        }
        if !self.sso_url.starts_with("https://") && !self.sso_url.starts_with("http://") {
            return Err(IdpError::InvalidEntityId(self.sso_url.clone()));
        }
        Ok(())
    }
}

// ── OIDC configuration ────────────────────────────────────────────────────────

/// Configuration for an OpenID Connect identity provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// OIDC issuer base URL (discovery endpoint is `<issuer>/.well-known/openid-configuration`).
    pub issuer: String,
    /// OAuth2 client ID.
    pub client_id: String,
    /// OAuth2 client secret.
    pub client_secret: String,
    /// Redirect URI registered with the IdP.
    pub redirect_uri: String,
    /// Requested scopes (space-separated string).
    pub scopes: String,
}

impl OidcConfig {
    pub fn new(
        issuer: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        Self {
            issuer: issuer.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            redirect_uri: String::new(),
            scopes: "openid email profile".to_string(),
        }
    }

    pub fn with_redirect_uri(mut self, uri: impl Into<String>) -> Self {
        self.redirect_uri = uri.into();
        self
    }

    pub fn with_scopes(mut self, scopes: impl Into<String>) -> Self {
        self.scopes = scopes.into();
        self
    }

    pub fn validate(&self) -> Result<(), IdpError> {
        if self.issuer.is_empty() {
            return Err(IdpError::MissingField("issuer".into()));
        }
        if self.client_id.is_empty() {
            return Err(IdpError::MissingField("client_id".into()));
        }
        if self.client_secret.is_empty() {
            return Err(IdpError::MissingField("client_secret".into()));
        }
        Ok(())
    }

    /// Derive the well-known discovery URL.
    pub fn discovery_url(&self) -> String {
        format!("{}/.well-known/openid-configuration", self.issuer.trim_end_matches('/'))
    }
}

// ── Protocol ──────────────────────────────────────────────────────────────────

/// The SSO wire protocol in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SsoProtocol {
    Saml2,
    Oidc,
}

impl SsoProtocol {
    pub fn label(&self) -> &str {
        match self {
            Self::Saml2 => "saml2",
            Self::Oidc  => "oidc",
        }
    }
}

// ── Identity provider ─────────────────────────────────────────────────────────

/// A configured external identity provider.
#[derive(Debug, Clone)]
pub struct IdentityProvider {
    pub name: String,
    protocol: SsoProtocol,
    saml_config: Option<SamlConfig>,
    oidc_config: Option<OidcConfig>,
    /// Human-readable tenant or organisation name.
    pub tenant: String,
    pub enabled: bool,
}

impl IdentityProvider {
    /// Construct a SAML 2.0 provider.
    pub fn saml(name: impl Into<String>, cfg: SamlConfig) -> Self {
        Self {
            name: name.into(),
            protocol: SsoProtocol::Saml2,
            saml_config: Some(cfg),
            oidc_config: None,
            tenant: String::new(),
            enabled: true,
        }
    }

    /// Construct an OIDC provider.
    pub fn oidc(cfg: OidcConfig) -> Self {
        let name = cfg.issuer.clone();
        Self {
            name,
            protocol: SsoProtocol::Oidc,
            saml_config: None,
            oidc_config: Some(cfg),
            tenant: String::new(),
            enabled: true,
        }
    }

    pub fn with_tenant(mut self, tenant: impl Into<String>) -> Self {
        self.tenant = tenant.into();
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn protocol(&self) -> &str { self.protocol.label() }

    pub fn saml_config(&self) -> Option<&SamlConfig> { self.saml_config.as_ref() }

    pub fn oidc_config(&self) -> Option<&OidcConfig> { self.oidc_config.as_ref() }

    /// Validate the underlying config.
    pub fn validate(&self) -> Result<(), IdpError> {
        match &self.protocol {
            SsoProtocol::Saml2 => {
                self.saml_config.as_ref()
                    .ok_or_else(|| IdpError::MissingField("saml_config".into()))?
                    .validate()
            }
            SsoProtocol::Oidc => {
                self.oidc_config.as_ref()
                    .ok_or_else(|| IdpError::MissingField("oidc_config".into()))?
                    .validate()
            }
        }
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// In-memory registry of configured identity providers.
#[derive(Debug, Default)]
pub struct IdpRegistry {
    providers: Vec<IdentityProvider>,
}

impl IdpRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, idp: IdentityProvider) -> Result<(), IdpError> {
        idp.validate()?;
        self.providers.push(idp);
        Ok(())
    }

    pub fn find(&self, name: &str) -> Option<&IdentityProvider> {
        self.providers.iter().find(|p| p.name == name)
    }

    pub fn count(&self) -> usize { self.providers.len() }

    pub fn enabled_providers(&self) -> Vec<&IdentityProvider> {
        self.providers.iter().filter(|p| p.enabled).collect()
    }

    pub fn providers_for_protocol(&self, protocol: SsoProtocol) -> Vec<&IdentityProvider> {
        self.providers.iter().filter(|p| p.protocol == protocol).collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn saml_cfg() -> SamlConfig {
        SamlConfig::new(
            "https://idp.corp.example/saml2",
            "https://idp.corp.example/saml2/sso",
            "MIIC...",
            "https://logos.example/sp",
            "https://logos.example/acs",
        )
    }

    fn oidc_cfg() -> OidcConfig {
        OidcConfig::new("https://sso.corp.example/oidc", "logos-client", "s3cr3t")
    }

    #[test]
    fn saml_config_validates_ok() {
        assert!(saml_cfg().validate().is_ok());
    }

    #[test]
    fn saml_config_missing_entity_id_err() {
        let mut cfg = saml_cfg();
        cfg.entity_id = String::new();
        assert_eq!(cfg.validate().unwrap_err(), IdpError::MissingField("entity_id".into()));
    }

    #[test]
    fn oidc_config_validates_ok() {
        assert!(oidc_cfg().validate().is_ok());
    }

    #[test]
    fn oidc_config_missing_client_id_err() {
        let mut cfg = oidc_cfg();
        cfg.client_id = String::new();
        assert_eq!(cfg.validate().unwrap_err(), IdpError::MissingField("client_id".into()));
    }

    #[test]
    fn oidc_discovery_url() {
        let cfg = oidc_cfg();
        assert!(cfg.discovery_url().ends_with("/.well-known/openid-configuration"));
    }

    #[test]
    fn identity_provider_protocol_labels() {
        let saml = IdentityProvider::saml("corp-saml", saml_cfg());
        let oidc = IdentityProvider::oidc(oidc_cfg());
        assert_eq!(saml.protocol(), "saml2");
        assert_eq!(oidc.protocol(), "oidc");
    }

    #[test]
    fn idp_registry_register_and_count() {
        let mut reg = IdpRegistry::new();
        reg.register(IdentityProvider::oidc(oidc_cfg())).unwrap();
        assert_eq!(reg.count(), 1);
    }

    #[test]
    fn idp_registry_find_by_name() {
        let mut reg = IdpRegistry::new();
        let idp = IdentityProvider::saml("corp-saml", saml_cfg());
        reg.register(idp).unwrap();
        assert!(reg.find("corp-saml").is_some());
        assert!(reg.find("unknown").is_none());
    }

    #[test]
    fn idp_registry_enabled_filter() {
        let mut reg = IdpRegistry::new();
        reg.register(IdentityProvider::oidc(oidc_cfg()).with_enabled(true)).unwrap();
        let disabled_cfg = OidcConfig::new("https://other.example/oidc", "c", "s");
        reg.register(IdentityProvider::oidc(disabled_cfg).with_enabled(false)).unwrap();
        assert_eq!(reg.enabled_providers().len(), 1);
    }

    #[test]
    fn idp_registry_protocol_filter() {
        let mut reg = IdpRegistry::new();
        reg.register(IdentityProvider::saml("s1", saml_cfg())).unwrap();
        reg.register(IdentityProvider::oidc(oidc_cfg())).unwrap();
        assert_eq!(reg.providers_for_protocol(SsoProtocol::Saml2).len(), 1);
        assert_eq!(reg.providers_for_protocol(SsoProtocol::Oidc).len(), 1);
    }
}
