/**
 * worker/layout.worker.ts
 *
 * Flex/grid layout computation via `logos-layout-wasm`.
 * Runs entirely off the main thread. The main thread posts a
 * COMPUTE_LAYOUT message and receives back LAYOUT_RESULT.
 *
 * Message protocol
 * ─────────────────
 * IN:
 *   { type: "COMPUTE_LAYOUT"; id: string; payload: LayoutRequest }
 *
 * OUT:
 *   { type: "LAYOUT_RESULT"; id: string; patches: Record<string, {x,y,w,h}> }
 *   { type: "LAYOUT_ERROR";  id: string; error: string }
 *   { type: "READY" }
 */

export interface LayoutRequest {
  /**
   * Flat array of nodes. Each node is the minimal info needed for
   * flex/grid layout: id, parentId, flex/grid properties, current bounds.
   */
  nodes: LayoutNode[];
  /** ID of the root frame/group to lay out. */
  rootId: string;
}

export interface LayoutNode {
  id: string;
  parentId: string | null;
  /** Current bounds (input) */
  x: number;
  y: number;
  w: number;
  h: number;
  /** Layout mode of this node's parent container */
  layoutMode?: "none" | "flex-row" | "flex-col" | "grid";
  /** Flex/grid child props */
  flexGrow?: number;
  flexShrink?: number;
  flexBasis?: number;
  alignSelf?: "auto" | "start" | "center" | "end" | "stretch";
  minW?: number;
  minH?: number;
  maxW?: number;
  maxH?: number;
  /** Container props (if this node is a flex/grid parent) */
  gap?: number;
  paddingTop?: number;
  paddingRight?: number;
  paddingBottom?: number;
  paddingLeft?: number;
  justifyContent?: "start" | "center" | "end" | "space-between" | "space-around";
  alignItems?: "start" | "center" | "end" | "stretch";
  gridTemplateColumns?: string;
  gridTemplateRows?: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// WASM module bootstrap
// ─────────────────────────────────────────────────────────────────────────────

let layoutReady = false;
let wasmModule: WebAssembly.Instance | null = null;

async function initWasm(): Promise<void> {
  // logos-layout-wasm is compiled from rust/logos-layout-wasm and published
  // as an npm package once the Rust build pipeline is wired up.  Until then
  // the pure-TS solver below is used as a complete fallback — no dynamic
  // import is attempted so Vite never tries to resolve the missing package.
  layoutReady = true;
  self.postMessage({ type: "READY" });
}

// ─────────────────────────────────────────────────────────────────────────────
// Pure-TS layout fallback (used when WASM isn't built yet)
// Supports flex-row and flex-col; grid follows in M4.
// ─────────────────────────────────────────────────────────────────────────────

function computeLayoutTS(
  nodes: LayoutNode[],
  rootId: string
): Record<string, { x: number; y: number; w: number; h: number }> {
  const byId = new Map<string, LayoutNode>(nodes.map((n) => [n.id, n]));
  const patches: Record<string, { x: number; y: number; w: number; h: number }> = {};

  function layout(nodeId: string, containerX: number, containerY: number): void {
    const node = byId.get(nodeId);
    if (!node) return;

    const children = nodes.filter((n) => n.parentId === nodeId);
    if (children.length === 0) return;

    const mode = node.layoutMode ?? "none";
    if (mode !== "flex-row" && mode !== "flex-col") return;

    const gap = node.gap ?? 0;
    const pl = node.paddingLeft ?? 0;
    const pt = node.paddingTop ?? 0;
    const pr = node.paddingRight ?? 0;
    const pb = node.paddingBottom ?? 0;

    const innerW = node.w - pl - pr;
    const innerH = node.h - pt - pb;
    const isRow = mode === "flex-row";

    // Measure fixed (non-grow) children
    const totalGap = gap * Math.max(0, children.length - 1);
    let fixedMain = 0;
    let totalGrow = 0;
    for (const child of children) {
      const grow = child.flexGrow ?? 0;
      totalGrow += grow;
      if (grow === 0) fixedMain += isRow ? child.w : child.h;
    }

    const freeMain = (isRow ? innerW : innerH) - fixedMain - totalGap;

    let cursor = isRow ? containerX + pl : containerY + pt;
    for (const child of children) {
      const grow = child.flexGrow ?? 0;
      const mainSize = grow > 0 ? freeMain * (grow / totalGrow) : isRow ? child.w : child.h;
      const crossSize = isRow ? innerH : innerW;

      const cx = isRow ? cursor : containerX + pl;
      const cy = isRow ? containerY + pt : cursor;

      patches[child.id] = {
        x: cx,
        y: cy,
        w: isRow ? mainSize : crossSize,
        h: isRow ? crossSize : mainSize,
      };

      cursor += (isRow ? mainSize : mainSize) + gap;

      // Recurse
      layout(child.id, cx, cy);
    }
  }

  const root = byId.get(rootId);
  if (root) layout(rootId, root.x, root.y);
  return patches;
}

// ─────────────────────────────────────────────────────────────────────────────
// Message handler
// ─────────────────────────────────────────────────────────────────────────────

self.onmessage = async (e: MessageEvent) => {
  const { type, id, payload } = e.data as {
    type: string;
    id: string;
    payload: LayoutRequest;
  };

  if (type !== "COMPUTE_LAYOUT") return;

  if (!layoutReady) {
    self.postMessage({ type: "LAYOUT_ERROR", id, error: "Worker not initialized" });
    return;
  }

  try {
    let patches: Record<string, { x: number; y: number; w: number; h: number }>;

    if (wasmModule) {
      // TODO: serialize to WASM memory, call layout(), read back result.
      // See rust/logos-layout-wasm/src/lib.rs for the ABI.
      // Placeholder until the JSON ABI is wired:
      patches = computeLayoutTS(payload.nodes, payload.rootId);
    } else {
      patches = computeLayoutTS(payload.nodes, payload.rootId);
    }

    self.postMessage({ type: "LAYOUT_RESULT", id, patches });
  } catch (err) {
    self.postMessage({
      type: "LAYOUT_ERROR",
      id,
      error: err instanceof Error ? err.message : String(err),
    });
  }
};

// Boot
initWasm();
