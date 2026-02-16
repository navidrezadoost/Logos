//! Binary format handling for .fig files.
//!
//! The .fig file format consists of:
//! 1. A fixed-size header with magic bytes, version, and metadata
//! 2. A zlib-compressed payload containing the serialized document
//! 3. The payload uses a field-based binary encoding (Kiwi format)

pub mod header;
pub mod kiwi;

pub use header::{FigHeader, FIG_MAGIC, SUPPORTED_VERSIONS};
pub use kiwi::{KiwiDecoder, KiwiValue, KiwiField};
