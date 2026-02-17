// Logos Enhanced Rect Shader — rounded rectangles with MSAA-compatible
// analytical anti-aliasing, gradient fills, and drop shadows.
//
// References:
// - Akenine-Möller, Real-Time Rendering, Ch. 5 (anti-aliasing)
// - Akenine-Möller, Real-Time Rendering, Ch. 6 (textures / gradients)
// - Inigo Quilez, 2D SDF functions (rounded-box SDF)

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

    // Per-instance  (80 bytes)
    @location(1) inst_position: vec2<f32>,
    @location(2) inst_size: vec2<f32>,
    @location(3) inst_color: vec4<f32>,
    @location(4) inst_border_radius: f32,
    @location(5) inst_z_index: f32,
    // Gradient
    @location(6) inst_grad_color: vec4<f32>,    // second gradient stop
    @location(7) inst_grad_params: vec4<f32>,   // (angle_rad, type, 0, 0)
    // Shadow
    @location(8) inst_shadow_color: vec4<f32>,
    @location(9) inst_shadow_params: vec4<f32>, // (offset_x, offset_y, blur_radius, spread)
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_uv: vec2<f32>,       // [0,1] within the rect
    @location(2) rect_size: vec2<f32>,       // pixel size for SDF
    @location(3) border_radius: f32,
    // Gradient
    @location(4) grad_color: vec4<f32>,
    @location(5) grad_params: vec4<f32>,
    // Shadow
    @location(6) shadow_color: vec4<f32>,
    @location(7) shadow_params: vec4<f32>,
};

// ─── Vertex shader ──────────────────────────────────────────────────
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    // Expand the quad to accommodate shadow extent
    let shadow_extent = in.inst_shadow_params.z + abs(in.inst_shadow_params.w)
                      + max(abs(in.inst_shadow_params.x), abs(in.inst_shadow_params.y));
    let expanded_size = in.inst_size + vec2<f32>(shadow_extent * 2.0, shadow_extent * 2.0);
    let expanded_pos = in.inst_position - vec2<f32>(shadow_extent, shadow_extent);

    let world_pos = expanded_pos + in.quad_pos * expanded_size;

    // Remap UV so that (0,0)→(1,1) maps to the *original* rect within the expanded quad
    let uv = (in.quad_pos * expanded_size - vec2<f32>(shadow_extent, shadow_extent)) / in.inst_size;

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, in.inst_z_index, 1.0);
    out.color = in.inst_color;
    out.local_uv = uv;
    out.rect_size = in.inst_size;
    out.border_radius = in.inst_border_radius;
    out.grad_color = in.inst_grad_color;
    out.grad_params = in.inst_grad_params;
    out.shadow_color = in.inst_shadow_color;
    out.shadow_params = in.inst_shadow_params;
    return out;
}

// ─── SDF: rounded rectangle ────────────────────────────────────────
fn sdf_rounded_rect(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// ─── Gradient evaluation ───────────────────────────────────────────
fn eval_gradient(uv: vec2<f32>, base_color: vec4<f32>, grad_color: vec4<f32>, params: vec4<f32>) -> vec4<f32> {
    let grad_type = params.y;

    // grad_type == 0: no gradient (solid fill)
    if grad_type < 0.5 {
        return base_color;
    }

    var t: f32 = 0.0;

    if grad_type < 1.5 {
        // Linear gradient: t = dot(uv - 0.5, direction) + 0.5
        let angle = params.x;
        let dir = vec2<f32>(sin(angle), -cos(angle));
        t = dot(uv - vec2<f32>(0.5, 0.5), dir) + 0.5;
    } else {
        // Radial gradient: t = distance from center
        let d = (uv - vec2<f32>(0.5, 0.5)) * 2.0;
        t = length(d);
    }

    t = clamp(t, 0.0, 1.0);
    return mix(base_color, grad_color, t);
}

// ─── Approximate Gaussian for shadow blur ──────────────────────────
// Uses smoothstep with σ-scaled falloff for GPU-friendly shadow.
fn shadow_alpha(d: f32, sigma: f32) -> f32 {
    if sigma < 0.001 {
        return select(0.0, 1.0, d < 0.0);
    }
    return 1.0 - smoothstep(-sigma * 1.5, sigma * 1.5, d);
}

// ─── Fragment shader ────────────────────────────────────────────────
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let half_size = in.rect_size * 0.5;
    let r = min(in.border_radius, min(half_size.x, half_size.y));

    // Current pixel in rect-local coordinates, centered
    let center = in.local_uv * in.rect_size - half_size;

    // ─── Drop shadow ────────────────────────────────────────
    let shadow_offset = in.shadow_params.xy;
    let blur_radius = in.shadow_params.z;
    let spread = in.shadow_params.w;

    var result = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    if in.shadow_color.a > 0.001 {
        // Shadow SDF: offset and spread-adjusted
        let shadow_p = center - shadow_offset;
        let shadow_half = half_size + vec2<f32>(spread, spread);
        let shadow_d = sdf_rounded_rect(shadow_p, shadow_half, r + max(spread, 0.0));
        let sigma = blur_radius * 0.5;
        let sa = shadow_alpha(shadow_d, sigma) * in.shadow_color.a;
        result = vec4<f32>(in.shadow_color.rgb, sa);
    }

    // ─── Fill shape ─────────────────────────────────────────
    let d = sdf_rounded_rect(center, half_size, r);

    // Analytical anti-aliasing: use fwidth() for screen-space derivative
    // Akenine-Möller Ch. 5 — screen-space edge anti-aliasing
    let fw = fwidth(d);
    let aa_width = max(fw, 0.5); // at least 0.5px feather
    let alpha = 1.0 - smoothstep(-aa_width, aa_width, d);

    if alpha < 0.001 && result.a < 0.001 {
        discard;
    }

    // Evaluate gradient
    let clamped_uv = clamp(in.local_uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let fill_color = eval_gradient(clamped_uv, in.color, in.grad_color, in.grad_params);

    // Composite: shadow behind, fill on top
    let fill = vec4<f32>(fill_color.rgb, fill_color.a * alpha);

    // Porter-Duff "over" compositing
    let out_a = fill.a + result.a * (1.0 - fill.a);
    if out_a < 0.001 {
        discard;
    }
    let out_rgb = (fill.rgb * fill.a + result.rgb * result.a * (1.0 - fill.a)) / out_a;

    return vec4<f32>(out_rgb, out_a);
}
