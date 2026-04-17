// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/bundle.rs — Packaging & release metadata
//
//  Pure-data module: no native UI deps, always compiled.  Provides the
//  single source of truth for the package identity, bundle targets, icon
//  paths, and build-matrix configuration used by both the CI workflow and
//  any self-update logic.

/// Application version taken from the Cargo manifest at compile time.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Human-readable application name for window titles and installers.
pub const APP_NAME: &str = "Logos";

/// Reverse-DNS identifier used for `.app` bundles, `.deb` packages, etc.
pub const BUNDLE_ID: &str = "io.logos.desktop";

/// Minimum supported macOS version for the `.app` bundle.
pub const MACOS_MIN_VERSION: &str = "10.15";

/// Default installer targets produced for each platform.
///
/// Linux  → AppImage + DEB
/// macOS  → DMG + APP
/// Windows → MSI + NSIS
pub const LINUX_TARGETS:   &[&str] = &["appimage", "deb"];
pub const MACOS_TARGETS:   &[&str] = &["dmg", "app"];
pub const WINDOWS_TARGETS: &[&str] = &["msi", "nsis"];

/// All supported icon sizes that must be present in the release bundle.
pub const ICON_SIZES: &[u32] = &[16, 32, 64, 128, 256, 512];

/// Relative (from the crate root) paths to required icon assets.
pub const ICON_PATHS: &[&str] = &[
    "assets/icons/16x16.png",
    "assets/icons/32x32.png",
    "assets/icons/64x64.png",
    "assets/icons/128x128.png",
    "assets/icons/256x256.png",
    "assets/icons/512x512.png",
    "assets/icons/icon.icns",
    "assets/icons/icon.ico",
];

/// CI minimum-test-count thresholds used by the workspace CI workflow.
///
/// Each entry is `(crate_name, minimum_passing_tests)`.  The CI script
/// fails if any crate falls below its threshold, preventing silent test
/// regressions from landing.
pub const CI_TEST_THRESHOLDS: &[(&str, u32)] = &[
    ("logos-core",              47),
    ("logos-layout",            59),
    ("logos-text",              48),
    ("logos-render",           102),   // 62 original + 39 gpu_verify + 1 doc
    ("logos-collab",           280),   // 213 original + 67 new
    ("logos-plugins",          596),
    ("logos-desktop",          297),   // 242 original + 55 phase5
    ("logos-ai",               235),
    ("logos-import-*",         139),
    ("logos-marketplace-*",     95),
];

/// Workspace-wide minimum.  The CI fails if total passing < this value.
pub const CI_WORKSPACE_MIN: u32 = 1_155;

/// Semantic version parsed into `(major, minor, patch)`.
///
/// Returns `None` if `APP_VERSION` does not conform to semver.
pub fn parse_version() -> Option<(u32, u32, u32)> {
    let mut parts = APP_VERSION.splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    // Strip any pre-release suffix (e.g. "0-alpha.1" → 0)
    let patch_str = parts.next()?;
    let patch: u32 = patch_str
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some((major, minor, patch))
}

/// Returns `true` when `version_str` is a valid semver string.
pub fn is_valid_semver(version_str: &str) -> bool {
    let mut parts = version_str.splitn(3, '.');
    let ok = parts
        .next()
        .and_then(|p| p.parse::<u32>().ok())
        .is_some();
    if !ok { return false; }
    let ok = parts
        .next()
        .and_then(|p| p.parse::<u32>().ok())
        .is_some();
    if !ok { return false; }
    parts.next().map(|p| {
        // patch may have pre-release suffix
        p.split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .is_some()
    }).unwrap_or(false)
}

/// Returns `true` when `bundle_id` looks like a valid reverse-DNS identifier.
pub fn is_valid_bundle_id(bundle_id: &str) -> bool {
    if bundle_id.is_empty() { return false; }
    let parts: Vec<&str> = bundle_id.split('.').collect();
    if parts.len() < 2 { return false; }
    parts.iter().all(|seg| {
        !seg.is_empty()
            && seg.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            && !seg.starts_with('-')
    })
}

/// Returns the GitHub Actions runner image for a given platform key.
///
/// Recognised keys: `"linux"`, `"macos"`, `"windows"`.
pub fn ci_runner(platform: &str) -> Option<&'static str> {
    match platform {
        "linux"   => Some("ubuntu-22.04"),
        "macos"   => Some("macos-14"),
        "windows" => Some("windows-2022"),
        _         => None,
    }
}

/// Returns the list of bundle targets for a given platform key.
pub fn bundle_targets(platform: &str) -> &'static [&'static str] {
    match platform {
        "linux"   => LINUX_TARGETS,
        "macos"   => MACOS_TARGETS,
        "windows" => WINDOWS_TARGETS,
        _         => &[],
    }
}

/// Artifact file extension for a given bundle target.
pub fn target_extension(target: &str) -> Option<&'static str> {
    match target {
        "appimage" => Some("AppImage"),
        "deb"      => Some("deb"),
        "dmg"      => Some("dmg"),
        "app"      => Some("app"),
        "msi"      => Some("msi"),
        "nsis"     => Some("exe"),
        _          => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_version_is_semver() {
        assert!(is_valid_semver(APP_VERSION), "APP_VERSION must be semver");
    }

    #[test]
    fn bundle_id_passes_validation() {
        assert!(is_valid_bundle_id(BUNDLE_ID));
    }
}
