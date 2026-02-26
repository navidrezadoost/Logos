//! Spreadsheet UI model — pure Rust data structures for rendering and interaction.
//!
//! This module contains the foundational types for a spreadsheet panel:
//!
//! - [`GridModel`] — column widths, row heights, cell geometry
//! - [`Viewport`] — visible area, scroll position, zoom
//! - [`HitTest`] — screen-to-cell coordinate mapping
//! - [`Selection`] — cell/range selection, keyboard navigation
//! - [`CellRenderData`] — computed render info for visible cells
//!
//! All types are pure Rust with no rendering dependencies, making them
//! fully testable and portable across WASM, desktop, and test harnesses.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐
//! │  RecalcEngine │  (formulas, values, deps)
//! └──────┬──────┘
//!        │ provides cell values
//! ┌──────▼──────┐
//! │ SpreadPanel  │  (orchestrator)
//! │  ├ GridModel  │  column/row geometry
//! │  ├ Viewport   │  scroll + zoom
//! │  ├ Selection  │  cursor + ranges
//! │  └ RenderData │  visible cell cache
//! └──────┬──────┘
//!        │ outputs CellRenderData[]
//! ┌──────▼──────┐
//! │  Renderer    │  (Skia / WebGL / test stub)
//! └─────────────┘
//! ```

pub mod grid;
pub mod viewport;
pub mod selection;
pub mod hit_test;
pub mod render_data;
pub mod tokenizer;
pub mod completion;
pub mod formula_bar;
pub mod panel;

pub use grid::GridModel;
pub use viewport::Viewport;
pub use selection::{Selection, SelectionRange};
pub use hit_test::HitTestResult;
pub use render_data::CellRenderData;
pub use panel::SpreadsheetPanel;
pub use tokenizer::{Token, TokenKind};
#[allow(unused_imports)]
pub use completion::{CompletionEngine, CompletionItem, CompletionKind, FunctionSignature};
pub use formula_bar::{FormulaBarState, FormulaBarMode, FormulaBarRenderData, FormulaSpan};
