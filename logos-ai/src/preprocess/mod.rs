//! Preprocessing — image and text tensor preparation.
//!
//! Converts raw inputs (images, text) into normalized tensors
//! suitable for the inference engine.

pub mod image_tensor;
pub mod tokenizer;

pub use image_tensor::ImageTensor;
pub use tokenizer::{TextTokenizer, TokenizerConfig};
