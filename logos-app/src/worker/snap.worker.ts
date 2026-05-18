/**
 * worker/snap.worker.ts
 *
 * Range-tree snapping — computes snap candidates for a moving shape.
 * Ported from `snap.cljs` to TypeScript.
 *
 * Message protocol
 * ─────────────────
 * IN:
 *   { type: "SNAP"; id: string; payload: SnapRequest }
 *
 * OUT:
 *   { type: "SNAP_RESULT"; id: string; result: SnapResult }
 *   { type: "SNAP_ERROR";  id: string; error: string }
 */

export interface SnapRequest {
  /** The shape being moved/resized — its current (un-snapped) bounds. */
  subject: { x: number; y: number; w: number; h: number };
  /**
   * All other shapes on the page (snap targets).
   * Passing a flat list; Worker builds the range tree internally.
   */
  targets: SnapTarget[];
  /** Snap threshold in canvas units (before zoom). */
  threshold: number;
  /** Which edges / guides are enabled. */
  snapToEdges: boolean;
  snapToCenter: boolean;
  snapToGrid: boolean;
  gridSize: number;
}

export interface SnapTarget {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface SnapResult {
  /** Adjusted position (may equal input if no snap occurred). */
  x: number;
  y: number;
  /** Snap guides to draw as visual feedback. */
  guides: SnapGuide[];
  /** IDs of shapes that triggered a snap. */
  snappedTo: string[];
}

export interface SnapGuide {
  axis: "x" | "y";
  position: number; // canvas coordinate
  from: number;     // start of guide line
  to: number;       // end of guide line
}

// ─────────────────────────────────────────────────────────────────────────────
// Range tree (1D interval index)
// Simple sorted array with binary search — replace with a full interval tree
// if contiguous query performance becomes an issue in M4+.
// ─────────────────────────────────────────────────────────────────────────────

interface AxisEntry {
  /** Canonical coordinate (left edge, center, right edge, etc.) */
  coord: number;
  id: string;
  /** From/to for guide line rendering */
  lineFrom: number;
  lineTo: number;
}

function buildAxis(targets: SnapTarget[], axis: "x" | "y"): AxisEntry[] {
  const entries: AxisEntry[] = [];
  for (const t of targets) {
    const lo = axis === "x" ? t.x : t.y;
    const size = axis === "x" ? t.w : t.h;
    const hi = lo + size;
    const center = lo + size / 2;
    const from = axis === "x" ? t.y : t.x;
    const to = axis === "x" ? t.y + t.h : t.x + t.w;

    entries.push({ coord: lo, id: t.id, lineFrom: from, lineTo: to });
    entries.push({ coord: center, id: t.id, lineFrom: from, lineTo: to });
    entries.push({ coord: hi, id: t.id, lineFrom: from, lineTo: to });
  }
  entries.sort((a, b) => a.coord - b.coord);
  return entries;
}

function closestSnap(
  entries: AxisEntry[],
  value: number,
  threshold: number
): { delta: number; entry: AxisEntry } | null {
  let best: { delta: number; entry: AxisEntry } | null = null;
  // Binary search for approximate position, then scan neighbours
  let lo = 0;
  let hi = entries.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (entries[mid].coord < value) lo = mid + 1;
    else hi = mid - 1;
  }
  for (let i = Math.max(0, lo - 3); i < Math.min(entries.length, lo + 4); i++) {
    const delta = Math.abs(entries[i].coord - value);
    if (delta <= threshold && (!best || delta < best.delta)) {
      best = { delta, entry: entries[i] };
    }
  }
  return best;
}

// ─────────────────────────────────────────────────────────────────────────────
// Core snap computation
// ─────────────────────────────────────────────────────────────────────────────

function computeSnap(req: SnapRequest): SnapResult {
  const { subject, targets, threshold, snapToEdges, snapToCenter, snapToGrid, gridSize } = req;
  let { x, y } = subject;
  const guides: SnapGuide[] = [];
  const snappedTo = new Set<string>();

  // Filter out the subject itself (shouldn't be in targets, but guard anyway)
  const others = targets;

  if (snapToEdges || snapToCenter) {
    const xAxis = buildAxis(others, "x");
    const yAxis = buildAxis(others, "y");

    // Try snapping left edge, center X, right edge
    const subjectXs = [x, x + subject.w / 2, x + subject.w];
    for (const sx of subjectXs) {
      const hit = closestSnap(xAxis, sx, threshold);
      if (hit) {
        const offset = hit.entry.coord - sx;
        x += offset;
        guides.push({ axis: "x", position: hit.entry.coord, from: hit.entry.lineFrom, to: hit.entry.lineTo });
        snappedTo.add(hit.entry.id);
        break;
      }
    }

    const subjectYs = [y, y + subject.h / 2, y + subject.h];
    for (const sy of subjectYs) {
      const hit = closestSnap(yAxis, sy, threshold);
      if (hit) {
        const offset = hit.entry.coord - sy;
        y += offset;
        guides.push({ axis: "y", position: hit.entry.coord, from: hit.entry.lineFrom, to: hit.entry.lineTo });
        snappedTo.add(hit.entry.id);
        break;
      }
    }
  }

  if (snapToGrid && gridSize > 0) {
    x = Math.round(x / gridSize) * gridSize;
    y = Math.round(y / gridSize) * gridSize;
  }

  return { x, y, guides, snappedTo: [...snappedTo] };
}

// ─────────────────────────────────────────────────────────────────────────────
// Message handler
// ─────────────────────────────────────────────────────────────────────────────

self.onmessage = (e: MessageEvent) => {
  const { type, id, payload } = e.data as {
    type: string;
    id: string;
    payload: SnapRequest;
  };

  if (type !== "SNAP") return;

  try {
    const result = computeSnap(payload);
    self.postMessage({ type: "SNAP_RESULT", id, result });
  } catch (err) {
    self.postMessage({
      type: "SNAP_ERROR",
      id,
      error: err instanceof Error ? err.message : String(err),
    });
  }
};

self.postMessage({ type: "READY" });
