// Logos Text Shader — instanced rendering of textured glyph quads.
//
// Each instance provides: position, size, uv_min, uv_max, color.
// A unit quad (0,0)→(1,1) is transformed per-instance and samples
// from a glyph atlas texture.

// ─── Camera uniform (shared with rect pipeline) ─────────────────────
struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// ─── Atlas texture + sampler ────────────────────────────────────────
@group(1) @binding(0)
var atlas_texture: texture_2d<f32>;
@group(1) @binding(1)
var atlas_sampler: sampler;

// ─── Vertex I/O ─────────────────────────────────────────────────────
struct VertexInput {
    // Per-vertex (unit quad)
    @location(0) quad_pos: vec2<f32>,

    // Per-instance
    @location(1) inst_position: vec2<f32>,
    @location(2) inst_size: vec2<f32>,
    @location(3) inst_uv_min: vec2<f32>,
    @location(4) inst_uv_max: vec2<f32>,
    @location(5) inst_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

// ─── Vertex shader ──────────────────────────────────────────────────
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    // World position: quad_pos ∈ [0,1] scaled to glyph size + offset.
    let world_pos = in.inst_position + in.quad_pos * in.inst_size;

    // Interpolate UV across the glyph's atlas region.
    let uv = mix(in.inst_uv_min, in.inst_uv_max, in.quad_pos);

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    out.uv = uv;
    out.color = in.inst_color;
    return out;
}

// ─── Fragment shader (alpha-tested glyph) ───────────────────────────
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample the alpha channel from the atlas.
    let atlas_sample = textureSample(atlas_texture, atlas_sampler, in.uv);
    let alpha = atlas_sample.a;

    // Apply text color with atlas alpha.
    if alpha < 0.004 {
        discard;
    }

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
