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

module.exports = { isDisposableImportAllowed, validateLoginResponse, validateImportResponse, chromiumCapability, makeReport, sanitizeText, writeReport };
