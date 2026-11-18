import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const appRoot = fileURLToPath(new URL("../../logos-app/", import.meta.url));
const fixtureUrl = new URL("../fixtures/figma-semantics/component-instance-domain-v1.json", import.meta.url);
const negativeUrl = new URL("../fixtures/figma-semantics/component-instance-v1-invalid-cases.json", import.meta.url);
const fixture = JSON.parse(await readFile(fixtureUrl, "utf8"));
const negativeCases = JSON.parse(await readFile(negativeUrl, "utf8"));
const require = createRequire(new URL("../../logos-app/package.json", import.meta.url));
const { createServer } = await import(require.resolve("vite"));
let server;
let domain;

before(async () => {
  server = await createServer({
    appType: "custom", configFile: false, logLevel: "error",
    optimizeDeps: { noDiscovery: true, include: [], entries: [] },
    root: appRoot, server: { middlewareMode: true, hmr: false },
  });
  domain = await server.ssrLoadModule("/src/document-core/componentInstanceDomainV1.ts");
});
after(async () => server?.close());

function clone() { return structuredClone(fixture); }
function setPath(object, path, value) {
  const parent = path.slice(0, -1).reduce((value, part) => value[part], object);
  parent[path.at(-1)] = value;
}
function diagnosticCodes(result) { return result.diagnostics.map((d) => d.code); }

test("candidate-schema-is-versioned-pure-and-validates-all-five-property-kinds", () => {
  const input = clone();
  const before = JSON.stringify(input);
  const result = domain.validateComponentInstanceDocument(input);
  assert.equal(domain.COMPONENT_INSTANCE_DOMAIN_VERSION, 1);
  assert.equal(result.ok, true, JSON.stringify(result.diagnostics));
  assert.strictEqual(result.document, input, "valid data is returned by reference; no copy or authority implied");
  assert.deepEqual(input, JSON.parse(before), "validation must not mutate source data");
  assert.ok(result.diagnostics.some((d) => d.code === "COMPONENT_SLOT_GUIDANCE_EXCEEDED" && d.severity === "warning"));
  assert.ok(result.diagnostics.some((d) => d.code === "COMPONENT_REFERENCE_UNRESOLVED" && d.severity === "warning"));
  assert.equal(result.diagnostics.some((d) => d.severity === "error"), false);
  const kinds = new Set(Object.values(input.components.card.properties).map((p) => p.kind));
  kinds.add(input.componentSets["icon-set"].properties.size.kind);
  assert.deepEqual([...kinds].sort(), ["boolean", "instance-swap", "slot", "text", "variant"]);
});

test("positive-fixture-preserves-ordered-set-members-and-authored-slot-content-identities", () => {
  const result = domain.validateComponentInstanceDocument(clone());
  assert.equal(result.ok, true);
  assert.deepEqual(result.document.componentSets["icon-set"].memberComponentIds, ["icon-small", "icon-large"]);
  assert.deepEqual(result.document.instances["card-instance"].slotContent["content-slot"].rootNodeIds, ["authored-1", "authored-2"]);
  assert.equal(result.document.instances["card-instance"].slotContent["content-slot"].nodes["authored-1"].parentId, null);
});

test("deterministic-negative-fixtures-fail-closed-with-stable-diagnostics-and-no-mutation", () => {
  for (const item of negativeCases) {
    const input = clone();
    setPath(input, item.path, item.value);
    const before = JSON.stringify(input);
    const first = domain.validateComponentInstanceDocument(input);
    const second = domain.validateComponentInstanceDocument(input);
    assert.equal(first.ok, false, item.name);
    assert.ok(diagnosticCodes(first).includes(item.expected), `${item.name}: ${JSON.stringify(first.diagnostics)}`);
    assert.deepEqual(first.diagnostics, second.diagnostics, `${item.name}: deterministic diagnostic ordering`);
    assert.equal(first.document, null);
    assert.equal(JSON.stringify(input), before, `${item.name}: validator does not mutate rejected input`);
  }
});

test("malformed-json-version-and-unknown-property-kind-reject-explicitly", () => {
  for (const value of [null, [], 1, "document", { version: 1, components: [], componentSets: {}, instances: {} }]) {
    const result = domain.validateComponentInstanceDocument(value);
    assert.equal(result.ok, false);
    assert.equal(result.document, null);
  }
  const wrongVersion = clone(); wrongVersion.version = 2;
  assert.ok(diagnosticCodes(domain.validateComponentInstanceDocument(wrongVersion)).includes("COMPONENT_VERSION_UNSUPPORTED"));
  const duplicateId = clone();
  duplicateId.instances.card = { ...duplicateId.instances["card-instance"], id: "card" };
  assert.ok(diagnosticCodes(domain.validateComponentInstanceDocument(duplicateId)).includes("COMPONENT_DUPLICATE_ID"));
  const unknownKind = clone(); unknownKind.components.card.properties["label-text"].kind = "future-property";
  assert.ok(diagnosticCodes(domain.validateComponentInstanceDocument(unknownKind)).includes("COMPONENT_PROPERTY_KIND_UNSUPPORTED"));
});

test("slot-guidance-only-warns-while-explicit-hard-limits-reject", () => {
  const advisory = clone();
  advisory.components.card.properties["content-slot"].hardLimits = { maximumEntries: 4 };
  const advisoryResult = domain.validateComponentInstanceDocument(advisory);
  assert.equal(advisoryResult.ok, true);
  assert.equal(advisoryResult.diagnostics.some((d) => d.code === "COMPONENT_SLOT_GUIDANCE_EXCEEDED" && d.severity === "warning"), true);
  const hard = clone();
  hard.components.card.properties["content-slot"].hardLimits.maximumEntries = 1;
  const hardResult = domain.validateComponentInstanceDocument(hard);
  assert.equal(hardResult.ok, false);
  assert.equal(hardResult.diagnostics.some((d) => d.code === "COMPONENT_SLOT_HARD_LIMIT_EXCEEDED" && d.severity === "error"), true);
});

test("explicit-preferred-instance-allow-list-accepts-an-allowed-slot-entry", () => {
  const input = clone();
  const slot = input.components.card.properties["content-slot"];
  slot.onlyAllowPreferredInstances = true;
  const content = input.instances["card-instance"].slotContent["content-slot"];
  content.rootNodeIds = ["authored-2"];
  delete content.nodes["authored-1"];
  const result = domain.validateComponentInstanceDocument(input);
  assert.equal(result.ok, true, JSON.stringify(result.diagnostics));
  assert.equal(result.diagnostics.some((d) => d.code === "COMPONENT_SLOT_CONTENT_NOT_ALLOWED"), false);
});

test("cycles-in-component-references-and-invalid-containment-graphs-reject", () => {
  const cyclic = clone();
  cyclic.components["icon-small"].nodes["icon-small-root"] = {
    id: "icon-small-root", kind: "instance", parentId: null, childIds: [],
    source: { kind: "local-component", componentId: "card" }, variantSelection: {},
  };
  const cycleResult = domain.validateComponentInstanceDocument(cyclic);
  assert.equal(cycleResult.ok, false);
  assert.ok(diagnosticCodes(cycleResult).includes("COMPONENT_REFERENCE_CYCLE"));
  const malformed = clone();
  malformed.components.card.nodes["card-label"].parentId = "card-surface";
  const graphResult = domain.validateComponentInstanceDocument(malformed);
  assert.equal(graphResult.ok, false);
  assert.ok(diagnosticCodes(graphResult).includes("COMPONENT_NODE_GRAPH_INVALID"));
});

test("unknown-unresolved-source-payload-is-preserved-but-never-treated-as-resolved", () => {
  const input = clone();
  const locator = input.instances["unresolved-instance"].source.locator;
  const result = domain.validateComponentInstanceDocument(input);
  assert.equal(result.ok, true);
  assert.strictEqual(result.document.instances["unresolved-instance"].source.locator, locator);
  assert.equal(result.diagnostics.some((d) => d.code === "COMPONENT_REFERENCE_UNRESOLVED" && d.severity === "warning"), true);
});
