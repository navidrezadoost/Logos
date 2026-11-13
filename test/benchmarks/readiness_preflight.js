#!/usr/bin/env node
"use strict";

// Capability preflight for the memory benchmark. Each row records concrete
// evidence; HTTP 200 by itself never establishes readiness.
const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const net = require("node:net");
const tls = require("node:tls");

const base = (process.env.LOGOS_URL || "http://127.0.0.1:6060").replace(/\/$/, "");
const fixture = path.resolve(process.env.BENCHMARK_FILE || path.join(__dirname, "fixtures/large-canvas.logos"));
const report = {};
let importedFileId = null;
const capabilities = ["postgresql", "redis", "backend", "authentication", "project_import", "browser_workspace", "measurement"];

function set(name, state, evidence) { report[name] = { state, evidence }; }
function allReady(results) { return capabilities.every((name) => results[name]?.state === "READY"); }
function command(bin, args, env = process.env) {
  const r = spawnSync(bin, args, { encoding: "utf8", env, timeout: 8000 });
  return r.error ? { ok: false, error: r.error.message } : { ok: r.status === 0, stdout: r.stdout.trim(), stderr: r.stderr.trim() };
}
function parseDbUrl(value) {
  if (!value) return null;
  try {
    const u = new URL(value);
    if (u.protocol !== "postgres:" && u.protocol !== "postgresql:") return null;
    return u;
  } catch { return null; }
}

async function checkPostgres() {
  const u = parseDbUrl(process.env.DATABASE_URL || process.env.TEST_DATABASE_URL);
  if (!u) return set("postgresql", "BLOCKED", "DATABASE_URL/TEST_DATABASE_URL is missing or invalid");
  const requiredColumns = {
    profile: ["id", "fullname", "email", "password", "lang", "theme", "photo_id", "is_active", "is_blocked", "is_demo", "is_muted", "created_at", "modified_at", "default_team_id", "default_project_id", "props", "deleted_at"],
    team: ["id"],
    team_profile_rel: ["team_id", "profile_id", "is_owner", "is_admin", "can_edit"],
    project: ["id", "team_id", "deleted_at"],
    project_profile_rel: ["project_id", "profile_id", "is_owner", "is_admin", "can_edit"],
    file: ["id", "project_id", "name", "revn", "vern", "is_shared", "modified_at", "created_at", "data"],
    file_profile_rel: ["file_id", "profile_id", "is_owner", "is_admin", "can_edit"],
    file_change: ["id", "file_id", "session_id", "profile_id", "created_at", "updated_at", "deleted_at", "revn", "base_revn", "rebased", "changes"],
    http_session_v2: ["id", "profile_id", "user_agent", "created_at", "modified_at"],
  };
  const values = Object.entries(requiredColumns).flatMap(([table, columns]) => columns.map((column) => `('${table}','${column}')`));
  const sql = `SELECT NOT EXISTS (SELECT 1 FROM (VALUES ${values.join(",")}) AS required(table_name,column_name) LEFT JOIN information_schema.columns c ON c.table_schema='public' AND c.table_name=required.table_name AND c.column_name=required.column_name WHERE c.column_name IS NULL)`;
  const env = { ...process.env };
  if (u.password) env.PGPASSWORD = decodeURIComponent(u.password);
  const r = command("psql", ["-X", "-v", "ON_ERROR_STOP=1", "-At", "-h", u.hostname,
    "-p", u.port || "5432", "-U", decodeURIComponent(u.username), "-d", u.pathname.slice(1), "-c", sql], env);
  if (!r.ok) return set("postgresql", "BLOCKED", "Connection/schema query failed or psql unavailable");
  set("postgresql", r.stdout === "t" ? "READY" : "BLOCKED",
    r.stdout === "t" ? "Connected; required auth/import tables and columns verified" : "Connected, but one or more required application auth/import schema tables or columns are absent");
}

async function checkRedis() {
  const url = process.env.REDIS_URL || process.env.TEST_REDIS_URL;
  if (!url) return set("redis", "BLOCKED", "REDIS_URL/TEST_REDIS_URL is not configured");
  try {
    if (await redisPing(url)) return set("redis", "READY", "Direct Redis protocol PING returned PONG");
  } catch {}
  set("redis", "BLOCKED", "Redis connection/PING failed");
}

function redisCommand(parts) {
  return `*${parts.length}\r\n${parts.map((part) => `$${Buffer.byteLength(part)}\r\n${part}\r\n`).join("")}`;
}

function redisPing(value) {
  let url;
  try { url = new URL(value); } catch { return Promise.reject(new Error("invalid redis URL")); }
  if (!["redis:", "rediss:"].includes(url.protocol) || !url.hostname) return Promise.reject(new Error("unsupported redis URL"));
  const commands = [];
  if (url.password) {
    const password = decodeURIComponent(url.password);
    commands.push(url.username ? ["AUTH", decodeURIComponent(url.username), password] : ["AUTH", password]);
  }
  const database = url.pathname.replace(/^\//, "");
  if (database && database !== "0") commands.push(["SELECT", database]);
  commands.push(["PING"]);
  return new Promise((resolve, reject) => {
    const secure = url.protocol === "rediss:";
    const socket = secure
      ? tls.connect({ host: url.hostname, port: Number(url.port || 6379), servername: url.hostname })
      : net.createConnection({ host: url.hostname, port: Number(url.port || 6379) });
    let response = "";
    let settled = false;
    const finish = (error, result) => {
      if (settled) return;
      settled = true;
      socket.destroy();
      if (error) reject(error);
      else resolve(result);
    };
    socket.setTimeout(5000, () => finish(new Error("redis timeout")));
    socket.once(secure ? "secureConnect" : "connect", () => socket.write(commands.map(redisCommand).join("")));
    socket.on("data", (chunk) => {
      response += chunk.toString("utf8");
      const replies = response.split("\r\n");
      if (replies.length <= commands.length) return;
      const values = replies.slice(0, commands.length);
      if (values.some((reply) => !reply.startsWith("+"))) return finish(new Error("redis command failed"));
      finish(null, values.at(-1) === "+PONG");
    });
    socket.once("error", () => finish(new Error("redis connection failed")));
  });
}

async function checkBackend() {
  try {
    const response = await fetch(`${base}/api/_health`, { signal: AbortSignal.timeout(5000) });
    let body = null;
    try { body = await response.json(); } catch {}
    const dependencies = process.env.BACKEND_DEPENDENCIES_READY === "true";
    const ready = response.ok && body?.status === "ok" && dependencies;
    set("backend", ready ? "READY" : "BLOCKED", ready
      ? "Health payload status=ok and PostgreSQL/Redis startup connectivity independently confirmed"
      : "Requires health payload status=ok plus BACKEND_DEPENDENCIES_READY=true from verified dependency connections");
  } catch {
    set("backend", "BLOCKED", "Backend health request failed");
  }
}

async function login() {
  const email = process.env.LOGOS_EMAIL;
  const password = process.env.LOGOS_PASSWORD;
  if (!email || !password) return { ok: false, reason: "LOGOS_EMAIL and LOGOS_PASSWORD for a provisioned benchmark account are not configured" };
  try {
    const response = await fetch(`${base}/api/rpc/command/login-with-password`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ email, password }), signal: AbortSignal.timeout(8000),
    });
    const cookie = response.headers.get("set-cookie");
    if (!response.ok || !cookie) return { ok: false, reason: `Supported login-with-password contract failed with HTTP ${response.status}` };
    return { ok: true, cookie: cookie.split(";")[0] };
  } catch { return { ok: false, reason: "Supported login endpoint could not be reached" }; }
}

async function checkBrowserAndMeasurement(auth) {
  let chromium;
  try { ({ chromium } = require("playwright")); }
  catch {
    set("browser_workspace", "BLOCKED", "Playwright Chromium is unavailable");
    set("measurement", "BLOCKED", "A supported Chromium runtime is required for CDP heap collection");
    return;
  }
  let browser;
  let stage = "launch";
  try {
    browser = await chromium.launch({ headless: true });
    const context = await browser.newContext();
    if (auth?.ok) {
      const [name, value] = auth.cookie.split("=", 2);
      await context.addCookies([{ name, value, url: base }]);
    }
    const workspaceTemplate = process.env.BENCHMARK_WORKSPACE_URL;
    const expectedPages = Number(process.env.BENCHMARK_EXPECTED_PAGES || "0");
    if (!workspaceTemplate || !workspaceTemplate.includes("{fileId}") || !importedFileId || !expectedPages) {
      set("browser_workspace", "BLOCKED", "Requires BENCHMARK_WORKSPACE_URL containing {fileId}, a verified imported file, and BENCHMARK_EXPECTED_PAGES");
      set("measurement", "BLOCKED", "Workspace fixture is not addressable and verified");
      return;
    }
    const page = await context.newPage();
    const workspaceUrl = workspaceTemplate.replace("{fileId}", encodeURIComponent(importedFileId));
    stage = "workspace navigation and verification";
    await page.goto(workspaceUrl, { waitUntil: "domcontentloaded", timeout: 10000 });
    const pageControls = await page.locator('[data-testid="page-item"]').count();
    const renderedShapes = await page.locator('[data-testid="rendered-shape"]').count();
    const build = await page.locator('meta[name="logos-build-id"]').getAttribute("content").catch(() => null);
    const canvas = page.locator('[data-testid="workspace-canvas"]');
    const workspaceFileId = await canvas.getAttribute("data-file-id").catch(() => null);
    const ready = Boolean(build) && workspaceFileId === importedFileId && pageControls === expectedPages && renderedShapes > 0;
    set("browser_workspace", ready ? "READY" : "BLOCKED", ready
      ? `Editor build ${build}; imported fixture identity, ${pageControls} page controls, and ${renderedShapes} rendered fixture shapes verified`
      : "Requires matching imported file ID, editor build ID, expected page-control count, and visible fixture shapes via supported data-testid markers");
    if (!ready) return set("measurement", "BLOCKED", "Workspace fixture verification failed; heap collection must not be interpreted as a benchmark");
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("HeapProfiler.enable");
    await cdp.send("HeapProfiler.collectGarbage");
    await cdp.send("HeapProfiler.takeHeapSnapshot", { reportProgress: false });
    set("measurement", "READY", "Chromium CDP HeapProfiler snapshot succeeded after workspace verification");
    await cdp.detach();
  } catch {
    if (!report.browser_workspace) set("browser_workspace", "BLOCKED", stage === "launch"
      ? "Chromium could not launch; browser application availability cannot be verified"
      : "Editor URL did not load a verified workspace fixture");
    set("measurement", "BLOCKED", stage === "launch"
      ? "Supported Chromium runtime is unavailable for CDP heap collection"
      : "Chromium/CDP heap collection could not be verified");
  } finally { if (browser) await browser.close(); }
}

async function main() {
  await Promise.all([checkPostgres(), checkRedis(), checkBackend()]);
  const auth = await login();
  set("authentication", auth.ok ? "READY" : "BLOCKED", auth.ok ? "Provisioned test account authenticated through login-with-password" : auth.reason);
  const projectId = process.env.BENCHMARK_PROJECT_ID;
  if (auth.ok && projectId && fs.existsSync(fixture)) {
    try {
      const form = new FormData();
      form.set("project-id", projectId);
      form.set("file", new Blob([fs.readFileSync(fixture)]), path.basename(fixture));
      const response = await fetch(`${base}/api/rpc/command/import-binfile`, {
        method: "POST", headers: { cookie: auth.cookie }, body: form, signal: AbortSignal.timeout(30000),
      });
      const data = await response.json().catch(() => null);
      const imported = response.ok && Array.isArray(data?.files) && data.files.length > 0 && typeof data.files[0].newId === "string" && data.files[0].newId.length > 0;
      if (imported) importedFileId = data.files[0].newId;
      set("project_import", imported ? "READY" : "BLOCKED", imported
        ? "Authenticated import-binfile accepted fixture into the configured destination project"
        : `Authorized fixture import contract failed with HTTP ${response.status}`);
    } catch { set("project_import", "BLOCKED", "Authorized fixture import request failed"); }
  } else {
    set("project_import", "BLOCKED", !auth.ok
      ? "Authentication prerequisite unavailable; import-binfile requires an authenticated session"
      : !projectId ? "BENCHMARK_PROJECT_ID for an editable destination project is not configured" : "Generated .logos fixture is missing");
  }
  await checkBrowserAndMeasurement(auth);
  const ready = allReady(report);
  const output = { contract: "logos-memory-benchmark-readiness/v1", status: ready ? "READY" : "BLOCKED", capabilities: report };
  console.log(JSON.stringify(output, null, 2));
  if (!ready) console.log("Memory measurement not run");
  if (process.env.READINESS_PREFLIGHT_REPORT) fs.writeFileSync(process.env.READINESS_PREFLIGHT_REPORT, `${JSON.stringify(output, null, 2)}\n`);
  process.exitCode = 0; // Blocked is an explicit valid infrastructure outcome.
  if (process.env.GITHUB_OUTPUT) fs.appendFileSync(process.env.GITHUB_OUTPUT, `measurement_ready=${ready}\n`);
}

if (require.main === module) main().catch((error) => {
  console.error(`Readiness preflight failed unexpectedly: ${error.message}`);
  process.exitCode = 1;
});

module.exports = { parseDbUrl, allReady, capabilities, redisPing };
