//! # batch
//!
//! Batch migration: recursively scan directories, convert all supported files.

use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use crate::{wizard::MigrationWizard, MigrationResult, MigrationError, Result, SourceFormatKind};

/// Batch migration configuration.
#[derive(Debug)]
pub struct BatchConfig {
    /// Root directory to scan.
    pub root_dir: PathBuf,
    /// Output directory for migrated .gos files.
    pub output_dir: PathBuf,
    /// If true, scan recursively.
    pub recursive: bool,
    /// Wizard config for each migration.
    pub wizard: MigrationWizard,
}

/// Result of a batch migration.
#[derive(Debug)]
pub struct BatchResult {
    pub migrated: Vec<(PathBuf, MigrationResult)>,
    pub failed: Vec<(PathBuf, MigrationError)>,
}

impl BatchConfig {
    /// Run the batch migration.
    pub fn run(&self) -> BatchResult {
        let mut migrated = Vec::new();
        let mut failed = Vec::new();
        let walker = if self.recursive {
            WalkDir::new(&self.root_dir).into_iter()
        } else {
            WalkDir::new(&self.root_dir).max_depth(1).into_iter()
        };
        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if let Some(fmt) = SourceFormatKind::from_extension(ext) {
                        match self.wizard.migrate_file(path) {
                            Ok(result) => {
                                // Write .gos file
                                let out_path = self.output_dir.join(
                                    path.file_stem().unwrap_or_default()
                                ).with_extension("gos");
                                if let Err(e) = fs::write(&out_path, result.snapshot.to_json().unwrap_or_default()) {
                                    failed.push((path.to_path_buf(), MigrationError::Io(e)));
                                } else {
                                    migrated.push((path.to_path_buf(), result));
                                }
                            }
                            Err(e) => failed.push((path.to_path_buf(), e)),
                        }
                    }
                }
            }
        }
        BatchResult { migrated, failed }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn batch_skips_non_design_files() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("not_a_design.txt");
        File::create(&file_path).unwrap();
        let cfg = BatchConfig {
            root_dir: dir.path().to_path_buf(),
            output_dir: dir.path().to_path_buf(),
            recursive: false,
            wizard: MigrationWizard::new(),
        };
        let result = cfg.run();
        assert_eq!(result.migrated.len(), 0);
        assert_eq!(result.failed.len(), 0);
    }
}
