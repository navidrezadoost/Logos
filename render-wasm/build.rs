// We need this script so OUT_DIR is automatically set and we can build
// the macros crate.
fn main() {
    // Skia's bundled textlayout build produces libharfbuzz.a, but
    // skia-bindings 0.87.0 does not emit a native link directive for it.
    // Link the bundled archive for native targets only; WebAssembly is
    // handled by its own target-specific renderer configuration.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux")
        && std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64")
    {
        println!("cargo:rustc-link-lib=static=harfbuzz");
    }
}
