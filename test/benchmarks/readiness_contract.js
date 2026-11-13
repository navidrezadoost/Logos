"use strict";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function isDisposableImportAllowed(env) {
  return env.BENCHMARK_DISPOSABLE_ENVIRONMENT === "true" &&
    UUID.test(env.BENCHMARK_PROJECT_ID || "") &&
    env.BENCHMARK_DISPOSABLE_CONFIRMATION === env.BENCHMARK_PROJECT_ID;
}

function validateLoginResponse({ status, setCookie, body }) {
  if (!Number.isInteger(status) || status < 200 || status >= 300 || typeof setCookie !== "string") return false;
  if (!body || typeof body !== "object" || typeof body.id !== "string" || !body.id || typeof body.email !== "string") return false;
  const pair = setCookie.split(";", 1)[0];
  const equals = pair.indexOf("=");
  return equals > 0 && /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(pair.slice(0, equals)) &&
    pair.slice(equals + 1).length > 0 && !/[\r\n]/.test(pair);
}

function validateRendererEvidence(evidence, expected) {
  if (!evidence || typeof evidence !== "object") return { ok: false, reason: "Renderer evidence adapter is unavailable" };
  const pageCount = Number(evidence.pageCount);
  const pixelDelta = Number(evidence.pixelDelta);
  const canvas = evidence.canvas;
  const ok = evidence.fileId === expected.fileId &&
    Number.isInteger(expected.pageCount) && pageCount === expected.pageCount &&
    canvas?.visible === true && Number(canvas.width) > 0 && Number(canvas.height) > 0 &&
    typeof evidence.baselineSha256 === "string" && /^[a-f0-9]{64}$/i.test(evidence.baselineSha256) &&
    Number.isFinite(pixelDelta) && pixelDelta > 0;
  return { ok, reason: ok ? "Renderer canvas dimensions, fixture identity, page count, and baseline pixel evidence verified" : "Renderer canvas, fixture identity, page count, or baseline pixel evidence did not match" };
}

function validateImportResponse({ ok, status, body }) {
  const id = body?.files?.[0]?.newId;
  return Boolean(ok && status >= 200 && status < 300 && Array.isArray(body?.files) && UUID.test(id || ""));
}

function chromiumCapability(available) {
  return available ? { state: "AVAILABLE" } : { state: "BLOCKED", evidence: "Playwright Chromium is unavailable" };
}

function makeReport(capabilities, capabilityStates, heapCapture = { state: "BLOCKED", evidence: "Workspace verification and Chromium CDP capability have not both been established" }) {
  const infrastructureReady = capabilities.every((name) => capabilityStates[name]?.state === "READY");
  return {
    contract: "logos-memory-benchmark-readiness/v2",
    infrastructure: { state: infrastructureReady ? "READY" : "BLOCKED", capabilities: capabilityStates },
    heap_capture_capability: heapCapture,
    benchmark_execution: { state: "NOT_MEASURED", evidence: "This workflow does not run the memory benchmark or produce performance metrics" },
  };
}

function sanitizeText(value, secrets = []) {
  let text = String(value ?? "").replace(/[\r\n\t]+/g, " ").slice(0, 500);
  for (const secret of secrets.filter((v) => typeof v === "string" && v.length >= 4)) text = text.split(secret).join("[REDACTED]");
  text = text.replace(/(postgres(?:ql)?:\/\/|rediss?:\/\/)[^\s"']+/gi, "$1[REDACTED]");
  return text;
}

function writeReport(writeFile, report, filePath, secrets = []) {
  const safe = JSON.parse(JSON.stringify(report, (_key, value) => typeof value === "string" ? sanitizeText(value, secrets) : value));
  if (filePath) {
    try { writeFile(filePath, `${JSON.stringify(safe, null, 2)}\n`); }
    catch { throw new Error("Readiness report generation failed"); }
  }
  return safe;
}

module.exports = { isDisposableImportAllowed, validateLoginResponse, validateImportResponse, validateRendererEvidence, chromiumCapability, makeReport, sanitizeText, writeReport };
