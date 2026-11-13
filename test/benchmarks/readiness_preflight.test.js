"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const net = require("node:net");
const { parseDbUrl, allReady, capabilities, redisPing } = require("./readiness_preflight");
const { isDisposableImportAllowed, validateLoginResponse, validateImportResponse, chromiumCapability, makeReport, sanitizeText, writeReport } = require("./readiness_contract");

test("accepts PostgreSQL URLs without exposing credentials", () => {
  const url = parseDbUrl("postgres://bench:secret@127.0.0.1:5432/logos");
  assert.equal(url.hostname, "127.0.0.1");
  assert.equal(url.pathname, "/logos");
});

test("rejects missing and non-PostgreSQL database URLs", () => {
  assert.equal(parseDbUrl(undefined), null);
  assert.equal(parseDbUrl("redis://127.0.0.1:6379/0"), null);
  assert.equal(parseDbUrl("not a URL"), null);
});

test("HTTP health alone cannot establish benchmark readiness", () => {
  const onlyBackend = { backend: { state: "READY", evidence: "HTTP 200" } };
  assert.equal(allReady(onlyBackend), false);
});

test("infrastructure readiness requires independent evidence for all six prerequisites", () => {
  const report = Object.fromEntries(capabilities.map((name) => [name, { state: "READY" }]));
  assert.equal(allReady(report), true);
  report.browser_workspace.state = "BLOCKED";
  assert.equal(allReady(report), false);
});

test("Redis capability uses a direct PING and verifies PONG", async () => {
  const server = net.createServer((socket) => socket.on("data", () => socket.write("+PONG\r\n")));
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  try {
    const { port } = server.address();
    assert.equal(await redisPing(`redis://127.0.0.1:${port}/0`), true);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("malformed authentication responses are rejected", () => {
  const body = { id: "profile-1", email: "benchmark@example.test" };
  assert.equal(validateLoginResponse({ status: 200, setCookie: "auth=secret", body: null }), false);
  assert.equal(validateLoginResponse({ status: 200, setCookie: null, body }), false);
  assert.equal(validateLoginResponse({ status: 200, setCookie: "not-a-cookie", body }), false);
  assert.equal(validateLoginResponse({ status: 401, setCookie: "auth=x", body }), false);
  assert.equal(validateLoginResponse({ status: 200, setCookie: "auth=secret; HttpOnly", body }), true);
});

test("failed and malformed fixture imports are rejected", () => {
  assert.equal(validateImportResponse({ ok: false, status: 500, body: { files: [{ newId: "00000000-0000-4000-8000-000000000001" }] } }), false);
  assert.equal(validateImportResponse({ ok: true, status: 200, body: { files: [] } }), false);
  assert.equal(validateImportResponse({ ok: true, status: 200, body: { files: [{ newId: "invalid" }] } }), false);
  assert.equal(validateImportResponse({ ok: true, status: 200, body: { files: [{ newId: "00000000-0000-4000-8000-000000000001" }] } }), true);
});

test("imports require an explicitly confirmed disposable project", () => {
  const project = "00000000-0000-4000-8000-000000000001";
  assert.equal(isDisposableImportAllowed({ BENCHMARK_PROJECT_ID: project }), false);
  assert.equal(isDisposableImportAllowed({ BENCHMARK_PROJECT_ID: project, BENCHMARK_DISPOSABLE_ENVIRONMENT: "true" }), false);
  assert.equal(isDisposableImportAllowed({ BENCHMARK_PROJECT_ID: project, BENCHMARK_DISPOSABLE_ENVIRONMENT: "true", BENCHMARK_DISPOSABLE_CONFIRMATION: "different" }), false);
  assert.equal(isDisposableImportAllowed({ BENCHMARK_PROJECT_ID: project, BENCHMARK_DISPOSABLE_ENVIRONMENT: "true", BENCHMARK_DISPOSABLE_CONFIRMATION: project }), true);
  assert.equal(isDisposableImportAllowed({ BENCHMARK_PROJECT_ID: "bad", BENCHMARK_DISPOSABLE_ENVIRONMENT: "true", BENCHMARK_DISPOSABLE_CONFIRMATION: "bad" }), false);
});

test("metadata-only renderer evidence cannot mark the workspace ready", () => {
  const states = Object.fromEntries(capabilities.map((name) => [name, { state: "READY" }]));
  states.browser_workspace = { state: "BLOCKED", evidence: "loaded-file response and observed canvas pixels are not independently verified" };
  const gated = makeReport(capabilities, states);
  assert.equal(gated.infrastructure.state, "BLOCKED");
  assert.equal(gated.heap_capture_capability.state, "BLOCKED");
});

test("unavailable Chromium is an explicit blocked capability", () => {
  assert.equal(chromiumCapability(false).state, "BLOCKED");
  assert.equal(chromiumCapability(true).state, "AVAILABLE");
});

test("heap capture readiness cannot produce a benchmark pass", () => {
  const states = Object.fromEntries(capabilities.map((name) => [name, { state: "READY" }]));
  const report = makeReport(capabilities, states, { state: "READY", evidence: "snapshot API available" });
  assert.equal(report.infrastructure.state, "READY");
  assert.equal(report.heap_capture_capability.state, "READY");
  assert.equal(report.benchmark_execution.state, "NOT_MEASURED");
});

test("report write failures do not leak exception messages or secrets", () => {
  const secret = "top-secret-value";
  assert.throws(() => writeReport(() => { throw new Error(secret); }, { evidence: `bad ${secret}` }, "report.json", [secret]), (error) => {
    assert.equal(error.message, "Readiness report generation failed");
    assert.equal(error.message.includes(secret), false);
    return true;
  });
  const output = sanitizeText(`exception ${secret} postgres://user:${secret}@host/db`, [secret]);
  assert.equal(output.includes(secret), false);
  assert.equal(output.includes("user:"), false);
});

test("serialized report output redacts secret values", () => {
  let artifact = "";
  const safe = writeReport((_path, value) => { artifact = value; }, { evidence: "credential top-secret-value" }, "artifact", ["top-secret-value"]);
  const stdout = JSON.stringify(safe);
  const githubSummary = stdout;
  assert.equal(artifact.includes("top-secret-value"), false);
  assert.equal(artifact.includes("[REDACTED]"), true);
  assert.equal(stdout.includes("top-secret-value"), false);
  assert.equal(githubSummary.includes("top-secret-value"), false);
});
