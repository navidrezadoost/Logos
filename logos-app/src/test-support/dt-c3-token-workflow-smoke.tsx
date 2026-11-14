import React from "react";
import { createRoot } from "react-dom/client";
import { Canvas } from "../components/canvas/Canvas";
import { Inspector } from "../components/inspector/Inspector";
import { useDocumentStore } from "../stores/documentStore";
import { useSelectionStore } from "../stores/selectionStore";
import { useUiStore } from "../stores/uiStore";
import { useTokenStore } from "../stores/tokenStore";
import { loadRustTokenEngine } from "../tokens/rustTokenEngine";
import { createTokenWorkflowProposal } from "../tokens/tokenWorkflow";
import type { ResolveRequest } from "../tokens/contract";
import { useTokenWorkflowPreviewStore } from "../stores/tokenWorkflowPreviewStore";

const report = document.getElementById("dt-c3-status") as HTMLPreElement;
let stage = "setup";
report.dataset.status = "running";
report.dataset.stage = stage;

function setStage(next: string): void {
  stage = next;
  report.dataset.stage = next;
  report.textContent = `running: ${next}`;
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function waitFor<T>(read: () => T | null | undefined, label: string, timeoutMs = 15000): Promise<T> {
  return new Promise((resolve, reject) => {
    const start = performance.now();
    const poll = () => {
      const value = read();
      if (value !== null && value !== undefined) return resolve(value);
      if (performance.now() - start > timeoutMs) return reject(new Error(`timeout waiting for ${label}`));
      window.setTimeout(poll, 20);
    };
    poll();
  });
}

function paint(): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, 50));
}

function documentSnapshot(): string {
  const { pages, pageOrder, currentPageId, shapes } = useDocumentStore.getState();
  return JSON.stringify({ pages, pageOrder, currentPageId, shapes });
}

function pixel(canvas: HTMLCanvasElement, x: number, y: number): number[] {
  const context = canvas.getContext("2d");
  assert(context, "Canvas 2D context missing");
  return Array.from(context.getImageData(x, y, 1, 1).data);
}

function assertPixel(canvas: HTMLCanvasElement, expected: number[], label: string): void {
  const actual = pixel(canvas, 30, 30);
  assert(actual.every((channel, index) => channel === expected[index]), `${label}: expected ${expected}, got ${actual}`);
}

function Workspace(): React.ReactElement {
  return <div style={{ display: "flex", width: 800, height: 500 }}>
    <main style={{ flex: 1, minWidth: 0 }}><Canvas /></main>
    <aside style={{ width: 310, height: 500 }}><Inspector /></aside>
  </div>;
}

async function run(): Promise<void> {
  useUiStore.setState({ zoom: 1, panX: 0, panY: 0, layersPanelOpen: false, activeTool: "select" });
  const shapeId = useDocumentStore.getState().addRect({ x: 20, y: 20, w: 52, h: 42 }, "#11aaee");
  useSelectionStore.getState().select(shapeId);
  const root = createRoot(document.getElementById("app")!);
  root.render(<Workspace />);

  const canvas = await waitFor(() => {
    const element = document.querySelector("canvas");
    const state = document.querySelector<HTMLElement>("[data-renderer-status]")?.dataset.rendererStatus;
    return element && state === "READY" ? element : null;
  }, "ready canvas");
  const pixelEvidence: Record<string, number[]> = {};
  assert(document.querySelector("[data-renderer-backend='canvas2d']"), "smoke must exercise the Canvas 2D pixel output");
  assertPixel(canvas, [17, 170, 238, 255], "initial direct fill");
  pixelEvidence.initialDirect = pixel(canvas, 30, 30);

  setStage("ordinary-direct-fill-edit");
  assert(document.querySelector("input[type=color]"), "ordinary fill color control missing");
  const shape = useDocumentStore.getState().shapes[shapeId];
  useDocumentStore.getState().updateShape(shapeId, {
    fills: shape.fills.map((fill) => fill.type === "solid" ? { ...fill, color: "#226644" } : fill),
  });
  await waitFor(() => {
    const fill = useDocumentStore.getState().shapes[shapeId].fills[0];
    return fill?.type === "solid" && fill.color === "#226644" ? true : null;
  }, "direct fill edit");
  await paint();
  assertPixel(canvas, [34, 102, 68, 255], "ordinary direct fill edit");
  pixelEvidence.directEdit = pixel(canvas, 30, 30);
  assert(!("tokenBinding" in useDocumentStore.getState().shapes[shapeId]), "direct fill edit unexpectedly added token metadata");

  const beforeWorkflow = documentSnapshot();
  const tokenRegistryBefore = localStorage.getItem("logos-token-store");
  const importedTokenStateBefore = JSON.stringify({ sets: useTokenStore.getState().sets, themes: useTokenStore.getState().themes });

  setStage("rust-adapter-invalid-reference-and-stale-prestate");
  const proposal = createTokenWorkflowProposal(shapeId, "#226644", "Primary Color");
  assert(proposal.directProperty.kind === "literal", "proposal did not retain its direct color");
  const directColor = proposal.directProperty.value;
  const engine = await loadRustTokenEngine("/js/logos_tokens_wasm.wasm");
  const missingTarget = { namespace: "document", tokenId: crypto.randomUUID() };
  const missingReferenceRequest: ResolveRequest = {
    contractVersion: "0.1.0", snapshotRevision: proposal.snapshot.revision, propertyType: "color",
    property: { kind: "binding", target: missingTarget, modeStrategy: { kind: "inherit" } },
    context: { explicit: {}, ancestorsNearestFirst: [], page: {}, document: {} },
  };
  const missingReference = engine.resolve(proposal.snapshot, missingReferenceRequest);
  assert(missingReference.status === "ERROR" && missingReference.diagnostics.some((item) => item.code === "MISSING_TOKEN"), "invalid reference did not return a controlled Rust diagnostic");
  const fallback = engine.resolve(proposal.snapshot, {
    ...missingReferenceRequest,
    property: { kind: "binding", target: missingTarget, modeStrategy: { kind: "inherit" }, fallback: directColor },
  });
  assert(fallback.status === "RESOLVED" && fallback.source === "FALLBACK" && fallback.diagnostics.some((item) => item.code === "MISSING_TOKEN" && item.severity === "warning"), "fallback resolution did not preserve its warning and distinct source");
  const stale = engine.validatePlan({ ...proposal.snapshot, revision: `${proposal.snapshot.revision}-stale` }, proposal.intent, proposal.context, proposal.plan);
  assert(stale.status === "REJECTED" && stale.diagnostics.some((item) => item.code === "STALE_REVISION"), "stale revision was not rejected by Rust");
  assert(documentSnapshot() === beforeWorkflow, "invalid Rust proposals mutated the document");

  setStage("create-token-bind-with-rust");
  document.querySelector<HTMLButtonElement>("[data-testid=create-bind-token]")!.click();
  const boundStatus = await waitFor(() => document.querySelector<HTMLElement>("[data-testid=token-workflow-status]")?.textContent?.startsWith("Bound in preview") ? document.querySelector<HTMLElement>("[data-testid=token-workflow-status]") : null, "Rust-created token binding preview");
  assert(boundStatus.textContent?.includes("TOKEN"), "preview did not report token resolution source");
  await paint();
  assertPixel(canvas, [34, 102, 68, 255], "token binding initial resolution");
  pixelEvidence.tokenBound = pixel(canvas, 30, 30);
  assert(documentSnapshot() === beforeWorkflow, "create and bind preview mutated the document store");
  assert(localStorage.getItem("logos-token-store") === tokenRegistryBefore, "preview wrote to the persisted local-storage token registry");
  assert(JSON.stringify({ sets: useTokenStore.getState().sets, themes: useTokenStore.getState().themes }) === importedTokenStateBefore, "preview changed the imported token store");

  const wireResult = engine.validatePlan(proposal.snapshot, proposal.intent, proposal.context, proposal.plan);
  const wireJson = JSON.stringify(wireResult);
  assert(wireResult.status === "READY" && wireResult.impact.propertyPreviews[0]?.resolution.source === "TOKEN", "Rust validation did not return a token-resolved property preview");
  assert(wireJson.includes("propertyPreviews") && wireJson.includes("planDigest") && !wireJson.includes("property_previews"), "Rust/TypeScript serialized contract fields do not match camelCase DTOs");
  assert(wireResult.impact.createdTokens === 1 && wireResult.impact.boundProperties === 1, "Rust impact summary omitted create/bind operations");

  setStage("preview-token-value-change");
  document.querySelector<HTMLButtonElement>("[data-testid=preview-token-value]")!.click();
  await waitFor(() => document.querySelector<HTMLElement>("[data-testid=token-workflow-status]")?.textContent?.includes("Token value preview · #EF476F") ? true : null, "updated Rust token preview");
  await paint();
  assertPixel(canvas, [239, 71, 111, 255], "updated token value on canvas");
  pixelEvidence.tokenValuePreview = pixel(canvas, 30, 30);
  assert(documentSnapshot() === beforeWorkflow, "token value preview mutated the document store");

  setStage("revert-preview");
  document.querySelector<HTMLButtonElement>("[data-testid=revert-token-preview]")!.click();
  await waitFor(() => document.querySelector<HTMLElement>("[data-testid=token-workflow-status]")?.textContent?.includes("Preview reverted") ? true : null, "preview revert");
  await paint();
  assertPixel(canvas, [34, 102, 68, 255], "reverted original fill");
  pixelEvidence.reverted = pixel(canvas, 30, 30);
  assert(documentSnapshot() === beforeWorkflow, "revert did not leave the document exactly unchanged");
  assert(localStorage.getItem("logos-token-store") === tokenRegistryBefore, "revert changed the persistent token registry");
  assert(Object.keys(useTokenWorkflowPreviewStore.getState().colors).length === 0, "revert left a transient canvas override behind");

  setStage("complete");
  report.dataset.status = "passed";
  report.textContent = JSON.stringify({ status: "passed", shapeId, pixelEvidence, checks: [
    "ordinary direct fill editing without token metadata",
    "typed ResolveRequest invokes the Rust WASM resolver",
    "agent-style create-token-and-bind plan invokes Rust validation",
    "Rust returns typed camelCase impact and effective fill preview",
    "invalid token references return controlled diagnostics",
    "fallback resolution keeps its warning and distinct source",
    "stale revisions reject and leave the document unchanged",
    "token value change visibly updates the canvas pixel through Rust resolution",
    "revert restores original output without document or local-storage mutation",
  ] }, null, 2);
  root.unmount();
}

void run().catch((error: unknown) => {
  report.dataset.status = "failed";
  report.dataset.stage = stage;
  report.textContent = JSON.stringify({ status: "failed", stage, error: String(error) }, null, 2);
});
