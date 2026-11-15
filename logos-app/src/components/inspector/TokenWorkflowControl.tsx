import { useLayoutEffect, useRef, useState } from "react";
import { useDocumentStore } from "../../stores/documentStore";
import { useSelectionStore } from "../../stores/selectionStore";
import { useTokenWorkflowPreviewStore } from "../../stores/tokenWorkflowPreviewStore";
import type { ResolveRequest, TokenDiagnostic, ValidationResult } from "../../tokens/contract";
import { loadRustTokenEngine } from "../../tokens/rustTokenEngine";
import type { RustTokenEngineLoader } from "../../tokens/rustTokenEngine";
import {
  createTokenValuePreview,
  createTokenWorkflowProposal,
  snapshotWithDefinition,
  type TokenWorkflowSession,
} from "../../tokens/tokenWorkflow";

declare const __LOGOS_TOKENS_WASM__: string | undefined;
const TOKEN_WASM_URL = typeof __LOGOS_TOKENS_WASM__ !== "undefined" ? __LOGOS_TOKENS_WASM__ : "/js/logos_tokens_wasm.wasm";

function statusDiagnostics(result: ValidationResult): TokenDiagnostic[] {
  return result.diagnostics.length ? result.diagnostics : result.impact.diagnostics;
}

function previewDiagnostics(result: ValidationResult, propertyPath: string): TokenDiagnostic[] {
  const resolution = result.impact.propertyPreviews.find((item) => item.propertyPath === propertyPath)?.resolution;
  return [...statusDiagnostics(result), ...(resolution?.diagnostics ?? [])];
}

function resolvedColor(result: ValidationResult, propertyPath: string): string {
  if (result.status !== "READY") throw new Error(statusDiagnostics(result).map((item) => `${item.code}: ${item.message}`).join("; ") || "Rust rejected the proposal.");
  const preview = result.impact.propertyPreviews.find((item) => item.propertyPath === propertyPath);
  if (!preview || preview.resolution.status !== "RESOLVED" || preview.resolution.value?.type !== "color") {
    throw new Error(preview?.resolution.diagnostics.map((item) => `${item.code}: ${item.message}`).join("; ") || "Rust returned no resolved fill preview.");
  }
  return preview.resolution.value.value.hex;
}

interface Props {
  shapeId: string;
  fillHex: string;
  fillIndex: number;
  pageId: string;
  documentId: string | null;
  engineLoader?: RustTokenEngineLoader;
}

export function TokenWorkflowControl({ shapeId, fillHex, fillIndex, pageId, documentId, engineLoader = loadRustTokenEngine }: Props): React.ReactElement {
  const [ownerId] = useState(() => crypto.randomUUID());
  const generation = useRef(0);
  const [name, setName] = useState("Primary Color");
  const [previewHex, setPreviewHex] = useState("#EF476F");
  const [status, setStatus] = useState("");
  const [diagnostics, setDiagnostics] = useState<TokenDiagnostic[]>([]);
  const [session, setSession] = useState<TokenWorkflowSession | null>(null);
  const [busy, setBusy] = useState(false);
  const setPreview = useTokenWorkflowPreviewStore((state) => state.setPreview);
  const clearPreview = useTokenWorkflowPreviewStore((state) => state.clearPreview);
  const displayedSource = status.match(/· (TOKEN|FALLBACK|DIRECT|LOCAL_OVERRIDE)$/)?.[1];

  useLayoutEffect(() => {
    generation.current += 1;
    setSession(null);
    setStatus("");
    setDiagnostics([]);
    setBusy(false);
    clearPreview(ownerId);
    return () => {
      generation.current += 1;
      clearPreview(ownerId);
    };
  }, [ownerId, shapeId, pageId, documentId, fillIndex, fillHex, clearPreview]);

  function isCurrent(capturedGeneration: number): boolean {
    if (generation.current !== capturedGeneration) return false;
    const state = useDocumentStore.getState();
    const selection = useSelectionStore.getState().selectedIds;
    const shape = state.shapes[shapeId];
    const fill = shape?.fills[fillIndex];
    return state.currentPageId === pageId && state.documentId === documentId &&
      selection.length === 1 && selection[0] === shapeId &&
      fill?.type === "solid" && fill.color.toUpperCase() === fillHex.toUpperCase();
  }

  function applyPreview(capturedGeneration: number, color: string): boolean {
    if (!isCurrent(capturedGeneration)) return false;
    setPreview({ ownerId, documentId, pageId, shapeId, fillIndex, color });
    return true;
  }

  async function createAndBind(): Promise<void> {
    const capturedGeneration = generation.current;
    setBusy(true);
    setDiagnostics([]);
    setStatus("Loading Rust token engine…");
    try {
      const proposal = createTokenWorkflowProposal(shapeId, fillHex, name, { documentId, pageId }, fillIndex);
      const engine = await engineLoader(TOKEN_WASM_URL);
      if (!isCurrent(capturedGeneration)) return;
      setStatus("Resolving selected fill in Rust…");
      const directRequest: ResolveRequest = {
        contractVersion: "0.1.0", snapshotRevision: proposal.snapshot.revision, propertyType: "color",
        property: proposal.directProperty,
        context: { explicit: {}, ancestorsNearestFirst: [], page: {}, document: {} },
      };
      const direct = engine.resolve(proposal.snapshot, directRequest);
      if (direct.status !== "RESOLVED" || direct.value?.value.hex !== fillHex.toUpperCase()) {
        throw new Error("Rust could not resolve the selected direct fill.");
      }
      setStatus("Validating create-and-bind plan in Rust…");
      const result = engine.validatePlan(proposal.snapshot, proposal.intent, proposal.context, proposal.plan);
      setDiagnostics(previewDiagnostics(result, proposal.propertyPath));
      const color = resolvedColor(result, proposal.propertyPath);
      if (!applyPreview(capturedGeneration, color)) return;
      setSession(proposal);
      setStatus(`Bound in preview · ${result.impact.propertyPreviews[0].resolution.source ?? "UNKNOWN"}`);
    } catch (error) {
      if (!isCurrent(capturedGeneration)) return;
      clearPreview(ownerId);
      setSession(null);
      setStatus(error instanceof Error ? error.message : "Token preview failed.");
    } finally {
      if (isCurrent(capturedGeneration)) setBusy(false);
    }
  }

  async function previewTokenValueChange(): Promise<void> {
    if (!session) return;
    const capturedGeneration = generation.current;
    setBusy(true);
    setDiagnostics([]);
    setStatus("Loading Rust token engine…");
    try {
      const engine = await engineLoader(TOKEN_WASM_URL);
      if (!isCurrent(capturedGeneration)) return;
      setStatus("Validating token value preview in Rust…");
      const currentExpression = session.definition.values[0].expression;
      const snapshot = snapshotWithDefinition(session, currentExpression);
      const plan = createTokenValuePreview(session, previewHex);
      const result = engine.validatePlan(snapshot, session.intent, session.context, plan);
      setDiagnostics(previewDiagnostics(result, session.propertyPath));
      const color = resolvedColor(result, session.propertyPath);
      if (!applyPreview(capturedGeneration, color)) return;
      setStatus(`Token value preview · ${color}`);
    } catch (error) {
      if (!isCurrent(capturedGeneration)) return;
      clearPreview(ownerId);
      setStatus(error instanceof Error ? error.message : "Token value preview failed.");
    } finally {
      if (isCurrent(capturedGeneration)) setBusy(false);
    }
  }

  function revert(): void {
    clearPreview(ownerId);
    setSession(null);
    setDiagnostics([]);
    setStatus("Preview reverted · document fill unchanged");
  }

  return (
    <div data-testid="token-workflow" data-status={session ? "bound" : "direct"} data-owner={ownerId} style={{ padding: "8px 12px", borderTop: "1px solid #313244" }}>
      <div style={{ color: "#a6adc8", fontSize: 10, marginBottom: 5 }}>Token preview</div>
      {!session && <div style={{ display: "flex", gap: 6 }}>
        <input aria-label="Token name" data-testid="token-name" value={name} onChange={(event) => setName(event.target.value)} style={{ minWidth: 0, flex: 1, background: "#181825", color: "#cdd6f4", border: "1px solid #45475a", borderRadius: 4, padding: "4px 6px", fontSize: 11 }} />
        <button data-testid="create-bind-token" onClick={() => void createAndBind()} disabled={busy} style={buttonStyle}>{busy ? "Resolving…" : "Preview as token"}</button>
      </div>}
      {session && <div style={{ display: "flex", gap: 6 }}>
        <input aria-label="Preview token color" data-testid="preview-token-color" type="color" value={previewHex} onChange={(event) => setPreviewHex(event.target.value)} style={{ width: 32, height: 28, padding: 1, border: "1px solid #45475a", borderRadius: 4, background: "#181825" }} />
        <button data-testid="preview-token-value" onClick={() => void previewTokenValueChange()} disabled={busy} style={buttonStyle}>{busy ? "Resolving…" : "Preview value change"}</button>
        <button data-testid="revert-token-preview" onClick={revert} disabled={busy} style={buttonStyle}>Revert preview</button>
      </div>}
      {status && <div data-testid="token-workflow-status" data-source={displayedSource} style={{ color: status.includes("failed") || status.includes("rejected") ? "#f38ba8" : "#a6e3a1", fontSize: 10, marginTop: 6, overflowWrap: "anywhere" }}>{status}</div>}
      {diagnostics.map((diagnostic, index) => <div key={`${diagnostic.code}-${index}`} data-diagnostic={diagnostic.code} style={{ color: diagnostic.severity === "warning" ? "#f9e2af" : "#f38ba8", fontSize: 10 }}>{diagnostic.code}: {diagnostic.message}</div>)}
      {session && <div style={{ color: "#585b70", fontSize: 9, marginTop: 3 }}>Temporary in-memory proposal; not saved to the document or token registry.</div>}
    </div>
  );
}

const buttonStyle: React.CSSProperties = {
  background: "#313244", color: "#cdd6f4", border: "1px solid #45475a", borderRadius: 4,
  padding: "5px 7px", fontSize: 10, cursor: "pointer", whiteSpace: "nowrap",
};
