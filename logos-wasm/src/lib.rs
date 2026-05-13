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
use std::sync::atomic::{AtomicU32, Ordering};

mod camera;
mod canvas;       // canvas panel rendering
mod canvas_input; // tool input handling
mod collab;
mod draw_utils;   // pure rendering helpers
mod editor;       // egui application shell
mod error;
mod panels;       // left / right / toolbar panels
mod state;        // editor state model
mod tools;        // tool modes

pub use camera::Camera;
pub use collab::{WasmConnectionState, WasmSyncConfig, WasmSyncState};
pub use error::WasmError;

// ── Layout-independent keyboard shortcut detection ───────────────────────────
// egui's WASM backend maps KeyboardEvent.key (logical, layout-dependent) to its
// Key enum. For non-Latin keyboards (Persian, Arabic, Hebrew, Greek, etc.) the
// character at the Z position is NOT "Z", so egui drops the event and the
// shortcut never fires.  We fix this by registering a capturing DOM listener
// that reads KeyboardEvent.code (physical, always "KeyZ" regardless of layout),
// stores pending bits in an atomic, and calls preventDefault so egui does not
// receive a garbled event.

/// Each bit represents a pending shortcut to execute this frame.
pub static PENDING_KEYS: AtomicU32 = AtomicU32::new(0);

pub const SK_UNDO:       u32 = 1 << 0;  // Ctrl+Z
pub const SK_REDO:       u32 = 1 << 1;  // Ctrl+Shift+Z  or  Ctrl+Y
pub const SK_COPY:       u32 = 1 << 2;  // Ctrl+C
pub const SK_CUT:        u32 = 1 << 3;  // Ctrl+X
pub const SK_PASTE:      u32 = 1 << 4;  // Ctrl+V
pub const SK_DUPLICATE:  u32 = 1 << 5;  // Ctrl+D
pub const SK_SELECT_ALL: u32 = 1 << 6;  // Ctrl+A

/// Register a capturing keydown listener on `window` that uses
/// `KeyboardEvent.code` for layout-independent shortcut detection.
fn register_keyboard_shortcuts() {
    use wasm_bindgen::JsCast;
    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };

    let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
        |e: web_sys::KeyboardEvent| {
            let ctrl = e.ctrl_key() || e.meta_key();
            if !ctrl { return; }
            let shift = e.shift_key();
            let bit = match e.code().as_str() {
                "KeyZ" if shift => Some(SK_REDO),
                "KeyZ"          => Some(SK_UNDO),
                "KeyY"          => Some(SK_REDO),
                "KeyC"          => Some(SK_COPY),
                "KeyX"          => Some(SK_CUT),
                "KeyV"          => Some(SK_PASTE),
                "KeyD"          => Some(SK_DUPLICATE),
                "KeyA"          => Some(SK_SELECT_ALL),
                _               => None,
            };
            if let Some(b) = bit {
                PENDING_KEYS.fetch_or(b, Ordering::Relaxed);
                e.prevent_default(); // stop browser (e.g. Ctrl+Z = browser undo)
            }
        },
    );

    // `true` = capture phase: fires before egui’s own canvas listener.
    let _ = window.add_event_listener_with_callback_and_bool(
        "keydown",
        closure.as_ref().unchecked_ref(),
        true,
    );
    closure.forget(); // intentional: lives for the page lifetime
}

/// Initialise panic hook — browser console error messages.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
    register_keyboard_shortcuts();
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

    let web_options = eframe::WebOptions {
        persist_egui_memory: true, // saves panel widths etc. to localStorage
        ..Default::default()
    };
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
