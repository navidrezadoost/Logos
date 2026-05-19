/*
 * render-webgpu/shaders/transformer.wgsl
 *
 * Phase 5.5 — Local LLM: transformer inference kernels.
 *
 * Decoder-only transformer, LLaMA-family architecture (RMSNorm + RoPE + SwiGLU).
 *
 * Unified binding layout — all entry points share the same four data bindings:
 *
 *   @group(0) @binding(0)  uniforms  LLMUniforms (uniform, 64 B)
 *   @group(0) @binding(1)  buf_rw    array<f32>  (read-write  — primary activation)
 *   @group(0) @binding(2)  buf_r0    array<f32>  (read-only   — first  weight/source)
 *   @group(0) @binding(3)  buf_r1    array<f32>  (read-only   — second weight/source)
 *   @group(0) @binding(4)  buf_rw2   array<f32>  (read-write  — secondary output)
 *
 * Token IDs are passed as f32 (ids < 8 192 are exactly representable in f32).
 * Argmax result is stored as f32 in buf_rw[0] and cast to u32 on CPU readback.
 *
 * Entry points:
 *   cs_embed        token-ID lookup into embedding table
 *   cs_rms_norm     RMS layer normalisation
 *   cs_matmul       tiled matrix multiply  A(M×K) × B(K×N) → C(M×N)
 *   cs_add_res      elementwise in-place add  buf_rw += buf_r0
 *   cs_rope         Rotary Position Embedding applied in-place to Q or K
 *   cs_attn_scores  scaled dot-product attention scores (causal)
 *   cs_attn_softmax row-wise in-place softmax
 *   cs_silu_gate    SwiGLU: buf_rw[i] *= silu(buf_r0[i])
 *   cs_argmax       greedy: buf_rw[0] = f32(argmax(buf_r0))
 *
 * References:
 *   Vaswani et al. "Attention Is All You Need" (2017)
 *   Su et al. "RoFormer" (2021)
 *   Touvron et al. "LLaMA" (2023)
 *   WebGPU Shading Language spec §11
 */

// ─── Uniforms ────────────────────────────────────────────────────────────────

struct LLMUniforms {
    seq_len:    u32,
    vocab_sz:   u32,
    d_model:    u32,
    d_ff:       u32,
    n_heads:    u32,
    head_dim:   u32,
    dim0:       u32,  // M for matmul / element count for elementwise ops
    dim1:       u32,  // K for matmul
    dim2:       u32,  // N for matmul
    rms_eps:    f32,
    pos_offset: u32,
    _p0:        u32,
    _p1:        u32,
    _p2:        u32,
    _p3:        u32,
    _p4:        u32,
};

@group(0) @binding(0) var<uniform>             u:       LLMUniforms;
@group(0) @binding(1) var<storage, read_write> buf_rw:  array<f32>;
@group(0) @binding(2) var<storage, read>       buf_r0:  array<f32>;
@group(0) @binding(3) var<storage, read>       buf_r1:  array<f32>;
@group(0) @binding(4) var<storage, read_write> buf_rw2: array<f32>;

// Workgroup shared memory (reused across separate dispatches).
var<workgroup> wg256a:  array<f32,  256>;
var<workgroup> wg256b:  array<f32,  256>;
var<workgroup> tile16a: array<f32,  256>; // 16×16
var<workgroup> tile16b: array<f32,  256>; // 16×16

const TILE: u32 = 16u;

// ─── cs_embed ────────────────────────────────────────────────────────────────
// buf_r0  = token IDs as f32[seq]
// buf_r1  = embedding table  (vocab × d_model)
// buf_rw  = hidden state     (seq × d_model)  — OUTPUT
// Dispatch: (ceil(seq/64), 1, 1)

@compute @workgroup_size(64)
fn cs_embed(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pos = gid.x;
    if (pos >= u.seq_len) { return; }
    let tid  = u32(buf_r0[pos]);
    let src  = tid * u.d_model;
    let dst  = pos * u.d_model;
    for (var d: u32 = 0u; d < u.d_model; d++) {
        buf_rw[dst + d] = buf_r1[src + d];
    }
}

// ─── cs_rms_norm ─────────────────────────────────────────────────────────────
// buf_r0  = input   (seq × d_model)
// buf_r1  = weight  (d_model,)
// buf_rw  = output  (seq × d_model)  — OUTPUT
// Dispatch: (seq_len, 1, 1)  — 1 workgroup of 256 threads per row

@compute @workgroup_size(256)
fn cs_rms_norm(
    @builtin(local_invocation_id) lid:  vec3<u32>,
    @builtin(workgroup_id)        wgid: vec3<u32>,
) {
    let row   = wgid.x;
    let lane  = lid.x;
    let dm    = u.d_model;
    let off   = row * dm;

    var ss: f32 = 0.0;
    var d = lane;
    while (d < dm) { let v = buf_r0[off + d]; ss += v * v; d += 256u; }
    wg256a[lane] = ss;
    workgroupBarrier();
    for (var s: u32 = 128u; s > 0u; s >>= 1u) {
        if (lane < s) { wg256a[lane] += wg256a[lane + s]; }
        workgroupBarrier();
    }
    let inv = 1.0 / sqrt(wg256a[0] / f32(dm) + u.rms_eps);
    workgroupBarrier();
    d = lane;
    while (d < dm) { buf_rw[off + d] = buf_r0[off + d] * inv * buf_r1[d]; d += 256u; }
}

// ─── cs_matmul ───────────────────────────────────────────────────────────────
// buf_r0 = A (M × K),  buf_r1 = B (K × N),  buf_rw = C (M × N) — OUTPUT
// uniforms: dim0=M, dim1=K, dim2=N
// Dispatch: (ceil(N/16), ceil(M/16), 1)

@compute @workgroup_size(16, 16)
fn cs_matmul(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_id)  lid: vec3<u32>,
) {
    let row = gid.y; let col = gid.x;
    let ty  = lid.y; let tx  = lid.x;
    let M = u.dim0; let K = u.dim1; let N = u.dim2;

    var acc: f32 = 0.0;
    for (var t: u32 = 0u; t < (K + TILE - 1u) / TILE; t++) {
        let ac = t * TILE + tx;
        tile16a[ty * TILE + tx] = select(0.0, buf_r0[row * K + ac],  row < M && ac < K);
        let br = t * TILE + ty;
        tile16b[ty * TILE + tx] = select(0.0, buf_r1[br  * N + col], br  < K && col < N);
        workgroupBarrier();
        for (var k: u32 = 0u; k < TILE; k++) {
            acc += tile16a[ty * TILE + k] * tile16b[k * TILE + tx];
        }
        workgroupBarrier();
    }
    if (row < M && col < N) { buf_rw[row * N + col] = acc; }
}

// ─── cs_add_res ──────────────────────────────────────────────────────────────
// buf_rw[i] += buf_r0[i]   (residual stream accumulation)
// dim0 = total element count
// Dispatch: (ceil(dim0/64), 1, 1)

@compute @workgroup_size(64)
fn cs_add_res(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= u.dim0) { return; }
    buf_rw[i] += buf_r0[i];
}

// ─── cs_rope ─────────────────────────────────────────────────────────────────
// buf_rw = Q or K  (seq × n_heads × head_dim)  in-place rotation
// pos_offset: base token position (for KV cache incremental decoding)
// Dispatch: (ceil(seq/64), n_heads, 1)

@compute @workgroup_size(64)
fn cs_rope(@builtin(global_invocation_id) gid: vec3<u32>) {
    let tok  = gid.x;
    let head = gid.y;
    if (tok >= u.seq_len || head >= u.n_heads) { return; }
    let pos  = tok + u.pos_offset;
    let hd   = u.head_dim;
    let base = (tok * u.n_heads + head) * hd;
    for (var i: u32 = 0u; i < hd / 2u; i++) {
        let theta = f32(pos) / pow(10000.0, 2.0 * f32(i) / f32(hd));
        let c = cos(theta); let s = sin(theta);
        let x0 = buf_rw[base + 2u * i];
        let x1 = buf_rw[base + 2u * i + 1u];
        buf_rw[base + 2u * i]      = x0 * c - x1 * s;
        buf_rw[base + 2u * i + 1u] = x0 * s + x1 * c;
    }
}

// ─── cs_attn_scores ──────────────────────────────────────────────────────────
// buf_r0 = Q (seq × n_heads × head_dim)
// buf_r1 = K (seq × n_heads × head_dim)
// buf_rw = scores (n_heads × seq × seq)  — OUTPUT
// Dispatch: (ceil(seq/16), ceil(seq/16), n_heads)

@compute @workgroup_size(16, 16)
fn cs_attn_scores(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_id)  lid: vec3<u32>,
) {
    let k_idx = gid.x; let q_idx = gid.y; let head = gid.z;
    let ty = lid.y;    let tx    = lid.x;
    let sl = u.seq_len; let hd = u.head_dim; let nh = u.n_heads;

    var acc: f32 = 0.0;
    for (var t: u32 = 0u; t < (hd + TILE - 1u) / TILE; t++) {
        let qd = t * TILE + tx;
        tile16a[ty * TILE + tx] = select(0.0, buf_r0[(q_idx * nh + head) * hd + qd], q_idx < sl && qd < hd);
        let kd = t * TILE + ty;
        tile16b[ty * TILE + tx] = select(0.0, buf_r1[(k_idx * nh + head) * hd + kd], k_idx < sl && kd < hd);
        workgroupBarrier();
        for (var d: u32 = 0u; d < TILE; d++) {
            acc += tile16a[ty * TILE + d] * tile16b[d * TILE + tx];
        }
        workgroupBarrier();
    }
    if (q_idx < sl && k_idx < sl) {
        let v = select(acc / sqrt(f32(hd)), -3.4028235e+38, k_idx > q_idx);
        buf_rw[(head * sl + q_idx) * sl + k_idx] = v;
    }
}

// ─── cs_attn_softmax ─────────────────────────────────────────────────────────
// buf_rw = scores (n_heads × seq × seq) in-place softmax per row
// Dispatch: (seq_len, n_heads, 1)

@compute @workgroup_size(64)
fn cs_attn_softmax(
    @builtin(local_invocation_id) lid:  vec3<u32>,
    @builtin(workgroup_id)        wgid: vec3<u32>,
) {
    let q_idx = wgid.x; let head = wgid.y; let lane = lid.x;
    let sl    = u.seq_len;
    let roff  = (head * sl + q_idx) * sl;

    var lmax: f32 = -3.4028235e+38;
    var k = lane;
    while (k < sl) { lmax = max(lmax, buf_rw[roff + k]); k += 64u; }
    wg256a[lane] = lmax;
    workgroupBarrier();
    for (var s: u32 = 32u; s > 0u; s >>= 1u) {
        if (lane < s) { wg256a[lane] = max(wg256a[lane], wg256a[lane + s]); }
        workgroupBarrier();
    }
    let rmax = wg256a[0];
    workgroupBarrier();

    var lsum: f32 = 0.0;
    k = lane;
    while (k < sl) {
        let e = exp(buf_rw[roff + k] - rmax);
        buf_rw[roff + k] = e; lsum += e; k += 64u;
    }
    wg256a[lane] = lsum;
    workgroupBarrier();
    for (var s: u32 = 32u; s > 0u; s >>= 1u) {
        if (lane < s) { wg256a[lane] += wg256a[lane + s]; }
        workgroupBarrier();
    }
    let inv = 1.0 / wg256a[0];
    workgroupBarrier();
    k = lane;
    while (k < sl) { buf_rw[roff + k] *= inv; k += 64u; }
}

// ─── cs_silu_gate ────────────────────────────────────────────────────────────
// SwiGLU: buf_rw[i] *= silu(buf_r0[i])   where silu(x) = x·σ(x)
// dim0 = total element count (seq_len × d_ff)
// Dispatch: (ceil(dim0/64), 1, 1)

@compute @workgroup_size(64)
fn cs_silu_gate(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= u.dim0) { return; }
    let x = buf_r0[i];
    buf_rw[i] = buf_rw[i] * (x / (1.0 + exp(-x)));
}

// ─── cs_argmax ───────────────────────────────────────────────────────────────
// buf_r0  = logits  (vocab_sz,)
// buf_rw[0] = argmax token id as f32   — OUTPUT
// Dispatch: (1, 1, 1)

@compute @workgroup_size(256)
fn cs_argmax(@builtin(local_invocation_id) lid: vec3<u32>) {
    let lane = lid.x; let vsz = u.vocab_sz;
    var bv: f32 = -3.4028235e+38; var bi: u32 = 0u;
    var i = lane;
    while (i < vsz) {
        let v = buf_r0[i];
        if (v > bv) { bv = v; bi = i; }
        i += 256u;
    }
    wg256a[lane] = bv; wg256b[lane] = f32(bi);
    workgroupBarrier();
    for (var s: u32 = 128u; s > 0u; s >>= 1u) {
        if (lane < s && wg256a[lane + s] > wg256a[lane]) {
            wg256a[lane] = wg256a[lane + s];
            wg256b[lane] = wg256b[lane + s];
        }
        workgroupBarrier();
    }
    if (lane == 0u) { buf_rw[0] = wg256b[0]; }
}
