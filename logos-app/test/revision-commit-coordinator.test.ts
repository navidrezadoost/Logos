import assert from "node:assert/strict";
import test from "node:test";
import { RevisionCommitCoordinator, type CandidateResult, type RevisionCoordinatorState, type RevisionOperationEnvelope } from "../src/document-core/revision-commit-coordinator";

type Doc = { id: string; page: string; value: number };
type Payload = { add: number; reject?: boolean; mismatch?: boolean };
type Selection = string[];
const initial = (): RevisionCoordinatorState<Doc, Selection> => ({
  canonicalDocumentId: "doc-A", pageId: "page-A", workspaceGeneration: 1,
  localSnapshotGeneration: 7, canonicalRevision: 4, canonicalRevisionAuthority: "server-response",
  canonicalVersion: 2, canonicalVersionAuthority: "server-response",
  document: { id: "doc-A", page: "page-A", value: 0 }, selection: ["shape-A"],
  consumedOperationIds: new Set(), canonicalStatus: "unchanged", rendererStatus: "not-published",
});
const envelope = (payload: Payload, overrides: Partial<RevisionOperationEnvelope<Payload>> = {}): RevisionOperationEnvelope<Payload> => ({
  contractVersion: "logos-operation-precondition/v1", canonicalDocumentId: "doc-A", pageId: "page-A",
  workspaceGeneration: 1, expectedCanonicalRevision: 4, canonicalRevisionAuthority: "server-response",
  expectedCanonicalVersion: 2, canonicalVersionAuthority: "server-response",
  localSnapshotGeneration: 7, operationId: "op-1", operationKind: "test-add", payload, candidateSchemaVersion: 1, ...overrides,
});
function rig(options: { gate?: Promise<void>; renderThrows?: boolean; evaluatorThrows?: boolean; server?: "conflict" | "ambiguous" } = {}) {
  let state = initial();
  let evaluations = 0;
  let publications = 0;
  let renders = 0;
  let selectionReconciliations = 0;
  let resolveGate: (() => void) | undefined;
  const gate = options.gate ?? new Promise<void>((resolve) => { resolveGate = resolve; });
  const deps = {
    getState: () => state,
    evaluate: async (doc: Readonly<Doc>, payload: Payload): Promise<CandidateResult<Doc>> => {
      evaluations += 1;
      if (options.evaluatorThrows) throw new Error("synthetic evaluator failure");
      await gate;
      if (payload.reject) return { ok: false, diagnostic: "RUST_OPERATION_REJECTED" };
      return { ok: true, document: { ...doc, value: doc.value + payload.add, ...(payload.mismatch ? { page: "other-page" } : {}) } };
    },
    validate: (doc: Readonly<Doc>) => doc.page === "page-A" && doc.id === "doc-A",
    reconcileSelection: (_doc: Readonly<Doc>, selection: Readonly<Selection>) => { selectionReconciliations += 1; return [...selection]; },
    publish: (doc: Doc, selection: Selection, localSnapshotGeneration: number) => {
      publications += 1; state = { ...state, document: doc, selection, localSnapshotGeneration };
    },
    renderCommittedState: () => { renders += 1; if (options.renderThrows) throw new Error("renderer lost"); },
    fingerprint: (payload: Payload) => JSON.stringify(payload),
    fingerprintDocument: (document: Readonly<Doc>) => JSON.stringify(document),
  };
  const coordinator = new RevisionCommitCoordinator(deps);
  return { state: () => state, set: (next: Partial<typeof state>) => { state = { ...state, ...next }; }, coordinator,
    counts: () => ({ evaluations, publications, renders, selectionReconciliations }), resolve: () => resolveGate?.() };
}
const immediate = () => Promise.resolve();

test("1 two operations from same revision are fenced by local generation after first publishes", async () => {
  const r = rig({ gate: immediate() });
  const first = await r.coordinator.execute(envelope({ add: 1 }, { operationId: "first" }));
  const second = await r.coordinator.execute(envelope({ add: 2 }, { operationId: "second" }));
  assert.equal(first.status, "accepted"); assert.equal(second.status, "stale");
  assert.equal(r.state().document.value, 1); assert.equal(r.counts().publications, 1);
});
test("2 stale operation after later local commit does not publish", async () => {
  const r = rig({ gate: immediate() }); await r.coordinator.execute(envelope({ add: 1 }));
  const out = await r.coordinator.execute(envelope({ add: 9 }, { operationId: "stale", localSnapshotGeneration: 7 }));
  assert.equal(out.status, "stale"); assert.equal(r.state().document.value, 1);
});
test("3 same operation ID delivered twice executes and publishes once", async () => {
  const r = rig({ gate: immediate() }); const a = envelope({ add: 1 });
  const [one, two] = await Promise.all([r.coordinator.execute(a), r.coordinator.execute(a)]);
  assert.deepEqual(one, two); assert.equal(r.counts().evaluations, 1); assert.equal(r.counts().publications, 1);
  assert.equal((await r.coordinator.execute(a)).status, "duplicate");
});
test("4 same operation ID with a different payload is rejected", async () => {
  const r = rig({ gate: immediate() }); await r.coordinator.execute(envelope({ add: 1 }));
  assert.equal((await r.coordinator.execute(envelope({ add: 2 }))).status, "id-conflict"); assert.equal(r.state().document.value, 1);
});
test("5 workspace A to B to A is rejected by generation even when document ID returns", async () => {
  const r = rig(); const pending = r.coordinator.execute(envelope({ add: 1 }));
  r.set({ canonicalDocumentId: "doc-B", workspaceGeneration: 2 }); r.set({ canonicalDocumentId: "doc-A", workspaceGeneration: 3 }); r.resolve();
  assert.equal((await pending).status, "stale"); assert.equal(r.counts().publications, 0);
});
test("6 page switch before publication rejects candidate", async () => {
  const r = rig(); const pending = r.coordinator.execute(envelope({ add: 1 }));
  r.set({ pageId: "page-B" }); r.resolve(); assert.equal((await pending).status, "stale"); assert.equal(r.counts().publications, 0);
});
test("7 Rust rejection changes neither document nor selection", async () => {
  const r = rig({ gate: immediate() }); const before = r.state();
  assert.equal((await r.coordinator.execute(envelope({ add: 1, reject: true }))).status, "rejected");
  assert.deepEqual(r.state().document, before.document); assert.deepEqual(r.state().selection, before.selection); assert.equal(r.counts().renders, 0);
});
test("8 Rust candidate projection mismatch fails immutable validation", async () => {
  const r = rig({ gate: immediate() }); assert.equal((await r.coordinator.execute(envelope({ add: 1, mismatch: true }))).status, "rejected");
  assert.equal(r.state().document.value, 0); assert.equal(r.counts().publications, 0);
});
test("9 evaluator exception is fail-closed", async () => {
  const r = rig({ evaluatorThrows: true }); assert.equal((await r.coordinator.execute(envelope({ add: 1 }))).status, "rejected");
  assert.equal(r.counts().publications, 0); assert.equal(r.state().document.value, 0);
});
test("10 server revision conflict is represented as a blocking result, not a fabricated local ack", async () => {
  const r = rig({ gate: immediate() }); r.set({ canonicalRevision: 5 });
  const result = await r.coordinator.execute(envelope({ add: 1 }));
  assert.equal(result.status, "rejected"); assert.equal(r.state().canonicalStatus, "unchanged"); assert.equal(r.state().document.value, 0);
  const conflicted = rig({ gate: immediate() }); conflicted.set({ canonicalStatus: "conflict" });
  assert.equal((await conflicted.coordinator.execute(envelope({ add: 1 }))).status, "server-conflict");
  assert.equal(conflicted.counts().publications, 0);
});
test("11 ambiguous network outcome cannot fall through to local publication", async () => {
  const r = rig({ gate: immediate() }); r.set({ canonicalStatus: "ambiguous" });
  const op = envelope({ add: 1 });
  const result = await r.coordinator.execute(op);
  assert.equal(result.status, "ambiguous"); assert.equal(r.counts().publications, 0);
  assert.equal((await r.coordinator.execute(op)).status, "duplicate");
});
test("12 offline replay has no canonical revision authority and is ineligible", async () => {
  const r = rig(); r.set({ canonicalRevision: null, canonicalRevisionAuthority: "unavailable", canonicalVersion: null, canonicalVersionAuthority: "unavailable" });
  assert.equal((await r.coordinator.execute(envelope({ add: 1 }, { expectedCanonicalRevision: null, canonicalRevisionAuthority: "unavailable", expectedCanonicalVersion: null, canonicalVersionAuthority: "unavailable" }))).status, "rejected");
  assert.equal(r.counts().publications, 0);
});
test("13 restore vern change fences a candidate even if revn is unchanged", async () => {
  const r = rig(); const pending = r.coordinator.execute(envelope({ add: 1 }));
  r.set({ canonicalRevision: 4, canonicalVersion: 3 }); r.resolve();
  assert.equal((await pending).status, "stale"); assert.equal(r.state().document.value, 0);
});
test("14 renderer failure is reported after one successful local publication", async () => {
  const r = rig({ gate: immediate(), renderThrows: true }); const result = await r.coordinator.execute(envelope({ add: 1 }));
  assert.deepEqual(result, { status: "accepted", localSnapshotGeneration: 8, rendererStatus: "failed" });
  assert.equal(r.state().document.value, 1); assert.equal(r.counts().publications, 1);
});
test("15 local publication is distinguishable from canonical server acknowledgement", async () => {
  const r = rig({ gate: immediate() }); const result = await r.coordinator.execute(envelope({ add: 1 }));
  assert.equal(result.status, "accepted"); assert.equal(r.state().canonicalRevision, 4);
  assert.equal(r.state().canonicalStatus, "unchanged"); assert.equal(r.state().localSnapshotGeneration, 8);
});
test("16 in-place document change without a generation bump invalidates the captured immutable snapshot", async () => {
  const r = rig(); const pending = r.coordinator.execute(envelope({ add: 1 }));
  r.state().document.value = 99;
  r.resolve();
  assert.equal((await pending).status, "stale"); assert.equal(r.state().document.value, 99);
  assert.equal(r.counts().publications, 0);
});
