const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const [modulePath, goRunner, fixtureDir] = process.argv.slice(2);
if (!modulePath || !goRunner || !fixtureDir) throw new Error("usage: compare-wasm.cjs WASM_MODULE GO_RUNNER FIXTURE_DIR");
const wasm = require(path.resolve(modulePath));
const cases = ["reorder.json", "cycle-reject.json", "detach-ids.json", "detach-collision.json"]
  .map((name) => JSON.parse(fs.readFileSync(path.join(fixtureDir, name), "utf8")));
const identityCases = JSON.parse(fs.readFileSync(path.join(fixtureDir, "projection-identities.json"), "utf8"));
cases.push(...identityCases.map((item) => item.request));
cases.push({ protocolVersion: 2, kind: "deriveIds", addresses: [] });

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
  return value;
}

for (const [index, request] of cases.entries()) {
  const requestText = JSON.stringify(request);
  const go = spawnSync(path.resolve(goRunner), [], {
    input: requestText,
    encoding: "utf8",
    env: { ...process.env, FG_C3_2_1_RUST_PLANNER: process.env.FG_C3_2_1_RUST_PLANNER },
  });
  assert.equal(go.status, 0, `Go→Rust invocation ${index} failed: ${go.stderr}`);
  const goResult = JSON.parse(go.stdout);
  const wasmResult = JSON.parse(wasm.evaluate_request_json(requestText));
  assert.deepEqual(canonical(goResult), canonical(wasmResult), `native Go→Rust vs WASM mismatch for case ${index}`);
}
console.log(`WASM/native Rust semantic parity: ${cases.length}/${cases.length} fixture requests equivalent (Node-hosted WASM; no browser claim).`);
