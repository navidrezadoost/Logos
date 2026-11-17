/**
 * Versioned, renderer-independent characterization of the current Logos
 * strict hierarchy subset. This is a compatibility contract and test oracle;
 * it is not wired as a second document store or mutation authority.
 */
export const FIGMA_HIERARCHY_CONTRACT_VERSION = 1 as const;

export const FIGMA_HIERARCHY_CONTRACT_V1 = Object.freeze({
  version: FIGMA_HIERARCHY_CONTRACT_VERSION,
  supportedKinds: ["frame", "group", "rect"] as const,
  containment: {
    page: ["frame", "rect"],
    frame: ["rect"],
    group: ["rect"],
    rect: [],
  } as const,
  unsupportedReferenceSemantics: ["section", "component-set", "instance"] as const,
  ordering: {
    pages: "pageOrder sequence; current editor order is retained verbatim",
    roots: "rootShapeIds sequence, front-to-back; index 0 is topmost",
    children: "children sequence, front-to-back; index 0 is topmost",
    canvasHitTest: "front-to-back; descend into children before parent",
    layersPanel: "front-to-back siblings; parent before its descendants",
    renderer: "back-to-front sibling arrays; reverse once at scene-projection boundary",
  },
  strictOperations: [
    "create-root",
    "insert-child",
    "rename",
    "delete",
    "reorder",
    "reorder-root-list",
    "reparent",
  ] as const,
});

export type StrictHierarchyKind = typeof FIGMA_HIERARCHY_CONTRACT_V1.supportedKinds[number];
export type StrictHierarchyParent = "page" | StrictHierarchyKind;

/** Check the current validated subset, without consulting renderer capabilities. */
export function allowsStrictContainment(parent: StrictHierarchyParent, child: string): boolean {
  const children = FIGMA_HIERARCHY_CONTRACT_V1.containment[parent];
  return (children as readonly string[]).includes(child);
}
