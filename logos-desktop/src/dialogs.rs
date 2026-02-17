// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/dialogs.rs — Native file dialogs via `rfd`
//
//  Provides native open/save/export dialogs using the `rfd` (Rusty File
//  Dialog) crate.  All dialogs are synchronous (blocking) since the
//  desktop app runs on a dedicated render thread and the file I/O is
//  fast enough to not require async.
//
//  This module wires into the existing `file_io` module for the actual
//  save/load logic.

use std::path::{Path, PathBuf};

use log::{debug, info, warn};
use rfd::FileDialog;

use crate::commands::ExportFormat;
use crate::file_io;

// ── Filter Constants ────────────────────────────────────────────

/// File extension for native Logos documents.
const LOGOS_EXTENSION: &str = "logos";

/// Human-readable name for the Logos file type.
const LOGOS_FILTER_NAME: &str = "Logos Document";

// ── Dialog Results ──────────────────────────────────────────────

/// Outcome of a dialog that may require user decisions.
#[derive(Debug, Clone, PartialEq)]
pub enum DialogResult {
    /// User picked a path.
    Selected(PathBuf),
    /// User cancelled the dialog.
    Cancelled,
}

/// Outcome of a save-before-close prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavePromptResult {
    /// User chose to save first.
    Save,
    /// User chose to discard changes.
    DontSave,
    /// User cancelled (stay in the document).
    Cancel,
}

/// Outcome of a confirmation dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmResult {
    Yes,
    No,
    Cancel,
}

// ── Open Dialog ─────────────────────────────────────────────────

/// Show a native "Open File" dialog filtered to `.logos` files.
///
/// Returns `DialogResult::Selected(path)` if the user picks a file,
/// or `DialogResult::Cancelled` if they dismiss the dialog.
pub fn open_document_dialog(start_dir: Option<&Path>) -> DialogResult {
    let mut dialog = FileDialog::new()
        .set_title("Open Logos Document")
        .add_filter(LOGOS_FILTER_NAME, &[LOGOS_EXTENSION]);

    if let Some(dir) = start_dir {
        dialog = dialog.set_directory(dir);
    } else {
        dialog = dialog.set_directory(&file_io::default_save_dir());
    }

    match dialog.pick_file() {
        Some(path) => {
            info!("Open dialog: selected {}", path.display());
            DialogResult::Selected(path)
        }
        None => {
            debug!("Open dialog: cancelled");
            DialogResult::Cancelled
        }
    }
}

/// Show a native "Open File" dialog for importing foreign formats.
///
/// Supports SVG, JSON, and common image formats.
pub fn import_dialog(start_dir: Option<&Path>) -> DialogResult {
    let mut dialog = FileDialog::new()
        .set_title("Import File")
        .add_filter("SVG Files", &["svg"])
        .add_filter("JSON Files", &["json"])
        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
        .add_filter("All Files", &["*"]);

    if let Some(dir) = start_dir {
        dialog = dialog.set_directory(dir);
    }

    match dialog.pick_file() {
        Some(path) => {
            info!("Import dialog: selected {}", path.display());
            DialogResult::Selected(path)
        }
        None => {
            debug!("Import dialog: cancelled");
            DialogResult::Cancelled
        }
    }
}

// ── Save Dialog ─────────────────────────────────────────────────

/// Show a native "Save As" dialog for `.logos` files.
///
/// `suggested_name` is the default filename (without extension).
pub fn save_as_dialog(
    suggested_name: &str,
    start_dir: Option<&Path>,
) -> DialogResult {
    let filename = if suggested_name.ends_with(&format!(".{}", LOGOS_EXTENSION)) {
        suggested_name.to_string()
    } else {
        format!("{}.{}", suggested_name, LOGOS_EXTENSION)
    };

    let mut dialog = FileDialog::new()
        .set_title("Save Logos Document")
        .set_file_name(&filename)
        .add_filter(LOGOS_FILTER_NAME, &[LOGOS_EXTENSION]);

    if let Some(dir) = start_dir {
        dialog = dialog.set_directory(dir);
    } else {
        dialog = dialog.set_directory(&file_io::default_save_dir());
    }

    match dialog.save_file() {
        Some(path) => {
            info!("Save As dialog: selected {}", path.display());
            DialogResult::Selected(path)
        }
        None => {
            debug!("Save As dialog: cancelled");
            DialogResult::Cancelled
        }
    }
}

// ── Export Dialog ────────────────────────────────────────────────

/// Show a native "Export" dialog for the specified format.
///
/// The dialog automatically sets the correct file extension and filter.
pub fn export_dialog(
    format: ExportFormat,
    suggested_name: &str,
    start_dir: Option<&Path>,
) -> DialogResult {
    let (title, filter_name, extension) = match format {
        ExportFormat::Png => ("Export as PNG", "PNG Image", "png"),
        ExportFormat::Svg => ("Export as SVG", "SVG Vector", "svg"),
        ExportFormat::Pdf => ("Export as PDF", "PDF Document", "pdf"),
        ExportFormat::Json => ("Export as JSON", "JSON Data", "json"),
    };

    let filename = format!("{}.{}", suggested_name, extension);

    let mut dialog = FileDialog::new()
        .set_title(title)
        .set_file_name(&filename)
        .add_filter(filter_name, &[extension]);

    if let Some(dir) = start_dir {
        dialog = dialog.set_directory(dir);
    }

    match dialog.save_file() {
        Some(path) => {
            info!("Export dialog ({}): selected {}", format, path.display());
            DialogResult::Selected(path)
        }
        None => {
            debug!("Export dialog ({}): cancelled", format);
            DialogResult::Cancelled
        }
    }
}

// ── Folder Picker ───────────────────────────────────────────────

/// Show a native folder picker dialog.
pub fn pick_folder_dialog(
    title: &str,
    start_dir: Option<&Path>,
) -> DialogResult {
    let mut dialog = FileDialog::new().set_title(title);

    if let Some(dir) = start_dir {
        dialog = dialog.set_directory(dir);
    }

    match dialog.pick_folder() {
        Some(path) => {
            info!("Folder dialog: selected {}", path.display());
            DialogResult::Selected(path)
        }
        None => {
            debug!("Folder dialog: cancelled");
            DialogResult::Cancelled
        }
    }
}

// ── Dialog Manager ──────────────────────────────────────────────

/// Manages dialog state and provides high-level dialog flows.
///
/// Tracks the last-used directory for each dialog type so subsequent
/// opens start where the user left off.
pub struct DialogManager {
    /// Last directory used for opening files.
    last_open_dir: Option<PathBuf>,
    /// Last directory used for saving files.
    last_save_dir: Option<PathBuf>,
    /// Last directory used for exporting.
    last_export_dir: Option<PathBuf>,
}

impl DialogManager {
    pub fn new() -> Self {
        Self {
            last_open_dir: None,
            last_save_dir: None,
            last_export_dir: None,
        }
    }

    /// Open document dialog, tracking the directory.
    pub fn open(&mut self) -> DialogResult {
        let result = open_document_dialog(self.last_open_dir.as_deref());
        if let DialogResult::Selected(ref path) = result {
            self.last_open_dir = path.parent().map(|p| p.to_path_buf());
        }
        result
    }

    /// Save-as dialog, tracking the directory.
    pub fn save_as(&mut self, suggested_name: &str) -> DialogResult {
        let result = save_as_dialog(suggested_name, self.last_save_dir.as_deref());
        if let DialogResult::Selected(ref path) = result {
            self.last_save_dir = path.parent().map(|p| p.to_path_buf());
        }
        result
    }

    /// Export dialog, tracking the directory.
    pub fn export(
        &mut self,
        format: ExportFormat,
        suggested_name: &str,
    ) -> DialogResult {
        let result = export_dialog(format, suggested_name, self.last_export_dir.as_deref());
        if let DialogResult::Selected(ref path) = result {
            self.last_export_dir = path.parent().map(|p| p.to_path_buf());
        }
        result
    }

    /// Import dialog, tracking the directory.
    pub fn import(&mut self) -> DialogResult {
        let result = import_dialog(self.last_open_dir.as_deref());
        if let DialogResult::Selected(ref path) = result {
            self.last_open_dir = path.parent().map(|p| p.to_path_buf());
        }
        result
    }

    /// Get the last open directory.
    pub fn last_open_directory(&self) -> Option<&Path> {
        self.last_open_dir.as_deref()
    }

    /// Get the last save directory.
    pub fn last_save_directory(&self) -> Option<&Path> {
        self.last_save_dir.as_deref()
    }

    /// Get the last export directory.
    pub fn last_export_directory(&self) -> Option<&Path> {
        self.last_export_dir.as_deref()
    }

    /// Set initial directories from a previously saved path.
    pub fn set_working_directory(&mut self, dir: &Path) {
        self.last_open_dir = Some(dir.to_path_buf());
        self.last_save_dir = Some(dir.to_path_buf());
    }
}

// ── Format Helpers ──────────────────────────────────────────────

/// Detect `ExportFormat` from a file extension.
pub fn format_from_extension(path: &Path) -> Option<ExportFormat> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| match ext.to_lowercase().as_str() {
            "png" => Some(ExportFormat::Png),
            "svg" => Some(ExportFormat::Svg),
            "pdf" => Some(ExportFormat::Pdf),
            "json" => Some(ExportFormat::Json),
            _ => None,
        })
}

/// Check if a path has the `.logos` extension.
pub fn is_logos_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case(LOGOS_EXTENSION))
        .unwrap_or(false)
}

/// Ensure a path has the `.logos` extension; append if missing.
pub fn ensure_logos_extension(path: &Path) -> PathBuf {
    if is_logos_file(path) {
        path.to_path_buf()
    } else {
        path.with_extension(LOGOS_EXTENSION)
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_result_equality() {
        let a = DialogResult::Selected(PathBuf::from("/tmp/test.logos"));
        let b = DialogResult::Selected(PathBuf::from("/tmp/test.logos"));
        assert_eq!(a, b);
        assert_ne!(a, DialogResult::Cancelled);
    }

    #[test]
    fn save_prompt_result_values() {
        assert_ne!(SavePromptResult::Save, SavePromptResult::DontSave);
        assert_ne!(SavePromptResult::Save, SavePromptResult::Cancel);
        assert_ne!(SavePromptResult::DontSave, SavePromptResult::Cancel);
    }

    #[test]
    fn confirm_result_values() {
        assert_ne!(ConfirmResult::Yes, ConfirmResult::No);
        assert_ne!(ConfirmResult::Yes, ConfirmResult::Cancel);
    }

    #[test]
    fn format_from_extension_png() {
        let path = PathBuf::from("image.png");
        assert_eq!(format_from_extension(&path), Some(ExportFormat::Png));
    }

    #[test]
    fn format_from_extension_svg() {
        let path = PathBuf::from("drawing.svg");
        assert_eq!(format_from_extension(&path), Some(ExportFormat::Svg));
    }

    #[test]
    fn format_from_extension_pdf() {
        let path = PathBuf::from("output.pdf");
        assert_eq!(format_from_extension(&path), Some(ExportFormat::Pdf));
    }

    #[test]
    fn format_from_extension_json() {
        let path = PathBuf::from("data.json");
        assert_eq!(format_from_extension(&path), Some(ExportFormat::Json));
    }

    #[test]
    fn format_from_extension_unknown() {
        let path = PathBuf::from("file.bmp");
        assert_eq!(format_from_extension(&path), None);
    }

    #[test]
    fn format_from_extension_case_insensitive() {
        let path = PathBuf::from("image.PNG");
        assert_eq!(format_from_extension(&path), Some(ExportFormat::Png));
    }

    #[test]
    fn format_from_extension_no_extension() {
        let path = PathBuf::from("noext");
        assert_eq!(format_from_extension(&path), None);
    }

    #[test]
    fn is_logos_file_true() {
        assert!(is_logos_file(Path::new("test.logos")));
        assert!(is_logos_file(Path::new("/home/user/docs/design.logos")));
    }

    #[test]
    fn is_logos_file_false() {
        assert!(!is_logos_file(Path::new("test.png")));
        assert!(!is_logos_file(Path::new("noext")));
    }

    #[test]
    fn is_logos_file_case_insensitive() {
        assert!(is_logos_file(Path::new("test.LOGOS")));
        assert!(is_logos_file(Path::new("test.Logos")));
    }

    #[test]
    fn ensure_logos_extension_adds() {
        let path = Path::new("design");
        assert_eq!(ensure_logos_extension(path), PathBuf::from("design.logos"));
    }

    #[test]
    fn ensure_logos_extension_preserves() {
        let path = Path::new("design.logos");
        assert_eq!(ensure_logos_extension(path), PathBuf::from("design.logos"));
    }

    #[test]
    fn ensure_logos_extension_replaces_other() {
        let path = Path::new("design.png");
        assert_eq!(ensure_logos_extension(path), PathBuf::from("design.logos"));
    }

    #[test]
    fn dialog_manager_initial_state() {
        let mgr = DialogManager::new();
        assert!(mgr.last_open_directory().is_none());
        assert!(mgr.last_save_directory().is_none());
        assert!(mgr.last_export_directory().is_none());
    }

    #[test]
    fn dialog_manager_set_working_directory() {
        let mut mgr = DialogManager::new();
        mgr.set_working_directory(Path::new("/tmp/project"));
        assert_eq!(mgr.last_open_directory(), Some(Path::new("/tmp/project")));
        assert_eq!(mgr.last_save_directory(), Some(Path::new("/tmp/project")));
        // Export directory is not set by working directory
        assert!(mgr.last_export_directory().is_none());
    }
}
