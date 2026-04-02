//! SAML 2.0 assertion parsing and attribute extraction.
//!
//! In a real deployment the assertions would be XML-signed; here we model
//! the post-validation domain objects and the parser state machine that
//! produces them from a (already-verified) assertion payload.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SamlError {
    #[error("assertion is expired")]
    Expired,
    #[error("signature verification failed")]
    SignatureInvalid,
    #[error("missing required attribute: {0}")]
    MissingAttribute(String),
    #[error("malformed assertion: {0}")]
    MalformedAssertion(String),
    #[error("audience mismatch: expected '{expected}', got '{got}'")]
    AudienceMismatch { expected: String, got: String },
    #[error("unknown name format: {0}")]
    UnknownNameFormat(String),
}

// ── Attribute ─────────────────────────────────────────────────────────────────

/// A single SAML attribute statement (name + multi-value).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SamlAttribute {
    pub name: String,
    pub values: Vec<String>,
}

impl SamlAttribute {
    pub fn new(name: impl Into<String>, values: Vec<impl Into<String>>) -> Self {
        Self {
            name: name.into(),
            values: values.into_iter().map(|v| v.into()).collect(),
        }
    }

    pub fn first(&self) -> Option<&str> { self.values.first().map(|s| s.as_str()) }
}

// ── Assertion ─────────────────────────────────────────────────────────────────

/// A validated and parsed SAML 2.0 assertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlAssertion {
    /// NameID — typically the user's email or UPN.
    pub subject: String,
    /// Issuer (IdP entity ID).
    pub issuer: String,
    /// Intended audience (SP entity ID).
    pub audience: String,
    /// Unix timestamp of `NotBefore`.
    pub not_before: u64,
    /// Unix timestamp of `NotOnOrAfter`.
    pub not_on_or_after: u64,
    /// All attribute statements.
    pub attributes: Vec<SamlAttribute>,
    /// Session index (IdP-assigned).
    pub session_index: Option<String>,
}

impl SamlAssertion {
    /// Look up the first value of a named attribute.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.iter().find(|a| a.name == name)?.first()
    }

    /// Convenience: extract the `email` attribute or fall back to `subject`.
    pub fn email(&self) -> &str {
        self.attribute("email")
            .or_else(|| self.attribute("http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress"))
            .unwrap_or(&self.subject)
    }

    /// Roles extracted from the `roles` or `groups` attribute (each value is one role).
    pub fn roles(&self) -> Vec<&str> {
        self.attributes
            .iter()
            .find(|a| a.name == "roles" || a.name == "groups")
            .map(|a| a.values.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Validate the assertion against the current time and expected audience.
    pub fn validate(&self, now_ts: u64, audience: &str) -> Result<(), SamlError> {
        if now_ts >= self.not_on_or_after {
            return Err(SamlError::Expired);
        }
        if self.audience != audience {
            return Err(SamlError::AudienceMismatch {
                expected: audience.to_string(),
                got: self.audience.clone(),
            });
        }
        Ok(())
    }
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// Parses a simplified SAML assertion encoded as a JSON object.
///
/// **Note**: In production this would parse signed XML; for the purposes of
/// this crate we use a JSON envelope so the tests remain self-contained
/// without pulling in heavy XML dependencies.
pub struct SamlParser {
    pub expected_audience: String,
    pub verify_signatures: bool,
}

impl SamlParser {
    pub fn new(expected_audience: impl Into<String>) -> Self {
        Self { expected_audience: expected_audience.into(), verify_signatures: true }
    }

    /// Parse a JSON-encoded assertion payload.
    pub fn parse(&self, payload: &str) -> Result<SamlAssertion, SamlError> {
        let v: serde_json::Value = serde_json::from_str(payload)
            .map_err(|e| SamlError::MalformedAssertion(e.to_string()))?;

        let subject = v["subject"].as_str()
            .ok_or_else(|| SamlError::MissingAttribute("subject".into()))?
            .to_string();
        let issuer = v["issuer"].as_str()
            .ok_or_else(|| SamlError::MissingAttribute("issuer".into()))?
            .to_string();
        let audience = v["audience"].as_str()
            .ok_or_else(|| SamlError::MissingAttribute("audience".into()))?
            .to_string();
        let not_before = v["not_before"].as_u64().unwrap_or(0);
        let not_on_or_after = v["not_on_or_after"].as_u64()
            .ok_or_else(|| SamlError::MissingAttribute("not_on_or_after".into()))?;
        let session_index = v["session_index"].as_str().map(|s| s.to_string());

        let mut attributes = Vec::new();
        if let Some(attrs) = v["attributes"].as_object() {
            for (k, val) in attrs {
                let values: Vec<String> = match val {
                    serde_json::Value::Array(a) => {
                        a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()
                    }
                    serde_json::Value::String(s) => vec![s.clone()],
                    _ => vec![val.to_string()],
                };
                attributes.push(SamlAttribute { name: k.clone(), values });
            }
        }

        Ok(SamlAssertion {
            subject,
            issuer,
            audience,
            not_before,
            not_on_or_after,
            attributes,
            session_index,
        })
    }

    /// Parse and validate in one step.
    pub fn parse_and_validate(&self, payload: &str, now_ts: u64) -> Result<SamlAssertion, SamlError> {
        let assertion = self.parse(payload)?;
        assertion.validate(now_ts, &self.expected_audience)?;
        Ok(assertion)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn assertion_json(not_on_or_after: u64) -> String {
        serde_json::json!({
            "subject": "user@corp.example",
            "issuer": "https://idp.corp.example/saml2",
            "audience": "https://logos.example/sp",
            "not_before": 1_000_000u64,
            "not_on_or_after": not_on_or_after,
            "session_index": "idx-abc",
            "attributes": {
                "email": "user@corp.example",
                "roles": ["publisher", "viewer"],
                "department": "Engineering"
            }
        }).to_string()
    }

    #[test]
    fn parse_valid_assertion() {
        let parser = SamlParser::new("https://logos.example/sp");
        let a = parser.parse(&assertion_json(9_999_999_999)).unwrap();
        assert_eq!(a.subject, "user@corp.example");
        assert_eq!(a.issuer, "https://idp.corp.example/saml2");
    }

    #[test]
    fn attribute_lookup() {
        let parser = SamlParser::new("https://logos.example/sp");
        let a = parser.parse(&assertion_json(9_999_999_999)).unwrap();
        assert_eq!(a.attribute("department"), Some("Engineering"));
    }

    #[test]
    fn email_falls_back_to_subject() {
        let json = serde_json::json!({
            "subject": "fallback@corp.example",
            "issuer": "https://idp.example",
            "audience": "sp",
            "not_on_or_after": 9_999_999_999u64,
            "attributes": {}
        }).to_string();
        let a = SamlParser::new("sp").parse(&json).unwrap();
        assert_eq!(a.email(), "fallback@corp.example");
    }

    #[test]
    fn roles_extracted() {
        let parser = SamlParser::new("https://logos.example/sp");
        let a = parser.parse(&assertion_json(9_999_999_999)).unwrap();
        let roles = a.roles();
        assert!(roles.contains(&"publisher"));
        assert!(roles.contains(&"viewer"));
    }

    #[test]
    fn expired_assertion_err() {
        let parser = SamlParser::new("https://logos.example/sp");
        let a = parser.parse(&assertion_json(100)).unwrap();
        let err = a.validate(200, "https://logos.example/sp").unwrap_err();
        assert_eq!(err, SamlError::Expired);
    }

    #[test]
    fn audience_mismatch_err() {
        let parser = SamlParser::new("https://logos.example/sp");
        let a = parser.parse(&assertion_json(9_999_999_999)).unwrap();
        let err = a.validate(1_000_001, "https://other.example/sp").unwrap_err();
        assert!(matches!(err, SamlError::AudienceMismatch { .. }));
    }

    #[test]
    fn parse_and_validate_valid() {
        let parser = SamlParser::new("https://logos.example/sp");
        assert!(parser.parse_and_validate(&assertion_json(9_999_999_999), 1_000_001).is_ok());
    }

    #[test]
    fn malformed_json_err() {
        let parser = SamlParser::new("sp");
        let err = parser.parse("not-json").unwrap_err();
        assert!(matches!(err, SamlError::MalformedAssertion(_)));
    }

    #[test]
    fn missing_subject_err() {
        let json = serde_json::json!({
            "issuer": "idp",
            "audience": "sp",
            "not_on_or_after": 9_999_999_999u64,
            "attributes": {}
        }).to_string();
        let err = SamlParser::new("sp").parse(&json).unwrap_err();
        assert_eq!(err, SamlError::MissingAttribute("subject".into()));
    }

    #[test]
    fn saml_attribute_multi_value() {
        let attr = SamlAttribute::new("groups", vec!["admin", "editor"]);
        assert_eq!(attr.first(), Some("admin"));
        assert_eq!(attr.values.len(), 2);
    }
}
