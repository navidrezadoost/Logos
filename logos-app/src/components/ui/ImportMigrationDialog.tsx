/**
 * components/ui/ImportMigrationDialog.tsx
 *
 * Modal dialog for importing design files from Sketch and Adobe XD.
 *
 * Supported sources:
 *   - Sketch (.sketch archive)
 *   - Adobe XD (.xd archive)
 *
 * Usage:
 *   <ImportMigrationDialog open={open} onClose={() => setOpen(false)} />
 */

import React, { useCallback, useRef, useState } from "react";
import { useTokenStore } from "../../stores/tokenStore";

// ─── Types ────────────────────────────────────────────────────────────────────

type ImportSource = "sketch" | "xd";

interface ImportMigrationDialogProps {
  open: boolean;
  onClose: () => void;
}

// ─── State machine ────────────────────────────────────────────────────────────

type Phase =
  | { kind: "idle" }
  | { kind: "loading" }
  | {
      kind: "success";
      tokenCount: number;
      themeCount: number;
      warnings: string[];
      documentName: string;
      /** Total shape count from node tree (v2 exports only). */
      shapeCount?: number;
      /** Number of pages imported (v2 exports only). */
      pageCount?: number;
    }
  | { kind: "error"; message: string };

// ─── Component ────────────────────────────────────────────────────────────────

export function ImportMigrationDialog({ open, onClose }: ImportMigrationDialogProps) {
  const [source, setSource] = useState<ImportSource>("sketch");
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });

  const sketchFileInputRef = useRef<HTMLInputElement>(null);
  const xdFileInputRef = useRef<HTMLInputElement>(null);
  const loadImport = useTokenStore((s) => s.loadImport);

  const reset = useCallback(() => {
    setPhase({ kind: "idle" });
  }, []);

  const handleClose = useCallback(() => {
    reset();
    onClose();
  }, [reset, onClose]);

  const handleSketchFilePick = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    setPhase({ kind: "loading" });
    const { importSketchFile } = await import("../../migration/sketch/sketch-importer");
    const result = await importSketchFile(file);

    if (!result.ok) {
      setPhase({ kind: "error", message: result.error });
      return;
    }

    // Load tokens from shared styles and swatches
    loadImport(result.tokenConversion.sets, result.tokenConversion.themes);

    const tokenCount = result.tokenConversion.sets.reduce(
      (acc, s) => acc + s.tokens.length, 0
    );
    setPhase({
      kind: "success",
      tokenCount,
      themeCount: result.tokenConversion.themes.length,
      warnings: [
        ...result.tokenConversion.warnings,
        ...result.shapeConversion.warnings,
      ],
      documentName: result.documentName,
      shapeCount: result.shapeConversion.shapes.length,
      pageCount: result.shapeConversion.pageRoots.length,
    });

    if (sketchFileInputRef.current) sketchFileInputRef.current.value = "";
  }, [loadImport]);

  const handleXdFilePick = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    setPhase({ kind: "loading" });
    const { importXdFile } = await import("../../migration/xd/xd-importer");
    const result = await importXdFile(file);

    if (!result.ok) {
      setPhase({ kind: "error", message: result.errorMessage });
      return;
    }

    loadImport(result.tokenConversion.sets, result.tokenConversion.themes);

    const tokenCount = result.tokenConversion.sets.reduce(
      (acc, s) => acc + s.tokens.length, 0
    );
    setPhase({
      kind: "success",
      tokenCount,
      themeCount: result.tokenConversion.themes.length,
      warnings: [
        ...result.tokenConversion.warnings,
        ...result.shapeConversion.warnings,
      ],
      documentName: result.documentName,
      shapeCount: result.shapeConversion.shapes.length,
      pageCount: result.shapeConversion.artboardRoots.length,
    });

    if (xdFileInputRef.current) xdFileInputRef.current.value = "";
  }, [loadImport]);

  if (!open) return null;

  return (
    <div style={styles.overlay} role="dialog" aria-modal aria-label="Import design tokens">
      <div style={styles.modal}>
        {/* Header */}
        <div style={styles.header}>
          <span style={styles.logo}>Import design tokens</span>
          <button style={styles.closeBtn} onClick={handleClose} aria-label="Close">✕</button>
        </div>

        {/* Source tabs */}
        <div style={styles.tabs}>
          {SOURCES.map((s) => (
            <button
              key={s.id}
              style={{
                ...styles.tab,
                ...(source === s.id ? styles.tabActive : {}),
                ...(s.comingSoon ? styles.tabDisabled : {}),
              }}
              disabled={s.comingSoon}
              onClick={() => { setSource(s.id); setPhase({ kind: "idle" }); }}
              title={s.comingSoon ? "Coming soon" : undefined}
            >
              {s.label}
              {s.comingSoon && <span style={styles.badge}>soon</span>}
            </button>
          ))}
        </div>

        {/* Body */}
        <div style={styles.body}>
          {(source === "sketch") && (
            <SketchPanel
              phase={phase}
              fileInputRef={sketchFileInputRef}
              onFilePick={handleSketchFilePick}
              onRetry={reset}
            />
          )}
          {(source === "xd") && (
            <XdPanel
              phase={phase}
              fileInputRef={xdFileInputRef}
              onFilePick={handleXdFilePick}
              onRetry={reset}
            />
          )}
        </div>
      </div>
    </div>
  );
}

// ─── Sub-panels ───────────────────────────────────────────────────────────────

function SketchPanel({
  phase,
  fileInputRef,
  onFilePick,
  onRetry,
}: {
  phase: Phase;
  fileInputRef: React.RefObject<HTMLInputElement>;
  onFilePick: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onRetry: () => void;
}) {
  return (
    <div style={styles.panelContent}>
      <p style={styles.description}>
        Drop a <code>.sketch</code> file exported from Sketch.
        Logos imports shared styles, color variables, symbols, artboards, layers,
        text, and Smart Layout — fully offline, no API key needed.
      </p>
      <a
        href="https://www.sketch.com/docs/designing/"
        target="_blank"
        rel="noopener noreferrer"
        style={styles.link}
      >
        ↗ Export from Sketch: File → Save As…
      </a>

      {phase.kind === "idle" && (
        <label style={styles.dropZone}>
          <input
            ref={fileInputRef}
            type="file"
            accept=".sketch"
            style={{ display: "none" }}
            onChange={onFilePick}
          />
          <span style={styles.dropIcon}>⬆</span>
          <span>Select <code>.sketch</code> file</span>
          <span style={styles.dropSub}>or drag and drop here</span>
        </label>
      )}

      {phase.kind === "loading" && <Spinner />}
      {phase.kind === "success" && <SuccessPanel phase={phase} onImportMore={onRetry} />}
      {phase.kind === "error" && <ErrorPanel message={phase.message} onRetry={onRetry} />}
    </div>
  );
}

function XdPanel({
  phase,
  fileInputRef,
  onFilePick,
  onRetry,
}: {
  phase: Phase;
  fileInputRef: React.RefObject<HTMLInputElement>;
  onFilePick: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onRetry: () => void;
}) {
  return (
    <div style={styles.panelContent}>
      <p style={styles.description}>
        Drop an <code>.xd</code> file exported from Adobe XD.
        Logos imports color resources, character styles, artboards, groups,
        shapes, and text — fully offline, no API key needed.
      </p>
      <a
        href="https://helpx.adobe.com/xd/help/export-design-assets.html"
        target="_blank"
        rel="noopener noreferrer"
        style={styles.link}
      >
        ↗ Export from Adobe XD: File → Save As… (.xd)
      </a>

      {phase.kind === "idle" && (
        <label style={styles.dropZone}>
          <input
            ref={fileInputRef}
            type="file"
            accept=".xd"
            style={{ display: "none" }}
            onChange={onFilePick}
          />
          <span style={styles.dropIcon}>⬆</span>
          <span>Select <code>.xd</code> file</span>
          <span style={styles.dropSub}>or drag and drop here</span>
        </label>
      )}

      {phase.kind === "loading" && <Spinner />}
      {phase.kind === "success" && <SuccessPanel phase={phase} onImportMore={onRetry} />}
      {phase.kind === "error" && <ErrorPanel message={phase.message} onRetry={onRetry} />}
    </div>
  );
}

function SuccessPanel({
  phase,
  onImportMore,
}: {
  phase: Extract<Phase, { kind: "success" }>;
  onImportMore: () => void;
}) {
  return (
    <div style={styles.successPanel}>
      <span style={styles.successIcon}>✓</span>
      <p style={styles.successTitle}>Import complete</p>
      <p style={styles.successSub}>
        <strong>{phase.documentName}</strong>
        {" — "}
        {phase.tokenCount} token{phase.tokenCount !== 1 ? "s" : ""},&nbsp;
        {phase.themeCount} theme{phase.themeCount !== 1 ? "s" : ""}
        {phase.shapeCount !== undefined && (
          <>,&nbsp;{phase.shapeCount} shape{phase.shapeCount !== 1 ? "s" : ""}{" across "}{phase.pageCount} page{(phase.pageCount ?? 0) !== 1 ? "s" : ""}</>
        )}
      </p>
      {phase.warnings.length > 0 && (
        <details style={styles.warnings}>
          <summary style={{ cursor: "pointer", color: "#f5a623" }}>
            {phase.warnings.length} warning{phase.warnings.length !== 1 ? "s" : ""}
          </summary>
          <ul style={{ marginTop: 8, paddingLeft: 16 }}>
            {phase.warnings.map((w, i) => (
              <li key={i} style={{ marginBottom: 4 }}>{w}</li>
            ))}
          </ul>
        </details>
      )}
      <button style={styles.primaryBtn} onClick={onImportMore}>
        Import more
      </button>
    </div>
  );
}

function ErrorPanel({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div style={styles.errorPanel}>
      <p style={styles.errorText}>✕ {message}</p>
      <button style={styles.secondaryBtn} onClick={onRetry}>Try again</button>
    </div>
  );
}

function Spinner() {
  return (
    <div style={{ textAlign: "center", padding: 24, color: "#888" }}>
      <div style={styles.spinner} />
      <p style={{ marginTop: 12, fontSize: 12 }}>Importing…</p>
    </div>
  );
}

// ─── Source config ────────────────────────────────────────────────────────────

const SOURCES: { id: ImportSource; label: string; comingSoon?: boolean }[] = [
  { id: "sketch",       label: "Sketch" },
  { id: "xd",          label: "Adobe XD" },
];

// ─── Styles ───────────────────────────────────────────────────────────────────

const styles = {
  overlay: {
    position: "fixed" as const,
    inset: 0,
    background: "rgba(0,0,0,0.7)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    zIndex: 10000,
  },
  modal: {
    background: "#1e1e1e",
    border: "1px solid #333",
    borderRadius: 12,
    width: 480,
    maxWidth: "calc(100vw - 32px)",
    maxHeight: "calc(100vh - 64px)",
    display: "flex" as const,
    flexDirection: "column" as const,
    overflow: "hidden" as const,
    boxShadow: "0 24px 80px rgba(0,0,0,0.8)",
  },
  header: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "16px 20px",
    borderBottom: "1px solid #2a2a2a",
  },
  logo: { fontSize: 16, fontWeight: 600, color: "#fff" },
  closeBtn: {
    background: "none",
    border: "none",
    color: "#888",
    fontSize: 16,
    cursor: "pointer",
    padding: "2px 6px",
    borderRadius: 4,
  },
  tabs: {
    display: "flex",
    gap: 0,
    borderBottom: "1px solid #2a2a2a",
    padding: "0 12px",
  },
  tab: {
    background: "none",
    border: "none",
    borderBottom: "2px solid transparent",
    color: "#888",
    fontSize: 12,
    fontWeight: 500,
    padding: "10px 12px",
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    gap: 6,
    transition: "color .15s",
  },
  tabActive: {
    color: "#7efff5",
    borderBottomColor: "#7efff5",
  },
  tabDisabled: {
    opacity: 0.5,
    cursor: "not-allowed" as const,
  },
  badge: {
    fontSize: 9,
    background: "#333",
    color: "#888",
    borderRadius: 4,
    padding: "1px 5px",
  },
  body: {
    overflowY: "auto" as const,
    flex: 1,
  },
  panelContent: {
    display: "flex",
    flexDirection: "column" as const,
    gap: 16,
    padding: 20,
  },
  description: {
    fontSize: 12,
    color: "#aaa",
    lineHeight: 1.6,
  },
  link: {
    color: "#7efff5",
    fontSize: 12,
    textDecoration: "none",
  },
  dropZone: {
    display: "flex",
    flexDirection: "column" as const,
    alignItems: "center",
    gap: 8,
    border: "2px dashed #333",
    borderRadius: 8,
    padding: 32,
    cursor: "pointer",
    color: "#888",
    fontSize: 13,
    transition: "border-color .15s",
  },
  dropIcon: { fontSize: 24, color: "#555" },
  dropSub:  { fontSize: 11, color: "#555" },
  fieldLabel: {
    display: "flex",
    flexDirection: "column" as const,
    gap: 6,
    fontSize: 11,
    color: "#aaa",
    fontWeight: 500,
  },
  fieldHint: { fontWeight: 400, color: "#666" },
  input: {
    background: "#2a2a2a",
    border: "1px solid #3a3a3a",
    borderRadius: 6,
    color: "#fff",
    padding: "8px 10px",
    fontSize: 12,
    outline: "none",
    fontFamily: "monospace",
  },
  primaryBtn: {
    background: "#7efff5",
    color: "#0a2a28",
    border: "none",
    borderRadius: 8,
    padding: "10px 0",
    fontSize: 13,
    fontWeight: 600,
    cursor: "pointer",
    width: "100%",
    marginTop: 4,
  },
  primaryBtnDisabled: {
    opacity: 0.4,
    cursor: "not-allowed" as const,
  },
  secondaryBtn: {
    background: "#2a2a2a",
    color: "#ccc",
    border: "1px solid #3a3a3a",
    borderRadius: 8,
    padding: "8px 16px",
    fontSize: 12,
    cursor: "pointer",
  },
  successPanel: {
    display: "flex",
    flexDirection: "column" as const,
    alignItems: "center",
    gap: 10,
    padding: "32px 20px 20px",
    textAlign: "center" as const,
  },
  successIcon:  { fontSize: 32, color: "#00d1b8" },
  successTitle: { fontSize: 15, fontWeight: 600, color: "#fff" },
  successSub:   { fontSize: 12, color: "#aaa" },
  warnings: {
    fontSize: 11,
    color: "#aaa",
    background: "#2a2a2a",
    borderRadius: 6,
    padding: "8px 12px",
    width: "100%",
    textAlign: "left" as const,
  },
  errorPanel: {
    display: "flex",
    flexDirection: "column" as const,
    gap: 12,
    padding: 20,
  },
  errorText: { fontSize: 12, color: "#ff4d6d", lineHeight: 1.5 },
  spinner: {
    width: 28,
    height: 28,
    border: "3px solid #333",
    borderTopColor: "#7efff5",
    borderRadius: "50%",
    margin: "0 auto",
    animation: "logos-spin 0.8s linear infinite",
  },
} as const;

// Inject keyframe for spinner once
if (typeof document !== "undefined") {
  const id = "logos-import-spinner-style";
  if (!document.getElementById(id)) {
    const style = document.createElement("style");
    style.id = id;
    style.textContent = "@keyframes logos-spin { to { transform: rotate(360deg); } }";
    document.head.appendChild(style);
  }
}
