export type RendererBackend = "canvas2d" | "wasm";

export const RENDERER_DIAGNOSTIC_CODES = {
  unsupportedShape: "LOGOS_RENDER_UNSUPPORTED_SHAPE",
  renderFailed: "LOGOS_RENDER_FAILED",
} as const;

export type RendererFrameDisposition = "none" | "previous" | "unverified";

export type RendererStatus =
  | { state: "READY"; backend: RendererBackend }
  | { state: "RENDERING"; backend: RendererBackend }
  | {
      state: "RECOVERING";
      backend: RendererBackend;
      frameDisposition: RendererFrameDisposition;
    }
  | {
      state: "UNSUPPORTED_SHAPE";
      backend: RendererBackend;
      code: typeof RENDERER_DIAGNOSTIC_CODES.unsupportedShape;
      shapeId: string;
      shapeType: string;
      frameDisposition: RendererFrameDisposition;
    }
  | {
      state: "RENDER_FAILED";
      backend: RendererBackend;
      code: typeof RENDERER_DIAGNOSTIC_CODES.renderFailed;
      frameDisposition: RendererFrameDisposition;
    }
  | {
      state: "SCENE_REJECTED";
      backend: RendererBackend;
      code: string;
      shapeId?: string;
      shapeType?: string;
      frameDisposition: RendererFrameDisposition;
    };

export class SceneRejectedRenderError extends Error {
  constructor(
    readonly code: string,
    readonly shapeId?: string,
    readonly shapeType?: string,
  ) {
    super(code);
    this.name = "SceneRejectedRenderError";
  }
}

/** Shared adapter error; raw engine exceptions stay out of the editor status. */
export class UnsupportedShapeRenderError extends Error {
  readonly code = RENDERER_DIAGNOSTIC_CODES.unsupportedShape;

  constructor(
    readonly shapeId: string,
    readonly shapeType: string,
    readonly backend: RendererBackend,
  ) {
    super(`Shape ${shapeId} (${shapeType}) is unsupported by ${backend}`);
    this.name = "UnsupportedShapeRenderError";
  }
}

export function rendererFailureStatus(
  error: unknown,
  backend: RendererBackend,
  hasPreviousFrame: boolean,
): Extract<RendererStatus, { state: "UNSUPPORTED_SHAPE" | "RENDER_FAILED" | "SCENE_REJECTED" }> {
  if (error instanceof UnsupportedShapeRenderError) {
    return {
      state: "UNSUPPORTED_SHAPE",
      backend: error.backend,
      code: error.code,
      shapeId: error.shapeId,
      shapeType: error.shapeType,
      frameDisposition: hasPreviousFrame ? "previous" : "none",
    };
  }

  if (error instanceof SceneRejectedRenderError) {
    const frameDisposition = error.code === "LOGOS_SCENE_RECOVERY_FAILED" || error.code === "LOGOS_SCENE_MODULE_POISONED"
      ? (hasPreviousFrame ? "unverified" : "none")
      : (hasPreviousFrame ? "previous" : "none");
    return {
      state: "SCENE_REJECTED",
      backend,
      code: error.code,
      shapeId: error.shapeId,
      shapeType: error.shapeType,
      frameDisposition,
    };
  }

  return {
    state: "RENDER_FAILED",
    backend,
    code: RENDERER_DIAGNOSTIC_CODES.renderFailed,
    // A generic renderer exception may occur after partial target writes.
    frameDisposition: hasPreviousFrame ? "unverified" : "none",
  };
}
