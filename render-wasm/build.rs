use std::env;

// We need this script so OUT_DIR is automatically set and we can build
// the macros crate.
fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    if target_os == "linux" && target_arch == "x86_64" {
        // Preserve the native Linux fix.
        println!("cargo:rustc-link-lib=static=harfbuzz");
    } else if target_os == "emscripten" && target_arch == "wasm32" {
        // Skia's Emscripten build produces the archive, and its build script
        // propagates the native search path to dependent crates.
        println!("cargo:rustc-link-lib=static=harfbuzz");
    }
}
