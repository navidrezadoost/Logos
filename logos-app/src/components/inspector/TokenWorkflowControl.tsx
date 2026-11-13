import { useEffect, useState } from "react";
import { useTokenWorkflowPreviewStore } from "../../stores/tokenWorkflowPreviewStore";
import type { ResolveRequest, TokenDiagnostic, ValidationResult } from "../../tokens/contract";
import { loadRustTokenEngine, type RustTokenEngine } from "../../tokens/rustTokenEngine";
import {
  createTokenValuePreview,
  createTokenWorkflowProposal,
  snapshotWithDefinition,
  type TokenWorkflowSession,
} from "../../tokens/tokenWorkflow";

declare const __LOGOS_TOKENS_WASM__: string | undefined;
const TOKEN_WASM_URL = typeof __LOGOS_TOKENS_WASM__ !== "undefined" ? __LOGOS_TOKENS_WASM__ : "/js/logos_tokens_wasm.wasm";
let enginePromise: Promise<RustTokenEngine> | undefined;

function loadEngine(): Promise<RustTokenEngine> {
  enginePromise ??= loadRustTokenEngine(TOKEN_WASM_URL);
  return enginePromise;
}

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

export function TokenWorkflowControl({ shapeId, fillHex, fillIndex }: { shapeId: string; fillHex: string; fillIndex: number }): React.ReactElement {
  const [name, setName] = useState("Primary Color");
  const [previewHex, setPreviewHex] = useState("#EF476F");
  const [status, setStatus] = useState("");
  const [diagnostics, setDiagnostics] = useState<TokenDiagnostic[]>([]);
  const [session, setSession] = useState<TokenWorkflowSession | null>(null);
  const [busy, setBusy] = useState(false);
  const setPreviewColor = useTokenWorkflowPreviewStore((state) => state.setColor);
  const clearPreviewColor = useTokenWorkflowPreviewStore((state) => state.clearColor);
  const displayedSource = status.match(/· (TOKEN|FALLBACK|DIRECT|LOCAL_OVERRIDE)$/)?.[1];

  useEffect(() => {
    setSession(null);
    setStatus("");
    setDiagnostics([]);
    clearPreviewColor(shapeId);
    return () => clearPreviewColor(shapeId);
  }, [shapeId, clearPreviewColor]);

  async function createAndBind(): Promise<void> {
    setBusy(true);
    setDiagnostics([]);
    setStatus("Loading Rust token engine…");
    try {
      const proposal = createTokenWorkflowProposal(shapeId, fillHex, name, fillIndex);
      const engine = await loadEngine();
      setStatus("Resolving selected fill in Rust…");
      const directRequest: ResolveRequest = {
        contractVersion: "0.1.0", snapshotRevision: proposal.snapshot.revision, propertyType: "color",
        property: proposal.directProperty,
        context: { explicit: {}, ancestorsNearestFirst: [], page: {}, document: {} },
      };
      const direct = engine.resolve(proposal.snapshot, directRequest);
      setStatus("Validating create-and-bind plan in Rust…");
      if (direct.status !== "RESOLVED" || direct.value?.value.hex !== fillHex.toUpperCase()) {
        throw new Error("Rust could not resolve the selected direct fill.");
      }
      const result = engine.validatePlan(proposal.snapshot, proposal.intent, proposal.context, proposal.plan);
      setDiagnostics(previewDiagnostics(result, proposal.propertyPath));
      const color = resolvedColor(result, proposal.propertyPath);
      setSession(proposal);
      setPreviewColor(shapeId, fillIndex, color);
      setStatus(`Bound in preview · ${result.impact.propertyPreviews[0].resolution.source ?? "UNKNOWN"}`);
    } catch (error) {
      setSession(null);
      setStatus(error instanceof Error ? error.message : "Token preview failed.");
    } finally {
      setBusy(false);
    }
  }

  async function previewTokenValueChange(): Promise<void> {
    if (!session) return;
    setBusy(true);
    setDiagnostics([]);
    setStatus("Loading Rust token engine…");
    try {
      const engine = await loadEngine();
      setStatus("Validating token value preview in Rust…");
      const currentExpression = session.definition.values[0].expression;
      const snapshot = snapshotWithDefinition(session, currentExpression);
      const plan = createTokenValuePreview(session, previewHex);
      const result = engine.validatePlan(snapshot, session.intent, session.context, plan);
      setDiagnostics(previewDiagnostics(result, session.propertyPath));
      const color = resolvedColor(result, session.propertyPath);
      setPreviewColor(shapeId, fillIndex, color);
      setStatus(`Token value preview · ${color}`);
    } catch (error) {
      setStatus(error instanceof Error ? error.message : "Token value preview failed.");
    } finally {
      setBusy(false);
    }
  }

  function revert(): void {
    clearPreviewColor(shapeId);
    setSession(null);
    setDiagnostics([]);
    setStatus("Preview reverted · document fill unchanged");
  }

  return (
    <div data-testid="token-workflow" data-status={session ? "bound" : "direct"} style={{ padding: "8px 12px", borderTop: "1px solid #313244" }}>
      <div style={{ color: "#a6adc8", fontSize: 10, marginBottom: 5 }}>Token preview</div>
      {!session && <div style={{ display: "flex", gap: 6 }}>
        <input aria-label="Token name" data-testid="token-name" value={name} onChange={(event) => setName(event.target.value)} style={{ minWidth: 0, flex: 1, background: "#181825", color: "#cdd6f4", border: "1px solid #45475a", borderRadius: 4, padding: "4px 6px", fontSize: 11 }} />
        <button data-testid="create-bind-token" onClick={() => void createAndBind()} disabled={busy} style={buttonStyle}>{busy ? "Resolving…" : "Save as token"}</button>
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
