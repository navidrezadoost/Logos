/** Rust RawShapeType values. Keep aligned with render-wasm/src/wasm/shapes/mod.rs. */
export const WASM_SHAPE_TYPE_MAP: Readonly<Record<string, number>> = Object.freeze({
  frame: 0,
  group: 1,
  bool: 2,
  rect: 3,
  path: 4,
  text: 5,
  circle: 6,
  ellipse: 6,
  "svg-raw": 7,
});

export function wasmShapeDiscriminant(type: string): number | null {
  return Object.prototype.hasOwnProperty.call(WASM_SHAPE_TYPE_MAP, type)
    ? WASM_SHAPE_TYPE_MAP[type]
    : null;
}
