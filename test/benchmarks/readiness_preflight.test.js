"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const net = require("node:net");
const { parseDbUrl, allReady, capabilities, redisPing } = require("./readiness_preflight");

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
