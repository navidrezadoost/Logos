//! # report
//!
//! Migration report types and helpers.

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportEntry {
    pub severity: Severity,
    pub message: String,
    pub element_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReport {
    pub source_format: String,
    pub total_layers: usize,
    pub converted_layers: usize,
    pub warnings: usize,
    pub errors: usize,
    pub entries: Vec<ReportEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_entry_serializes() {
        let entry = ReportEntry {
            severity: Severity::Warning,
            message: "Test warning".to_string(),
            element_id: Some("layer1".to_string()),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("Test warning"));
    }

    #[test]
    fn migration_report_counts() {
        let report = MigrationReport {
            source_format: "Sketch".to_string(),
            total_layers: 10,
            converted_layers: 9,
            warnings: 1,
            errors: 0,
            entries: vec![ReportEntry {
                severity: Severity::Warning,
                message: "Missing font".to_string(),
                element_id: None,
            }],
        };
        assert_eq!(report.warnings, 1);
        assert_eq!(report.errors, 0);
    }
}
