use std::{env, fs, path::Path, process::Command};

// We need this script so OUT_DIR is automatically set and we can build
// the macros crate.
fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    if target_os == "linux" && target_arch == "x86_64" {
        // Preserve the native Linux fix.
        println!("cargo:rustc-link-lib=static=harfbuzz");
    } else if target_os == "emscripten" && target_arch == "wasm32" {
        // Skia builds this archive with emar for the current target, but
        // skia-bindings 0.87.0 omits it from its Cargo link directives.
        link_emscripten_harfbuzz();
    }
}

fn link_emscripten_harfbuzz() {
    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR is not set");
    let skia_build_dir = Path::new(&out_dir)
        .parent()
        .and_then(Path::parent)
        .expect("unexpected Cargo OUT_DIR layout");

    let archive = fs::read_dir(skia_build_dir)
        .expect("cannot inspect Skia build output directory")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("skia-bindings-")
        })
        .map(|entry| entry.path().join("out/skia/libharfbuzz.a"))
        .find(|path| path.is_file())
        .unwrap_or_else(|| {
            panic!(
                "Skia's Emscripten build did not produce libharfbuzz.a under {}",
                skia_build_dir.display()
            )
        });

    let archive_headers = command_output(
        Command::new("llvm-readobj")
            .arg("--file-headers")
            .arg(&archive),
        "inspect the HarfBuzz archive format",
    );
    if !archive_headers.contains("Format: WASM") || !archive_headers.contains("Arch: wasm32") {
        panic!(
            "HarfBuzz archive is not a wasm32 archive: {}\n{}",
            archive.display(),
            archive_headers
        );
    }

    let symbols = command_output(
        Command::new("llvm-nm")
            .args(["-g", "--defined-only"])
            .arg(&archive),
        "inspect HarfBuzz archive symbols",
    );
    for required in ["hb_buffer_create", "hb_shape", "hb_font_create"] {
        if !symbols.lines().any(|line| {
            line.split_whitespace()
                .last()
                .is_some_and(|symbol| symbol == required || symbol == format!("_{required}"))
        }) {
            panic!(
                "{} is missing from Emscripten HarfBuzz archive {}",
                required,
                archive.display()
            );
        }
    }

    println!("cargo:rerun-if-changed={}", archive.display());
    println!(
        "cargo:rustc-link-search=native={}",
        archive.parent().unwrap().display()
    );
    println!("cargo:rustc-link-lib=static=harfbuzz");
    println!(
        "cargo:warning=Verified wasm32 HarfBuzz archive and required symbols: {}",
        archive.display()
    );
}

fn command_output(command: &mut Command, description: &str) -> String {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("failed to {description}: {error}"));
    if !output.status.success() {
        panic!(
            "failed to {description}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8_lossy(&output.stdout).into_owned()
}
