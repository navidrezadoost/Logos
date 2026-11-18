import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";

const require = createRequire(import.meta.url);
const wasmModulePath = process.env.FG_C3_6_WASM_NODE_MODULE;
const rustPlannerPath = process.env.FG_C3_6_RUST_PLANNER;
if (!wasmModulePath || !rustPlannerPath) {
  throw new Error("Set FG_C3_6_WASM_NODE_MODULE and FG_C3_6_RUST_PLANNER to the generated Node WASM module and native planner executable.");
}
const wasm = require(wasmModulePath);
const fixture = JSON.parse(await readFile(new URL("../fixtures/figma-semantics/component-instance-domain-v1.json", import.meta.url), "utf8"));
const clean = structuredClone(fixture);
clean.components.card.properties["content-slot"].guidance.maximumEntries = 2;
delete clean.instances["external-instance"];
delete clean.instances["unresolved-instance"];

const requests = [
  { requestId: "wasm-clean", documentJson: JSON.stringify(clean) },
  { requestId: "wasm-warning", documentJson: JSON.stringify(fixture) },
  { requestId: "wasm-duplicate", documentJson: String.raw`{"version":1,"nested":{"property":1,"pro\u0070erty":2},"components":{},"componentSets":{},"instances":{}}` },
  { requestId: "wasm-depth", documentJson: `${"[".repeat(65)}0${"]".repeat(65)}` },
].map(({ requestId, documentJson }) => ({
  protocolVersion: 2,
  semanticVersion: 1,
  diagnosticVersion: 1,
  requestId,
  kind: "validateComponentInstanceV1",
  componentSemanticVersion: 1,
  documentJson,
}));

for (const request of requests) {
  const raw = JSON.stringify(request);
  const body = Buffer.from(raw, "utf8");
  const header = Buffer.alloc(4);
  header.writeUInt32BE(body.length);
  const nativeRun = spawnSync(rustPlannerPath, { input: Buffer.concat([header, body]) });
  assert.equal(nativeRun.status, 0, `${request.requestId}: native planner exited ${nativeRun.status}`);
  assert.equal(nativeRun.stderr.length, 0, `${request.requestId}: native planner wrote stderr`);
  const responseLength = nativeRun.stdout.readUInt32BE(0);
  const native = JSON.parse(nativeRun.stdout.subarray(4, 4 + responseLength).toString("utf8"));
  const wasmResult = JSON.parse(wasm.evaluate_request_json(raw));
  assert.deepStrictEqual(wasmResult, native, `${request.requestId}: Node-hosted WASM differs from native Rust`);
}

console.log(JSON.stringify({ nativeWasmCompared: requests.length, matched: requests.length, divergent: 0 }));
