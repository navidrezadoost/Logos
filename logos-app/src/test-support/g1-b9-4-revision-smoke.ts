import { create } from "zustand";
import initRust, { evaluate_document_operation_json, validate_document_json } from "../document-core/rust-wasm/logos_types";
import fixture from "../document-core/fixtures/react-supported-v1.json";
import { fromRustDocument, toRustCandidate, type ReactDocumentSnapshot } from "../document-core/react-adapter";
import { RevisionCommitCoordinator, type RevisionCoordinatorState } from "../document-core/revision-commit-coordinator";

type Payload = Record<string, unknown>;
type SmokeState = RevisionCoordinatorState<ReactDocumentSnapshot, string[]>;
const output = document.querySelector<HTMLPreElement>("#result")!;
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const canonical = (value: unknown): unknown => Array.isArray(value) ? value.map(canonical) :
  value && typeof value === "object" ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical((value as Record<string, unknown>)[key])])) : value;
const equal = (a: unknown, b: unknown) => JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));

async function run(): Promise<void> {
  await initRust();
  const snapshot = clone(fixture) as unknown as ReactDocumentSnapshot;
  const shapeId = "66666666-6666-4666-8666-666666666666";
  const source = snapshot.shapes[shapeId];
  if (!source) throw new Error("Rust fixture target missing");
  const initial: SmokeState = {
    canonicalDocumentId: snapshot.documentId, pageId: snapshot.pageOrder[0], workspaceGeneration: 1,
    localSnapshotGeneration: 7, canonicalRevision: 12, canonicalRevisionAuthority: "server-response",
    canonicalVersion: 3, canonicalVersionAuthority: "server-response", document: snapshot,
    selection: [shapeId], consumedOperationIds: new Set(), canonicalStatus: "unchanged", rendererStatus: "not-published",
  };
  const store = create<SmokeState>(() => initial);
  let evalCount = 0;
  let renderCount = 0;
  const coordinator = new RevisionCommitCoordinator<ReactDocumentSnapshot, string[], Payload>({
    getState: store.getState,
    evaluate: async (before, payload) => {
      evalCount += 1;
      const converted = toRustCandidate(before);
      if (!converted.ok) return { ok: false, diagnostic: `REACT_ADAPTER:${converted.issues[0]?.code}` };
      const result = JSON.parse(evaluate_document_operation_json(JSON.stringify(converted.candidate), JSON.stringify(payload))) as {
        ok: boolean; document?: Record<string, unknown>; diagnostics?: Array<{ code: string }>;
      };
      if (!result.ok || !result.document) return { ok: false, diagnostic: `RUST_REJECTED:${result.diagnostics?.[0]?.code ?? "unknown"}` };
      const projected = fromRustDocument(result.document);
      const validation = JSON.parse(validate_document_json(JSON.stringify(result.document))) as { ok: boolean };
      if (!validation.ok || !equal(projected.documentId, before.documentId) || projected.pageOrder[0] !== before.pageOrder[0]) {
        return { ok: false, diagnostic: "RUST_REACT_PROJECTION_MISMATCH" };
      }
      return { ok: true, document: projected };
    },
    validate: (document) => {
      const candidate = toRustCandidate(document);
      return candidate.ok && JSON.parse(validate_document_json(JSON.stringify(candidate.candidate))).ok === true;
    },
    reconcileSelection: (document, selected) => selected.filter((id) => Boolean(document.shapes[id])),
    publish: (document, selection, localSnapshotGeneration) => store.setState({ ...store.getState(), document, selection, localSnapshotGeneration }),
    renderCommittedState: () => {
      if (store.getState().document.shapes[shapeId].name !== "G1-B9.4 WASM accepted") throw new Error("renderer was not called from committed state");
      renderCount += 1;
    },
    fingerprint: (payload) => JSON.stringify(payload),
    fingerprintDocument: (document) => JSON.stringify(canonical(document)),
  });
  const validPayload = { kind: "updateShape", shapeId, expectedParentId: source.parentId,
    name: "G1-B9.4 WASM accepted", selrect: null, transform: null, opacity: null };
  const env = (operationId: string, payload: Payload, overrides: Partial<Parameters<typeof coordinator.execute>[0]> = {}) => ({
    contractVersion: "logos-operation-precondition/v1" as const, canonicalDocumentId: snapshot.documentId,
    pageId: snapshot.pageOrder[0], workspaceGeneration: 1, expectedCanonicalRevision: 12,
    canonicalRevisionAuthority: "server-response" as const, expectedCanonicalVersion: 3,
    canonicalVersionAuthority: "server-response" as const, localSnapshotGeneration: 7,
    operationId, operationKind: "updateShape", payload, candidateSchemaVersion: 1, ...overrides,
  });
  const accepted = await coordinator.execute(env("wasm-op-1", validPayload));
  if (accepted.status !== "accepted" || store.getState().document.shapes[shapeId].name !== "G1-B9.4 WASM accepted") throw new Error("valid Rust/WASM candidate was not published");
  const duplicate = await coordinator.execute(env("wasm-op-1", validPayload));
  if (duplicate.status !== "duplicate" || evalCount !== 1 || renderCount !== 1) throw new Error("duplicate operation was evaluated or rendered twice");
  const rejectedPayload = { ...validPayload, expectedParentId: "00000000-0000-4000-8000-000000000000", name: "must reject" };
  const rejected = await coordinator.execute(env("wasm-op-2", rejectedPayload, { localSnapshotGeneration: 8 }));
  if (rejected.status !== "rejected" || store.getState().document.shapes[shapeId].name !== "G1-B9.4 WASM accepted") throw new Error("Rust rejection mutated Zustand state");
  const stale = await coordinator.execute(env("wasm-op-3", validPayload, { localSnapshotGeneration: 7, workspaceGeneration: 0 }));
  if (stale.status !== "stale" || renderCount !== 1) throw new Error("stale workspace candidate was published");
  output.dataset.status = "passed";
  output.textContent = JSON.stringify({ status: "PASS", wasm: "logos-types actual WASM ABI", projection: "existing React adapter/fromRustDocument",
    store: "disposable Zustand store", accepted, duplicate, rejected, stale, evaluations: evalCount, rendererPublications: renderCount,
    localGeneration: store.getState().localSnapshotGeneration, canonicalRevision: store.getState().canonicalRevision,
    browser: { userAgent: navigator.userAgent, platform: navigator.platform, hardwareConcurrency: navigator.hardwareConcurrency,
      viewport: { width: innerWidth, height: innerHeight }, devicePixelRatio }, productionStoreTouched: false, persistenceOpened: false }, null, 2);
}

run().catch((error: unknown) => {
  output.dataset.status = "failed";
  output.textContent = JSON.stringify({ status: "FAIL", message: String(error), stack: error instanceof Error ? error.stack : null }, null, 2);
});
