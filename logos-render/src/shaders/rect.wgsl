// Logos Rect Shader — instanced rendering of rounded rectangles.
//
// Each instance provides: position, size, color, border_radius, z_index.
// A unit quad (0,0)→(1,1) is transformed per-instance.

// ─── Camera uniform ─────────────────────────────────────────────────
struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// ─── Vertex I/O ─────────────────────────────────────────────────────
struct VertexInput {
    // Per-vertex
    @location(0) quad_pos: vec2<f32>,

    // Per-instance
    @location(1) inst_position: vec2<f32>,
    @location(2) inst_size: vec2<f32>,
    @location(3) inst_color: vec4<f32>,
    @location(4) inst_border_radius: f32,
    @location(5) inst_z_index: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_uv: vec2<f32>,      // [0,1] within the rect
    @location(2) rect_size: vec2<f32>,      // pixel size for SDF
    @location(3) border_radius: f32,
};

// ─── Vertex shader ──────────────────────────────────────────────────
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    // World position: quad_pos ∈ [0,1] scaled to instance size + offset
    let world_pos = in.inst_position + in.quad_pos * in.inst_size;

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, in.inst_z_index, 1.0);
    out.color = in.inst_color;
    out.local_uv = in.quad_pos;
    out.rect_size = in.inst_size;
    out.border_radius = in.inst_border_radius;
    return out;
}

// ─── Fragment shader (with rounded-corner SDF) ──────────────────────
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let half_size = in.rect_size * 0.5;
    let center = in.local_uv * in.rect_size - half_size;
    let r = min(in.border_radius, min(half_size.x, half_size.y));

    // Signed distance to rounded rectangle
    let q = abs(center) - half_size + vec2<f32>(r, r);
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;

    // Anti-aliased edge (1px feather)
    let alpha = 1.0 - smoothstep(-0.5, 0.5, d);

    if alpha < 0.001 {
        discard;
    }

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
