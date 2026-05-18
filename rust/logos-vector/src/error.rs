//! `VectorError` — domain error type for logo-vector operations.

use std::fmt;

/// Errors that can occur during vector network operations.
#[derive(Debug, Clone, PartialEq)]
pub enum VectorError {
    /// The referenced anchor index does not exist in the network.
    AnchorNotFound(usize),
    /// The referenced segment index does not exist in the network.
    SegmentNotFound(usize),
    /// Attempted to add a segment between an anchor and itself.
    SelfLoop(usize),
    /// Attempted to add a duplicate segment (same start, end, direction).
    DuplicateSegment { start: usize, end: usize },
    /// The region boundary does not form a closed walk (V2).
    OpenBoundary,
}

impl fmt::Display for VectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnchorNotFound(id) => write!(f, "anchor {id} not found"),
            Self::SegmentNotFound(id) => write!(f, "segment {id} not found"),
            Self::SelfLoop(id) => write!(f, "self-loop on anchor {id}"),
            Self::DuplicateSegment { start, end } => {
                write!(f, "duplicate segment {start} → {end}")
            }
            Self::OpenBoundary => write!(f, "region boundary is not a closed walk"),
        }
    }
}

impl std::error::Error for VectorError {}
