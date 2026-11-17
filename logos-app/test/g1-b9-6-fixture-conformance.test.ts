import assert from "node:assert/strict";
import test from "node:test";
import { buildG1B95Fixture, makeG1B95Operation } from "../src/test-support/g1-b9-5-workload";
import { evaluateFixtureOperation, withFixtureDocumentIdentity } from "../src/test-support/g1-b9-6-fixture-conformance";
import type { HierarchyOperation } from "../src/stores/hierarchyOperations";

test("initial fixture gets a stable test-only UUID without changing persisted identity", () => {
  const fixture = buildG1B95Fixture(21, 1);
  assert.match(fixture.documentId, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-8[0-9a-f]{3}-[0-9a-f]{12}$/i);
  const stateProjection = { pageOrder: fixture.pageOrder, pages: { [fixture.page.id]: fixture.page }, shapes: fixture.shapes };
  assert.equal(withFixtureDocumentIdentity(fixture.documentId, stateProjection).documentId, fixture.documentId);
});

test("subsequent operation preparation and repeated fixture resets retain the deterministic fixture identity", () => {
  const initial = buildG1B95Fixture(21, 100);
  const reset = buildG1B95Fixture(21, 960021);
  assert.equal(reset.documentId, initial.documentId);
  const operation = makeG1B95Operation(reset, "rename", 123);
  assert.equal(operation.kind, "rename");
  const snapshot = withFixtureDocumentIdentity(reset.documentId, {
    pageOrder: reset.pageOrder, pages: { [reset.page.id]: reset.page }, shapes: reset.shapes,
  });
  assert.equal(snapshot.documentId, initial.documentId);
});

test("missing and malformed fixture identity fail before invoking Rust", () => {
  const fixture = buildG1B95Fixture(21, 1);
  const projection = { pageOrder: fixture.pageOrder, pages: { [fixture.page.id]: fixture.page }, shapes: fixture.shapes };
  let calls = 0;
  const rust = { evaluateDocumentOperationJson: () => { calls += 1; return "{}"; } };
  assert.throws(() => withFixtureDocumentIdentity(null, projection), /FIXTURE_DOCUMENT_ID_INVALID/);
  assert.throws(() => withFixtureDocumentIdentity("not-a-uuid", projection), /FIXTURE_DOCUMENT_ID_INVALID/);
  assert.equal(calls, 0);
});

test("every supported operation reaches Rust candidate evaluation and rejection is not bypassed", () => {
  const fixture = buildG1B95Fixture(21, 1);
  const projection = { pageOrder: fixture.pageOrder, pages: { [fixture.page.id]: fixture.page }, shapes: fixture.shapes };
  let calls = 0;
  const rust = { evaluateDocumentOperationJson: (documentJson: string) => {
    calls += 1;
    assert.equal(JSON.parse(documentJson).id, fixture.documentId);
    return "{}";
  } };
  const operation = makeG1B95Operation(fixture, "rename", 77) as Exclude<HierarchyOperation, { kind: `compat-${string}` }>;
  assert.throws(() => evaluateFixtureOperation(fixture.documentId, projection, operation, rust), /RUST_CANDIDATE_REJECTED/);
  assert.equal(calls, 1);
});
