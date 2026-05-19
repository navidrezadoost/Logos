/**
 * render-webgpu/llm-pipeline.ts
 *
 * Phase 5.5 — Local LLM: transformer inference pipeline.
 *
 * Orchestrates the nine WGSL compute kernels from transformer.wgsl into a
 * complete autoregressive decoder loop:
 *
 *   Prompt tokens → [embed] → N × [RMSNorm + QKV proj + RoPE + Attention +
 *                                   RMSNorm + FFN (SwiGLU)] → [lm_head] →
 *                   [argmax] → next token → append → repeat until EOS
 *
 * Model architecture (LLaMA-family, read from weight header):
 *   • Decoder-only (GPT-style), causal attention
 *   • RMSNorm (no mean-centre, just RMS scaling + learned weight)
 *   • RoPE positional embeddings on Q and K
 *   • SwiGLU FFN (gate_proj × silu(up_proj) → down_proj)
 *
 * GPU buffer strategy:
 *   ┌ Static (init, from weights) ───────────────────────────────────────────┐
 *   │  embed_table     (vocab × d_model)  f32                                │
 *   │  Per-layer L:                                                           │
 *   │    attn_norm_w[L]  (d_model,)       f32  RMSNorm weight before attn    │
 *   │    q_proj[L]       (d_model × d_model) f32                             │
 *   │    k_proj[L]       (d_model × d_model) f32                             │
 *   │    v_proj[L]       (d_model × d_model) f32                             │
 *   │    o_proj[L]       (d_model × d_model) f32                             │
 *   │    ffn_norm_w[L]   (d_model,)       f32  RMSNorm weight before FFN     │
 *   │    ffn_gate[L]     (d_model × d_ff) f32                                │
 *   │    ffn_up[L]       (d_model × d_ff) f32                                │
 *   │    ffn_down[L]     (d_ff × d_model) f32                                │
 *   │  final_norm_w    (d_model,)         f32                                 │
 *   │  lm_head_w       (d_model × vocab)  f32                                 │
 *   └────────────────────────────────────────────────────────────────────────┘
 *   ┌ Activation (per inference, reused) ───────────────────────────────────┐
 *   │  token_ids_buf   (max_seq,)    f32  (token IDs cast to f32)           │
 *   │  hidden_buf      (seq × d_model)   f32  main activation buffer        │
 *   │  residual_buf    (seq × d_model)   f32  residual stream               │
 *   │  norm_buf        (seq × d_model)   f32  scratch for normalised hidden  │
 *   │  q_buf, k_buf, v_buf            f32  QKV projections                  │
 *   │  attn_out_buf    (seq × d_model)   f32  weighted V                    │
 *   │  scores_buf      (n_heads × seq × seq) f32  attention scores          │
 *   │  gate_buf        (seq × d_ff)      f32  gate branch                   │
 *   │  up_buf          (seq × d_ff)      f32  up branch                     │
 *   │  ffn_out_buf     (seq × d_model)   f32  FFN output                    │
 *   │  logits_buf      (vocab,)          f32  final logits                   │
 *   │  result_buf      (1,)              f32  argmax result                  │
 *   │  readback_buf    (1,)   MAP_READ staging for result_buf               │
 *   └────────────────────────────────────────────────────────────────────────┘
 *
 * All bind groups share a single GPUBindGroupLayout (5-binding: uniform +
 * 4×storage), updated per dispatch via different GPUBindGroup objects.
 * This avoids pipeline recreation and minimises CPU overhead per token.
 */

import transformerSource from "./shaders/transformer.wgsl?raw";
import { encode, decode, TOKEN_EOS, TOKEN_BOS, truncate, VOCAB_SIZE } from "./llm-tokenizer";
import { loadWeights, type LoadedWeights, type ModelConfig, type ProgressCallback } from "./llm-weights";

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/** Uniform buffer size: 16 × 4B = 64 bytes. */
const UNIFORM_BYTES = 64;

/** RMSNorm epsilon. */
const RMS_EPS = 1e-5;

/** Maximum new tokens to generate per request. */
const MAX_NEW_TOKENS = 128;

/** System prompt injected before every user message. */
const SYSTEM_PROMPT =
  "You are an AI design assistant for Logos. Output concise JSON shape " +
  "creation commands. Example: {\"action\":\"create\",\"type\":\"rect\"," +
  "\"name\":\"Button\",\"fills\":[{\"type\":\"solid\",\"color\":\"#89b4fa\"}]," +
  "\"bounds\":{\"x\":0,\"y\":0,\"w\":160,\"h\":48}}\n";

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

export type LLMLoadState =
  | { phase: "idle" }
  | { phase: "loading"; loaded: number; total: number; label: string }
  | { phase: "ready"; config: ModelConfig }
  | { phase: "error"; message: string };

export type TokenCallback = (token: string) => void;

// ─────────────────────────────────────────────────────────────────────────────
// Buffer helpers
// ─────────────────────────────────────────────────────────────────────────────

function mkBuf(
  device: GPUDevice,
  label: string,
  size: number,
  usage: GPUBufferUsageFlags,
): GPUBuffer {
  return device.createBuffer({ label, size: Math.max(size, 16), usage });
}

function uploadF32(device: GPUDevice, dst: GPUBuffer, data: Float32Array): void {
  device.queue.writeBuffer(dst, 0, data);
}

// ─────────────────────────────────────────────────────────────────────────────
// LLMPipeline
// ─────────────────────────────────────────────────────────────────────────────

export class LLMPipeline {
  // ── Device ──────────────────────────────────────────────────────────────────
  private device!: GPUDevice;

  // ── Compiled pipelines ──────────────────────────────────────────────────────
  private pipeEmbed!:       GPUComputePipeline;
  private pipeRmsNorm!:     GPUComputePipeline;
  private pipeMatmul!:      GPUComputePipeline;
  private pipeAddRes!:      GPUComputePipeline;
  private pipeRope!:        GPUComputePipeline;
  private pipeAttnScores!:  GPUComputePipeline;
  private pipeAttnSoftmax!: GPUComputePipeline;
  private pipeSiluGate!:    GPUComputePipeline;
  private pipeArgmax!:      GPUComputePipeline;

  // ── Bind group layout (shared by all pipelines) ──────────────────────────
  private bgl!: GPUBindGroupLayout;

  // ── Static weight buffers (one per tensor) ────────────────────────────────
  private embedTableBuf!: GPUBuffer;

  // Per-layer weight buffers (indexed by layer).
  private attnNormBufs!: GPUBuffer[];
  private qProjBufs!:    GPUBuffer[];
  private kProjBufs!:    GPUBuffer[];
  private vProjBufs!:    GPUBuffer[];
  private oProjBufs!:    GPUBuffer[];
  private ffnNormBufs!:  GPUBuffer[];
  private gateProjBufs!: GPUBuffer[];
  private upProjBufs!:   GPUBuffer[];
  private downProjBufs!: GPUBuffer[];

  private finalNormBuf!: GPUBuffer;
  private lmHeadBuf!:    GPUBuffer;

  // ── Activation buffers ──────────────────────────────────────────────────────
  private tokenIdsBuf!:  GPUBuffer;
  private hiddenBuf!:    GPUBuffer;
  private residualBuf!:  GPUBuffer;
  private normBuf!:      GPUBuffer;
  private qBuf!:         GPUBuffer;
  private kBuf!:         GPUBuffer;
  private vBuf!:         GPUBuffer;
  private attnOutBuf!:   GPUBuffer;
  private scoresBuf!:    GPUBuffer;
  private gateBuf!:      GPUBuffer;
  private upBuf!:        GPUBuffer;
  private ffnOutBuf!:    GPUBuffer;
  private logitsBuf!:    GPUBuffer;
  private resultBuf!:    GPUBuffer;
  private readbackBuf!:  GPUBuffer;

  // ── Uniform buffer ─────────────────────────────────────────────────────────
  private uniformsBuf!: GPUBuffer;

  // ── Model config ───────────────────────────────────────────────────────────
  private cfg!: ModelConfig;

  // ─────────────────────────────────────────────────────────────────────────
  // Lifecycle
  // ─────────────────────────────────────────────────────────────────────────

  /** Compile all pipelines.  Call once per GPUDevice. */
  async initPipelines(device: GPUDevice): Promise<void> {
    this.device = device;

    const module = device.createShaderModule({
      label: "logos-transformer",
      code:  transformerSource,
    });

    // Single BGL for all 9 kernels: uniform + 4×storage.
    this.bgl = device.createBindGroupLayout({
      label: "logos-llm-bgl",
      entries: [
        { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
        { binding: 2, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
        { binding: 3, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
        { binding: 4, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
      ],
    });

    const layout = device.createPipelineLayout({ bindGroupLayouts: [this.bgl] });
    const ep = (name: string): GPUComputePipelineDescriptor => ({
      label: `logos-${name}`, layout,
      compute: { module, entryPoint: name },
    });

    [
      this.pipeEmbed, this.pipeRmsNorm, this.pipeMatmul, this.pipeAddRes,
      this.pipeRope, this.pipeAttnScores, this.pipeAttnSoftmax,
      this.pipeSiluGate, this.pipeArgmax,
    ] = await Promise.all([
      device.createComputePipelineAsync(ep("cs_embed")),
      device.createComputePipelineAsync(ep("cs_rms_norm")),
      device.createComputePipelineAsync(ep("cs_matmul")),
      device.createComputePipelineAsync(ep("cs_add_res")),
      device.createComputePipelineAsync(ep("cs_rope")),
      device.createComputePipelineAsync(ep("cs_attn_scores")),
      device.createComputePipelineAsync(ep("cs_attn_softmax")),
      device.createComputePipelineAsync(ep("cs_silu_gate")),
      device.createComputePipelineAsync(ep("cs_argmax")),
    ]);

    // Uniform + readback (size-independent of model).
    this.uniformsBuf = mkBuf(device, "logos-llm-uniforms", UNIFORM_BYTES,
      GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST);
    this.readbackBuf = mkBuf(device, "logos-llm-readback", 4,
      GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ);
  }

  /**
   * Upload model weights to GPU and allocate all activation buffers.
   * Called once after downloading weights.
   */
  loadModel(weights: LoadedWeights): void {
    const device = this.device;
    const { config: cfg, tensors } = weights;
    this.cfg = cfg;

    const { dModel, dFF, nLayers, vocabSize, maxSeq, nHeads } = cfg;
    const headDim = dModel / nHeads;

    const u = GPUBufferUsage;
    const W = u.STORAGE | u.COPY_DST;   // weight buffers: read-only at runtime
    const A = u.STORAGE | u.COPY_DST;   // activation: read-write at runtime

    // Helper: upload a named tensor to a new GPU buffer.
    const up = (name: string, label: string): GPUBuffer => {
      const data = tensors.get(name);
      if (!data) throw new Error(`Missing tensor "${name}" in weight file`);
      const buf = mkBuf(device, label, data.byteLength, W);
      uploadF32(device, buf, data);
      return buf;
    };

    // ── Embedding table ──────────────────────────────────────────────────────
    this.embedTableBuf = up("embed.weight", "logos-embed-table");

    // ── Per-layer weights ────────────────────────────────────────────────────
    this.attnNormBufs = [];
    this.qProjBufs    = [];
    this.kProjBufs    = [];
    this.vProjBufs    = [];
    this.oProjBufs    = [];
    this.ffnNormBufs  = [];
    this.gateProjBufs = [];
    this.upProjBufs   = [];
    this.downProjBufs = [];

    for (let l = 0; l < nLayers; l++) {
      this.attnNormBufs.push(up(`layers.${l}.attention_norm.weight`, `attn-norm-${l}`));
      this.qProjBufs.push(   up(`layers.${l}.attention.wq.weight`,   `q-proj-${l}`));
      this.kProjBufs.push(   up(`layers.${l}.attention.wk.weight`,   `k-proj-${l}`));
      this.vProjBufs.push(   up(`layers.${l}.attention.wv.weight`,   `v-proj-${l}`));
      this.oProjBufs.push(   up(`layers.${l}.attention.wo.weight`,   `o-proj-${l}`));
      this.ffnNormBufs.push( up(`layers.${l}.ffn_norm.weight`,        `ffn-norm-${l}`));
      this.gateProjBufs.push(up(`layers.${l}.feed_forward.w1.weight`, `gate-proj-${l}`));
      this.upProjBufs.push(  up(`layers.${l}.feed_forward.w3.weight`, `up-proj-${l}`));
      this.downProjBufs.push(up(`layers.${l}.feed_forward.w2.weight`, `down-proj-${l}`));
    }

    // ── Final norm + LM head ─────────────────────────────────────────────────
    this.finalNormBuf = up("norm.weight",      "logos-final-norm");
    this.lmHeadBuf    = up("output.weight",    "logos-lm-head");

    // ── Activation buffers ───────────────────────────────────────────────────
    const seq  = maxSeq;
    const f4   = (n: number) => n * 4;

    this.tokenIdsBuf = mkBuf(device, "logos-token-ids",  f4(seq),                  A);
    this.hiddenBuf   = mkBuf(device, "logos-hidden",      f4(seq * dModel),         A);
    this.residualBuf = mkBuf(device, "logos-residual",    f4(seq * dModel),         A);
    this.normBuf     = mkBuf(device, "logos-norm-scratch",f4(seq * dModel),         A);
    this.qBuf        = mkBuf(device, "logos-Q",           f4(seq * dModel),         A);
    this.kBuf        = mkBuf(device, "logos-K",           f4(seq * dModel),         A);
    this.vBuf        = mkBuf(device, "logos-V",           f4(seq * dModel),         A);
    this.attnOutBuf  = mkBuf(device, "logos-attn-out",    f4(seq * dModel),         A);
    this.scoresBuf   = mkBuf(device, "logos-scores",      f4(nHeads * seq * seq),   A);
    this.gateBuf     = mkBuf(device, "logos-gate",        f4(seq * dFF),            A);
    this.upBuf       = mkBuf(device, "logos-up",          f4(seq * dFF),            A);
    this.ffnOutBuf   = mkBuf(device, "logos-ffn-out",     f4(seq * dModel),         A);
    this.logitsBuf   = mkBuf(device, "logos-logits",      f4(vocabSize),            A);
    this.resultBuf   = mkBuf(device, "logos-result",      4,
      GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST);
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Inference
  // ─────────────────────────────────────────────────────────────────────────

  /**
   * Run autoregressive generation on `prompt`.
   *
   * @param prompt       User's natural-language prompt.
   * @param onToken      Called with each decoded token string as it is generated.
   * @param signal       AbortSignal for cancellation.
   * @returns            Full generated string (same as all onToken calls joined).
   */
  async generate(
    prompt: string,
    onToken?: TokenCallback,
    signal?: AbortSignal,
  ): Promise<string> {
    const { dModel, dFF, nLayers, vocabSize, maxSeq, nHeads } = this.cfg;
    const headDim = dModel / nHeads;

    // ── Tokenise ──────────────────────────────────────────────────────────────
    const systemIds  = [TOKEN_BOS, ...encode(SYSTEM_PROMPT).slice(1, -1)];
    const promptIds  = encode(prompt);
    const allIds     = truncate([...systemIds, ...promptIds], maxSeq - MAX_NEW_TOKENS);
    let   seq        = allIds.length;

    // Upload token IDs as f32.
    const idF32 = new Float32Array(allIds.map(Number));
    this.device.queue.writeBuffer(this.tokenIdsBuf, 0, idF32);

    // ── Initial embed pass (whole prompt at once) ─────────────────────────────
    this._writeUniforms({ seq_len: seq, vocab_sz: vocabSize, d_model: dModel,
      d_ff: dFF, n_heads: nHeads, head_dim: headDim,
      dim0: seq * dModel, dim1: 0, dim2: 0,
      rms_eps: RMS_EPS, pos_offset: 0 });

    {
      const enc  = this.device.createCommandEncoder({ label: "logos-llm-embed" });
      const pass = enc.beginComputePass();
      pass.setPipeline(this.pipeEmbed);
      pass.setBindGroup(0, this._bg(this.hiddenBuf, this.tokenIdsBuf, this.embedTableBuf));
      pass.dispatchWorkgroups(Math.ceil(seq / 64));
      pass.end();
      this.device.queue.submit([enc.finish()]);
    }

    // Copy hidden → residual (residual stream = embedding).
    await this._copy(this.hiddenBuf, this.residualBuf, seq * dModel * 4);

    // ── Transformer layers ─────────────────────────────────────────────────────
    for (let l = 0; l < nLayers && !signal?.aborted; l++) {
      await this._transformerLayer(l, seq);
    }

    // ── lm_head + argmax for each new token ───────────────────────────────────
    const outputIds: number[] = [];
    let tokensGenerated = 0;

    while (tokensGenerated < MAX_NEW_TOKENS && !signal?.aborted) {
      // Apply final norm on last token's hidden state.
      const lastOff = (seq - 1) * dModel * 4;
      this._writeUniforms({ seq_len: 1, vocab_sz: vocabSize, d_model: dModel,
        d_ff: dFF, n_heads: nHeads, head_dim: headDim,
        dim0: dModel, dim1: dModel, dim2: vocabSize,
        rms_eps: RMS_EPS, pos_offset: seq - 1 });

      const finalEnc = this.device.createCommandEncoder({ label: "logos-llm-lmhead" });
      const fp = finalEnc.beginComputePass();

      // 1. final_norm on last hidden → normBuf row 0.
      fp.setPipeline(this.pipeRmsNorm);
      fp.setBindGroup(0, this._bg(
        this.normBuf,
        this.residualBuf,   // input = residual stream (last token)
        this.finalNormBuf,
      ));
      fp.dispatchWorkgroups(1);  // 1 row

      // 2. lm_head matmul: normBuf(1 × d_model) × lm_head(d_model × vocab) → logits.
      fp.setPipeline(this.pipeMatmul);  // dim0=1, dim1=dModel, dim2=vocab
      fp.setBindGroup(0, this._bg(
        this.logitsBuf,
        this.normBuf,
        this.lmHeadBuf,
      ));
      fp.dispatchWorkgroups(Math.ceil(vocabSize / 16), 1);

      // 3. argmax.
      fp.setPipeline(this.pipeArgmax);
      fp.setBindGroup(0, this._bg(
        this.resultBuf,
        this.logitsBuf,
        this.logitsBuf,  // placeholder (unused)
      ));
      fp.dispatchWorkgroups(1);

      fp.end();
      finalEnc.copyBufferToBuffer(this.resultBuf, 0, this.readbackBuf, 0, 4);
      this.device.queue.submit([finalEnc.finish()]);

      // Readback.
      await this.readbackBuf.mapAsync(GPUMapMode.READ, 0, 4);
      const tokenId = Math.round(new Float32Array(this.readbackBuf.getMappedRange(0, 4))[0]);
      this.readbackBuf.unmap();

      if (tokenId === TOKEN_EOS) break;

      outputIds.push(tokenId);
      const tokenStr = decode([tokenId]);
      onToken?.(tokenStr);
      tokensGenerated++;

      // Append new token to sequence and run another transformer pass.
      seq++;
      if (seq > maxSeq) break;
      const newIdF32 = new Float32Array([tokenId]);
      this.device.queue.writeBuffer(this.tokenIdsBuf, (seq - 1) * 4, newIdF32);

      // Embed the single new token into hidden position (seq-1).
      this._writeUniforms({ seq_len: 1, vocab_sz: vocabSize, d_model: dModel,
        d_ff: dFF, n_heads: nHeads, head_dim: headDim,
        dim0: dModel, dim1: 0, dim2: 0,
        rms_eps: RMS_EPS, pos_offset: seq - 1 });
      {
        const enc  = this.device.createCommandEncoder({ label: "logos-embed-next" });
        const pass = enc.beginComputePass();
        // Embed token at position (seq-1): we point buf_rw to that row.
        pass.setPipeline(this.pipeEmbed);
        pass.setBindGroup(0, this._bg(this.hiddenBuf, this.tokenIdsBuf, this.embedTableBuf));
        pass.dispatchWorkgroups(1); // 1 thread for 1 token
        pass.end();
        this.device.queue.submit([enc.finish()]);
      }
      await this._copyRow(this.hiddenBuf, this.residualBuf, seq - 1, dModel);

      // Run transformer layers with seq updated.
      for (let l = 0; l < nLayers && !signal?.aborted; l++) {
        await this._transformerLayer(l, seq);
      }
    }

    return decode(outputIds);
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Private: one transformer layer
  // ─────────────────────────────────────────────────────────────────────────

  private async _transformerLayer(l: number, seq: number): Promise<void> {
    const { dModel, dFF, nHeads } = this.cfg;
    const headDim = dModel / nHeads;
    const device  = this.device;

    const enc  = device.createCommandEncoder({ label: `logos-layer-${l}` });
    const pass = enc.beginComputePass();

    // ── Attention sub-layer ────────────────────────────────────────────────────

    // 1. attn_norm  (residual → norm_buf)
    this._setUniforms(pass, { seq_len: seq, d_model: dModel, d_ff: dFF,
      n_heads: nHeads, head_dim: headDim, dim0: seq * dModel,
      vocab_sz: VOCAB_SIZE, dim1: 0, dim2: 0, rms_eps: RMS_EPS, pos_offset: 0 });
    pass.setPipeline(this.pipeRmsNorm);
    pass.setBindGroup(0, this._bg(this.normBuf, this.residualBuf, this.attnNormBufs[l]));
    pass.dispatchWorkgroups(seq);

    // 2. Q/K/V projections: norm_buf × {q,k,v}_proj → {q,k,v}_buf
    const mmDispX = Math.ceil(dModel / 16);
    const mmDispY = Math.ceil(seq    / 16);
    this._setUniforms(pass, { seq_len: seq, d_model: dModel, d_ff: dFF,
      n_heads: nHeads, head_dim: headDim, dim0: seq, dim1: dModel, dim2: dModel,
      vocab_sz: VOCAB_SIZE, rms_eps: RMS_EPS, pos_offset: 0 });

    pass.setPipeline(this.pipeMatmul);
    pass.setBindGroup(0, this._bg(this.qBuf, this.normBuf, this.qProjBufs[l]));
    pass.dispatchWorkgroups(mmDispX, mmDispY);
    pass.setBindGroup(0, this._bg(this.kBuf, this.normBuf, this.kProjBufs[l]));
    pass.dispatchWorkgroups(mmDispX, mmDispY);
    pass.setBindGroup(0, this._bg(this.vBuf, this.normBuf, this.vProjBufs[l]));
    pass.dispatchWorkgroups(mmDispX, mmDispY);

    // 3. RoPE on Q and K (in-place).
    this._setUniforms(pass, { seq_len: seq, d_model: dModel, d_ff: dFF,
      n_heads: nHeads, head_dim: headDim, dim0: seq * dModel,
      vocab_sz: VOCAB_SIZE, dim1: 0, dim2: 0, rms_eps: RMS_EPS, pos_offset: 0 });
    pass.setPipeline(this.pipeRope);
    // Single null read-only buf (unused by cs_rope, but BGL requires it).
    const nullBuf = this.tokenIdsBuf;  // reused as a placeholder read-only binding
    pass.setBindGroup(0, this._bg(this.qBuf, nullBuf, nullBuf));
    pass.dispatchWorkgroups(Math.ceil(seq / 64), nHeads);
    pass.setBindGroup(0, this._bg(this.kBuf, nullBuf, nullBuf));
    pass.dispatchWorkgroups(Math.ceil(seq / 64), nHeads);

    // 4. Attention scores: Q × K^T → scores.
    this._setUniforms(pass, { seq_len: seq, d_model: dModel, d_ff: dFF,
      n_heads: nHeads, head_dim: headDim, dim0: seq,
      vocab_sz: VOCAB_SIZE, dim1: headDim, dim2: seq, rms_eps: RMS_EPS, pos_offset: 0 });
    pass.setPipeline(this.pipeAttnScores);
    pass.setBindGroup(0, this._bg(this.scoresBuf, this.qBuf, this.kBuf));
    pass.dispatchWorkgroups(Math.ceil(seq / 16), Math.ceil(seq / 16), nHeads);

    // 5. Softmax (in-place on scores).
    pass.setPipeline(this.pipeAttnSoftmax);
    pass.setBindGroup(0, this._bg(this.scoresBuf, nullBuf, nullBuf));
    pass.dispatchWorkgroups(seq, nHeads);

    // 6. scores × V → attn_out  (custom matmul with n_heads batching is
    //    approximated here as a standard matmul over the full seq×seq × seq×d_model).
    this._setUniforms(pass, { seq_len: seq, d_model: dModel, d_ff: dFF,
      n_heads: nHeads, head_dim: headDim,
      dim0: seq * nHeads, dim1: seq, dim2: headDim,
      vocab_sz: VOCAB_SIZE, rms_eps: RMS_EPS, pos_offset: 0 });
    pass.setPipeline(this.pipeMatmul);
    pass.setBindGroup(0, this._bg(this.attnOutBuf, this.scoresBuf, this.vBuf));
    pass.dispatchWorkgroups(Math.ceil(headDim / 16), Math.ceil(seq * nHeads / 16));

    // 7. output projection: attn_out × o_proj → hidden.
    this._setUniforms(pass, { seq_len: seq, d_model: dModel, d_ff: dFF,
      n_heads: nHeads, head_dim: headDim, dim0: seq, dim1: dModel, dim2: dModel,
      vocab_sz: VOCAB_SIZE, rms_eps: RMS_EPS, pos_offset: 0 });
    pass.setBindGroup(0, this._bg(this.hiddenBuf, this.attnOutBuf, this.oProjBufs[l]));
    pass.dispatchWorkgroups(mmDispX, mmDispY);

    // 8. Residual add: residual += hidden.
    this._setUniforms(pass, { seq_len: seq, d_model: dModel, d_ff: dFF,
      n_heads: nHeads, head_dim: headDim, dim0: seq * dModel,
      vocab_sz: VOCAB_SIZE, dim1: 0, dim2: 0, rms_eps: RMS_EPS, pos_offset: 0 });
    pass.setPipeline(this.pipeAddRes);
    pass.setBindGroup(0, this._bg(this.residualBuf, this.hiddenBuf, nullBuf));
    pass.dispatchWorkgroups(Math.ceil(seq * dModel / 64));

    // ── FFN sub-layer ──────────────────────────────────────────────────────────

    // 9. ffn_norm (residual → norm_buf).
    pass.setPipeline(this.pipeRmsNorm);
    pass.setBindGroup(0, this._bg(this.normBuf, this.residualBuf, this.ffnNormBufs[l]));
    pass.dispatchWorkgroups(seq);

    // 10. gate_proj: norm_buf × gate_proj_w → gate_buf.
    const fffDispX = Math.ceil(dFF  / 16);
    this._setUniforms(pass, { seq_len: seq, d_model: dModel, d_ff: dFF,
      n_heads: nHeads, head_dim: headDim, dim0: seq, dim1: dModel, dim2: dFF,
      vocab_sz: VOCAB_SIZE, rms_eps: RMS_EPS, pos_offset: 0 });
    pass.setPipeline(this.pipeMatmul);
    pass.setBindGroup(0, this._bg(this.gateBuf, this.normBuf, this.gateProjBufs[l]));
    pass.dispatchWorkgroups(fffDispX, mmDispY);

    // 11. up_proj: norm_buf × up_proj_w → up_buf.
    pass.setBindGroup(0, this._bg(this.upBuf, this.normBuf, this.upProjBufs[l]));
    pass.dispatchWorkgroups(fffDispX, mmDispY);

    // 12. SwiGLU: gate_buf[i] *= silu(up_buf[i]).
    this._setUniforms(pass, { seq_len: seq, d_model: dModel, d_ff: dFF,
      n_heads: nHeads, head_dim: headDim, dim0: seq * dFF,
      vocab_sz: VOCAB_SIZE, dim1: 0, dim2: 0, rms_eps: RMS_EPS, pos_offset: 0 });
    pass.setPipeline(this.pipeSiluGate);
    pass.setBindGroup(0, this._bg(this.gateBuf, this.upBuf, nullBuf));
    pass.dispatchWorkgroups(Math.ceil(seq * dFF / 64));

    // 13. down_proj: gate_buf × down_proj_w → ffn_out.
    this._setUniforms(pass, { seq_len: seq, d_model: dModel, d_ff: dFF,
      n_heads: nHeads, head_dim: headDim, dim0: seq, dim1: dFF, dim2: dModel,
      vocab_sz: VOCAB_SIZE, rms_eps: RMS_EPS, pos_offset: 0 });
    pass.setPipeline(this.pipeMatmul);
    pass.setBindGroup(0, this._bg(this.ffnOutBuf, this.gateBuf, this.downProjBufs[l]));
    pass.dispatchWorkgroups(mmDispX, mmDispY);

    // 14. Residual add: residual += ffn_out.
    this._setUniforms(pass, { seq_len: seq, d_model: dModel, d_ff: dFF,
      n_heads: nHeads, head_dim: headDim, dim0: seq * dModel,
      vocab_sz: VOCAB_SIZE, dim1: 0, dim2: 0, rms_eps: RMS_EPS, pos_offset: 0 });
    pass.setPipeline(this.pipeAddRes);
    pass.setBindGroup(0, this._bg(this.residualBuf, this.ffnOutBuf, nullBuf));
    pass.dispatchWorkgroups(Math.ceil(seq * dModel / 64));

    pass.end();
    this.device.queue.submit([enc.finish()]);

    // Yield to allow UI updates between layers.
    await this.device.queue.onSubmittedWorkDone();
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Private helpers
  // ─────────────────────────────────────────────────────────────────────────

  /** Create a bind group: uniform(0) + read-write(1) + r0(2) + r1(3) + rw2(4). */
  private _bg(
    rw:  GPUBuffer,
    r0:  GPUBuffer,
    r1:  GPUBuffer,
    rw2?: GPUBuffer,
  ): GPUBindGroup {
    return this.device.createBindGroup({
      layout: this.bgl,
      entries: [
        { binding: 0, resource: { buffer: this.uniformsBuf } },
        { binding: 1, resource: { buffer: rw  } },
        { binding: 2, resource: { buffer: r0  } },
        { binding: 3, resource: { buffer: r1  } },
        { binding: 4, resource: { buffer: rw2 ?? rw } },
      ],
    });
  }

  /** Write uniform fields to GPU (64-byte block). */
  private _writeUniforms(fields: {
    seq_len: number; vocab_sz: number; d_model: number; d_ff: number;
    n_heads: number; head_dim: number; dim0: number; dim1: number;
    dim2: number; rms_eps: number; pos_offset: number;
  }): void {
    const buf = new ArrayBuffer(UNIFORM_BYTES);
    const dv  = new DataView(buf);
    dv.setUint32( 0,  fields.seq_len,   true);
    dv.setUint32( 4,  fields.vocab_sz,  true);
    dv.setUint32( 8,  fields.d_model,   true);
    dv.setUint32(12,  fields.d_ff,      true);
    dv.setUint32(16,  fields.n_heads,   true);
    dv.setUint32(20,  fields.head_dim,  true);
    dv.setUint32(24,  fields.dim0,      true);
    dv.setUint32(28,  fields.dim1,      true);
    dv.setUint32(32,  fields.dim2,      true);
    dv.setFloat32(36, fields.rms_eps,   true);
    dv.setUint32(40,  fields.pos_offset,true);
    this.device.queue.writeBuffer(this.uniformsBuf, 0, buf);
  }

  /** Set uniforms via an active compute pass (write before dispatches). */
  private _setUniforms(
    _pass: GPUComputePassEncoder,
    fields: Parameters<LLMPipeline["_writeUniforms"]>[0],
  ): void {
    this._writeUniforms(fields);
  }

  /** Copy `byteLen` bytes from src → dst via a command encoder + submit. */
  private async _copy(src: GPUBuffer, dst: GPUBuffer, byteLen: number): Promise<void> {
    const enc = this.device.createCommandEncoder({ label: "logos-copy" });
    enc.copyBufferToBuffer(src, 0, dst, 0, byteLen);
    this.device.queue.submit([enc.finish()]);
    await this.device.queue.onSubmittedWorkDone();
  }

  /** Copy a single row from src[rowIdx] to dst[rowIdx]. */
  private async _copyRow(src: GPUBuffer, dst: GPUBuffer, rowIdx: number, dModel: number): Promise<void> {
    const byteOff = rowIdx * dModel * 4;
    const byteLen = dModel * 4;
    const enc = this.device.createCommandEncoder({ label: "logos-copy-row" });
    enc.copyBufferToBuffer(src, byteOff, dst, byteOff, byteLen);
    this.device.queue.submit([enc.finish()]);
    await this.device.queue.onSubmittedWorkDone();
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Static factory: load + init in one call
  // ─────────────────────────────────────────────────────────────────────────

  /**
   * Convenience factory: initialise pipelines, download/cache weights,
   * and return a ready-to-use LLMPipeline.
   */
  static async create(
    device: GPUDevice,
    modelUrl = "/models/logos-ai-sm.bin",
    onProgress?: ProgressCallback,
    signal?: AbortSignal,
  ): Promise<LLMPipeline> {
    const pipeline = new LLMPipeline();
    await pipeline.initPipelines(device);
    const weights = await loadWeights(modelUrl, onProgress, false, signal);
    pipeline.loadModel(weights);
    return pipeline;
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Cleanup
  // ─────────────────────────────────────────────────────────────────────────

  destroy(): void {
    const bufs = [
      this.embedTableBuf, this.finalNormBuf, this.lmHeadBuf,
      this.tokenIdsBuf, this.hiddenBuf, this.residualBuf, this.normBuf,
      this.qBuf, this.kBuf, this.vBuf, this.attnOutBuf, this.scoresBuf,
      this.gateBuf, this.upBuf, this.ffnOutBuf, this.logitsBuf,
      this.resultBuf, this.readbackBuf, this.uniformsBuf,
      ...(this.attnNormBufs  ?? []), ...(this.qProjBufs    ?? []),
      ...(this.kProjBufs     ?? []), ...(this.vProjBufs    ?? []),
      ...(this.oProjBufs     ?? []), ...(this.ffnNormBufs  ?? []),
      ...(this.gateProjBufs  ?? []), ...(this.upProjBufs   ?? []),
      ...(this.downProjBufs  ?? []),
    ];
    for (const b of bufs) b?.destroy();
  }
}
