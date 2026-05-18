//! logos-vector — Half-edge vector network graph.
//!
//! A **vector network** is a directed graph of anchors connected by cubic
//! Bézier segments. Unlike a Bézier path (an ordered chain with a single
//! start and end), a vector network allows any number of segments per
//! anchor, enabling complex topology: T-junctions, star shapes, and
//! arbitrary boolean-ready geometry.
//!
//! # Architecture
//!
//! ```text
//! VectorNetwork
//!   ├── anchors:  Vec<Anchor>   — spatial points with optional handles
//!   ├── segments: Vec<Segment>  — directed cubic Bézier edges
//!   └── regions:  Vec<Region>  — closed cycles (filled areas) [V2]
//! ```
//!
//! # Module layout
//!
//! | Module      | Responsibility                                    |
//! |-------------|---------------------------------------------------|
//! | `graph`     | `VectorNetwork` — the top-level container + CRUD  |
//! | `anchor`    | `Anchor` — position + handles + incident tracking |
//! | `segment`   | `Segment` — directed Bézier edge between anchors  |
//! | `region`    | `Region` — closed cycle (V2 — placeholder for now)|
//! | `error`     | `VectorError` — domain error type                 |
//!
//! # Example
//!
//! ```rust
//! use logos_vector::VectorNetwork;
//!
//! let mut net = VectorNetwork::new();
//! let a = net.add_anchor(0.0, 0.0);
//! let b = net.add_anchor(100.0, 0.0);
//! let c = net.add_anchor(50.0, 80.0);
//!
//! let s0 = net.add_segment(a, b).unwrap();
//! let s1 = net.add_segment(b, c).unwrap();
//! let s2 = net.add_segment(c, a).unwrap();
//!
//! assert_eq!(net.anchor(a).unwrap().incident_segments().len(), 2);
//! ```

pub mod anchor;
pub mod cycle;
pub mod error;
pub mod graph;
pub mod region;
pub mod segment;

pub use anchor::Anchor;
pub use cycle::find_regions;
pub use error::VectorError;
pub use graph::VectorNetwork;
pub use region::Region;
pub use segment::Segment;
