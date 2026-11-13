"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { parseDbUrl, allReady, capabilities } = require("./readiness_preflight");

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

test("readiness requires independent evidence for all seven capabilities", () => {
  const report = Object.fromEntries(capabilities.map((name) => [name, { state: "READY" }]));
  assert.equal(allReady(report), true);
  report.browser_workspace.state = "BLOCKED";
  assert.equal(allReady(report), false);
});
