// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) KALEIDOS INC
//
// logos-layout-wasm
// ─────────────────
// WASM binary that exposes the logos-layout flex and grid engines via a
// simple JSON-in / JSON-out C-ABI.  No wasm-bindgen; compatible with any
// `WebAssembly.instantiate` call in the browser or Node.
//
// Memory protocol
// ───────────────
//   1. JS calls `logos_alloc(len)` to allocate an input buffer.
//   2. JS writes JSON bytes into wasmMemory at the returned pointer.
//   3. JS calls `logos_calc_flex_layout(ptr, len)` (or grid variant).
//      Rust reads JSON, runs the layout engine, serialises the result
//      to a static output buffer, and returns the output byte length.
//   4. JS reads `logos_output_ptr()` bytes (count = return value of step 3).
//   5. JS calls `logos_free_input(ptr, len)` and `logos_free_output()`.
//
// JSON protocol: see `FlexInput` / `FlexOutput` / `GridInput` / `GridOutput`.

use std::alloc::{alloc, dealloc, Layout};
use std::sync::Mutex;

use logos_layout::flex::{
    ChildLayoutData, ChildShape as FlexChildShape,
    FlexContainer, compute_positions,
};
use logos_layout::grid::{
    GridContainer,
    calc_grid_layout_data, compute_positions as grid_compute_positions, GridChildShape,
};
use serde::{Deserialize, Serialize};

// =============================================================================
// Memory management  (mirrors render-wasm/src/mem.rs)
// =============================================================================

const ALLOC_ALIGN: usize = 8;

static OUTPUT_BUF: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Allocate `len` bytes for an input buffer.
/// Returns a pointer the JS side can write JSON bytes into.
#[no_mangle]
pub unsafe extern "C" fn logos_alloc(len: u32) -> *mut u8 {
    let len = len as usize;
    let layout = Layout::from_size_align_unchecked(len, ALLOC_ALIGN);
    let ptr = alloc(layout);
    if ptr.is_null() {
        panic!("logos_alloc: allocation failed for len={len}");
    }
    ptr
}

/// Free an input buffer previously allocated by `logos_alloc`.
#[no_mangle]
pub unsafe extern "C" fn logos_free_input(ptr: *mut u8, len: u32) {
    let len = len as usize;
    let layout = Layout::from_size_align_unchecked(len, ALLOC_ALIGN);
    dealloc(ptr, layout);
}

/// Return the pointer to the current output buffer (valid until `logos_free_output`).
#[no_mangle]
pub extern "C" fn logos_output_ptr() -> *const u8 {
    let guard = OUTPUT_BUF.lock().unwrap();
    match guard.as_ref() {
        Some(v) => v.as_ptr(),
        None => std::ptr::null(),
    }
}

/// Release the output buffer.
#[no_mangle]
pub extern "C" fn logos_free_output() {
    let mut guard = OUTPUT_BUF.lock().unwrap();
    *guard = None;
}

fn write_output(bytes: Vec<u8>) -> u32 {
    let len = bytes.len() as u32;
    let mut guard = OUTPUT_BUF.lock().unwrap();
    *guard = Some(bytes);
    len
}

// =============================================================================
// JSON protocol — Flex
// =============================================================================

#[derive(Deserialize, Debug)]
struct FlexChildInput {
    pub id: u64,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub min_width: Option<f64>,
    pub max_width: Option<f64>,
    pub min_height: Option<f64>,
    pub max_height: Option<f64>,
    /// "fix" | "fill" | "auto"
    #[serde(default = "default_fix")]
    pub h_sizing: String,
    #[serde(default = "default_fix")]
    pub v_sizing: String,
    /// "auto" | "start" | "end" | "center" | "stretch"
    #[serde(default = "default_auto_str")]
    pub align_self: String,
    #[serde(default)]
    pub absolute: bool,
}

fn default_fix() -> String {
    "fix".to_string()
}
fn default_auto_str() -> String {
    "auto".to_string()
}

#[derive(Deserialize, Debug)]
struct FlexInput {
    pub container_width: f64,
    pub container_height: f64,
    #[serde(default = "default_row")]
    pub direction: String,
    #[serde(default = "default_nowrap")]
    pub wrap: String,
    #[serde(default = "default_start")]
    pub justify_content: String,
    #[serde(default = "default_start")]
    pub align_items: String,
    #[serde(default = "default_start")]
    pub align_content: String,
    #[serde(default)]
    pub row_gap: f64,
    #[serde(default)]
    pub column_gap: f64,
    #[serde(default)]
    pub padding_top: f64,
    #[serde(default)]
    pub padding_right: f64,
    #[serde(default)]
    pub padding_bottom: f64,
    #[serde(default)]
    pub padding_left: f64,
    pub children: Vec<FlexChildInput>,
}

fn default_row() -> String { "row".to_string() }
fn default_nowrap() -> String { "no-wrap".to_string() }
fn default_start() -> String { "start".to_string() }

#[derive(Serialize, Deserialize, Debug)]
struct PositionedChildOut {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Serialize, Deserialize, Debug)]
struct FlexOutput {
    pub children: Vec<PositionedChildOut>,
}

// =============================================================================
// JSON protocol — Grid
// =============================================================================

#[derive(Deserialize, Debug)]
struct GridTrackInput {
    /// "fixed" | "flex" | "percent" | "auto"
    #[serde(rename = "type")]
    pub track_type: String,
    #[serde(default)]
    pub value: f64,
}

#[derive(Deserialize, Debug)]
struct GridCellInput {
    pub shape_id: u64,
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
}

#[derive(Deserialize, Debug)]
struct GridChildInput {
    pub id: u64,
    #[serde(default)]
    pub min_width: f64,
    #[serde(default = "default_huge")]
    pub max_width: f64,
    #[serde(default)]
    pub min_height: f64,
    #[serde(default = "default_huge")]
    pub max_height: f64,
}

fn default_huge() -> f64 { 1e9 }

#[derive(Deserialize, Debug)]
struct GridInput {
    pub container_width: f64,
    pub container_height: f64,
    #[serde(default)]
    pub column_gap: f64,
    #[serde(default)]
    pub row_gap: f64,
    #[serde(default)]
    pub padding_top: f64,
    #[serde(default)]
    pub padding_right: f64,
    #[serde(default)]
    pub padding_bottom: f64,
    #[serde(default)]
    pub padding_left: f64,
    #[serde(default = "default_start")]
    pub justify_items: String,
    #[serde(default = "default_start")]
    pub align_items: String,
    #[serde(default = "default_start")]
    pub justify_content: String,
    #[serde(default = "default_start")]
    pub align_content: String,
    #[serde(default = "default_row")]
    pub direction: String,
    pub columns: Vec<GridTrackInput>,
    pub rows: Vec<GridTrackInput>,
    #[serde(default)]
    pub cells: Vec<GridCellInput>,
    pub children: Vec<GridChildInput>,
}

#[derive(Serialize, Debug)]
struct GridChildOut {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub col: usize,
    pub row: usize,
}

#[derive(Serialize, Debug)]
struct GridOutput {
    pub resolved_columns: Vec<f64>,
    pub resolved_rows: Vec<f64>,
    pub children: Vec<GridChildOut>,
}

// =============================================================================
// Flex layout engine bridge
// =============================================================================

/// Build a `FlexContainer` from `FlexInput` using `FlexContainer::from_options`.
fn build_flex_container(input: &FlexInput) -> FlexContainer {
    FlexContainer::from_options(
        Some(&input.direction),
        Some(&input.wrap),
        Some(&input.align_items),
        Some(&input.align_content),
        Some(&input.justify_content),
        Some(input.row_gap),
        Some(input.column_gap),
        Some(input.padding_top),
        Some(input.padding_right),
        Some(input.padding_bottom),
        Some(input.padding_left),
    )
}

/// Map `FlexChildInput` → `FlexChildShape` using the string sizing modes.
fn build_child_shape(child: &FlexChildInput) -> FlexChildShape {
    use logos_layout::flex::{SizingMode, AlignSelf};

    let h_sizing = match child.h_sizing.as_str() {
        "fill"  => SizingMode::Fill,
        "auto"  => SizingMode::Auto,
        _       => SizingMode::Fix,
    };
    let v_sizing = match child.v_sizing.as_str() {
        "fill"  => SizingMode::Fill,
        "auto"  => SizingMode::Auto,
        _       => SizingMode::Fix,
    };
    let align_self = match child.align_self.as_str() {
        "start"   => AlignSelf::Start,
        "end"     => AlignSelf::End,
        "center"  => AlignSelf::Center,
        "stretch" => AlignSelf::Stretch,
        _         => AlignSelf::Auto,
    };

    FlexChildShape {
        width:      child.width,
        height:     child.height,
        min_width:  child.min_width,
        max_width:  child.max_width,
        min_height: child.min_height,
        max_height: child.max_height,
        h_sizing,
        v_sizing,
        align_self,
        absolute: child.absolute,
    }
}

fn run_flex_layout(input: FlexInput) -> FlexOutput {
    let container = build_flex_container(&input);
    let (pad_top, _pad_right, pad_bottom, pad_left) = container.padding;

    // Available space after padding
    let avail_w = (input.container_width  - pad_left - _pad_right).max(0.0);
    let avail_h = (input.container_height - pad_top  - pad_bottom).max(0.0);

    // Build (id, ChildLayoutData) pairs
    let children_data: Vec<(u64, ChildLayoutData)> = input.children
        .iter()
        .map(|ch| {
            let shape = build_child_shape(ch);
            let layout_data = ChildLayoutData::from_shape(&shape, &container);
            (ch.id, layout_data)
        })
        .collect();

    let lines = compute_positions(&container, &children_data, avail_w, avail_h);

    // Offset by padding to get container-relative coordinates
    let offset_x = container.padding.3;  // pad_left
    let offset_y = container.padding.0;  // pad_top

    let children: Vec<PositionedChildOut> = lines
        .into_iter()
        .flat_map(|line| line.children.into_iter())
        .map(|c| PositionedChildOut {
            id:     c.id,
            x:      c.x + offset_x,
            y:      c.y + offset_y,
            width:  c.width,
            height: c.height,
        })
        .collect();

    FlexOutput { children }
}

/// Calculate flex layout.
///
/// `ptr` points to `len` bytes of UTF-8 JSON matching `FlexInput`.
/// Returns the byte length of the output JSON written to the static output
/// buffer (read via `logos_output_ptr()`). Returns 0 on error.
#[no_mangle]
pub unsafe extern "C" fn logos_calc_flex_layout(ptr: *const u8, len: u32) -> u32 {
    let bytes = std::slice::from_raw_parts(ptr, len as usize);
    let input: FlexInput = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return 0,
    };

    let output = run_flex_layout(input);
    let json = match serde_json::to_vec(&output) {
        Ok(v) => v,
        Err(_) => return 0,
    };

    write_output(json)
}

// =============================================================================
// Grid layout engine bridge
// =============================================================================

fn build_grid_container(input: &GridInput) -> GridContainer {
    use logos_layout::grid::{
        GridTrack, GridCell, GridDirection as GD,
        AlignContent, AlignItems, JustifyContent, JustifyItems,
    };

    // Parse tracks
    let columns: Vec<GridTrack> = input.columns.iter().map(|t| {
        match t.track_type.as_str() {
            "flex"    => GridTrack::flex(t.value),
            "percent" => GridTrack::percent(t.value),
            "auto"    => GridTrack::auto(),
            _         => GridTrack::fixed(t.value),
        }
    }).collect();

    let rows: Vec<GridTrack> = input.rows.iter().map(|t| {
        match t.track_type.as_str() {
            "flex"    => GridTrack::flex(t.value),
            "percent" => GridTrack::percent(t.value),
            "auto"    => GridTrack::auto(),
            _         => GridTrack::fixed(t.value),
        }
    }).collect();

    let justify_items = match input.justify_items.as_str() {
        "end"     => JustifyItems::End,
        "center"  => JustifyItems::Center,
        "stretch" => JustifyItems::Stretch,
        _         => JustifyItems::Start,
    };
    let align_items = match input.align_items.as_str() {
        "end"     => AlignItems::End,
        "center"  => AlignItems::Center,
        "stretch" => AlignItems::Stretch,
        _         => AlignItems::Start,
    };
    let justify_content = match input.justify_content.as_str() {
        "end"           => JustifyContent::End,
        "center"        => JustifyContent::Center,
        "space-between" => JustifyContent::SpaceBetween,
        "space-around"  => JustifyContent::SpaceAround,
        "space-evenly"  => JustifyContent::SpaceEvenly,
        "stretch"       => JustifyContent::Stretch,
        _               => JustifyContent::Start,
    };
    let align_content = match input.align_content.as_str() {
        "end"           => AlignContent::End,
        "center"        => AlignContent::Center,
        "space-between" => AlignContent::SpaceBetween,
        "space-around"  => AlignContent::SpaceAround,
        "space-evenly"  => AlignContent::SpaceEvenly,
        "stretch"       => AlignContent::Stretch,
        _               => AlignContent::Start,
    };
    let direction = match input.direction.as_str() {
        "column" => GD::Column,
        _        => GD::Row,
    };

    // Build cells map
    let cells: std::collections::HashMap<u64, GridCell> = input.cells.iter().map(|c| {
        (c.shape_id, GridCell {
            id:           c.shape_id,
            row:          c.row,
            column:       c.column,
            row_span:     c.row_span,
            column_span:  c.column_span,
            area_name:    None,
            position:     logos_layout::grid::GridPosition::Manual,
            align_self:   logos_layout::grid::TrackAlignSelf::Auto,
            justify_self: logos_layout::grid::TrackJustifySelf::Auto,
            shapes:       vec![c.shape_id],
        })
    }).collect();

    GridContainer {
        columns,
        rows,
        column_gap: input.column_gap,
        row_gap: input.row_gap,
        padding: (
            input.padding_top,
            input.padding_right,
            input.padding_bottom,
            input.padding_left,
        ),
        justify_items,
        align_items,
        justify_content,
        align_content,
        direction,
        cells,
    }
}

fn run_grid_layout(input: GridInput) -> GridOutput {
    let container = build_grid_container(&input);

    let children: Vec<GridChildShape> = input.children.iter().map(|c| {
        GridChildShape {
            id:         c.id,
            min_width:  c.min_width,
            max_width:  c.max_width,
            min_height: c.min_height,
            max_height: c.max_height,
        }
    }).collect();

    let (resolved, child_layouts) = calc_grid_layout_data(
        &container,
        &children,
        input.container_width,
        input.container_height,
    );

    // Compute (x, y) positions
    let positioned = grid_compute_positions(&container, &resolved, &child_layouts, &[]);

    let children_out: Vec<GridChildOut> = positioned.iter().map(|p| {
        GridChildOut {
            id:     p.id,
            x:      p.x,
            y:      p.y,
            width:  p.width,
            height: p.height,
            col:    p.col,
            row:    p.row,
        }
    }).collect();

    GridOutput {
        resolved_columns: resolved.columns.clone(),
        resolved_rows: resolved.rows.clone(),
        children: children_out,
    }
}

/// Calculate grid layout.
///
/// Same memory protocol as `logos_calc_flex_layout`.
/// Input JSON matches `GridInput`, output matches `GridOutput`.
#[no_mangle]
pub unsafe extern "C" fn logos_calc_grid_layout(ptr: *const u8, len: u32) -> u32 {
    let bytes = std::slice::from_raw_parts(ptr, len as usize);
    let input: GridInput = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return 0,
    };

    let output = run_grid_layout(input);
    let json = match serde_json::to_vec(&output) {
        Ok(v) => v,
        Err(_) => return 0,
    };

    write_output(json)
}

// =============================================================================
// Tests (native — run with `cargo test`)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn flex_round_trip(json: &str) -> FlexOutput {
        let input: FlexInput = serde_json::from_str(json).expect("parse");
        run_flex_layout(input)
    }

    fn grid_round_trip(json: &str) -> GridOutput {
        let input: GridInput = serde_json::from_str(json).expect("parse");
        run_grid_layout(input)
    }

    // ── Flex tests ─────────────────────────────────────────────────────────

    #[test]
    fn flex_single_child_start_align() {
        let out = flex_round_trip(r#"{
            "container_width": 400,
            "container_height": 200,
            "direction": "row",
            "wrap": "no-wrap",
            "justify_content": "start",
            "align_items": "start",
            "align_content": "start",
            "row_gap": 0,
            "column_gap": 0,
            "children": [
                {"id": 1, "width": 100.0, "height": 80.0}
            ]
        }"#);
        assert_eq!(out.children.len(), 1);
        assert_eq!(out.children[0].id, 1);
        assert_eq!(out.children[0].x, 0.0);
        assert_eq!(out.children[0].y, 0.0);
        assert_eq!(out.children[0].width, 100.0);
        assert_eq!(out.children[0].height, 80.0);
    }

    #[test]
    fn flex_two_children_row_gap() {
        let out = flex_round_trip(r#"{
            "container_width": 500,
            "container_height": 200,
            "direction": "row",
            "wrap": "no-wrap",
            "justify_content": "start",
            "align_items": "start",
            "align_content": "start",
            "column_gap": 20,
            "children": [
                {"id": 1, "width": 100.0, "height": 80.0},
                {"id": 2, "width": 150.0, "height": 60.0}
            ]
        }"#);
        assert_eq!(out.children.len(), 2);
        // child 1 at x=0, child 2 at x=100+20=120
        let c1 = out.children.iter().find(|c| c.id == 1).unwrap();
        let c2 = out.children.iter().find(|c| c.id == 2).unwrap();
        assert_eq!(c1.x, 0.0);
        assert!((c2.x - 120.0).abs() < 1e-6);
    }

    #[test]
    fn flex_with_padding() {
        let out = flex_round_trip(r#"{
            "container_width": 400,
            "container_height": 300,
            "direction": "row",
            "padding_top": 10,
            "padding_left": 20,
            "children": [
                {"id": 1, "width": 100.0, "height": 80.0}
            ]
        }"#);
        let c = &out.children[0];
        // x should be offset by padding_left=20, y by padding_top=10
        assert_eq!(c.x, 20.0);
        assert_eq!(c.y, 10.0);
    }

    #[test]
    fn flex_column_direction() {
        let out = flex_round_trip(r#"{
            "container_width": 200,
            "container_height": 500,
            "direction": "column",
            "row_gap": 10,
            "children": [
                {"id": 1, "width": 100.0, "height": 80.0},
                {"id": 2, "width": 100.0, "height": 60.0}
            ]
        }"#);
        let c1 = out.children.iter().find(|c| c.id == 1).unwrap();
        let c2 = out.children.iter().find(|c| c.id == 2).unwrap();
        assert_eq!(c1.y, 0.0);
        assert!((c2.y - 90.0).abs() < 1e-6);  // 80 + gap(10)
    }

    #[test]
    fn flex_empty_children() {
        let out = flex_round_trip(r#"{
            "container_width": 400,
            "container_height": 200,
            "children": []
        }"#);
        assert!(out.children.is_empty());
    }

    // ── Grid tests ─────────────────────────────────────────────────────────

    #[test]
    fn grid_two_fixed_columns() {
        let out = grid_round_trip(r#"{
            "container_width": 800,
            "container_height": 400,
            "columns": [{"type": "fixed", "value": 200}, {"type": "fixed", "value": 300}],
            "rows": [{"type": "fixed", "value": 150}],
            "children": [
                {"id": 1},
                {"id": 2}
            ]
        }"#);
        assert_eq!(out.resolved_columns, vec![200.0, 300.0]);
        assert_eq!(out.resolved_rows, vec![150.0]);
        assert_eq!(out.children.len(), 2);

        let c1 = out.children.iter().find(|c| c.id == 1).unwrap();
        let c2 = out.children.iter().find(|c| c.id == 2).unwrap();
        assert_eq!(c1.x, 0.0);
        assert_eq!(c1.col, 1);
        assert_eq!(c2.x, 200.0);
        assert_eq!(c2.col, 2);
    }

    #[test]
    fn grid_one_fr_column() {
        // Single 1fr column in 600px container → 600px
        let out = grid_round_trip(r#"{
            "container_width": 600,
            "container_height": 400,
            "columns": [{"type": "flex", "value": 1}],
            "rows": [{"type": "fixed", "value": 100}],
            "children": [{"id": 1}]
        }"#);
        assert!((out.resolved_columns[0] - 600.0).abs() < 1e-6);
    }

    #[test]
    fn grid_with_gap_and_padding() {
        let out = grid_round_trip(r#"{
            "container_width": 800,
            "container_height": 400,
            "column_gap": 10,
            "padding_left": 5,
            "columns": [{"type": "fixed", "value": 100}, {"type": "fixed", "value": 200}],
            "rows": [{"type": "fixed", "value": 80}],
            "children": [{"id": 1}, {"id": 2}]
        }"#);
        // col_starts: [5, 5+100+10=115]
        let c1 = out.children.iter().find(|c| c.id == 1).unwrap();
        let c2 = out.children.iter().find(|c| c.id == 2).unwrap();
        assert_eq!(c1.x, 5.0);
        assert_eq!(c2.x, 115.0);
    }

    #[test]
    fn grid_explicit_cell_placement() {
        let out = grid_round_trip(r#"{
            "container_width": 600,
            "container_height": 400,
            "columns": [{"type": "fixed", "value": 100}, {"type": "fixed", "value": 100}, {"type": "fixed", "value": 100}],
            "rows": [{"type": "fixed", "value": 100}, {"type": "fixed", "value": 100}],
            "cells": [
                {"shape_id": 99, "row": 2, "column": 3, "row_span": 1, "column_span": 1}
            ],
            "children": [{"id": 99}]
        }"#);
        let c = out.children.iter().find(|c| c.id == 99).unwrap();
        assert_eq!(c.col, 3);
        assert_eq!(c.row, 2);
        // x = col_start[2] = 100+100 = 200
        assert_eq!(c.x, 200.0);
    }

    // ── WASM ABI round-trip tests ───────────────────────────────────────────

    #[test]
    fn abi_flex_round_trip() {
        let input = r#"{"container_width":400,"container_height":200,"children":[{"id":42,"width":100.0,"height":80.0}]}"#;
        let bytes = input.as_bytes();

        let output_len = unsafe {
            let ptr = logos_alloc(bytes.len() as u32);
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            let out_len = logos_calc_flex_layout(ptr, bytes.len() as u32);
            logos_free_input(ptr, bytes.len() as u32);
            out_len
        };

        assert!(output_len > 0, "expected non-zero output");

        let out_json = {
            let guard = OUTPUT_BUF.lock().unwrap();
            String::from_utf8(guard.as_ref().unwrap().clone()).unwrap()
        };
        logos_free_output();

        let out: FlexOutput = serde_json::from_str(&out_json).unwrap();
        assert_eq!(out.children.len(), 1);
        assert_eq!(out.children[0].id, 42);
    }

    #[test]
    fn abi_grid_round_trip() {
        let input = r#"{"container_width":600,"container_height":400,"columns":[{"type":"fixed","value":200},{"type":"fixed","value":300}],"rows":[{"type":"fixed","value":100}],"children":[{"id":1},{"id":2}]}"#;
        let bytes = input.as_bytes();

        let output_len = unsafe {
            let ptr = logos_alloc(bytes.len() as u32);
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            let out_len = logos_calc_grid_layout(ptr, bytes.len() as u32);
            logos_free_input(ptr, bytes.len() as u32);
            out_len
        };

        assert!(output_len > 0);

        logos_free_output();
    }

    #[test]
    fn abi_bad_json_returns_zero() {
        let bad = b"not valid json";
        let output_len = unsafe {
            let ptr = logos_alloc(bad.len() as u32);
            std::ptr::copy_nonoverlapping(bad.as_ptr(), ptr, bad.len());
            let out_len = logos_calc_flex_layout(ptr, bad.len() as u32);
            logos_free_input(ptr, bad.len() as u32);
            out_len
        };
        assert_eq!(output_len, 0);
    }
}
