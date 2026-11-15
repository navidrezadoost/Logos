/** Test-build-only stage probes for G1-B9.5. Production builds compile this off. */
export type G1B95Stage =
  | "react_hierarchy_planning_validation"
  | "zustand_publication_wall"
  | "selection_reconciliation";

type ProbeWindow = typeof globalThis & {
  __logosG1B95Probe?: (stage: G1B95Stage, milliseconds: number) => void;
};

declare const __LOGOS_G1_B9_5_PERF_BUILD__: boolean;

export function recordG1B95Stage(stage: G1B95Stage, milliseconds: number): void {
  if (typeof __LOGOS_G1_B9_5_PERF_BUILD__ !== "undefined" && __LOGOS_G1_B9_5_PERF_BUILD__) {
    try { (globalThis as ProbeWindow).__logosG1B95Probe?.(stage, milliseconds); } catch { /* test evidence must not affect editing */ }
  }
}
