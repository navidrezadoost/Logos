import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { isDeepStrictEqual } from "node:util";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const appRoot = fileURLToPath(new URL("../../logos-app/", import.meta.url));
const fixtureUrl = new URL("../fixtures/figma-semantics/component-instance-domain-v1.json", import.meta.url);
const negativeUrl = new URL("../fixtures/figma-semantics/component-instance-v1-invalid-cases.json", import.meta.url);
const fixture = JSON.parse(await readFile(fixtureUrl, "utf8"));
const negativeCases = JSON.parse(await readFile(negativeUrl, "utf8"));
const require = createRequire(new URL("../../logos-app/package.json", import.meta.url));
const { createServer } = await import(require.resolve("vite"));
const server = await createServer({
  appType: "custom", configFile: false, logLevel: "error", optimizeDeps: { noDiscovery: true, include: [], entries: [] },
  root: appRoot, server: { middlewareMode: true, hmr: false },
});

const clone = () => structuredClone(fixture);
function setPath(input, path, value) {
  const parent = path.slice(0, -1).reduce((at, part) => at[part], input);
  parent[path.at(-1)] = value;
}
function reverseObjectInsertion(value) {
  if (Array.isArray(value)) return value.map(reverseObjectInsertion);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).reverse().map(([k, v]) => [k, reverseObjectInsertion(v)]));
  return value;
}
function component(id, target, swap = null) {
  const nodeId = `${id}-ref`;
  return { id, name: id, setId: null, rootNodeIds: [nodeId], nodes: {
    [nodeId]: { id: nodeId, kind: "instance", parentId: null, childIds: [], source: { kind: "local-component", componentId: target }, variantSelection: {} },
  }, properties: swap ? { [`${id}-swap`]: { id: `${id}-swap`, name: "Swap", kind: "instance-swap", targetNodeId: nodeId, allowedComponentIds: swap, defaultComponentId: swap[0] } } : {}, variantValues: {} };
}
function instance(id, componentId, propertyValues = {}) {
  return { id, source: { kind: "local-component", componentId }, variantSelection: {}, propertyValues, overrides: [], slotContent: {} };
}

const cases = [{ name: "positive-generated", input: clone() }];
for (const negative of negativeCases) {
  const input = clone();
  setPath(input, negative.path, negative.value);
  cases.push({ name: negative.name, input });
}
const unknownVersion = clone(); unknownVersion.version = 2;
cases.push({ name: "unsupported-version", input: unknownVersion });
cases.push({ name: "non-object-json", input: null });
cases.push({ name: "json-text-decode-error", input: "not-json", rustLine: "{not-json" });
const multiple = clone();
multiple.instances["button-instance"].variantSelection.size = "medium";
multiple.instances["card-instance"].propertyValues["show-icon"] = "false";
cases.push({ name: "multiple-errors", input: multiple });
const sharedIdentity = clone();
sharedIdentity.components.card.nodes.card = { id: "card", kind: "shape", parentId: null, childIds: [] };
sharedIdentity.components.card.rootNodeIds.push("card");
cases.push({ name: "duplicate-component-definition-node-identity", input: sharedIdentity });
const malformedDiscriminator = clone();
malformedDiscriminator.instances["card-instance"].source = { kind: "future-reference", componentId: "card" };
cases.push({ name: "malformed-reference-discriminator", input: malformedDiscriminator });
const malformedDocument = { version: 1, components: [], componentSets: {}, instances: {} };
cases.push({ name: "malformed-document-records", input: malformedDocument });
const malformedComponent = clone();
malformedComponent.components.card.name = null;
cases.push({ name: "malformed-component-record", input: malformedComponent });
const mismatchedId = clone();
mismatchedId.components.card.id = "other-card";
cases.push({ name: "component-record-id-mismatch", input: mismatchedId });
const invalidProperty = clone();
invalidProperty.components.card.properties["label-text"].defaultValue = 17;
cases.push({ name: "invalid-property-kind-default", input: invalidProperty });
const containmentCycle = clone();
containmentCycle.components.card.nodes["card-surface"].childIds = ["card-surface"];
cases.push({ name: "containment-cycle", input: containmentCycle });
const badExternal = clone();
badExternal.instances["external-instance"].source.revision = 3;
cases.push({ name: "malformed-external-reference", input: badExternal });
const badMembership = clone();
badMembership.components["icon-small"].setId = null;
cases.push({ name: "nonreciprocal-set-membership", input: badMembership });
const invalidTuple = clone();
invalidTuple.components["icon-large"].variantValues.size = "medium";
cases.push({ name: "invalid-member-variant-tuple", input: invalidTuple });
const duplicateOverride = clone();
duplicateOverride.instances["card-instance"].overrides.push(structuredClone(duplicateOverride.instances["card-instance"].overrides[0]));
cases.push({ name: "duplicate-override-address", input: duplicateOverride });
const badOverrideValue = clone();
badOverrideValue.instances["card-instance"].overrides[0].value = "not-an-array";
cases.push({ name: "override-value-type", input: badOverrideValue });
const badSlotGraph = clone();
badSlotGraph.instances["card-instance"].slotContent["content-slot"].nodes["authored-1"].future = true;
cases.push({ name: "slot-node-unknown-field", input: badSlotGraph });
const invalidTarget = clone();
invalidTarget.components.card.properties["label-text"].targetNodeId = "card-surface";
cases.push({ name: "incompatible-property-target", input: invalidTarget });
cases.push({ name: "reversed-object-insertion-order", input: reverseObjectInsertion(multiple) });
const oppositeSwaps = {
  version: 1,
  components: {
    leaf: { id: "leaf", name: "leaf", setId: null, rootNodeIds: ["leaf-node"], nodes: { "leaf-node": { id: "leaf-node", kind: "shape", parentId: null, childIds: [] } }, properties: {}, variantValues: {} },
    a: component("a", "leaf", ["leaf", "b"]), b: component("b", "leaf", ["leaf", "a"]),
  }, componentSets: {}, instances: {
    "a-instance": instance("a-instance", "a", { "a-swap": { kind: "local-component", componentId: "b" } }),
    "b-instance": instance("b-instance", "b", { "b-swap": { kind: "local-component", componentId: "a" } }),
  },
};
cases.push({ name: "independent-opposite-instance-swaps", input: oppositeSwaps });
const selfRecursive = clone();
selfRecursive.components.self = { id: "self", name: "Self", setId: null, rootNodeIds: ["self-ref"], nodes: {
  "self-ref": { id: "self-ref", kind: "instance", parentId: null, childIds: [], source: { kind: "local-component", componentId: "self" }, variantSelection: {} },
}, properties: {}, variantValues: {} };
selfRecursive.instances.selfUse = instance("selfUse", "self");
cases.push({ name: "direct-self-expansion-cycle", input: selfRecursive });
const mutualRecursive = clone();
mutualRecursive.components.alpha = { id: "alpha", name: "Alpha", setId: null, rootNodeIds: ["alpha-ref"], nodes: {
  "alpha-ref": { id: "alpha-ref", kind: "instance", parentId: null, childIds: [], source: { kind: "local-component", componentId: "beta" }, variantSelection: {} },
}, properties: {}, variantValues: {} };
mutualRecursive.components.beta = { id: "beta", name: "Beta", setId: null, rootNodeIds: ["beta-ref"], nodes: {
  "beta-ref": { id: "beta-ref", kind: "instance", parentId: null, childIds: [], source: { kind: "local-component", componentId: "alpha" }, variantSelection: {} },
}, properties: {}, variantValues: {} };
mutualRecursive.instances.mutualUse = instance("mutualUse", "alpha");
cases.push({ name: "mutual-definition-expansion-cycle", input: mutualRecursive });

try {
  const domain = await server.ssrLoadModule("/src/document-core/componentInstanceDomainV1.ts");
  const tsResults = cases.map(({ input }) => {
    const before = structuredClone(input);
    const result = domain.validateComponentInstanceDocument(input);
    assert.deepEqual(input, before, "TypeScript validator must not mutate input");
    return result;
  });
  const cargo = spawnSync("cargo", ["run", "--quiet", "--manifest-path", "spikes/fg-c3-2-1/rust-planner/Cargo.toml", "--bin", "component_instance_validate"], {
    cwd: root, input: cases.map(({ input, rustLine }) => rustLine ?? JSON.stringify(input)).join("\n") + "\n", encoding: "utf8", maxBuffer: 16 * 1024 * 1024,
  });
  if (cargo.status !== 0) throw new Error(`Rust differential runner failed (${cargo.status}): ${cargo.stderr}`);
  const rustResults = cargo.stdout.trimEnd().split("\n").map((line) => JSON.parse(line));
  assert.equal(rustResults.length, cases.length, "one Rust response per shared input");

  let matched = 0;
  const mismatches = [];
  for (let index = 0; index < cases.length; index++) {
    const ts = tsResults[index]; const rust = rustResults[index];
    const comparableTs = { ok: ts.ok, diagnostics: ts.diagnostics, document: ts.document };
    if (isDeepStrictEqual(comparableTs, rust)) matched++;
    else mismatches.push({ name: cases[index].name, typescript: comparableTs, rust });
  }
  const observedCodes = [...new Set(tsResults.flatMap((result) => result.diagnostics.map((d) => d.code)))].sort();
  const report = { cases: cases.length, matched, divergent: mismatches.length, unsupported: 0, observedDiagnosticCodes: observedCodes, detailPolicy: "exact code/path/severity/detail and semantic JSON payload equality" };
  console.log(JSON.stringify(report));
  if (mismatches.length) {
    console.error(JSON.stringify(mismatches, null, 2));
    process.exitCode = 1;
  }
} finally {
  await server.close();
}
