/**
 * G1-B9.4 disposable coordinator contract.
 *
 * This module is intentionally not imported by the production editor. It models
 * the one-owner publication boundary used by the conformance harness. Its
 * operation-ID table lives only as long as the coordinator instance and is not
 * durable exactly-once delivery.
 */
export interface RevisionOperationEnvelope<Payload> {
  contractVersion: "logos-operation-precondition/v1";
  canonicalDocumentId: string;
  pageId: string;
  workspaceGeneration: number;
  expectedCanonicalRevision: number | null;
  canonicalRevisionAuthority: "server-response" | "unavailable";
  /** Go `vern` restore epoch; separate from update-history `revn`. */
  expectedCanonicalVersion: number | null;
  canonicalVersionAuthority: "server-response" | "unavailable";
  localSnapshotGeneration: number;
  operationId: string;
  operationKind: string;
  payload: Payload;
  candidateSchemaVersion: number;
}

export interface RevisionCoordinatorState<Document, Selection> {
  canonicalDocumentId: string;
  pageId: string;
  workspaceGeneration: number;
  localSnapshotGeneration: number;
  canonicalRevision: number | null;
  canonicalRevisionAuthority: "server-response" | "unavailable";
  canonicalVersion: number | null;
  canonicalVersionAuthority: "server-response" | "unavailable";
  document: Document;
  selection: Selection;
  consumedOperationIds: ReadonlySet<string>;
  canonicalStatus: "unchanged" | "conflict" | "ambiguous";
  rendererStatus: "not-published" | "published" | "failed";
}

export type CandidateResult<Document> =
  | { ok: true; document: Document }
  | { ok: false; diagnostic: string };

export type CoordinatorResult =
  | { status: "accepted"; localSnapshotGeneration: number; rendererStatus: "published" | "failed" }
  | { status: "rejected"; diagnostic: string }
  | { status: "stale"; diagnostic: string }
  | { status: "duplicate"; diagnostic: "OPERATION_ALREADY_CONSUMED" }
  | { status: "id-conflict"; diagnostic: "OPERATION_ID_PAYLOAD_MISMATCH" }
  | { status: "server-conflict"; diagnostic: string }
  | { status: "ambiguous"; diagnostic: string };

export interface CoordinatorDependencies<Document, Selection, Payload> {
  getState(): RevisionCoordinatorState<Document, Selection>;
  evaluate(document: Readonly<Document>, payload: Payload): Promise<CandidateResult<Document>>;
  validate(document: Readonly<Document>): boolean;
  reconcileSelection(document: Readonly<Document>, selection: Readonly<Selection>): Selection;
  publish(document: Document, selection: Selection, nextLocalGeneration: number): void;
  renderCommittedState(): void;
  fingerprint(payload: Payload): string;
  fingerprintDocument(document: Readonly<Document>): string;
}

type PendingEntry = { fingerprint: string; promise: Promise<CoordinatorResult> };

function deepFreeze<T>(value: T): T {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const child of Object.values(value as Record<string, unknown>)) deepFreeze(child);
  }
  return value;
}

/**
 * A test-only local publication coordinator. It refuses candidates without a
 * server-issued revision but does not call Go or claim durable server commit.
 */
export class RevisionCommitCoordinator<Document, Selection, Payload> {
  private readonly pending = new Map<string, PendingEntry>();
  private readonly completed = new Map<string, { fingerprint: string; result: CoordinatorResult }>();

  constructor(private readonly deps: CoordinatorDependencies<Document, Selection, Payload>) {}

  execute(envelope: RevisionOperationEnvelope<Payload>): Promise<CoordinatorResult> {
    const fingerprint = this.deps.fingerprint(envelope.payload);
    const completed = this.completed.get(envelope.operationId);
    if (completed) return Promise.resolve(completed.fingerprint === fingerprint
      ? { status: "duplicate", diagnostic: "OPERATION_ALREADY_CONSUMED" }
      : { status: "id-conflict", diagnostic: "OPERATION_ID_PAYLOAD_MISMATCH" });
    const pending = this.pending.get(envelope.operationId);
    if (pending) return pending.fingerprint === fingerprint
      ? pending.promise
      : Promise.resolve({ status: "id-conflict", diagnostic: "OPERATION_ID_PAYLOAD_MISMATCH" });

    const promise = this.evaluateAndPublish(envelope).then((result) => {
      if (result.status === "accepted" || result.status === "server-conflict" || result.status === "ambiguous") {
        this.completed.set(envelope.operationId, { fingerprint, result });
      }
      return result;
    }).finally(() => this.pending.delete(envelope.operationId));
    this.pending.set(envelope.operationId, { fingerprint, promise });
    return promise;
  }

  private async evaluateAndPublish(envelope: RevisionOperationEnvelope<Payload>): Promise<CoordinatorResult> {
    const captured = this.deps.getState();
    if (!envelope.operationId || !envelope.operationKind || envelope.candidateSchemaVersion !== 1) {
      return { status: "rejected", diagnostic: "OPERATION_ENVELOPE_UNSUPPORTED" };
    }
    if (envelope.contractVersion !== "logos-operation-precondition/v1" ||
        envelope.canonicalDocumentId !== captured.canonicalDocumentId ||
        envelope.pageId !== captured.pageId ||
        envelope.workspaceGeneration !== captured.workspaceGeneration ||
        envelope.localSnapshotGeneration !== captured.localSnapshotGeneration) {
      return { status: "stale", diagnostic: "PRECONDITION_CAPTURE_MISMATCH" };
    }
    if (captured.canonicalStatus === "conflict") return { status: "server-conflict", diagnostic: "CANONICAL_REVISION_CONFLICT" };
    if (captured.canonicalStatus === "ambiguous") return { status: "ambiguous", diagnostic: "CANONICAL_COMMIT_OUTCOME_AMBIGUOUS" };
    if (envelope.canonicalRevisionAuthority !== "server-response" ||
        envelope.expectedCanonicalRevision === null ||
        captured.canonicalRevisionAuthority !== "server-response" ||
        envelope.expectedCanonicalRevision !== captured.canonicalRevision ||
        envelope.canonicalVersionAuthority !== "server-response" ||
        envelope.expectedCanonicalVersion === null ||
        captured.canonicalVersionAuthority !== "server-response" ||
        envelope.expectedCanonicalVersion !== captured.canonicalVersion) {
      return { status: "rejected", diagnostic: "CANONICAL_REVISION_UNAVAILABLE_OR_MISMATCHED" };
    }

    let immutableSnapshot: Document;
    let capturedDocumentFingerprint: string;
    try {
      immutableSnapshot = deepFreeze(structuredClone(captured.document));
      capturedDocumentFingerprint = this.deps.fingerprintDocument(immutableSnapshot);
    } catch {
      return { status: "rejected", diagnostic: "SNAPSHOT_CAPTURE_FAILED" };
    }

    let evaluated: CandidateResult<Document>;
    try {
      evaluated = await this.deps.evaluate(immutableSnapshot, envelope.payload);
    } catch {
      return { status: "rejected", diagnostic: "CANDIDATE_EVALUATION_FAILED" };
    }
    if (!evaluated.ok) return { status: "rejected", diagnostic: evaluated.diagnostic };
    try {
      if (!this.deps.validate(evaluated.document)) return { status: "rejected", diagnostic: "CANDIDATE_VALIDATION_FAILED" };
    } catch {
      return { status: "rejected", diagnostic: "CANDIDATE_VALIDATION_FAILED" };
    }

    const current = this.deps.getState();
    let currentDocumentFingerprint: string;
    try { currentDocumentFingerprint = this.deps.fingerprintDocument(current.document); }
    catch { return { status: "rejected", diagnostic: "CURRENT_SNAPSHOT_RECHECK_FAILED" }; }
    if (current.canonicalDocumentId !== envelope.canonicalDocumentId ||
        current.pageId !== envelope.pageId ||
        current.workspaceGeneration !== envelope.workspaceGeneration ||
        current.localSnapshotGeneration !== envelope.localSnapshotGeneration ||
        current.canonicalRevisionAuthority !== "server-response" ||
        current.canonicalRevision !== envelope.expectedCanonicalRevision ||
        current.canonicalVersionAuthority !== "server-response" ||
        current.canonicalVersion !== envelope.expectedCanonicalVersion ||
        current.canonicalStatus !== "unchanged" ||
        currentDocumentFingerprint !== capturedDocumentFingerprint) {
      if (current.canonicalStatus === "conflict") return { status: "server-conflict", diagnostic: "CANONICAL_REVISION_CONFLICT" };
      if (current.canonicalStatus === "ambiguous") return { status: "ambiguous", diagnostic: "CANONICAL_COMMIT_OUTCOME_AMBIGUOUS" };
      return { status: "stale", diagnostic: "PRECONDITION_CHANGED_BEFORE_PUBLICATION" };
    }

    let immutableCandidate: Document;
    try { immutableCandidate = deepFreeze(structuredClone(evaluated.document)); }
    catch { return { status: "rejected", diagnostic: "CANDIDATE_IMMUTABLE_CAPTURE_FAILED" }; }
    let selection: Selection;
    try { selection = this.deps.reconcileSelection(immutableCandidate, current.selection); }
    catch { return { status: "rejected", diagnostic: "SELECTION_RECONCILIATION_FAILED" }; }
    const nextLocalGeneration = current.localSnapshotGeneration + 1;
    this.deps.publish(immutableCandidate, selection, nextLocalGeneration);
    try {
      this.deps.renderCommittedState();
      return { status: "accepted", localSnapshotGeneration: nextLocalGeneration, rendererStatus: "published" };
    } catch {
      return { status: "accepted", localSnapshotGeneration: nextLocalGeneration, rendererStatus: "failed" };
    }
  }
}
