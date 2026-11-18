import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const path = new URL("../fixtures/figma-semantics/fg-c3-5-protocol-adversarial-v1.json", import.meta.url);
const contract = JSON.parse(await readFile(path, "utf8"));

test("FG-C3.5 adversarial protocol fixture records FG-C3.6 local execution", () => {
  assert.equal(contract.fixtureContractVersion, 1);
  assert.equal(contract.proposal, "fg-c3.5-proposal-v1");
  assert.equal(contract.executionStatus, "executed-by-fg-c3.6-local-tests");
  assert.equal(contract.operation, "validateComponentInstanceV1");
  assert.equal(contract.cases.length, 12);
  assert.equal(new Set(contract.cases.map(({ id }) => id)).size, contract.cases.length);
  assert.deepEqual(new Set(contract.cases.map(({ id }) => id)), new Set([
    "valid-clean", "valid-warning-only", "rejected-component-graph", "unsupported-component-semantic-version",
    "malformed-request-envelope", "duplicate-json-member-policy", "oversized-document-payload", "oversized-request-frame", "excessive-diagnostic-output", "stale-response-correlation",
    "truncated-input-frame", "restart-after-semantic-rejection",
  ]));
  assert.ok(contract.cases.every(({ expected }) => expected && typeof expected.kind === "string"));
});

test("isolated validation operation is implemented but remains unadvertised", async () => {
  const source = await readFile(new URL("../../spikes/fg-c3-2-1/rust-planner/src/lib.rs", import.meta.url), "utf8");
  assert.match(source, /"componentInstanceDomain": \{"status": "not-supported-here"/);
  assert.match(source, /ValidateComponentInstanceV1/);
  assert.doesNotMatch(source, /"componentInstanceDomain": \{"status": "supported"/);
});
