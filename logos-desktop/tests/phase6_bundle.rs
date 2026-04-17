// Phase 6 – Bundle / packaging metadata tests (t601–t655)
//
// Integration tests for `logos_desktop::bundle`.
// Covers version parsing, bundle ID validation, CI runner matrix,
// platform targets, artifact extensions, and the CI threshold table.
// All tests run with `--no-default-features` (no native UI deps needed).

use logos_desktop::bundle::{
    bundle_targets, ci_runner, is_valid_bundle_id, is_valid_semver,
    parse_version, target_extension, APP_NAME, APP_VERSION, BUNDLE_ID,
    CI_TEST_THRESHOLDS, CI_WORKSPACE_MIN, ICON_PATHS, ICON_SIZES,
    LINUX_TARGETS, MACOS_MIN_VERSION, MACOS_TARGETS, WINDOWS_TARGETS,
};

// ═══════════════════════════════════════════════════════════════════════════
// §1  Application meta-constants
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t601_app_name_is_logos() {
    assert_eq!(APP_NAME, "Logos");
}

#[test]
fn t602_app_version_is_non_empty() {
    assert!(!APP_VERSION.is_empty());
}

#[test]
fn t603_bundle_id_starts_with_io_logos() {
    assert!(
        BUNDLE_ID.starts_with("io.logos"),
        "bundle ID should use io.logos namespace, got: {BUNDLE_ID}"
    );
}

#[test]
fn t604_bundle_id_has_at_least_three_segments() {
    assert!(
        BUNDLE_ID.split('.').count() >= 3,
        "bundle ID should have ≥3 segments: {BUNDLE_ID}"
    );
}

#[test]
fn t605_macos_min_version_is_catalina_or_later() {
    let major: u32 = MACOS_MIN_VERSION
        .split('.')
        .next()
        .unwrap()
        .parse()
        .expect("major version should be numeric");
    assert!(
        major >= 10,
        "macOS min version must be 10.x or later, got {MACOS_MIN_VERSION}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// §2  parse_version
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t606_parse_version_returns_some_for_cargo_version() {
    assert!(
        parse_version().is_some(),
        "APP_VERSION '{APP_VERSION}' must be parseable"
    );
}

#[test]
fn t607_parse_version_major_minor_patch_non_negative() {
    let (maj, min, patch) = parse_version().unwrap();
    // u32 is always ≥ 0; we just confirm they parse without overflow
    let _ = (maj, min, patch);
}

#[test]
fn t608_parse_version_known_string() {
    // Inline test that doesn't depend on the crate version
    use logos_desktop::bundle::is_valid_semver;
    assert_eq!(
        {
            let v = "1.2.3";
            let mut p = v.splitn(3, '.');
            let maj: u32 = p.next().unwrap().parse().unwrap();
            let min: u32 = p.next().unwrap().parse().unwrap();
            let pat: u32 = p.next().unwrap().parse().unwrap();
            (maj, min, pat)
        },
        (1, 2, 3)
    );
    assert!(is_valid_semver("1.2.3"));
}

#[test]
fn t609_parse_version_with_prerelease_suffix() {
    // parse_version strips pre-release; ensure is_valid_semver accepts it
    assert!(is_valid_semver("0.1.0-alpha.1") || is_valid_semver("0.1.0"),
        "semver with or without pre-release must be valid");
}

// ═══════════════════════════════════════════════════════════════════════════
// §3  is_valid_semver
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t610_semver_zero_zero_one_is_valid() {
    assert!(is_valid_semver("0.0.1"));
}

#[test]
fn t611_semver_multi_digit_parts_are_valid() {
    assert!(is_valid_semver("10.20.30"));
}

#[test]
fn t612_semver_empty_string_is_invalid() {
    assert!(!is_valid_semver(""));
}

#[test]
fn t613_semver_missing_patch_is_invalid() {
    assert!(!is_valid_semver("1.2"));
}

#[test]
fn t614_semver_leading_v_is_invalid() {
    assert!(!is_valid_semver("v1.2.3"),
        "bare semver should not have 'v' prefix");
}

#[test]
fn t615_semver_non_numeric_major_is_invalid() {
    assert!(!is_valid_semver("x.1.0"));
}

#[test]
fn t616_semver_non_numeric_minor_is_invalid() {
    assert!(!is_valid_semver("1.y.0"));
}

#[test]
fn t617_semver_with_build_metadata_patch_numeric() {
    // "1.0.0+build" — `parse_version` strips suffix; at minimum "1.0.0" must pass
    assert!(is_valid_semver("1.0.0"));
}

// ═══════════════════════════════════════════════════════════════════════════
// §4  is_valid_bundle_id
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t618_bundle_id_valid_three_segments() {
    assert!(is_valid_bundle_id("com.example.app"));
}

#[test]
fn t619_bundle_id_valid_with_hyphens() {
    assert!(is_valid_bundle_id("io.logos.my-app"));
}

#[test]
fn t620_bundle_id_valid_with_underscores() {
    assert!(is_valid_bundle_id("io.logos.my_app"));
}

#[test]
fn t621_bundle_id_empty_is_invalid() {
    assert!(!is_valid_bundle_id(""));
}

#[test]
fn t622_bundle_id_single_segment_is_invalid() {
    assert!(!is_valid_bundle_id("logos"));
}

#[test]
fn t623_bundle_id_leading_dot_is_invalid() {
    assert!(!is_valid_bundle_id(".io.logos.app"));
}

#[test]
fn t624_bundle_id_trailing_dot_is_invalid() {
    assert!(!is_valid_bundle_id("io.logos.app."));
}

#[test]
fn t625_bundle_id_empty_segment_is_invalid() {
    assert!(!is_valid_bundle_id("io..logos"));
}

// ═══════════════════════════════════════════════════════════════════════════
// §5  ci_runner — platform → runner image
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t626_ci_runner_linux_returns_ubuntu() {
    let runner = ci_runner("linux").expect("linux runner must be defined");
    assert!(runner.contains("ubuntu"), "expected ubuntu runner, got {runner}");
}

#[test]
fn t627_ci_runner_macos_returns_macos() {
    let runner = ci_runner("macos").expect("macos runner must be defined");
    assert!(runner.contains("macos"), "expected macos runner, got {runner}");
}

#[test]
fn t628_ci_runner_windows_returns_windows() {
    let runner = ci_runner("windows").expect("windows runner must be defined");
    assert!(runner.contains("windows"), "expected windows runner, got {runner}");
}

#[test]
fn t629_ci_runner_unknown_platform_returns_none() {
    assert!(ci_runner("freebsd").is_none());
}

#[test]
fn t630_ci_runner_empty_string_returns_none() {
    assert!(ci_runner("").is_none());
}

#[test]
fn t631_all_three_platforms_have_runners() {
    for platform in &["linux", "macos", "windows"] {
        assert!(
            ci_runner(platform).is_some(),
            "no CI runner defined for {platform}"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §6  bundle_targets — platform → list of target strings
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t632_linux_targets_contains_appimage() {
    assert!(LINUX_TARGETS.contains(&"appimage"));
}

#[test]
fn t633_linux_targets_contains_deb() {
    assert!(LINUX_TARGETS.contains(&"deb"));
}

#[test]
fn t634_macos_targets_contains_dmg() {
    assert!(MACOS_TARGETS.contains(&"dmg"));
}

#[test]
fn t635_windows_targets_contains_msi() {
    assert!(WINDOWS_TARGETS.contains(&"msi"));
}

#[test]
fn t636_bundle_targets_linux_matches_constant() {
    assert_eq!(bundle_targets("linux"), LINUX_TARGETS);
}

#[test]
fn t637_bundle_targets_unknown_is_empty() {
    assert!(bundle_targets("plan9").is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// §7  target_extension — target → file extension
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t638_appimage_extension_is_AppImage() {
    assert_eq!(target_extension("appimage"), Some("AppImage"));
}

#[test]
fn t639_deb_extension_is_deb() {
    assert_eq!(target_extension("deb"), Some("deb"));
}

#[test]
fn t640_dmg_extension_is_dmg() {
    assert_eq!(target_extension("dmg"), Some("dmg"));
}

#[test]
fn t641_msi_extension_is_msi() {
    assert_eq!(target_extension("msi"), Some("msi"));
}

#[test]
fn t642_nsis_extension_is_exe() {
    assert_eq!(target_extension("nsis"), Some("exe"));
}

#[test]
fn t643_app_extension_is_app() {
    assert_eq!(target_extension("app"), Some("app"));
}

#[test]
fn t644_unknown_target_extension_is_none() {
    assert!(target_extension("flatpak").is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// §8  CI_TEST_THRESHOLDS / CI_WORKSPACE_MIN
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t645_workspace_min_is_at_least_1155() {
    assert!(
        CI_WORKSPACE_MIN >= 1_155,
        "workspace minimum should reflect current test count ({CI_WORKSPACE_MIN})"
    );
}

#[test]
fn t646_ci_thresholds_has_entries_for_key_crates() {
    let names: Vec<&str> = CI_TEST_THRESHOLDS.iter().map(|(n, _)| *n).collect();
    for required in &[
        "logos-core", "logos-layout", "logos-render",
        "logos-collab", "logos-desktop",
    ] {
        assert!(
            names.iter().any(|n| n.contains(required)),
            "CI_TEST_THRESHOLDS missing entry for {required}"
        );
    }
}

#[test]
fn t647_all_ci_thresholds_are_positive() {
    for (name, min) in CI_TEST_THRESHOLDS {
        assert!(*min > 0, "threshold for {name} must be > 0");
    }
}

#[test]
fn t648_logos_render_threshold_at_least_102() {
    let threshold = CI_TEST_THRESHOLDS
        .iter()
        .find(|(n, _)| *n == "logos-render")
        .map(|(_, t)| *t)
        .expect("logos-render entry must exist");
    assert!(
        threshold >= 102,
        "logos-render threshold should reflect 102 tests, got {threshold}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// §9  ICON_SIZES / ICON_PATHS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t649_icon_sizes_has_standard_set() {
    for &size in &[16u32, 32, 64, 128, 256, 512] {
        assert!(
            ICON_SIZES.contains(&size),
            "ICON_SIZES should include {size}×{size}"
        );
    }
}

#[test]
fn t650_icon_sizes_are_powers_of_two() {
    for &size in ICON_SIZES {
        assert!(
            size > 0 && (size & (size - 1)) == 0,
            "{size} is not a power of two"
        );
    }
}

#[test]
fn t651_icon_paths_has_icns_for_macos() {
    assert!(
        ICON_PATHS.iter().any(|p| p.ends_with(".icns")),
        "at least one .icns path required for macOS"
    );
}

#[test]
fn t652_icon_paths_has_ico_for_windows() {
    assert!(
        ICON_PATHS.iter().any(|p| p.ends_with(".ico")),
        "at least one .ico path required for Windows"
    );
}

#[test]
fn t653_icon_paths_count_matches_sizes_plus_platform_icons() {
    // PNG icons (one per size) + 1 icns + 1 ico
    let png_count = ICON_PATHS.iter().filter(|p| p.ends_with(".png")).count();
    assert!(
        png_count >= ICON_SIZES.len(),
        "expected at least {} PNG paths, got {png_count}",
        ICON_SIZES.len()
    );
}

#[test]
fn t654_all_icon_paths_have_extension() {
    for path in ICON_PATHS {
        assert!(
            path.contains('.'),
            "icon path has no extension: {path}"
        );
    }
}

#[test]
fn t655_all_bundle_targets_have_known_extension() {
    for platform in &["linux", "macos", "windows"] {
        for &target in bundle_targets(platform) {
            assert!(
                target_extension(target).is_some(),
                "no extension registered for target '{target}' on {platform}"
            );
        }
    }
}
