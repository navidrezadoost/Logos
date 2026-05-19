/**
 * render-webgpu/glyph-atlas.ts
 *
 * Phase 5.2 — Glyph Atlas
 *
 * Rasterises individual glyphs using Canvas 2D and packs them into a
 * 2048×2048 single-channel (R8) GPU texture used by the text pipeline.
 *
 * Algorithm
 * ─────────
 *   Shelf packer:
 *     • A "shelf" is one horizontal band of the atlas.
 *     • New glyphs are placed at the cursor on the current shelf.
 *     • When the cursor would exceed the atlas width, a new shelf is opened
 *       immediately below the tallest glyph on the previous shelf.
 *   This gives near-optimal packing for variable-height glyphs.
 *
 * GPU texture format
 * ──────────────────
 *   "r8unorm" (1 byte/texel).  The text.wgsl shader reads `.r` and uses it
 *   as the alpha mask, multiplied by the user-supplied fill colour.
 *   R8 keeps the atlas at 4 MiB (2048² × 1) rather than 16 MiB for RGBA8.
 *
 *   NOTE: WebGPU requires "r8unorm" textures to have `sampleType: "float"`
 *   and they must be sampled with a `sampler` not `sampler_comparison`.
 *
 * Usage
 * ─────
 *   const atlas = GlyphAtlas.create(device);
 *
 *   // Get (or lazily rasterise) the UV rect for a glyph:
 *   const rect = atlas.getGlyph("Inter", 400, 16, "A");
 *   // → { u: 0.003, v: 0.0, uw: 0.007, uh: 0.010 }
 *
 *   // After a batch of getGlyph() calls, upload any newly rasterised glyphs:
 *   atlas.flush(device);
 */

import { GLYPH_ATLAS_SIZE } from "./constants";

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/** UV rect for one glyph in the atlas (0–1 normalised coords). */
export interface GlyphUVRect {
  /** Left edge U. */
  u:  number;
  /** Top edge V. */
  v:  number;
  /** UV span in U direction. */
  uw: number;
  /** UV span in V direction. */
  vh: number;
  /** Width in canvas pixels at the rasterisation font size. */
  widthPx:  number;
  /** Height in canvas pixels at the rasterisation font size. */
  heightPx: number;
}

/** Canvas-space layout for one glyph, used by TextPipeline. */
export interface GlyphLayout {
  /** Atlas UV rect for sampling. */
  uv: GlyphUVRect;
  /** Advance width in font-size pixels (pre-scaled). */
  advancePx: number;
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/** Pixel padding around each glyph to prevent bilinear bleed. */
const GLYPH_PAD = 2;

/** OffscreenCanvas used for CPU-side glyph measurement + rasterisation. */
let _offscreen: OffscreenCanvas | null = null;
let _ctx2d: OffscreenCanvasRenderingContext2D | null = null;

function get2DContext(): OffscreenCanvasRenderingContext2D {
  if (_ctx2d) return _ctx2d;
  // 512×512 scratch surface — large enough for any single glyph.
  _offscreen = new OffscreenCanvas(512, 512);
  _ctx2d     = _offscreen.getContext("2d", { willReadFrequently: true })!;
  return _ctx2d;
}

/** Build a font string accepted by Canvas 2D. */
function fontString(family: string, weight: number, sizePx: number): string {
  return `${weight} ${sizePx}px ${family}`;
}

/** Unique cache key for a glyph. */
function glyphKey(family: string, weight: number, sizePx: number, char: string): string {
  return `${family}:${weight}:${sizePx}:${char}`;
}

// ─────────────────────────────────────────────────────────────────────────────
// Shelf packer
// ─────────────────────────────────────────────────────────────────────────────

interface Shelf {
  /** Y coordinate of the shelf's top edge. */
  top:     number;
  /** Height allocated to this shelf (= tallest glyph + padding). */
  height:  number;
  /** Next free X position (cursor). */
  cursor:  number;
}

class ShelfPacker {
  private shelves: Shelf[] = [];
  private readonly size: number;

  constructor(size: number) {
    this.size = size;
    this.shelves.push({ top: 0, height: 0, cursor: 0 });
  }

  /**
   * Allocate a slot for a glyph of size `(w, h)` (includes padding).
   * Returns `{ x, y }` top-left in texel coordinates, or `null` if the
   * atlas is full.
   */
  alloc(w: number, h: number): { x: number; y: number } | null {
    // Try to fit onto an existing shelf that is tall enough.
    for (const shelf of this.shelves) {
      if (shelf.height >= h && shelf.cursor + w <= this.size) {
        const x = shelf.cursor;
        shelf.cursor += w;
        return { x, y: shelf.top };
      }
    }

    // Open a new shelf below the last one.
    const last = this.shelves[this.shelves.length - 1];
    const newTop = last.top + last.height;
    if (newTop + h > this.size) return null; // Atlas full.

    const shelf: Shelf = { top: newTop, height: h, cursor: w };
    this.shelves.push(shelf);
    return { x: 0, y: newTop };
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// GlyphAtlas
// ─────────────────────────────────────────────────────────────────────────────

export class GlyphAtlas {
  readonly texture: GPUTexture;
  readonly sampler: GPUSampler;

  private readonly packer  = new ShelfPacker(GLYPH_ATLAS_SIZE);
  private readonly glyphMap = new Map<string, GlyphUVRect>();

  /**
   * Pending CPU-side patches that haven't been flushed to the GPU yet.
   * Each entry: { x, y, w, h, data: Uint8Array }.
   */
  private readonly pending: Array<{
    x: number; y: number; w: number; h: number; data: Uint8Array;
  }> = [];

  private constructor(texture: GPUTexture, sampler: GPUSampler) {
    this.texture = texture;
    this.sampler = sampler;
  }

  // ── Factory ──────────────────────────────────────────────────────────────

  static create(device: GPUDevice): GlyphAtlas {
    const texture = device.createTexture({
      label:  "logos-glyph-atlas",
      size:   [GLYPH_ATLAS_SIZE, GLYPH_ATLAS_SIZE],
      format: "r8unorm",
      usage:  GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });

    const sampler = device.createSampler({
      label:        "logos-glyph-sampler",
      magFilter:    "linear",
      minFilter:    "linear",
      addressModeU: "clamp-to-edge",
      addressModeV: "clamp-to-edge",
    });

    return new GlyphAtlas(texture, sampler);
  }

  // ── Glyph lookup / rasterisation ─────────────────────────────────────────

  /**
   * Get the UV rect for a single character.
   *
   * If the glyph is not yet in the atlas it is rasterised immediately and
   * queued for GPU upload (which happens on the next `flush()` call).
   *
   * @param family   Font family, e.g. "Inter".
   * @param weight   Font weight (100–900), e.g. 400.
   * @param sizePx   Font size in canvas pixels.
   * @param char     Single Unicode character to rasterise.
   * @returns        UV rect + advance width, or `null` if the atlas is full.
   */
  getGlyph(
    family:  string,
    weight:  number,
    sizePx:  number,
    char:    string,
  ): GlyphLayout | null {
    const key = glyphKey(family, weight, sizePx, char);

    if (this.glyphMap.has(key)) {
      const uv = this.glyphMap.get(key)!;
      return { uv, advancePx: uv.widthPx };
    }

    return this._rasterise(key, family, weight, sizePx, char);
  }

  /**
   * Convenience: lay out a string into an array of positioned glyph quads.
   *
   * @param text    String to lay out.
   * @param family  Font family.
   * @param weight  Font weight.
   * @param sizePx  Font size in canvas pixels.
   * @param originX Starting X (canvas space, left of first character).
   * @param baselineY Baseline Y (canvas space).
   * @returns Array of { uv, canvasX, canvasY, w, h } for each glyph.
   */
  layoutText(
    text:      string,
    family:    string,
    weight:    number,
    sizePx:    number,
    originX:   number,
    baselineY: number,
  ): Array<{
    uv:      GlyphUVRect;
    x:       number;
    y:       number;
    w:       number;
    h:       number;
  }> {
    const result: ReturnType<GlyphAtlas["layoutText"]> = [];
    let cursorX = originX;

    for (const char of text) {
      const layout = this.getGlyph(family, weight, sizePx, char);
      if (!layout) continue;

      const { uv } = layout;
      result.push({
        uv,
        x: cursorX,
        y: baselineY - uv.heightPx, // top-edge from baseline
        w: uv.widthPx,
        h: uv.heightPx,
      });
      cursorX += layout.advancePx;
    }

    return result;
  }

  // ── GPU upload ────────────────────────────────────────────────────────────

  /**
   * Upload all newly rasterised glyphs to the GPU texture.
   * Call once per frame after all `getGlyph()` / `layoutText()` calls.
   */
  flush(device: GPUDevice): void {
    if (this.pending.length === 0) return;

    for (const patch of this.pending) {
      device.queue.writeTexture(
        { texture: this.texture, origin: { x: patch.x, y: patch.y } },
        patch.data,
        { bytesPerRow: patch.w },
        { width: patch.w, height: patch.h },
      );
    }

    this.pending.length = 0;
  }

  // ── Internal ─────────────────────────────────────────────────────────────

  private _rasterise(
    key:    string,
    family: string,
    weight: number,
    sizePx: number,
    char:   string,
  ): GlyphLayout | null {
    const ctx  = get2DContext();
    const font = fontString(family, weight, sizePx);
    ctx.font   = font;

    // Measure the glyph.
    const metrics  = ctx.measureText(char);
    const advance  = metrics.width;
    const glyphW   = Math.ceil(advance)  + GLYPH_PAD * 2;
    const glyphH   = Math.ceil(sizePx * 1.4) + GLYPH_PAD * 2; // ~1.4× em approx cap+descender

    // Allocate a slot in the atlas.
    const slot = this.packer.alloc(glyphW, glyphH);
    if (!slot) {
      console.warn(`[logos/webgpu] GlyphAtlas full — cannot add glyph "${char}".`);
      return null;
    }

    // Rasterise onto the scratch canvas.
    ctx.clearRect(0, 0, glyphW, glyphH);
    ctx.fillStyle = "#ffffff";
    ctx.textBaseline = "alphabetic";
    ctx.fillText(char, GLYPH_PAD, glyphH - GLYPH_PAD - Math.ceil(sizePx * 0.25));

    // Extract alpha channel as R8 data.
    const imgData = ctx.getImageData(0, 0, glyphW, glyphH);
    const r8 = new Uint8Array(glyphW * glyphH);
    for (let i = 0; i < r8.length; i++) {
      // Use the red channel (white text → all channels equal → just red).
      r8[i] = imgData.data[i * 4];
    }

    // Store UV rect.
    const uv: GlyphUVRect = {
      u:        slot.x / GLYPH_ATLAS_SIZE,
      v:        slot.y / GLYPH_ATLAS_SIZE,
      uw:       glyphW / GLYPH_ATLAS_SIZE,
      vh:       glyphH / GLYPH_ATLAS_SIZE,
      widthPx:  Math.ceil(advance),
      heightPx: glyphH,
    };
    this.glyphMap.set(key, uv);

    // Queue for GPU upload.
    this.pending.push({ x: slot.x, y: slot.y, w: glyphW, h: glyphH, data: r8 });

    return { uv, advancePx: Math.ceil(advance) };
  }

  destroy(): void {
    this.texture.destroy();
  }
}
