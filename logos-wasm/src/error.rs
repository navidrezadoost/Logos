//! WASM-friendly error types.
//!
//! All error variants convert to `JsValue` via `Display` for clean
//! browser console output and JavaScript `catch` handling.

use wasm_bindgen::prelude::*;
use std::fmt;

/// Error type for WASM operations.
///
/// Converts to `JsValue` (JavaScript string) for clean error
/// propagation across the WASM boundary.
#[derive(Debug, Clone)]
pub enum WasmError {
    /// GPU initialization or rendering error.
    Gpu(String),
    /// Layout computation error.
    Layout(String),
    /// Document operation error.
    Document(String),
    /// Render pipeline error.
    Render(String),
    /// Canvas element error.
    Canvas(String),
    /// Invalid argument from JavaScript.
    InvalidArg(String),
}

impl fmt::Display for WasmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gpu(msg) => write!(f, "Logos GPU error: {msg}"),
            Self::Layout(msg) => write!(f, "Logos layout error: {msg}"),
            Self::Document(msg) => write!(f, "Logos document error: {msg}"),
            Self::Render(msg) => write!(f, "Logos render error: {msg}"),
            Self::Canvas(msg) => write!(f, "Logos canvas error: {msg}"),
            Self::InvalidArg(msg) => write!(f, "Logos invalid argument: {msg}"),
        }
    }
}

impl From<WasmError> for JsValue {
    fn from(err: WasmError) -> JsValue {
        JsValue::from_str(&err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_gpu() {
        let err = WasmError::Gpu("no adapter".into());
        assert_eq!(err.to_string(), "Logos GPU error: no adapter");
    }

    #[test]
    fn test_display_layout() {
        let err = WasmError::Layout("node not found".into());
        assert_eq!(err.to_string(), "Logos layout error: node not found");
    }

    #[test]
    fn test_display_document() {
        let err = WasmError::Document("layer missing".into());
        assert_eq!(err.to_string(), "Logos document error: layer missing");
    }

    #[test]
    fn test_display_render() {
        let err = WasmError::Render("surface lost".into());
        assert_eq!(err.to_string(), "Logos render error: surface lost");
    }

    #[test]
    fn test_display_canvas() {
        let err = WasmError::Canvas("element not found".into());
        assert_eq!(err.to_string(), "Logos canvas error: element not found");
    }

    #[test]
    fn test_display_invalid_arg() {
        let err = WasmError::InvalidArg("bad uuid".into());
        assert_eq!(err.to_string(), "Logos invalid argument: bad uuid");
    }

    #[test]
    fn test_debug_format() {
        let err = WasmError::Gpu("test".into());
        let debug = format!("{err:?}");
        assert!(debug.contains("Gpu"));
        assert!(debug.contains("test"));
    }

    #[test]
    fn test_clone() {
        let err = WasmError::Layout("cloned".into());
        let err2 = err.clone();
        assert_eq!(err.to_string(), err2.to_string());
    }
}
