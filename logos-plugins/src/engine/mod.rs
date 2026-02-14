//! JavaScript engine integration using boa_engine.
//!
//! Provides a real JavaScript runtime for plugin execution,
//! replacing the Day 18 placeholder expression evaluator with
//! a full ES2023-compliant engine.
//!
//! Architecture:
//! ```text
//! ┌────────────────────────────────────────────┐
//! │              JsEngine                      │
//! │  ┌──────────────────────────────────────┐  │
//! │  │    boa_engine::Context (ES2023)      │  │
//! │  │  ┌────────────┐  ┌───────────────┐  │  │
//! │  │  │  JS Source  │  │  Host API     │  │  │
//! │  │  │  Evaluator  │  │  (Logos.*)    │  │  │
//! │  │  └────────────┘  └───────────────┘  │  │
//! │  └──────────────────────────────────────┘  │
//! │  ┌─────────────┐  ┌──────────────────┐    │
//! │  │ Resource     │  │  Permission      │    │
//! │  │ Limits       │  │  Guard           │    │
//! │  │ (50MB/10ms) │  │  (OWASP)         │    │
//! │  └─────────────┘  └──────────────────┘    │
//! └────────────────────────────────────────────┘
//!              │
//!              ▼
//!     logos_core::Document
//! ```
//!
//! ## Performance Targets
//!
//! | Operation | Cold | Warm | Reference |
//! |-----------|------|------|-----------|
//! | Isolate creation | <5ms | <100μs | V8 Design Doc |
//! | First execution | <10ms | <1ms | Software Architecture |
//! | Host function | — | <500ns | Computer Architecture §2.3 |
//! | Memory limit | 50MB hard cap | — | Secure Programming |
//! | Timeout | 10ms default | — | OWASP Testing Guide |

pub mod events;
pub mod host_api;
pub mod js_runtime;

pub use events::EventBus;
pub use js_runtime::JsEngine;
