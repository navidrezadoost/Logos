/** Test-build-only stage probes for G1-B9.5. Production builds compile this off. */
export type G1B95Stage =
  | "react_hierarchy_planning_validation"
  | "zustand_publication_wall"
  | "selection_reconciliation";

type ProbeWindow = typeof globalThis & {
  __logosG1B95Probe?: (stage: G1B95Stage, milliseconds: number) => void;
  __g1b95?: {
    activeSample: () => { sampleId: string; expectedFrameGeneration: number } | null;
    recordCanvasStage: (sampleId: string, stage: string, timestamp: number, frameGeneration: number) => void;
    publishFrame: (sampleId: string, timestamp: number, frameGeneration: number) => void;
  };
};

declare const __LOGOS_G1_B9_5_PERF_BUILD__: boolean;

export function recordG1B95Stage(stage: G1B95Stage, milliseconds: number): void {
  if (typeof __LOGOS_G1_B9_5_PERF_BUILD__ !== "undefined" && __LOGOS_G1_B9_5_PERF_BUILD__) {
    try { (globalThis as ProbeWindow).__logosG1B95Probe?.(stage, milliseconds); } catch { /* test evidence must not affect editing */ }
  }
}

/** Browser-local, test-build-only timing hooks. These do nothing in production builds. */
export function activeG1B95Sample(): { sampleId: string; expectedFrameGeneration: number } | null {
  if (typeof __LOGOS_G1_B9_5_PERF_BUILD__ === "undefined" || !__LOGOS_G1_B9_5_PERF_BUILD__) return null;
  try { return (globalThis as ProbeWindow).__g1b95?.activeSample() ?? null; } catch { return null; }
}

export function recordG1B95CanvasTimestamp(
  sampleId: string,
  stage: string,
  timestamp: number,
  frameGeneration: number,
): void {
  if (typeof __LOGOS_G1_B9_5_PERF_BUILD__ === "undefined" || !__LOGOS_G1_B9_5_PERF_BUILD__) return;
  try { (globalThis as ProbeWindow).__g1b95?.recordCanvasStage(sampleId, stage, timestamp, frameGeneration); } catch { /* diagnostics must not affect editing */ }
}

export function publishG1B95Frame(sampleId: string, timestamp: number, frameGeneration: number): void {
  if (typeof __LOGOS_G1_B9_5_PERF_BUILD__ === "undefined" || !__LOGOS_G1_B9_5_PERF_BUILD__) return;
  try { (globalThis as ProbeWindow).__g1b95?.publishFrame(sampleId, timestamp, frameGeneration); } catch { /* diagnostics must not affect editing */ }
}
