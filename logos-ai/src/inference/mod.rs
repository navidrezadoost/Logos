//! Inference engine — unified runtime for running AI models.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    InferenceEngine                           │
//! │  ┌─────────────┐  ┌──────────────┐  ┌───────────────────┐  │
//! │  │LayoutGen    │  │StyleTransfer │  │AssetGenerator     │  │
//! │  │             │  │              │  │                   │  │
//! │  │ ┌─────────┐ │  │ ┌──────────┐ │  │ ┌───────────────┐ │  │
//! │  │ │OnnxSess │ │  │ │OnnxSess  │ │  │ │OnnxSess      │ │  │
//! │  │ │(or sim) │ │  │ │(or sim)  │ │  │ │(or sim)      │ │  │
//! │  │ └─────────┘ │  │ └──────────┘ │  │ └───────────────┘ │  │
//! │  └─────────────┘  └──────────────┘  └───────────────────┘  │
//! └──────────────────────────────────────────────────────────────┘
//! ```

pub mod layout_gen;
pub mod style_transfer;
pub mod asset_gen;
pub mod engine;
pub mod onnx_session;

// Phase 12B: AI-powered design intelligence
pub mod design_suggest;
pub mod accessibility;
pub mod color_harmony;
pub mod smart_constraints;
pub mod component_recommend;
pub mod pipeline;
