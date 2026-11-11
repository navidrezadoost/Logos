/**
 * Compile-time React projection fixture for test/fixtures/p0-document-compat.
 * This is intentionally not imported by the application runtime. It makes the
 * expected projection type-check against the existing React Shape contract.
 */

import type { Shape } from "../shapes";

export const p0DocumentProjectionShape: Shape = {
  id: "f0000000-0000-4000-8000-000000000003",
  type: "rect",
  name: "P0 rectangle",
  bounds: { x: 10, y: 20, w: 80, h: 40 },
  transform: [1, 0, 0, 1, 0, 0],
  rotation: 0,
  fills: [{ type: "solid", color: "#336699", opacity: 1 }],
  opacity: 1,
  hidden: false,
  locked: false,
  parentId: null,
  children: [],
};
