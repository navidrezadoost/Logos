/**
 * components/layers/LayersPanel.tsx  (M3 revision)
 *
 * Virtualized layers panel using @tanstack/react-virtual.
 * Supports 10 000+ shapes without DOM overhead — only the visible rows
 * are mounted.
 */

import { useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useCurrentPageScene, useDocumentStore } from "../../stores/documentStore";
import { useSelectionStore, useIsSelected } from "../../stores/selectionStore";
import { createRect, type Shape } from "../../types/shapes";
import type { HierarchyOperation } from "../../stores/hierarchyOperations";
import { MultiSelectChips, type ScrollMode } from "../ui/MultiSelectChips";
import { theme } from "../../theme/colors";

const ITEM_HEIGHT = 32;

export function LayersPanel(): React.ReactElement {
  const { page, shapes: pageShapeRecords } = useCurrentPageScene();
  const shapesById = new Map(pageShapeRecords.map((shape) => [shape.id, shape]));
  const treeRows: Array<{ shape: Shape; depth: number }> = [];
  const visited = new Set<string>();
  const stack = (page?.rootShapeIds ?? []).slice().reverse().map((id) => ({ id, depth: 0 }));
  while (stack.length > 0) {
    const { id, depth } = stack.pop()!;
    const shape = shapesById.get(id);
    if (!shape || visited.has(id)) continue;
    visited.add(id);
    treeRows.push({ shape, depth });
    for (const childId of (Array.isArray(shape.children) ? [...shape.children] : []).reverse()) stack.push({ id: childId, depth: depth + 1 });
  }
  const shapes = treeRows.map(({ shape }) => shape);
  const [operationDiagnostic, setOperationDiagnostic] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const lastSelectedIdRef = useRef<string | null>(null);

  // ── Type filter ───────────────────────────────────────────────────────────
  const allTypes = Array.from(new Set(shapes.map((s) => s.type))).sort() as string[];
  const [filteredTypes, setFilteredTypes] = useState<string[]>([]);
  const [filterScrollMode, setFilterScrollMode] = useState<ScrollMode>("wrap");

  const visibleShapes =
    filteredTypes.length === 0
      ? treeRows
      : treeRows.filter(({ shape }) => filteredTypes.includes(shape.type));

  function handleRowSelect(e: React.MouseEvent<HTMLDivElement>, shape: Shape) {
    const { select, toggleSelect, selectRange } = useSelectionStore.getState();

    if (e.shiftKey && lastSelectedIdRef.current) {
      const start = visibleShapes.findIndex(({ shape: item }) => item.id === lastSelectedIdRef.current);
      const end = visibleShapes.findIndex(({ shape: item }) => item.id === shape.id);
      if (start !== -1 && end !== -1) {
        const [from, to] = start < end ? [start, end] : [end, start];
          selectRange(visibleShapes.slice(from, to + 1).map(({ shape: item }) => item.id));
        return;
      }
    }

    if (e.metaKey || e.ctrlKey) {
      toggleSelect(shape.id);
      lastSelectedIdRef.current = shape.id;
      return;
    }

    select(shape.id);
    lastSelectedIdRef.current = shape.id;
  }

  function applyOperation(operation: HierarchyOperation): boolean {
    const result = useDocumentStore.getState().applyHierarchyOperation(operation);
    if (!result.ok) {
      setOperationDiagnostic(result.diagnostic.code);
      return false;
    }
    setOperationDiagnostic(null);
    return true;
  }

  function insertChild(parent: Shape): void {
    const child = createRect(
      crypto.randomUUID(),
      "Rectangle",
      { x: parent.bounds.x, y: parent.bounds.y, w: Math.min(80, parent.bounds.w), h: Math.min(60, parent.bounds.h) },
      "#0000ff",
    );
    const result = useDocumentStore.getState().applyHierarchyOperation({ kind: "insert-child", parentId: parent.id, shape: child, index: 0 });
    if (!result.ok) setOperationDiagnostic(result.diagnostic.code);
    else {
      setOperationDiagnostic(null);
      useSelectionStore.getState().select(child.id);
    }
  }

  const virtualizer = useVirtualizer({
    count: visibleShapes.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ITEM_HEIGHT,
    overscan: 8,
  });

  return (
    <div style={panelStyle}>
      <div style={headerStyle}>Layers</div>
      {operationDiagnostic && <div role="alert" data-testid="hierarchy-operation-diagnostic" style={{ padding: "6px 8px", color: theme.error, fontSize: 11 }}>{operationDiagnostic}</div>}

      {/* Type filter bar — only shown when shapes exist */}
      {allTypes.length > 0 && (
        <div style={{ padding: "6px 8px", borderBottom: `1px solid ${theme.border}` }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 4 }}>
            <span style={{ fontSize: 10, color: theme.textDim, textTransform: "uppercase", letterSpacing: "0.05em" }}>
              Filter
            </span>
            <div style={{ display: "flex", gap: 2 }}>
              {(["wrap", "scroll", "truncate"] as ScrollMode[]).map((mode) => (
                <button
                  key={mode}
                  onClick={() => setFilterScrollMode(mode)}
                  title={`${mode} mode`}
                  style={{
                    background: filterScrollMode === mode ? theme.surface : "transparent",
                    border: "1px solid",
                    borderColor: filterScrollMode === mode ? theme.borderStrong : "transparent",
                    borderRadius: 3,
                    color: filterScrollMode === mode ? theme.text : theme.borderStrong,
                    fontSize: 10,
                    padding: "1px 4px",
                    cursor: "pointer",
                  }}
                >
                  {mode[0].toUpperCase()}
                </button>
              ))}
            </div>
          </div>
          <MultiSelectChips
            options={allTypes}
            selected={filteredTypes}
            onChange={setFilteredTypes}
            getLabel={(v) => v}
            getIcon={(v) => shapeIcon(v)}
            scrollMode={filterScrollMode}
            maxVisible={3}
            allowEmpty
          />
        </div>
      )}
      {visibleShapes.length === 0 ? (
        <div style={{ padding: 12, color: "#45475a", fontSize: 11 }}>
          {filteredTypes.length > 0 ? "No shapes match the active filter." : "No shapes yet. Press R to add one."}
        </div>
      ) : (
        <div ref={scrollRef} style={{ overflowY: "auto", flex: 1 }}>
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((vRow) => {
              const { shape, depth } = visibleShapes[vRow.index];
              return (
                <div
                  key={shape.id}
                  style={{
                    position: "absolute",
                    top: vRow.start,
                    left: 0,
                    right: 0,
                    height: ITEM_HEIGHT,
                  }}
                >
                  <ShapeRow
                    shape={shape}
                    depth={depth}
                    onSelect={handleRowSelect}
                    siblings={shape.parentId ? shapesById.get(shape.parentId)?.children ?? [] : page?.rootShapeIds ?? []}
                    containers={shapes.filter((candidate) =>
                      (candidate.type === "frame" || candidate.type === "group") &&
                      candidate.id !== shape.id && !isDescendant(candidate.id, shape.id, shapesById)
                    )}
                    onOperation={applyOperation}
                    onInsertChild={insertChild}
                  />
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────

function ShapeRow({
  shape,
  depth,
  onSelect,
  siblings,
  containers,
  onOperation,
  onInsertChild,
}: {
  shape: Shape;
  depth: number;
  onSelect: (e: React.MouseEvent<HTMLDivElement>, shape: Shape) => void;
  siblings: readonly string[];
  containers: Shape[];
  onOperation: (operation: HierarchyOperation) => boolean;
  onInsertChild: (parent: Shape) => void;
}): React.ReactElement {
  const isSelected = useIsSelected(shape.id);
  const siblingsIndex = siblings.indexOf(shape.id);

  return (
    <div
      onClick={(e) => onSelect(e, shape)}
      data-layer-id={shape.id}
      style={{
        display: "flex",
        alignItems: "center",
        height: ITEM_HEIGHT,
        padding: `0 8px 0 ${8 + depth * 14}px`,
        gap: 6,
        cursor: "pointer",
        background: isSelected ? theme.surface : "transparent",
        borderLeft: isSelected ? `2px solid ${theme.accent}` : "2px solid transparent",
        fontSize: 12,
        color: theme.text,
        userSelect: "none",
      }}
      title="Click to select · Ctrl/Cmd-click to toggle · Shift-click for range"
      onMouseEnter={(e) => {
        if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = theme.panelHover;
      }}
      onMouseLeave={(e) => {
        if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = "transparent";
      }}
    >
      <span style={{ color: "#7f849c", fontSize: 13 }}>{shapeIcon(shape.type)}</span>
      <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
        {shape.name}
      </span>
      <details onClick={(e) => e.stopPropagation()} style={{ position: "relative" }}>
        <summary aria-label={`Actions for ${shape.name}`} title={`Actions for ${shape.name}`} style={{ listStyle: "none", cursor: "pointer", padding: "0 4px", color: theme.textMuted }}>⋯</summary>
        <div style={{ position: "absolute", top: 20, right: 0, zIndex: 20, width: 190, padding: 8, display: "grid", gap: 5, background: theme.panel, border: `1px solid ${theme.borderStrong}`, borderRadius: 5, boxShadow: "0 6px 20px #0008" }}>
          <button type="button" aria-label={`Rename ${shape.name}`} disabled={shape.locked} onClick={() => {
            const name = window.prompt("Layer name", shape.name);
            if (name !== null) onOperation({ kind: "rename", shapeId: shape.id, name });
          }}>Rename</button>
          <button type="button" aria-label={`Move ${shape.name} up`} disabled={shape.locked || siblingsIndex <= 0} onClick={() => onOperation({ kind: "reorder", shapeId: shape.id, toIndex: siblingsIndex - 1 })}>Move up</button>
          <button type="button" aria-label={`Move ${shape.name} down`} disabled={shape.locked || siblingsIndex < 0 || siblingsIndex >= siblings.length - 1} onClick={() => onOperation({ kind: "reorder", shapeId: shape.id, toIndex: siblingsIndex + 1 })}>Move down</button>
          {(shape.type === "frame" || shape.type === "group") && <button type="button" aria-label={`Add child to ${shape.name}`} disabled={shape.locked} onClick={() => onInsertChild(shape)}>Add rectangle inside</button>}
          <label style={{ display: "grid", gap: 3, fontSize: 11 }}>Move to
            <select aria-label={`Move ${shape.name} to`} value={shape.parentId ?? "__root__"} disabled={shape.locked} onChange={(event) => {
              const parentId = event.currentTarget.value === "__root__" ? null : event.currentTarget.value;
              if (parentId !== shape.parentId) onOperation({ kind: "reparent", shapeId: shape.id, parentId, index: 0, preserveWorldPosition: true });
            }}>
              <option value="__root__" disabled={shape.parentId === null}>Page root (keep position)</option>
              {containers.map((container) => <option key={container.id} value={container.id} disabled={container.id === shape.parentId || container.locked}>{container.name} (keep position)</option>)}
            </select>
          </label>
          <button type="button" aria-label={`Delete ${shape.name}`} disabled={shape.locked} onClick={() => onOperation({ kind: "delete", shapeId: shape.id })}>Delete subtree</button>
        </div>
      </details>
    </div>
  );
}

function isDescendant(candidateId: string, ancestorId: string, shapes: ReadonlyMap<string, Shape>): boolean {
  const pending = [...(shapes.get(ancestorId)?.children ?? [])];
  const seen = new Set<string>();
  while (pending.length > 0) {
    const id = pending.pop()!;
    if (id === candidateId) return true;
    if (seen.has(id)) continue;
    seen.add(id);
    pending.push(...(shapes.get(id)?.children ?? []));
  }
  return false;
}

function shapeIcon(type: string): string {
  switch (type) {
    case "rect": return "▭";
    case "ellipse":
    case "circle": return "○";
    case "text": return "T";
    case "path":
    case "bool": return "✏";
    case "frame": return "⬜";
    case "group": return "▤";
    default: return "◻";
  }
}

const panelStyle: React.CSSProperties = {
  width: 220,
  background: theme.panel,
  borderRight: `1px solid ${theme.border}`,
  display: "flex",
  flexDirection: "column",
  flexShrink: 0,
};

const headerStyle: React.CSSProperties = {
  padding: "8px 12px",
  fontSize: 11,
  fontWeight: 600,
  color: theme.textMuted,
  letterSpacing: "0.05em",
  textTransform: "uppercase",
  borderBottom: `1px solid ${theme.border}`,
};
