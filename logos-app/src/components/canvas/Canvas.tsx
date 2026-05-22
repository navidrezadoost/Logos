/**
 * components/canvas/Canvas.tsx  (M3 revision)
 *
 * Subscribes to documentStore and re-renders the Rust/Skia scene whenever
 * the shape list changes. Handles the WASM lifecycle (load → init → sync)
 * and a Canvas 2D fallback for environments without the Emscripten build.
 *
 * M3 additions:
 *  - Mouse-drag shape creation (tools: rect, ellipse)
 *  - Preview overlay div during drag
 *  - Worker pool initialization on mount
 */

import { useEffect, useLayoutEffect, useRef, useState, useCallback } from "react";
import {
  loadRenderWasm,
  initCanvasContext,
  cleanUp,
  type RenderWasmModule,
} from "../../render-wasm/module";
import { syncScene, syncScene2D } from "../../render-wasm/scene";
import { useCurrentPageShapes, useDocumentStore } from "../../stores/documentStore";
import { useSelectionStore } from "../../stores/selectionStore";
import { useUiStore } from "../../stores/uiStore";
import { usePenStore } from "../../stores/penStore";
import { useComponentStore } from "../../stores/componentStore";
import { useProtoStore } from "../../stores/prototypeStore";
import { useDevModeStore } from "../../stores/devModeStore";
import { useWebGPURenderer } from "../../render-webgpu/useWebGPURenderer";
import { PrototypeOverlay } from "../prototype/PrototypeOverlay";
import { DRAG_COMPONENT_TYPE } from "../assets/AssetsPanel";
import { workerPool } from "../../worker";
import { createRect } from "../../types/shapes";

const WASM_JS_URL: string =
  typeof __RENDER_WASM_JS__ !== "undefined" ? __RENDER_WASM_JS__ : "/js/render-wasm.js";
const WASM_WASM_URL: string =
  typeof __RENDER_WASM_WASM__ !== "undefined" ? __RENDER_WASM_WASM__ : "/js/render-wasm.wasm";

type RenderMode = "loading" | "wasm" | "fallback";

interface DragState {
  startX: number;
  startY: number;
  currentX: number;
  currentY: number;
}

export function Canvas(): React.ReactElement {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const moduleRef = useRef<RenderWasmModule | null>(null);
  const modeRef = useRef<RenderMode>("loading");

  const [mode, setMode] = useState<RenderMode>("loading");
  const [size, setSize] = useState({ w: 800, h: 600 });
  const [drag, setDrag] = useState<DragState | null>(null);

  // Hand-tool pan drag (stored in a ref to avoid re-render every frame)
  const panDragRef = useRef<{ startClientX: number; startClientY: number; startPanX: number; startPanY: number } | null>(null);
  const [isPanning, setIsPanning] = useState(false);

  // Scale-tool drag (ref to avoid per-frame re-renders)
  interface ScaleDrag {
    shapeId: string;
    origX: number; origY: number; origW: number; origH: number;
    /** Canvas-space center of the shape at drag start. */
    cx: number; cy: number;
    /** Distance from mouse to center at drag start (canvas coords). */
    startDist: number;
  }
  const scaleDragRef = useRef<ScaleDrag | null>(null);

  const shapes = useCurrentPageShapes();
  const clearSelection = useSelectionStore((s) => s.clearSelection);
  const { select, selectRange } = useSelectionStore();
  const { zoom, panX, panY, activeTool, setTool, setPan } = useUiStore();
  const { addRect, addShape, addVectorNetwork, addInstanceShape, updateShape } = useDocumentStore();
  const { createInstance, components } = useComponentStore();

  // Pen tool state
  const pen = usePenStore();

  // ── WebGPU tile renderer (Phase 5, feature-gated) ─────────────────────────
  // canvasRef is the same <canvas> element Skia renders to.
  // When WebGPU is unavailable this returns null and has zero overhead.
  const gpuRenderer = useWebGPURenderer(canvasRef, shapes, zoom, panX, panY);

  // ── Resize observer ────────────────────────────────────────────────────────
  useLayoutEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const { width, height } = entries[0].contentRect;
      setSize({ w: Math.floor(width), h: Math.floor(height) });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // ── WASM boot + worker pool init (once) ───────────────────────────────────
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let cancelled = false;

    // Initialize background workers (non-blocking)
    workerPool.init();

    (async () => {
      const mod = await loadRenderWasm(WASM_JS_URL, WASM_WASM_URL);
      if (cancelled) return;
      if (mod !== null) {
        const ok = initCanvasContext(mod, canvas);
        if (ok) {
          moduleRef.current = mod;
          modeRef.current = "wasm";
          setMode("wasm");
          return;
        }
      }
      modeRef.current = "fallback";
      setMode("fallback");
    })();

    return () => {
      cancelled = true;
      const mod = moduleRef.current;
      if (mod && canvasRef.current) {
        cleanUp(mod, canvasRef.current);
        moduleRef.current = null;
      }
      workerPool.terminate();
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Re-render on store changes ─────────────────────────────────────────────
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || modeRef.current === "loading") return;

    if (modeRef.current === "wasm" && moduleRef.current) {
      syncScene(moduleRef.current, shapes, size.w, size.h);
    } else {
      const ctx = canvas.getContext("2d");
      if (ctx) {
        ctx.save();
        ctx.translate(panX, panY);
        ctx.scale(zoom, zoom);
        syncScene2D(ctx, shapes, size.w / zoom, size.h / zoom);
        ctx.restore();
      }
    }
  }, [shapes, size, zoom, panX, panY, mode]);

  // ── Mouse handlers for draw tools ─────────────────────────────────────────

  /** Convert mouse event coords to canvas-local coordinates (applying pan/zoom). */
  function toCanvas(e: React.MouseEvent): { x: number; y: number } {
    const rect = containerRef.current!.getBoundingClientRect();
    return {
      x: (e.clientX - rect.left - panX) / zoom,
      y: (e.clientY - rect.top  - panY) / zoom,
    };
  }

  // All drag-to-draw tools (share the DragState / preview overlay)
  const isDrawTool =
    activeTool === "rect"      || activeTool === "ellipse"  ||
    activeTool === "frame"     || activeTool === "selection" ||
    activeTool === "slice"     || activeTool === "line"      ||
    activeTool === "arrow"     || activeTool === "polygon"   ||
    activeTool === "star";
  const isPenTool = activeTool === "path";
  const isPrototypeTool = activeTool === "prototype";
  const isDevTool = activeTool === "dev";
  const isHandTool = activeTool === "hand";

  // Prototype store
  const proto = useProtoStore();

  // Dev mode store
  const devMode = useDevModeStore();

  // ── Commit pen session (close or open path) ────────────────────────────────
  const commitPen = useCallback(
    (closed: boolean) => {
      const { anchors, segments, reset } = pen;
      if (anchors.length < 2) { reset(); return; }

      let finalSegments = [...segments];
      if (closed && anchors.length >= 3) {
        // Close the path: add segment from last anchor back to first
        finalSegments.push({ s: anchors.length - 1, e: 0 });
      }

      const id = addVectorNetwork(anchors, finalSegments);
      select(id);
      reset();
      setTool("select");
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [pen, addVectorNetwork, select, setTool]
  );

  // Commit open path on Escape while pen tool is active
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (!isPenTool) return;
      if (e.key === "Escape") {
        commitPen(false);
      } else if (e.key === "Enter") {
        commitPen(true);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isPenTool, commitPen]);

  const handleMouseDown = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const { x, y } = toCanvas(e);

      // ── Prototype tool ──────────────────────────────────────────────────────
      if (isPrototypeTool) {
        const hitShape = shapes.find(
          (s) => s.parentId === null &&
            x >= s.bounds.x && x <= s.bounds.x + s.bounds.w &&
            y >= s.bounds.y && y <= s.bounds.y + s.bounds.h
        );
        if (hitShape) {
          if (proto.pendingSource && proto.pendingSource !== hitShape.id) {
            // Complete connection
            proto.addInteraction(proto.pendingSource, {
              trigger: "click",
              target: hitShape.id,
              transition: "instant",
              duration: 300,
              easing: "ease",
            });
            proto.setPendingSource(null);
            proto.setArrowCursor(null);
          } else {
            // Start new connection
            proto.setPendingSource(hitShape.id);
          }
        } else {
          // Clicked empty space — cancel
          proto.setPendingSource(null);
          proto.setArrowCursor(null);
          proto.clearConnectionSelection();
        }
        return;
      }

      // ── Pen tool ────────────────────────────────────────────────────────────
      if (isPenTool) {
        e.preventDefault();

        // Check if clicking close to the first anchor → close path
        if (pen.anchors.length >= 3) {
          const first = pen.anchors[0];
          const dist = Math.hypot(x - first.x, y - first.y);
          if (dist * zoom < 10) {
            commitPen(true);
            return;
          }
        }

        // Add a new anchor; start handle drag
        pen.addAnchor(x, y);
        pen.startAnchorDrag(pen.anchors.length, x, y); // index after add
        return;
      }
      // ── Scale tool — begin uniform scale drag ────────────────────────────────
      if (activeTool === "scale") {
        // Hit-test: find the topmost shape under the cursor
        const hit = [...shapes].reverse().find(
          (s) => x >= s.bounds.x && x <= s.bounds.x + s.bounds.w &&
                 y >= s.bounds.y && y <= s.bounds.y + s.bounds.h
        );
        if (hit) {
          // Select it if not already selected
          select(hit.id);
          const cx = hit.bounds.x + hit.bounds.w / 2;
          const cy = hit.bounds.y + hit.bounds.h / 2;
          const dx = x - cx;
          const dy = y - cy;
          const dist = Math.sqrt(dx * dx + dy * dy);
          scaleDragRef.current = {
            shapeId: hit.id,
            origX: hit.bounds.x, origY: hit.bounds.y,
            origW: hit.bounds.w, origH: hit.bounds.h,
            cx, cy,
            startDist: dist > 1 ? dist : 1, // avoid division by zero
          };
        }
        e.preventDefault();
        return;
      }
      // ── Hand tool — begin pan drag ─────────────────────────────────────────
      if (isHandTool) {
        panDragRef.current = { startClientX: e.clientX, startClientY: e.clientY, startPanX: panX, startPanY: panY };
        setIsPanning(true);
        e.preventDefault();
        return;
      }

      // ── Rect / ellipse / frame / slice / etc. drag ────────────────────────
      if (!isDrawTool) return;
      setDrag({ startX: x, startY: y, currentX: x, currentY: y });
      e.preventDefault();
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [isPenTool, isDrawTool, isPrototypeTool, isHandTool, pen, zoom, commitPen, panX, panY, proto, shapes]
  );

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const { x, y } = toCanvas(e);

      if (isPrototypeTool && proto.pendingSource) {
        const rect = containerRef.current!.getBoundingClientRect();
        proto.setArrowCursor({ x: e.clientX - rect.left, y: e.clientY - rect.top });
        return;
      }

      // ── Dev mode hover-inspect ─────────────────────────────────────────────
      if (isDevTool) {
        const hit = shapes.find(
          (s) =>
            x >= s.bounds.x &&
            x <= s.bounds.x + s.bounds.w &&
            y >= s.bounds.y &&
            y <= s.bounds.y + s.bounds.h
        );
        devMode.setInspectedShape(hit?.id ?? null);
        return;
      }

      if (isPenTool) {
        pen.setCursor({ x, y });
        if (pen.draggingAnchor !== null) {
          pen.updateAnchorHandle(x, y);
        }
        return;
      }

      // ── Hand tool pan ─────────────────────────────────────────────────────
      if (isHandTool && panDragRef.current) {
        const dx = e.clientX - panDragRef.current.startClientX;
        const dy = e.clientY - panDragRef.current.startClientY;
        setPan(panDragRef.current.startPanX + dx, panDragRef.current.startPanY + dy);
        return;
      }

      // ── Scale tool — live resize ──────────────────────────────────────────
      if (activeTool === "scale" && scaleDragRef.current) {
        const sd = scaleDragRef.current;
        const ddx = x - sd.cx;
        const ddy = y - sd.cy;
        const newDist = Math.sqrt(ddx * ddx + ddy * ddy);
        if (newDist < 1) return;
        const factor = newDist / sd.startDist;
        const newW = Math.max(2, sd.origW * factor);
        const newH = Math.max(2, sd.origH * factor);
        updateShape(sd.shapeId, {
          bounds: { x: sd.cx - newW / 2, y: sd.cy - newH / 2, w: newW, h: newH },
        });
        return;
      }

      if (!drag) return;
      setDrag((d) => d ? { ...d, currentX: x, currentY: y } : null);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [isPenTool, isPrototypeTool, isDevTool, isHandTool, pen, drag, panX, panY, zoom, proto, devMode, shapes, setPan]
  );

  const handleMouseUp = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (isPrototypeTool) return; // mouseup handled in mousedown

      if (isPenTool) {
        pen.endAnchorDrag();
        return;
      }

      // ── Hand tool — end pan drag ─────────────────────────────────────────
      if (isHandTool) {
        panDragRef.current = null;
        setIsPanning(false);
        return;
      }

      // ── Scale tool — end drag ────────────────────────────────────────────
      if (activeTool === "scale") {
        scaleDragRef.current = null;
        return;
      }

      if (!drag) {
        // No drag — if select tool, clear selection on empty canvas click
        if (activeTool === "select" && e.target === containerRef.current) {
          clearSelection();
        }
        return;
      }

      const minX = Math.min(drag.startX, drag.currentX);
      const minY = Math.min(drag.startY, drag.currentY);
      const w = Math.abs(drag.currentX - drag.startX);
      const h = Math.abs(drag.currentY - drag.startY);

      setDrag(null);

      // Ignore tiny accidental drags
      if (w < 4 || h < 4) return;

      const bounds = { x: minX, y: minY, w, h };

      if (activeTool === "rect") {
        const id = addRect(bounds);
        select(id);

      } else if (activeTool === "ellipse") {
        const id = crypto.randomUUID();
        const count = shapes.filter((s) => s.type === "ellipse").length + 1;
        const shape = createRect(id, `Ellipse ${count}`, bounds, "#0000ff");
        addShape({ ...shape, type: "ellipse" });
        select(id);

      } else if (activeTool === "frame") {
        const id = crypto.randomUUID();
        const count = shapes.filter((s) => s.type === "frame").length + 1;
        const shape = createRect(id, `Frame ${count}`, bounds, "#ffffff");
        addShape({ ...shape, type: "frame" });
        select(id);

      } else if (activeTool === "selection") {
        // Marquee — select all shapes whose bounds intersect the drag rect
        const hitIds = shapes
          .filter((s) =>
            s.bounds.x < minX + w && s.bounds.x + s.bounds.w > minX &&
            s.bounds.y < minY + h && s.bounds.y + s.bounds.h > minY
          )
          .map((s) => s.id);
        if (hitIds.length > 0) selectRange(hitIds);
        setTool("select");
        return; // don't revert tool again below

      } else if (activeTool === "slice") {
        const id = crypto.randomUUID();
        const count = shapes.filter((s) => s.name?.startsWith("Slice")).length + 1;
        const shape = createRect(id, `Slice ${count}`, bounds, "#0eaeff");
        addShape({ ...shape, type: "rect", fills: [{ type: "solid", color: "#0eaeff", opacity: 0.1 }] });
        select(id);

      } else if (activeTool === "line" || activeTool === "arrow") {
        const label = activeTool === "arrow" ? "Arrow" : "Line";
        const count = shapes.filter((s) => s.name?.startsWith(label)).length + 1;
        const id = addVectorNetwork(
          [{ x: drag.startX, y: drag.startY }, { x: drag.currentX, y: drag.currentY }],
          [{ s: 0, e: 1 }]
        );
        // Rename to Line/Arrow N
        const created = useDocumentStore.getState().shapes[id];
        if (created) addShape({ ...created, name: `${label} ${count}` });
        select(id);

      } else if (activeTool === "polygon") {
        // Placeholder: equilateral triangle inscribed in the drag bounds
        const id = crypto.randomUUID();
        const count = shapes.filter((s) => s.name?.startsWith("Polygon")).length + 1;
        const shape = createRect(id, `Polygon ${count}`, bounds, "#7c3aed");
        addShape({ ...shape, type: "path" });
        select(id);

      } else if (activeTool === "star") {
        const id = crypto.randomUUID();
        const count = shapes.filter((s) => s.name?.startsWith("Star")).length + 1;
        const shape = createRect(id, `Star ${count}`, bounds, "#d97706");
        addShape({ ...shape, type: "path" });
        select(id);
      }

      // Return to select tool after drawing
      setTool("select");
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [isPenTool, isPrototypeTool, isHandTool, pen, drag, activeTool, addRect, addShape, addVectorNetwork, select, selectRange, setTool, shapes, clearSelection, commitPen]
  );

  const cursor =
    isPanning                                    ? "grabbing"
    : isHandTool                                 ? "grab"
    : scaleDragRef.current                       ? "nwse-resize"
    : activeTool === "scale"                     ? "crosshair"
    : isDrawTool           ? "crosshair"
    : isPenTool            ? "crosshair"
    : isPrototypeTool      ? "crosshair"
    : activeTool === "text" ? "text"
    : "default";

  // Preview rect during drag (in pan/zoom space)
  const previewStyle: React.CSSProperties | null = drag
    ? (() => {
        const x = Math.min(drag.startX, drag.currentX) * zoom + panX;
        const y = Math.min(drag.startY, drag.currentY) * zoom + panY;
        const w = Math.abs(drag.currentX - drag.startX) * zoom;
        const h = Math.abs(drag.currentY - drag.startY) * zoom;
        return {
          position: "absolute" as const,
          left: x, top: y, width: w, height: h,
          border: "1px solid #89b4fa",
          background: "rgba(137,180,250,0.12)",
          pointerEvents: "none" as const,
          boxSizing: "border-box" as const,
        };
      })()
    : null;

  return (
    <div
      ref={containerRef}
      style={{ flex: 1, position: "relative", overflow: "hidden", background: "#313244", cursor }}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes(DRAG_COMPONENT_TYPE)) {
          e.preventDefault();
          e.dataTransfer.dropEffect = "copy";
        }
      }}
      onDrop={(e) => {
        const componentId = e.dataTransfer.getData(DRAG_COMPONENT_TYPE);
        if (!componentId) return;
        e.preventDefault();

        const rect = containerRef.current?.getBoundingClientRect();
        const canvasX = rect ? (e.clientX - rect.left) / zoom - panX : 0;
        const canvasY = rect ? (e.clientY - rect.top) / zoom - panY : 0;

        const comp = components[componentId];
        if (!comp) return;

        const instanceId = crypto.randomUUID();
        const meta = createInstance(instanceId, componentId);

        // Use component master bounds if available, otherwise fallback size.
        // The documentStore.shapes may hold the component shell; look it up.
        const masterShape = useDocumentStore.getState().shapes[componentId];
        const w = masterShape?.bounds.w ?? 200;
        const h = masterShape?.bounds.h ?? 100;

        addInstanceShape(instanceId, comp.name, { x: canvasX, y: canvasY, w, h }, meta);
      }}
    >
      <canvas
        ref={canvasRef}
        width={size.w}
        height={size.h}
        style={{ display: "block", width: "100%", height: "100%", pointerEvents: "none" }}
      />

      {/* Drag preview overlay */}
      {previewStyle && <div style={previewStyle} />}

      {/* Prototype tool overlay — arrows and connection drawing */}
      {isPrototypeTool && (
        <PrototypeOverlay
          shapes={shapes}
          zoom={zoom}
          panX={panX}
          panY={panY}
          arrowDragEnd={proto.arrowCursor}
        />
      )}

      {/* Pen tool overlay — SVG drawn on top of the WASM canvas */}
      {isPenTool && (
        <PenOverlay
          anchors={pen.anchors}
          segments={pen.segments}
          cursor={pen.cursor}
          zoom={zoom}
          panX={panX}
          panY={panY}
        />
      )}

      {shapes.length === 0 && mode !== "loading" && (
        <div style={{
          position: "absolute", inset: 0, display: "flex",
          alignItems: "center", justifyContent: "center",
          pointerEvents: "none", color: "#585b70",
          fontSize: "13px", fontFamily: "monospace",
        }}>
          Press R to add a rectangle
        </div>
      )}

      <span style={{
        position: "absolute", bottom: 8, right: 8, padding: "2px 8px",
        borderRadius: "4px", fontSize: "11px", fontFamily: "monospace",
        background: mode === "wasm" ? "#a6e3a1" : mode === "fallback" ? "#f38ba8" : "#cdd6f4",
        color: "#1e1e2e",
      }}>
        {mode === "loading" && "⏳ loading…"}
        {mode === "wasm"     && "✓ render-wasm / Skia"}
        {mode === "fallback" && "⚠ Canvas 2D fallback"}
      </span>

      {/* WebGPU active badge */}
      {gpuRenderer?.active && (
        <span
          title="Phase 5: WebGPU tile renderer active"
          style={{
            position: "absolute", bottom: 8, right: 172,
            padding: "2px 8px", borderRadius: "4px",
            fontSize: "11px", fontFamily: "monospace",
            background: "#cba6f7", color: "#1e1e2e",
          }}
        >
          ⚡ WebGPU
        </span>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Pen preview SVG overlay
// ─────────────────────────────────────────────────────────────────────────────

import type { VNAnchor, VNSegment } from "../../types/shapes";
import type { PenMousePos } from "../../stores/penStore";

interface PenOverlayProps {
  anchors: VNAnchor[];
  segments: VNSegment[];
  cursor: PenMousePos | null;
  zoom: number;
  panX: number;
  panY: number;
}

function toScreen(x: number, y: number, zoom: number, panX: number, panY: number) {
  return { sx: x * zoom + panX, sy: y * zoom + panY };
}

function PenOverlay({ anchors, segments, cursor, zoom, panX, panY }: PenOverlayProps): React.ReactElement {
  const ANCHOR_R = 4;
  const CLOSE_THRESHOLD_PX = 10;

  // Build SVG path string from committed segments
  let pathD = "";
  for (const seg of segments) {
    const a = anchors[seg.s];
    const b = anchors[seg.e];
    if (!a || !b) continue;
    const { sx: ax, sy: ay } = toScreen(a.x, a.y, zoom, panX, panY);
    const { sx: bx, sy: by } = toScreen(b.x, b.y, zoom, panX, panY);
    if (seg.c1 && seg.c2) {
      const { sx: c1x, sy: c1y } = toScreen(seg.c1[0], seg.c1[1], zoom, panX, panY);
      const { sx: c2x, sy: c2y } = toScreen(seg.c2[0], seg.c2[1], zoom, panX, panY);
      pathD += `M ${ax} ${ay} C ${c1x} ${c1y} ${c2x} ${c2y} ${bx} ${by} `;
    } else {
      pathD += `M ${ax} ${ay} L ${bx} ${by} `;
    }
  }

  // Live preview segment from last anchor to cursor
  let previewD = "";
  if (anchors.length > 0 && cursor) {
    const last = anchors[anchors.length - 1];
    const { sx: lx, sy: ly } = toScreen(last.x, last.y, zoom, panX, panY);
    const { sx: cx, sy: cy } = toScreen(cursor.x, cursor.y, zoom, panX, panY);
    previewD = `M ${lx} ${ly} L ${cx} ${cy}`;
  }

  // Is cursor near the first anchor (close-path hint)?
  let nearFirst = false;
  if (anchors.length >= 3 && cursor) {
    const first = anchors[0];
    const dx = (cursor.x - first.x) * zoom;
    const dy = (cursor.y - first.y) * zoom;
    nearFirst = Math.hypot(dx, dy) < CLOSE_THRESHOLD_PX;
  }

  return (
    <svg
      style={{
        position: "absolute",
        inset: 0,
        width: "100%",
        height: "100%",
        pointerEvents: "none",
        overflow: "visible",
      }}
    >
      {/* Committed path segments */}
      {pathD && (
        <path d={pathD} fill="none" stroke="#89b4fa" strokeWidth={1.5} />
      )}

      {/* Live preview segment */}
      {previewD && (
        <path d={previewD} fill="none" stroke="#89b4fa" strokeWidth={1.5} strokeDasharray="4 3" />
      )}

      {/* Anchor dots */}
      {anchors.map((a, i) => {
        const { sx, sy } = toScreen(a.x, a.y, zoom, panX, panY);
        const isFirst = i === 0;
        const highlight = isFirst && nearFirst;
        return (
          <circle
            key={i}
            cx={sx}
            cy={sy}
            r={highlight ? ANCHOR_R + 2 : ANCHOR_R}
            fill={highlight ? "#a6e3a1" : "#1e1e2e"}
            stroke={highlight ? "#a6e3a1" : "#89b4fa"}
            strokeWidth={1.5}
          />
        );
      })}
    </svg>
  );
}
