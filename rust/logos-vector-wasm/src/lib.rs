// logos-vector-wasm/src/lib.rs
//
// WASM bridge for logos-vector + logos-vector-ops.
//
// ## Memory protocol  (identical to logos-layout-wasm)
//
//   1. JS calls `logos_vn_alloc(len)` — allocates a byte buffer, returns ptr.
//   2. JS writes UTF-8 JSON into WASM memory at the returned pointer.
//   3. JS calls `logos_vn_boolean_op(ptr, len)` or `logos_vn_find_regions(ptr, len)`.
//      Rust reads + parses JSON, runs the operation, serialises the result to
//      JSON, stores it in OUTPUT_BUF, returns the output byte length.
//   4. JS reads `logos_vn_output_ptr()` for exactly `len_returned` bytes.
//   5. JS calls `logos_vn_free_input(ptr, len)` and `logos_vn_free_output()`.
//
// ## JSON schemas
//
// ### `logos_vn_boolean_op` — boolean set operation on two regions
//
// **Input**
// ```json
// {
//   "net_a":   { "anchors": [{"x":0,"y":0,"hi":null,"ho":null},...],
//                "segments": [{"s":0,"e":1,"c1":null,"c2":null},...] },
//   "net_b":   { ... },
//   "region_a": [0, 1, 2],
//   "region_b": [0, 1, 2],
//   "op": "union" | "intersect" | "subtract" | "exclude"
// }
// ```
//
// **Output** (success)
// ```json
// { "ok": true,
//   "anchors":  [{"x":f64,"y":f64,"hi":null,"ho":null}, ...],
//   "segments": [{"s":usize,"e":usize,"c1":null,"c2":null}, ...],
//   "regions":  [[0,1,2,3,...], ...] }
// ```
//
// **Output** (error)
// ```json
// { "ok": false, "error": "description" }
// ```
//
// ### `logos_vn_find_regions` — detect closed regions in a network
//
// **Input**
// ```json
// { "net": { "anchors": [...], "segments": [...] } }
// ```
//
// **Output** (success)
// ```json
// { "ok": true, "regions": [[seg_ids...], ...] }
// ```

use std::alloc::{alloc, dealloc, Layout};
use std::sync::Mutex;

use logos_vector::{Region, VectorNetwork};
use logos_vector_ops::{boolean_op, BoolOp, BoolResult};
use serde::{Deserialize, Serialize};

// =============================================================================
// Output buffer
// =============================================================================

const ALLOC_ALIGN: usize = 8;

static OUTPUT_BUF: Mutex<Option<Vec<u8>>> = Mutex::new(None);

fn write_output(bytes: Vec<u8>) -> u32 {
    let len = bytes.len() as u32;
    *OUTPUT_BUF.lock().unwrap() = Some(bytes);
    len
}

fn write_error(msg: &str) -> u32 {
    let json = format!(r#"{{"ok":false,"error":{}}}"#, serde_json::to_string(msg).unwrap());
    write_output(json.into_bytes())
}

// =============================================================================
// Memory management (C-ABI)
// =============================================================================

/// Allocate `len` bytes for an input buffer.
#[no_mangle]
pub unsafe extern "C" fn logos_vn_alloc(len: u32) -> *mut u8 {
    let layout = Layout::from_size_align_unchecked(len as usize, ALLOC_ALIGN);
    let ptr = alloc(layout);
    if ptr.is_null() {
        panic!("logos_vn_alloc: OOM for len={len}");
    }
    ptr
}

/// Free an input buffer.
#[no_mangle]
pub unsafe extern "C" fn logos_vn_free_input(ptr: *mut u8, len: u32) {
    let layout = Layout::from_size_align_unchecked(len as usize, ALLOC_ALIGN);
    dealloc(ptr, layout);
}

/// Pointer to the output buffer written by the last operation.
#[no_mangle]
pub extern "C" fn logos_vn_output_ptr() -> *const u8 {
    OUTPUT_BUF.lock().unwrap().as_ref().map(|v| v.as_ptr()).unwrap_or(std::ptr::null())
}

/// Release the output buffer.
#[no_mangle]
pub extern "C" fn logos_vn_free_output() {
    *OUTPUT_BUF.lock().unwrap() = None;
}

// =============================================================================
// JSON types — VectorNetwork representation
// =============================================================================

/// Serialised anchor. `hi` = handle_in, `ho` = handle_out.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnchorJson {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub hi: Option<[f64; 2]>,
    #[serde(default)]
    pub ho: Option<[f64; 2]>,
}

/// Serialised segment. `s` = start_anchor, `e` = end_anchor.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SegmentJson {
    pub s: usize,
    pub e: usize,
    #[serde(default)]
    pub c1: Option<[f64; 2]>,
    #[serde(default)]
    pub c2: Option<[f64; 2]>,
}

/// Serialised VectorNetwork (anchors + segments, no regions — regions are
/// computed separately or returned by operations).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VnetJson {
    pub anchors:  Vec<AnchorJson>,
    pub segments: Vec<SegmentJson>,
}

/// Serialisation output that includes detected regions.
#[derive(Serialize, Deserialize)]
struct VnetOutputJson {
    pub ok:       bool,
    pub anchors:  Vec<AnchorJson>,
    pub segments: Vec<SegmentJson>,
    pub regions:  Vec<Vec<usize>>,
}

#[derive(Serialize, Deserialize)]
struct FindRegionsOutputJson {
    pub ok:      bool,
    pub regions: Vec<Vec<usize>>,
}

// =============================================================================
// Helpers — convert between VnetJson ↔ VectorNetwork
// =============================================================================

fn vnet_from_json(j: &VnetJson) -> VectorNetwork {
    let mut net = VectorNetwork::new();

    for a in &j.anchors {
        let hi = a.hi.map(|p| (p[0], p[1]));
        let ho = a.ho.map(|p| (p[0], p[1]));
        if hi.is_some() || ho.is_some() {
            net.add_anchor_with_handles(a.x, a.y, hi, ho);
        } else {
            net.add_anchor(a.x, a.y);
        }
    }

    for s in &j.segments {
        let _ = match (s.c1, s.c2) {
            (Some(c1), Some(c2)) => net.add_cubic_segment(s.s, s.e, (c1[0], c1[1]), (c2[0], c2[1])),
            _ => net.add_segment(s.s, s.e),
        };
    }

    net
}

fn vnet_to_json(net: &VectorNetwork) -> VnetJson {
    let anchors: Vec<AnchorJson> = net
        .anchors()
        .map(|(_, a)| AnchorJson {
            x:  a.x,
            y:  a.y,
            hi: a.handle_in.map(|(x, y)| [x, y]),
            ho: a.handle_out.map(|(x, y)| [x, y]),
        })
        .collect();

    let segments: Vec<SegmentJson> = net
        .segments()
        .map(|(_, s)| SegmentJson {
            s:  s.start_anchor,
            e:  s.end_anchor,
            c1: s.control1.map(|(x, y)| [x, y]),
            c2: s.control2.map(|(x, y)| [x, y]),
        })
        .collect();

    VnetJson { anchors, segments }
}

fn regions_to_json(regions: &[Region]) -> Vec<Vec<usize>> {
    regions.iter().map(|r| r.boundary.clone()).collect()
}

// =============================================================================
// logos_vn_boolean_op
// =============================================================================

/// Input JSON for the boolean op endpoint.
#[derive(Serialize, Deserialize)]
struct BoolOpInput {
    pub net_a:    VnetJson,
    pub net_b:    VnetJson,
    pub region_a: Vec<usize>,
    pub region_b: Vec<usize>,
    pub op:       String,
}

/// Perform a boolean set operation on two network regions.
///
/// Returns the byte length of the output JSON written to `OUTPUT_BUF`.
/// Call `logos_vn_output_ptr()` to read the JSON.
#[no_mangle]
pub unsafe extern "C" fn logos_vn_boolean_op(ptr: *const u8, len: u32) -> u32 {
    let slice = std::slice::from_raw_parts(ptr, len as usize);
    let input: BoolOpInput = match serde_json::from_slice(slice) {
        Ok(v)  => v,
        Err(e) => return write_error(&format!("parse error: {e}")),
    };

    let bool_op = match input.op.as_str() {
        "union"     => BoolOp::Union,
        "intersect" => BoolOp::Intersect,
        "subtract"  => BoolOp::Subtract,
        "exclude"   => BoolOp::Exclude,
        other       => return write_error(&format!("unknown op: {other}")),
    };

    let net_a = vnet_from_json(&input.net_a);
    let net_b = vnet_from_json(&input.net_b);
    let region_a = Region::with_boundary(input.region_a, None);
    let region_b = Region::with_boundary(input.region_b, None);

    let result: BoolResult = boolean_op(&net_a, &region_a, &net_b, &region_b, bool_op);

    let vj = vnet_to_json(&result.network);
    let regions = regions_to_json(&result.regions);

    let out = VnetOutputJson {
        ok: true,
        anchors:  vj.anchors,
        segments: vj.segments,
        regions,
    };

    write_output(serde_json::to_vec(&out).unwrap_or_default())
}

// =============================================================================
// logos_vn_find_regions
// =============================================================================

/// Input JSON for the find-regions endpoint.
#[derive(Serialize, Deserialize)]
struct FindRegionsInput {
    pub net: VnetJson,
}

/// Detect all closed regions in a vector network via DCEL cycle detection.
///
/// Returns the byte length of the output JSON.
#[no_mangle]
pub unsafe extern "C" fn logos_vn_find_regions(ptr: *const u8, len: u32) -> u32 {
    let slice = std::slice::from_raw_parts(ptr, len as usize);
    let input: FindRegionsInput = match serde_json::from_slice(slice) {
        Ok(v)  => v,
        Err(e) => return write_error(&format!("parse error: {e}")),
    };

    let mut net = vnet_from_json(&input.net);
    let regions = net.find_regions();
    let region_vecs = regions_to_json(regions);

    let out = FindRegionsOutputJson { ok: true, regions: region_vecs };
    write_output(serde_json::to_vec(&out).unwrap_or_default())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sq_net_json(x0: f64, y0: f64, x1: f64, y1: f64) -> VnetJson {
        VnetJson {
            anchors: vec![
                AnchorJson { x: x0, y: y0, hi: None, ho: None },
                AnchorJson { x: x1, y: y0, hi: None, ho: None },
                AnchorJson { x: x1, y: y1, hi: None, ho: None },
                AnchorJson { x: x0, y: y1, hi: None, ho: None },
            ],
            segments: vec![
                SegmentJson { s: 0, e: 1, c1: None, c2: None },
                SegmentJson { s: 1, e: 2, c1: None, c2: None },
                SegmentJson { s: 2, e: 3, c1: None, c2: None },
                SegmentJson { s: 3, e: 0, c1: None, c2: None },
            ],
        }
    }

    fn run_bool_op(net_a: VnetJson, region_a: Vec<usize>,
                   net_b: VnetJson, region_b: Vec<usize>,
                   op: &str) -> VnetOutputJson
    {
        let input = BoolOpInput {
            net_a, net_b, region_a, region_b,
            op: op.to_string(),
        };
        let json = serde_json::to_vec(&input).unwrap();
        let ptr = json.as_ptr();
        let len = json.len() as u32;
        let out_len = unsafe { logos_vn_boolean_op(ptr, len) };
        let out_ptr = logos_vn_output_ptr();
        let out_slice = unsafe { std::slice::from_raw_parts(out_ptr, out_len as usize) };
        let out: VnetOutputJson = serde_json::from_slice(out_slice).unwrap();
        logos_vn_free_output();
        out
    }

    // ── Union ────────────────────────────────────────────────────────────────

    #[test]
    fn union_two_squares_via_abi() {
        // A = 3×3, B = tall 1×5 rect crossing A
        let na = sq_net_json(0.0, 0.0, 3.0, 3.0);
        let nb = VnetJson {
            anchors: vec![
                AnchorJson { x: 1.0, y: -1.0, hi: None, ho: None },
                AnchorJson { x: 2.0, y: -1.0, hi: None, ho: None },
                AnchorJson { x: 2.0, y:  4.0, hi: None, ho: None },
                AnchorJson { x: 1.0, y:  4.0, hi: None, ho: None },
            ],
            segments: vec![
                SegmentJson { s: 0, e: 1, c1: None, c2: None },
                SegmentJson { s: 1, e: 2, c1: None, c2: None },
                SegmentJson { s: 2, e: 3, c1: None, c2: None },
                SegmentJson { s: 3, e: 0, c1: None, c2: None },
            ],
        };
        let out = run_bool_op(na, vec![0,1,2,3], nb, vec![0,1,2,3], "union");
        assert!(out.ok);
        assert!(!out.regions.is_empty(), "union should produce at least one region");
    }

    // ── Intersect ────────────────────────────────────────────────────────────

    #[test]
    fn intersect_two_squares_via_abi() {
        let na = sq_net_json(0.0, 0.0, 3.0, 3.0);
        let nb = VnetJson {
            anchors: vec![
                AnchorJson { x: 1.0, y: -1.0, hi: None, ho: None },
                AnchorJson { x: 2.0, y: -1.0, hi: None, ho: None },
                AnchorJson { x: 2.0, y:  4.0, hi: None, ho: None },
                AnchorJson { x: 1.0, y:  4.0, hi: None, ho: None },
            ],
            segments: vec![
                SegmentJson { s: 0, e: 1, c1: None, c2: None },
                SegmentJson { s: 1, e: 2, c1: None, c2: None },
                SegmentJson { s: 2, e: 3, c1: None, c2: None },
                SegmentJson { s: 3, e: 0, c1: None, c2: None },
            ],
        };
        let out = run_bool_op(na, vec![0,1,2,3], nb, vec![0,1,2,3], "intersect");
        assert!(out.ok);
        assert!(!out.regions.is_empty(), "intersect should produce a region");
    }

    // ── find_regions round-trip ───────────────────────────────────────────────

    #[test]
    fn find_regions_triangle() {
        // Triangle: 3 anchors, 3 segments forming a closed loop
        let net = VnetJson {
            anchors: vec![
                AnchorJson { x: 0.0, y: 0.0, hi: None, ho: None },
                AnchorJson { x: 1.0, y: 0.0, hi: None, ho: None },
                AnchorJson { x: 0.5, y: 1.0, hi: None, ho: None },
            ],
            segments: vec![
                SegmentJson { s: 0, e: 1, c1: None, c2: None },
                SegmentJson { s: 1, e: 2, c1: None, c2: None },
                SegmentJson { s: 2, e: 0, c1: None, c2: None },
            ],
        };
        let input = FindRegionsInput { net };
        let json = serde_json::to_vec(&input).unwrap();
        let ptr = json.as_ptr();
        let len = json.len() as u32;
        let out_len = unsafe { logos_vn_find_regions(ptr, len) };
        let out_ptr = logos_vn_output_ptr();
        let out_slice = unsafe { std::slice::from_raw_parts(out_ptr, out_len as usize) };
        let out: FindRegionsOutputJson = serde_json::from_slice(out_slice).unwrap();
        logos_vn_free_output();
        assert!(out.ok);
        // A closed triangle has one region
        assert_eq!(out.regions.len(), 1, "triangle should have 1 region");
    }

    // ── Error handling ────────────────────────────────────────────────────────

    #[test]
    fn unknown_op_returns_error() {
        let na = sq_net_json(0.0, 0.0, 1.0, 1.0);
        let input = BoolOpInput {
            net_a: na.clone(), net_b: na.clone(),
            region_a: vec![0,1,2,3], region_b: vec![0,1,2,3],
            op: "xor_wrong".to_string(),
        };
        let json = serde_json::to_vec(&input).unwrap();
        let ptr = json.as_ptr();
        let len = json.len() as u32;
        let out_len = unsafe { logos_vn_boolean_op(ptr, len) };
        let out_ptr = logos_vn_output_ptr();
        let out_slice = unsafe { std::slice::from_raw_parts(out_ptr, out_len as usize) };
        let out: serde_json::Value = serde_json::from_slice(out_slice).unwrap();
        logos_vn_free_output();
        assert_eq!(out["ok"], false, "unknown op should return ok=false");
        assert!(out.get("error").is_some(), "should have error field");
    }

    #[test]
    fn alloc_free_roundtrip() {
        let len = 256_u32;
        let ptr = unsafe { logos_vn_alloc(len) };
        assert!(!ptr.is_null());
        unsafe { logos_vn_free_input(ptr, len) };
    }
}
