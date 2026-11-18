const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const [modulePath, goRunner, rustPlanner, fixtureDir] = process.argv.slice(2);
if (!modulePath || !goRunner || !rustPlanner || !fixtureDir) throw new Error("usage: compare-wasm.cjs WASM_MODULE GO_RUNNER RUST_PLANNER FIXTURE_DIR");
const wasm = require(path.resolve(modulePath));
const cases = ["reorder.json", "cycle-reject.json", "detach-ids.json", "detach-collision.json"]
  .map((name) => JSON.parse(fs.readFileSync(path.join(fixtureDir, name), "utf8")));
const identityCases = JSON.parse(fs.readFileSync(path.join(fixtureDir, "projection-identities.json"), "utf8"));
cases.push(...identityCases.map((item) => item.request));
cases.push({ protocolVersion: 2, semanticVersion: 1, diagnosticVersion: 1, requestId: "capability-parity", kind: "capabilities" });
cases.push({ protocolVersion: 9, semanticVersion: 1, diagnosticVersion: 1, requestId: "unsupported-protocol", kind: "capabilities" });

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
  return value;
}

function frame(value) {
  const body = Buffer.from(JSON.stringify(value));
  const header = Buffer.alloc(4);
  header.writeUInt32BE(body.length);
  return Buffer.concat([header, body]);
}

const go = spawnSync(path.resolve(goRunner), [], {
  input: Buffer.concat(cases.map(frame)),
  encoding: null,
  env: { ...process.env, FG_C3_2_2_RUST_PLANNER: path.resolve(rustPlanner) },
});
assert.equal(go.status, 0, `persistent Go coordinator failed: ${go.stderr?.toString()}`);
let offset = 0;
const goResults = [];
while (offset < go.stdout.length) {
  assert.ok(offset + 4 <= go.stdout.length, "partial Go response header");
  const length = go.stdout.readUInt32BE(offset);
  offset += 4;
  assert.ok(offset + length <= go.stdout.length, "partial Go response body");
  goResults.push(JSON.parse(go.stdout.subarray(offset, offset + length).toString("utf8")));
  offset += length;
}
assert.equal(goResults.length, cases.length, "persistent Go coordinator response count");

for (const [index, request] of cases.entries()) {
  const requestText = JSON.stringify(request);
  const goResult = goResults[index];
  const wasmResult = JSON.parse(wasm.evaluate_request_json(requestText));
  assert.deepEqual(canonical(goResult), canonical(wasmResult), `native Go→Rust vs WASM mismatch for case ${index}`);
}
console.log(`Persistent Go/native Rust vs Node-hosted WASM parity: ${cases.length}/${cases.length} fixture requests equivalent (no browser claim).`);
