// logos-desktop/src/lib.rs — library entry point (for integration tests)
//
//  Pure-data modules (no GTK / wgpu / winit) are always compiled.
//  Native-desktop modules are gated on the `desktop-ui` feature so that
//  `cargo test --no-default-features` works without GTK headers installed.

// ── Always available ─────────────────────────────────────────────────────────
pub mod commands;
pub mod panels;
pub mod variants;
pub mod undo;
pub mod accessibility;
pub mod bundle;
pub mod interaction;
// ── Desktop client UI state (pure-data, no GTK/wgpu) ─────────────────────────
pub mod server_connect;
pub mod login;
pub mod session_state;
pub mod company_hub;
pub mod project_browser;
pub mod conflict_reviewer;
pub mod admin_panel;

// ── Require desktop-ui feature (GTK / wgpu / winit) ─────────────────────────
#[cfg(feature = "desktop-ui")]
pub mod presence;
#[cfg(feature = "desktop-ui")]
pub mod file_io;
#[cfg(feature = "desktop-ui")]
pub mod shortcuts;
#[cfg(feature = "desktop-ui")]
pub mod toolbar;
#[cfg(feature = "desktop-ui")]
pub mod palette;
#[cfg(feature = "desktop-ui")]
pub mod tabs;
#[cfg(feature = "desktop-ui")]
pub mod menus;
#[cfg(feature = "desktop-ui")]
pub mod dialogs;
#[cfg(feature = "desktop-ui")]
pub mod tray;
#[cfg(feature = "desktop-ui")]
pub mod updater;
#[cfg(feature = "desktop-ui")]
pub mod marketplace;
