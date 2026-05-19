/**
 * render-webgpu/gradient-atlas.ts
 *
 * Phase 5.2 — Gradient Atlas
 *
 * Manages a 256×256 RGBA8 GPU texture used as a gradient look-up table.
 * Each row stores the colour ramp for one gradient:
 *   - Row 0 → gradient slot 0
 *   - Row n → gradient slot n
 *
 * The texture is sampled in tile.wgsl using:
 *   textureSample(gradient_atlas, gradient_smp, vec2f(t, atlas_v))
 * where `t ∈ [0, 1]` is the position along the gradient and
 * `atlas_v = (slot + 0.5) / GRADIENT_ATLAS_H` centres sampling in the row.
 *
 * Usage
 * ─────
 *   const ga = GradientAtlas.create(device);
 *
 *   // Register a gradient once (or whenever it changes):
 *   const slot = ga.register(gradientFill);
 *
 *   // After all registrations for this frame are done, flush to GPU:
 *   ga.flush(device);
 *
 *   // Retrieve the atlas_v value to pack into GradientEntry:
 *   const atlasV = ga.atlasV(slot);
 *
 *   // Bind texture + sampler:
 *   { binding: N, resource: ga.texture.createView() }
 *   { binding: M, resource: ga.sampler }
 */

import type { GradientFill, GradientStop } from "../types/shapes";
import { GRADIENT_ATLAS_W, GRADIENT_ATLAS_H } from "./constants";

const ATLAS_FORMAT: GPUTextureFormat = "rgba8unorm";

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/** Parse a CSS hex color to [r, g, b] ∈ [0, 255]. */
function hexToRGB(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  const s = h.length === 3 ? h.split("").map((c) => c + c).join("") : h.slice(0, 6);
  return [
    parseInt(s.slice(0, 2), 16),
    parseInt(s.slice(2, 4), 16),
    parseInt(s.slice(4, 6), 16),
  ];
}

/** Lerp two [0,255] channels. */
function lerp(a: number, b: number, t: number): number {
  return Math.round(a + (b - a) * t);
}

/**
 * Rasterise a gradient stop array into a flat Uint8Array row of
 * `GRADIENT_ATLAS_W × 4` bytes (RGBA).
 */
function rasteriseGradient(stops: GradientStop[], fillOpacity: number): Uint8Array {
  // Sort stops by position (defensive — they should already be sorted).
  const sorted = [...stops].sort((a, b) => a.position - b.position);

  const row = new Uint8Array(GRADIENT_ATLAS_W * 4);
  const n   = sorted.length;

  for (let px = 0; px < GRADIENT_ATLAS_W; px++) {
    const t = px / (GRADIENT_ATLAS_W - 1);

    // Find the two stops that bracket t.
    let lo = 0;
    let hi = n - 1;
    for (let i = 0; i < n - 1; i++) {
      if (t >= sorted[i].position && t <= sorted[i + 1].position) {
        lo = i;
        hi = i + 1;
        break;
      }
    }

    const span = sorted[hi].position - sorted[lo].position;
    const f    = span < 1e-6 ? 0 : (t - sorted[lo].position) / span;

    const [r0, g0, b0] = hexToRGB(sorted[lo].color);
    const [r1, g1, b1] = hexToRGB(sorted[hi].color);
    const a0 = Math.round(sorted[lo].opacity * fillOpacity * 255);
    const a1 = Math.round(sorted[hi].opacity * fillOpacity * 255);

    const base = px * 4;
    row[base + 0] = lerp(r0, r1, f);
    row[base + 1] = lerp(g0, g1, f);
    row[base + 2] = lerp(b0, b1, f);
    row[base + 3] = lerp(a0, a1, f);
  }

  return row;
}

// ─────────────────────────────────────────────────────────────────────────────
// GradientAtlas
// ─────────────────────────────────────────────────────────────────────────────

export class GradientAtlas {
  readonly texture: GPUTexture;
  readonly sampler: GPUSampler;

  /** Dirty rows that need to be flushed to the GPU this frame. */
  private dirtyRows = new Set<number>();

  /** CPU-side atlas data — written rows, uploaded on flush(). */
  private readonly cpuData = new Uint8Array(GRADIENT_ATLAS_W * GRADIENT_ATLAS_H * 4);

  /** Next free slot index. */
  private nextSlot = 0;

  /** Map from a stable gradient identity key → slot index. */
  private readonly slotMap = new Map<string, number>();

  private constructor(texture: GPUTexture, sampler: GPUSampler) {
    this.texture = texture;
    this.sampler = sampler;
  }

  // ── Factory ──────────────────────────────────────────────────────────────

  static create(device: GPUDevice): GradientAtlas {
    const texture = device.createTexture({
      label:  "logos-gradient-atlas",
      size:   [GRADIENT_ATLAS_W, GRADIENT_ATLAS_H],
      format: ATLAS_FORMAT,
      usage:  GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });

    const sampler = device.createSampler({
      label:        "logos-gradient-sampler",
      magFilter:    "linear",
      minFilter:    "linear",
      addressModeU: "clamp-to-edge",
      addressModeV: "clamp-to-edge",
    });

    // Initialise atlas to fully transparent black so unused rows don't
    // produce artefacts if accidentally sampled.
    // (The flush call on first frame will upload the actual data.)
    return new GradientAtlas(texture, sampler);
  }

  // ── Registration ─────────────────────────────────────────────────────────

  /**
   * Register a `GradientFill` and return its slot index.
   *
   * The slot is stable for the lifetime of this atlas.  If the same gradient
   * has been registered before (same identity key), the existing slot is
   * returned without re-rasterising.
   *
   * Call `flush()` after all registrations to upload dirty rows.
   */
  register(fill: GradientFill): number {
    const key = gradientKey(fill);
    if (this.slotMap.has(key)) return this.slotMap.get(key)!;

    const slot = this.nextSlot;
    if (slot >= GRADIENT_ATLAS_H) {
      // Atlas full — evict slot 0 as a simple overflow strategy
      // (design docs rarely have > 256 simultaneous gradients).
      console.warn(
        "[logos/webgpu] GradientAtlas: overflow — evicting slot 0. " +
        "Consider calling reset() at document load."
      );
      return 0;
    }

    this.nextSlot++;
    this.slotMap.set(key, slot);

    // Rasterise gradient row.
    const row = rasteriseGradient(fill.gradient.stops, fill.opacity);
    const rowOffset = slot * GRADIENT_ATLAS_W * 4;
    this.cpuData.set(row, rowOffset);
    this.dirtyRows.add(slot);

    return slot;
  }

  /**
   * Compute the atlas V-coordinate for `slot` (centre of the texel row).
   *
   *   atlas_v = (slot + 0.5) / GRADIENT_ATLAS_H
   */
  atlasV(slot: number): number {
    return (slot + 0.5) / GRADIENT_ATLAS_H;
  }

  // ── GPU upload ────────────────────────────────────────────────────────────

  /**
   * Upload all dirty rows to the GPU texture.
   * Must be called once per frame before the tile render pass.
   */
  flush(device: GPUDevice): void {
    if (this.dirtyRows.size === 0) return;

    for (const slot of this.dirtyRows) {
      const rowOffset = slot * GRADIENT_ATLAS_W * 4;
      const rowData   = this.cpuData.subarray(rowOffset, rowOffset + GRADIENT_ATLAS_W * 4);

      device.queue.writeTexture(
        { texture: this.texture, origin: { x: 0, y: slot } },
        rowData,
        { bytesPerRow: GRADIENT_ATLAS_W * 4, rowsPerImage: 1 },
        { width: GRADIENT_ATLAS_W, height: 1 },
      );
    }

    this.dirtyRows.clear();
  }

  // ── State management ─────────────────────────────────────────────────────

  /**
   * Clear the slot registry.
   * Call this when loading a new document so gradient slots can be reused.
   * Does NOT destroy the GPU texture.
   */
  reset(): void {
    this.nextSlot = 0;
    this.slotMap.clear();
    this.dirtyRows.clear();
  }

  destroy(): void {
    this.texture.destroy();
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Identity key
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Build a stable string key that uniquely identifies a gradient.
 * Two gradients with the same stops/type/coords map to the same key.
 */
function gradientKey(fill: GradientFill): string {
  const g = fill.gradient;
  const stops = g.stops
    .map((s) => `${s.position.toFixed(4)}:${s.color}:${s.opacity.toFixed(4)}`)
    .join("|");
  return `${g.type}:${g.startX}:${g.startY}:${g.endX}:${g.endY}:${fill.opacity}:${stops}`;
}
