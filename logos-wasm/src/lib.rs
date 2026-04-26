//! # logos-wasm — pure-Rust design editor compiled to WebAssembly
//!
//! Zero Java. Zero ClojureScript. Zero Node (at runtime).
//!
//! Built with eframe/egui (immediate-mode UI), logos-core, logos-layout, logos-render.
//!
//! ## Build
//! ```bash
//! cd logos-wasm && trunk build --release
//! ```

use wasm_bindgen::prelude::*;

mod camera;
mod collab;
mod editor;   // egui design-editor application
mod error;
mod panels;   // left / right / toolbar panels
mod state;    // editor state model
mod tools;    // tool modes

pub use camera::Camera;
pub use collab::{WasmConnectionState, WasmSyncConfig, WasmSyncState};
pub use error::WasmError;

/// Initialise panic hook — browser console error messages.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Start the Logos design editor.
///
/// Mounts the eframe app into the browser canvas with id `logos-canvas`.
#[wasm_bindgen]
pub fn run_app() -> Result<(), JsValue> {
    use wasm_bindgen::JsCast;

    // Look up the canvas element we placed in index.html
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("logos-canvas"))
        .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .ok_or_else(|| JsValue::from_str("Could not find #logos-canvas"))?;

    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async move {
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(editor::LogosEditor::new(cc)))),
            )
            .await;
        if let Err(e) = result {
            web_sys::console::error_1(&format!("Logos startup failed: {e:?}").into());
        }
    });
    Ok(())
}
