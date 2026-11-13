import { useId } from "react";
import type { RendererStatus } from "../../render-wasm/renderer-status";
import { theme } from "../../theme/colors";

interface RendererStatusOverlayProps {
  status: RendererStatus | null;
  shapeName?: string;
  onSelectShape?: () => void;
  onRetry?: () => void;
}

function backendLabel(backend: "canvas2d" | "wasm"): string {
  return backend === "canvas2d" ? "Canvas 2D fallback" : "WASM renderer";
}

function frameMessage(status: RendererStatus): string | null {
  if (!("frameDisposition" in status)) return null;
  if (
    (status.state === "UNSUPPORTED_SHAPE" || status.state === "RECOVERING") &&
    status.frameDisposition === "previous"
  ) {
    return "The canvas is dimmed and shows the last successful frame, not the current document.";
  }
  if (status.frameDisposition === "unverified") {
    return "The dimmed canvas is not verified as current; previous or partial output may remain.";
  }
  if (status.frameDisposition === "none") {
    return "No current frame is available. Your document data remains in place.";
  }
  return null;
}

export function RendererStatusOverlay({
  status,
  shapeName,
  onSelectShape,
  onRetry,
}: RendererStatusOverlayProps): React.ReactElement | null {
  const titleId = useId();
  if (
    !status ||
    status.state === "READY" ||
    status.state === "RENDERING"
  ) return null;

  const isUnsupported = status.state === "UNSUPPORTED_SHAPE";
  const isRecovering = status.state === "RECOVERING";
  const diagnosticCode = isUnsupported
    ? status.code
    : status.state === "RENDER_FAILED"
      ? status.code
      : null;
  const title = isRecovering
    ? "Checking the updated canvas…"
    : isUnsupported
      ? "Some content can’t be displayed"
      : "The canvas could not be updated";
  const description = isUnsupported
    ? `This ${status.shapeType} shape is not supported by ${backendLabel(status.backend)}.`
    : isRecovering
      ? "The editor is trying the updated content. The dimmed canvas is not current until rendering succeeds."
      : `The ${backendLabel(status.backend)} could not complete this update. The document was not changed by the renderer.`;

  return (
    <div
      data-testid="renderer-compatibility-overlay"
      data-state={status.state}
      data-backend={status.backend}
      data-frame-disposition={status.frameDisposition}
      style={overlayStyle}
      onMouseDown={(event) => event.stopPropagation()}
      onMouseMove={(event) => event.stopPropagation()}
      onMouseUp={(event) => event.stopPropagation()}
      onDragOver={(event) => event.stopPropagation()}
      onDrop={(event) => event.stopPropagation()}
    >
      <section
        role={isRecovering ? "status" : "alert"}
        aria-live={isRecovering ? "polite" : "assertive"}
        aria-atomic="true"
        aria-labelledby={titleId}
        style={cardStyle}
      >
        <div style={{ color: theme.error, fontSize: 11, fontWeight: 700, letterSpacing: "0.06em", textTransform: "uppercase" }}>
          Renderer compatibility
        </div>
        <h2 id={titleId} style={{ margin: "7px 0", color: theme.text, fontSize: 16, lineHeight: 1.35 }}>
          {title}
        </h2>
        <p style={paragraphStyle}>{description}</p>
        {isUnsupported && (
          <p data-testid="renderer-document-safety" style={{ ...paragraphStyle, color: theme.text, fontWeight: 600 }}>
            Your document data has not been deleted or changed by this render failure.
          </p>
        )}
        {isUnsupported && (
          <div style={detailStyle}>
            <span>{shapeName ? `${shapeName} · ` : ""}{status.shapeType}</span>
            <code>{status.shapeId}</code>
            <span>{backendLabel(status.backend)}</span>
          </div>
        )}
        {frameMessage(status) && (
          <p data-testid="renderer-frame-warning" style={{ ...paragraphStyle, color: theme.text, fontWeight: 600 }}>
            {frameMessage(status)}
          </p>
        )}
        {diagnosticCode && (
          <div style={{ color: theme.textMuted, fontSize: 11, marginTop: 8 }}>
            Diagnostic: <code>{diagnosticCode}</code>
          </div>
        )}
        {(isUnsupported && onSelectShape) || (!isUnsupported && !isRecovering && onRetry) ? (
          <div style={{ display: "flex", gap: 8, marginTop: 14 }}>
            {isUnsupported && onSelectShape ? (
              <button type="button" onClick={onSelectShape} style={actionStyle}>
                Select in Layers
              </button>
            ) : !isRecovering && onRetry ? (
              <button type="button" onClick={onRetry} style={actionStyle}>
                Retry rendering
              </button>
            ) : null}
          </div>
        ) : null}
      </section>
    </div>
  );
}

const overlayStyle: React.CSSProperties = {
  position: "absolute",
  inset: 0,
  zIndex: 100,
  display: "flex",
  justifyContent: "center",
  alignItems: "flex-start",
  boxSizing: "border-box",
  padding: "56px 16px 16px",
  background: "rgba(23, 27, 36, 0.62)",
  pointerEvents: "auto",
};

const cardStyle: React.CSSProperties = {
  width: "min(100%, 520px)",
  boxSizing: "border-box",
  padding: "16px 18px",
  border: `1px solid ${theme.error}`,
  borderRadius: 9,
  background: theme.panel,
  boxShadow: "0 12px 36px rgba(0, 0, 0, 0.38)",
  color: theme.text,
  fontFamily: "Inter, system-ui, sans-serif",
};

const paragraphStyle: React.CSSProperties = {
  margin: "8px 0",
  color: theme.textSecondary,
  fontSize: 13,
  lineHeight: 1.5,
};

const detailStyle: React.CSSProperties = {
  display: "grid",
  gridTemplateColumns: "minmax(0, 1fr) auto",
  gap: "5px 10px",
  marginTop: 12,
  color: theme.textSecondary,
  fontSize: 12,
  overflowWrap: "anywhere",
};

const actionStyle: React.CSSProperties = {
  border: 0,
  borderRadius: 5,
  padding: "7px 10px",
  background: theme.error,
  color: theme.panel,
  fontSize: 12,
  fontWeight: 700,
  cursor: "pointer",
};
