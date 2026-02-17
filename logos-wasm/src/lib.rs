//! # logos-wasm — WebAssembly target for the Logos design engine
//!
//! Exposes the full Document → Layout → Render pipeline to JavaScript
//! via `wasm-bindgen` + WebGPU. All engine optimizations (308 ns layout,
//! 131 ns partial GPU upload, 3 ns steady-state frame) carry over unchanged.
//!
//! ## Build
//!
//! ```bash
//! wasm-pack build logos-wasm --target web --release
//! ```
//!
//! ## Usage (JavaScript)
//!
//! ```javascript
//! import init, { LogosApp } from './pkg/logos_wasm.js';
//! await init();
//! const app = await LogosApp.create(document.getElementById('canvas'));
//! app.set_clear_color(0.1, 0.1, 0.18, 1.0);
//! app.load_demo_scene(100);
//! function frame() { app.render_frame(); requestAnimationFrame(frame); }
//! requestAnimationFrame(frame);
//! ```

/// App module — WASM-only (requires WebGPU canvas surface).
#[cfg(target_arch = "wasm32")]
pub mod app;

/// Camera module — pure Rust, compiles on all targets.
pub mod camera;

/// Collaboration sync module — protocol state compiles everywhere,
/// WebSocket transport is WASM-only.
pub mod collab;

/// Error types — compile on all targets for testing.
pub mod error;

#[cfg(target_arch = "wasm32")]
pub use app::LogosApp;
pub use camera::Camera;
pub use collab::{WasmConnectionState, WasmSyncConfig, WasmSyncState};
pub use error::WasmError;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Initialize panic hook for browser console error messages.
/// Called automatically when the WASM module loads.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Log to the browser console (debug helper).
#[cfg(target_arch = "wasm32")]
macro_rules! console_log {
    ($($t:tt)*) => {
        web_sys::console::log_1(&format!($($t)*).into())
    }
}
#[cfg(target_arch = "wasm32")]
pub(crate) use console_log;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_accessible() {
        let cam = Camera::new(800.0, 600.0);
        assert_eq!(cam.viewport_size(), (800.0, 600.0));
    }

    #[test]
    fn test_error_accessible() {
        let err = WasmError::Gpu("test".into());
        assert!(err.to_string().contains("GPU"));
    }
}
