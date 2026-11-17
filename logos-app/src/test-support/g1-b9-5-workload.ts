import type { Page } from "../stores/documentStore";
import type { HierarchyOperation } from "../stores/hierarchyOperations";
import type { Shape } from "../types/shapes";

export type G1B95Size = 21 | 85 | 341;
export type G1B95OperationKind = "create" | "insert-child" | "rename" | "sibling-reorder" | "root-reorder" | "subtree-delete" | "reparent";

const LEVEL_COUNTS: Record<G1B95Size, readonly number[]> = {
  21: [2, 8, 11],
  85: [2, 8, 32, 43],
  341: [2, 8, 32, 128, 171],
};

function uuid(counter: number): string {
  return `95000000-0000-4000-8000-${counter.toString(16).padStart(12, "0")}`;
}

export interface G1B95Fixture {
  /** Stable test-only identity for Rust candidate evaluation; never written to the document store. */
  documentId: string;
  page: Page;
  pageOrder: string[];
  shapes: Record<string, Shape>;
  idsByLevel: string[][];
  rootA: string;
  rootB: string;
  branchA: string;
  branchB: string;
  redLeaf: string;
  orangeLeaf: string;
  blueLeaf: string;
}

export function buildG1B95Fixture(size: G1B95Size, serial = 1): G1B95Fixture {
  const counts = LEVEL_COUNTS[size];
  const pageId = uuid(900000 + size * 10 + serial);
  const shapes: Record<string, Shape> = {};
  const idsByLevel: string[][] = counts.map(() => []);
  let idCounter = 1;

  const add = (level: number, parentId: string | null, rootIndex: number, branchIndex: number): string => {
    const id = uuid(idCounter++);
    const isLeaf = level === counts.length - 1;
    const fill = isLeaf && rootIndex === 0 && branchIndex === 0 ? "#ff0000"
      : isLeaf && rootIndex === 0 && branchIndex === 1 ? "#ff8800"
        : isLeaf && rootIndex === 1 && branchIndex === 0 ? "#0000ff" : null;
    const shape: Shape = {
      id,
      type: level === 0 ? "frame" : isLeaf ? "rect" : level % 2 === 1 ? "group" : "frame",
      name: `fixture-${size}-${level}-${idCounter}`,
      bounds: !isLeaf ? { x: 0, y: 0, w: 800, h: 600 }
        : fill ? { x: 20, y: 20, w: 60, h: 60 }
          : { x: 120 + (idCounter % 5) * 22, y: 120 + (idCounter % 4) * 22, w: 14, h: 14 },
      transform: [1, 0, 0, 1, 0, 0],
      rotation: 0,
      fills: fill ? [{ type: "solid", color: fill, opacity: 1 }] : [],
      opacity: 1,
      hidden: false,
      locked: false,
      parentId,
      children: [],
    };
    shapes[id] = shape;
    idsByLevel[level].push(id);
    return id;
  };

  const roots = [add(0, null, 0, 0), add(0, null, 1, 0)];
  let parents = roots;
  for (let level = 1; level < counts.length; level += 1) {
    const targetCount = counts[level];
    let remaining = targetCount;
    let nextParents: string[] = [];
    let branchOrdinal = 0;
    for (let parentIndex = 0; parentIndex < parents.length; parentIndex += 1) {
      const parentsLeft = parents.length - parentIndex;
      const childCount = Math.ceil(remaining / parentsLeft);
      const parentId = parents[parentIndex];
      const parent = shapes[parentId];
      const rootIndex = level === 1 ? parentIndex : Number(isDescendantOf(shapes, parentId, roots[1]));
      const branchIndex = level === 1 ? 0 : firstLevelBranchIndex(shapes, parentId, roots[rootIndex]);
      for (let childIndex = 0; childIndex < childCount; childIndex += 1) {
        const id = add(level, parentId, rootIndex, branchIndex || branchOrdinal);
        parent.children.push(id);
        nextParents.push(id);
        branchOrdinal += 1;
      }
      remaining -= childCount;
    }
    parents = nextParents;
  }
  const branchA = shapes[roots[0]].children[0];
  const branchB = shapes[roots[0]].children[1];
  const redLeaf = descendantLeaves(shapes, branchA)[0];
  const orangeLeaf = descendantLeaves(shapes, branchB)[0];
  const blueLeaf = descendantLeaves(shapes, shapes[roots[1]].children[0])[0];
  // The branch identity determines the three overlap colors at the final level.
  shapes[redLeaf].fills = [{ type: "solid", color: "#ff0000", opacity: 1 }];
  shapes[orangeLeaf].fills = [{ type: "solid", color: "#ff8800", opacity: 1 }];
  shapes[blueLeaf].fills = [{ type: "solid", color: "#0000ff", opacity: 1 }];
  for (const id of [redLeaf, orangeLeaf, blueLeaf]) shapes[id].bounds = { x: 20, y: 20, w: 60, h: 60 };

  const page: Page = { id: pageId, name: `G1-B9.5 ${size}`, rootShapeIds: roots };
  return { documentId: uuid(700000 + size), page, pageOrder: [pageId], shapes, idsByLevel,
    rootA: roots[0], rootB: roots[1], branchA, branchB, redLeaf, orangeLeaf, blueLeaf };
}

function isDescendantOf(shapes: Record<string, Shape>, id: string, ancestorId: string): boolean {
  let parent = shapes[id]?.parentId ?? null;
  while (parent) {
    if (parent === ancestorId) return true;
    parent = shapes[parent]?.parentId ?? null;
  }
  return false;
}

function firstLevelBranchIndex(shapes: Record<string, Shape>, id: string, rootId: string): number {
  let childId = id;
  while (shapes[childId]?.parentId && shapes[childId].parentId !== rootId) childId = shapes[childId].parentId!;
  return Math.max(0, shapes[rootId]?.children.indexOf(childId) ?? 0);
}

function descendantLeaves(shapes: Record<string, Shape>, id: string): string[] {
  const found: string[] = [];
  const pending = [id];
  while (pending.length) {
    const current = pending.pop()!;
    const shape = shapes[current];
    if (!shape.children.length) found.push(current);
    else pending.push(...shape.children);
  }
  return found;
}

export function makeG1B95Operation(fixture: G1B95Fixture, kind: G1B95OperationKind, serial: number): HierarchyOperation {
  switch (kind) {
    case "create":
      return { kind: "create-root", shape: {
        id: uuid(800000 + serial), type: "rect", name: `created-${serial}`,
        bounds: { x: 420, y: 35, w: 40, h: 40 }, transform: [1, 0, 0, 1, 0, 0], rotation: 0,
        fills: [{ type: "solid", color: "#00ff00", opacity: 1 }], opacity: 1, hidden: false, locked: false,
        parentId: null, children: [],
      } };
    case "insert-child":
      return { kind: "insert-child", parentId: fixture.rootA, index: 0, shape: {
        id: uuid(810000 + serial), type: "rect", name: `inserted-${serial}`,
        bounds: { x: 120, y: 120, w: 30, h: 30 }, transform: [1, 0, 0, 1, 0, 0], rotation: 0,
        fills: [{ type: "solid", color: "#ff00ff", opacity: 1 }], opacity: 1, hidden: false, locked: false,
        parentId: null, children: [],
      } };
    case "rename": return { kind: "rename", shapeId: fixture.redLeaf, name: `renamed-${serial}` };
    case "sibling-reorder": return { kind: "reorder", shapeId: fixture.branchA, toIndex: 1 };
    case "root-reorder": return { kind: "reorder-root-list", rootShapeIds: [fixture.rootB, fixture.rootA] };
    case "subtree-delete": return { kind: "delete", shapeId: fixture.branchA };
    case "reparent": return { kind: "reparent", shapeId: fixture.redLeaf, parentId: fixture.branchB, index: 0, preserveWorldPosition: true };
  }
}
