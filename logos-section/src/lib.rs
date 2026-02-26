//! # logos-section
//!
//! Section container hierarchy, paradigm management, and queries.
//!
//! This crate provides three primary capabilities:
//!
//! 1. **Hierarchy** — section nesting, parent-child relationships,
//!    tree traversal, depth limits, and reparenting operations.
//!
//! 2. **Paradigm** — workspace-level "design paradigm" that controls
//!    how the tool presents containers (Artboard-centric, Frame-centric,
//!    or Section-centric). Paradigm affects default creation mode and
//!    panel behaviour.
//!
//! 3. **Query** — find sections by name, list sections containing a
//!    given layer, filter by collapse/lock/visibility state, etc.

pub mod hierarchy;
pub mod paradigm;
pub mod query;
