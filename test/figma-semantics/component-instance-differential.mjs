import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { mkdir, readFile, writeFile } from "node:fs/promises";
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

const GENERATOR_VERSION = "fg-c3.5-seeded-v1";
const SEED_MATRIX = [0x0c3c0401, 0x5eedc301, 0x20261118];
const SAMPLES_PER_SEED = 48;
function seededRandom(seed) {
  let state = seed >>> 0;
  return () => {
    state ^= state << 13; state ^= state >>> 17; state ^= state << 5;
    return (state >>> 0) / 0x1_0000_0000;
  };
}
const caseFor = (name, input, extra = {}) => ({ name, input, generatorVersion: GENERATOR_VERSION, ...extra });

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

// Fixed-seed valid graphs vary reference depth and document size while keeping
// the fixture's component sets, typed properties, overrides, and slots intact.
for (const seed of SEED_MATRIX) {
  const random = seededRandom(seed);
  const originalSeedCorpus = seed === SEED_MATRIX[0];
  const generatedPrefix = originalSeedCorpus ? (sample) => `gen-${sample}` : (sample) => `gen-${seed.toString(16)}-${sample}`;
  const casePrefix = originalSeedCorpus ? `seed-${seed}` : `seed-${seed.toString(16)}`;
  for (let sample = 0; sample < SAMPLES_PER_SEED; sample++) {
    const input = clone();
    const depth = 1 + Math.floor(random() * 8);
    let target = "card";
    for (let level = 0; level < depth; level++) {
      const id = `${generatedPrefix(sample)}-component-${level}`;
      input.components[id] = component(id, target);
      target = id;
    }
    const extraInstances = Math.floor(random() * 9);
    for (let index = 0; index < extraInstances; index++) {
      const id = `${generatedPrefix(sample)}-instance-${index}`;
      const componentId = index % 2 ? target : "card";
      input.instances[id] = instance(id, componentId);
    }
    cases.push(caseFor(`${casePrefix}-valid-${sample}`, input, { seed, sample }));

    const mutation = structuredClone(input);
    const generatedIds = Object.keys(mutation.components).filter((id) => id.startsWith(`${generatedPrefix(sample)}-`));
    const selected = generatedIds[Math.floor(random() * generatedIds.length)];
    mutation.components[selected].nodes[`${selected}-ref`].source.componentId = originalSeedCorpus ? `missing-${sample}` : `missing-${seed.toString(16)}-${sample}`;
    cases.push(caseFor(`${casePrefix}-invalid-reference-${sample}`, mutation, { seed, sample }));
  }

  // Exercise several interactions in one invalid document: malformed set
  // variant selection, an unauthorized swap target, and broken slot topology.
  const combined = clone();
  combined.components.card.nodes["card-icon"].variantSelection.size = `unknown-${seed.toString(16)}`;
  combined.instances["card-instance"].propertyValues["icon-swap"] = { kind: "local-component", componentId: "card" };
  combined.instances["card-instance"].slotContent["content-slot"].nodes["authored-2"].parentId = "missing-slot-parent";
  cases.push(caseFor(`seed-${seed.toString(16)}-combined-variant-swap-slot`, combined, { seed, sample: "combined" }));
}

// The JSON-text entrypoint and JSON.parse must agree on the same wire input.
function rawCase(name, rawLine) {
  cases.push(caseFor(name, JSON.parse(rawLine), { rawLine }));
}
rawCase("duplicate-json-member-last-wins", JSON.stringify(clone()).replace('"version": 1,', '"version": 2, "version": 1,'));
rawCase("large-integer-rounds-as-javascript-number", JSON.stringify(clone()).replace('"asset-key-1"', '"asset-key-1", "large": 9007199254740993'));
rawCase("integer-valued-float-and-negative-zero", JSON.stringify(clone()).replace('"asset-key-1"', '"asset-key-1", "float": 1.0, "negativeZero": -0'));
rawCase("exponent-float-value", JSON.stringify(clone()).replace('"asset-key-1"', '"asset-key-1", "exponent": 1.25e-120'));

const absentOptional = clone(); delete absentOptional.components.card.setId;
cases.push({ name: "optional-null-value", input: clone() });
cases.push({ name: "optional-field-absent", input: absentOptional });
const unusualKeys = clone();
unusualKeys.components[""] = { id: "", name: "", setId: null, rootNodeIds: [], nodes: {}, properties: {}, variantValues: {} };
unusualKeys.components["𐀀"] = { id: "", name: "", setId: null, rootNodeIds: [], nodes: {}, properties: { "\uE000": { id: "", name: "", kind: "future" } }, variantValues: {} };
unusualKeys.components["\uE000"] = { id: "\uE000", name: "", setId: null, rootNodeIds: [], nodes: {}, properties: {}, variantValues: {} };
Object.defineProperty(unusualKeys.instances, "__proto__", { value: instance("__proto__", "missing"), enumerable: true, configurable: true, writable: true });
unusualKeys.instances.constructor = instance("constructor", "missing");
cases.push({ name: "unicode-empty-and-unusual-identities", input: unusualKeys });
cases.push({ name: "long-diagnostic-path", input: (() => { const value = clone(); const key = "k".repeat(1024); value.instances["unresolved-instance"].propertyValues[key] = true; return value; })() });
cases.push({ name: "large-envelope-256-instances", input: (() => { const value = clone(); for (let i = 0; i < 256; i++) value.instances[`bulk-${String(i).padStart(3, "0")}`] = instance(`bulk-${i}`, "card"); return value; })() });
cases.push({ name: "deep-containment-graph", input: (() => { const value = clone(); const card = value.components.card; const chain = []; for (let i = 0; i < 48; i++) { const id = `deep-${i}`; chain.push(id); card.nodes[id] = { id, kind: "container", parentId: i ? `deep-${i - 1}` : null, childIds: i < 47 ? [`deep-${i + 1}`] : [] }; } card.rootNodeIds.push(chain[0]); return value; })() });
const javascriptOnly = clone();
javascriptOnly.instances["unresolved-instance"].source.locator.nonFinite = Number.NaN;
cases.push({ name: "unsupported-javascript-only-nan", input: javascriptOnly, supported: false, unsupportedReason: "NaN has no valid JSON wire representation" });
for (const testCase of cases) {
  testCase.supportClass = testCase.supported === false ? "typescript-only" : "shared-wire";
  if (testCase.supportClass === "typescript-only" && !testCase.unsupportedReason) throw new Error(`Unsupported case lacks a reason: ${testCase.name}`);
}

try {
  const domain = await server.ssrLoadModule("/src/document-core/componentInstanceDomainV1.ts");
  const tsResults = cases.map(({ input, rawLine }) => {
    const parsedInput = rawLine === undefined ? input : JSON.parse(rawLine);
    const before = structuredClone(parsedInput);
    const result = domain.validateComponentInstanceDocument(parsedInput);
    assert.deepEqual(parsedInput, before, "TypeScript validator must not mutate input");
    return result;
  });
  const supportedCases = cases.map((testCase, index) => ({ testCase, ts: tsResults[index], index })).filter(({ testCase }) => testCase.supportClass === "shared-wire");
  const unsupportedCases = cases.filter(({ supportClass }) => supportClass === "typescript-only").map(({ name, unsupportedReason }) => ({ name, reason: unsupportedReason }));
  const unknownClassifications = cases.filter(({ supportClass }) => !["shared-wire", "typescript-only"].includes(supportClass));
  const cargo = spawnSync("cargo", ["run", "--quiet", "--manifest-path", "spikes/fg-c3-2-1/rust-planner/Cargo.toml", "--bin", "component_instance_validate"], {
    cwd: root, input: supportedCases.map(({ testCase: { input, rawLine, rustLine } }) => rawLine ?? rustLine ?? JSON.stringify(input)).join("\n") + "\n", encoding: "utf8", maxBuffer: 16 * 1024 * 1024,
  });
  const runnerFailures = [];
  const parserFailures = [];
  const semanticMismatches = [];
  if (cargo.error || cargo.status !== 0) {
    runnerFailures.push({ code: "RUST_RUNNER_FAILED", status: cargo.status, message: cargo.error?.message ?? cargo.stderr });
  }
  const rawRustLines = cargo.status === 0 ? cargo.stdout.trimEnd().split("\n") : [];
  if (cargo.status === 0 && rawRustLines.length !== supportedCases.length) {
    runnerFailures.push({ code: "RUST_RESPONSE_COUNT_MISMATCH", expected: supportedCases.length, actual: rawRustLines.length });
  }
  const rustResults = rawRustLines.map((line, index) => {
    try { return JSON.parse(line); }
    catch (error) {
      parserFailures.push({ case: supportedCases[index]?.testCase.name ?? `response-${index}`, message: String(error) });
      return null;
    }
  });
  let matched = 0;
  const mismatches = [];
  for (let index = 0; index < Math.min(supportedCases.length, rustResults.length); index++) {
    const { testCase, ts } = supportedCases[index]; const rust = rustResults[index];
    if (rust === null) continue;
    const comparableTs = { ok: ts.ok, diagnostics: ts.diagnostics, document: ts.document };
    if (isDeepStrictEqual(comparableTs, rust)) matched++;
    else mismatches.push({ case: testCase, typescript: comparableTs, rust, minimization: "full failing input preserved; generated case has bounded depth and size" });
  }
  // On failure only, greedily remove top-level records while preserving the
  // exact mismatch. This deterministic reducer is bounded by the input record
  // count and does not affect successful-run cost or validator semantics.
  for (const mismatch of mismatches) {
    if (mismatch.case.rawLine !== undefined || !mismatch.case.input || typeof mismatch.case.input !== "object") continue;
    const candidate = structuredClone(mismatch.case.input);
    mismatch.originalInput = structuredClone(candidate);
    const isStillDifferent = (value) => {
      const ts = domain.validateComponentInstanceDocument(value);
      const line = spawnSync("cargo", ["run", "--quiet", "--manifest-path", "spikes/fg-c3-2-1/rust-planner/Cargo.toml", "--bin", "component_instance_validate"], {
        cwd: root, input: `${JSON.stringify(value)}\n`, encoding: "utf8", maxBuffer: 16 * 1024 * 1024,
      });
      if (line.status !== 0) return false;
      const rust = JSON.parse(line.stdout.trim());
      return !isDeepStrictEqual({ ok: ts.ok, diagnostics: ts.diagnostics, document: ts.document }, rust);
    };
    let removed = 0;
    for (const group of ["components", "componentSets", "instances"]) {
      const records = candidate[group];
      if (!records || typeof records !== "object" || Array.isArray(records)) continue;
      for (const key of Object.keys(records)) {
        const saved = records[key]; delete records[key];
        if (isStillDifferent(candidate)) removed++;
        else records[key] = saved;
      }
    }
    mismatch.case.input = candidate;
    mismatch.minimization = `deterministic top-level record reduction removed ${removed} record(s); originalInput preserves the initial failure`;
  }
  const observedCodes = [...new Set(supportedCases.flatMap(({ ts }) => ts.diagnostics.map((d) => d.code)))].sort();
  const componentJsonInvalidOutcomes = supportedCases
    .filter(({ ts }) => ts.diagnostics.some(({ code }) => code === "COMPONENT_JSON_INVALID"))
    .map(({ testCase }) => testCase.name);
  semanticMismatches.push(...mismatches);
  const report = {
    generatorVersion: GENERATOR_VERSION,
    seedMatrix: SEED_MATRIX,
    samplesPerSeed: SAMPLES_PER_SEED,
    cases: cases.length,
    compared: supportedCases.length,
    matched,
    unsupported: unsupportedCases.length,
    unsupportedCases,
    unknownClassifications: unknownClassifications.map(({ name, supportClass }) => ({ name, supportClass })),
    runnerFailures,
    parserFailures,
    componentJsonInvalidOutcomes,
    divergent: semanticMismatches.length,
    observedDiagnosticCodes: observedCodes,
    detailPolicy: "exact code/path/severity/detail and semantic JSON payload equality",
  };
  console.log(JSON.stringify(report));
  if (mismatches.length || runnerFailures.length || parserFailures.length || unknownClassifications.length) {
    const failureDir = new URL("../fixtures/figma-semantics/fg-c3-4-1-failures/", import.meta.url);
    await mkdir(failureDir, { recursive: true });
    for (let index = 0; index < mismatches.length; index++) {
      await writeFile(new URL(`${String(index).padStart(3, "0")}-${mismatches[index].case.name.replace(/[^a-zA-Z0-9_-]/g, "_")}.json`, failureDir), `${JSON.stringify(mismatches[index], null, 2)}\n`);
    }
    console.error(JSON.stringify(mismatches, null, 2));
    process.exitCode = 1;
  }
} finally {
  await server.close();
}
