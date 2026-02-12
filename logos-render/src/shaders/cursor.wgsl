// Logos Cursor Shader — instanced rendering of remote cursors.
//
// Each instance provides: position (world coords), color, selection_rect.
// Renders a cursor pointer shape using SDF, with optional selection highlight.
//
// Reference: Akenine-Möller, Real-Time Rendering, Section 18.6

// ─── Camera uniform ─────────────────────────────────────────────────
struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// ─── Vertex I/O ─────────────────────────────────────────────────────
struct VertexInput {
    // Per-vertex (unit quad)
    @location(0) quad_pos: vec2<f32>,

    // Per-instance
    @location(1) inst_position: vec2<f32>,   // cursor world position
    @location(2) inst_color: vec4<f32>,      // cursor RGBA
    @location(3) inst_selection: vec4<f32>,  // selection rect (x, y, w, h)
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_uv: vec2<f32>,
};

// ─── Constants ──────────────────────────────────────────────────────
const CURSOR_SIZE: f32 = 20.0;   // pixels
const Z_CURSOR: f32 = 900.0;     // above everything except UI

// ─── Vertex shader ──────────────────────────────────────────────────
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    // Position the unit quad as a CURSOR_SIZE×CURSOR_SIZE square
    // at the cursor position, with slight offset so tip is at position
    let offset = in.quad_pos * vec2<f32>(CURSOR_SIZE, CURSOR_SIZE);
    let world_pos = in.inst_position + offset;

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, Z_CURSOR, 1.0);
    out.color = in.inst_color;
    out.local_uv = in.quad_pos;
    return out;
}

// ─── Fragment shader (cursor pointer SDF) ───────────────────────────
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.local_uv;

    // Cursor pointer shape:
    // Triangle pointing down-right from top-left corner
    //   (0,0) ──► (0.4, 0)
    //     │         ╱
    //     │       ╱
    //     │     ╱
    //     ▼   ╱
    //   (0, 0.7) ─ (0.2, 0.5)
    //
    // Using signed distance to triangle edges

    // Arrow body: triangle from (0,0) to (0, 0.7) to (0.35, 0.55)
    let p = uv;

    // Edge 1: left edge (x = 0.05, small margin)
    let e1 = p.x - 0.05;

    // Edge 2: bottom-left to tip diagonal
    // Line from (0.05, 0.75) to (0.4, 0.55)
    let e2 = (p.y - 0.05) - p.x * 1.5;

    // Edge 3: top to tip diagonal
    // Line from (0.05, 0.05) to (0.4, 0.55)
    let e3 = p.x * 1.4 - (p.y - 0.05);

    // Inside the arrow if all edges > 0
    let inside = step(0.0, e1) * step(0.0, -e2 + 0.65) * step(0.0, e3);

    // Anti-aliased edges
    let aa1 = 1.0 - smoothstep(-0.02, 0.02, -e1);
    let aa2 = 1.0 - smoothstep(-0.02, 0.02, e2 - 0.65);
    let aa3 = 1.0 - smoothstep(-0.02, 0.02, -e3);
    let alpha = aa1 * aa2 * aa3;

    if alpha < 0.01 {
        discard;
    }

    // White outline for visibility on any background
    let outline_alpha = alpha;
    let inner_alpha = aa1 * aa2 * aa3;

    // Slight dark border for contrast
    let border_d = min(min(e1, 0.65 - e2), e3);
    let border = 1.0 - smoothstep(0.0, 0.06, border_d);

    let final_color = mix(in.color.rgb, vec3<f32>(1.0, 1.0, 1.0), border * 0.3);

    return vec4<f32>(final_color, alpha * in.color.a);
}
