// logos-collab/src/network/mod.rs
//
//! Async HTTP network layer for desktop → Logos-server communication.
//!
//! Gated on the `http-client` feature for the actual reqwest code.
//! The DTO types in `api` and the error types in `client` are always compiled.

pub mod client;
pub mod api;

pub use client::{ApiError, ApiResult, ClientConfig};
#[cfg(feature = "http-client")]
pub use client::HttpClient;
