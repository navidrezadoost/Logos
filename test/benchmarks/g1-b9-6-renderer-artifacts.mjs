import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import path from "node:path";

export const RENDERER_ASSET_PATHS = Object.freeze(["render-wasm.js", "render-wasm.wasm", "worker/render.js", "shared.js"]);
export const EXPECTED_EMSDK_COMMIT = "24fc909c0da13ef641d5ae75e89b5a97f25e37aa";
export const EXPECTED_EMSCRIPTEN_VERSION = "4.0.6";
export const EXPECTED_RUST_VERSION = "1.93.0";
export const EXPECTED_PNPM_VERSION = "10.28.0";
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function validateSourceIdentity(value) {
  if (typeof value !== "string" || !/^[a-f0-9]{40}:[a-f0-9]{40}$/i.test(value))
    throw new Error("BLOCKED: renderer source identity is invalid");
  return value;
}

export function validateRendererProvenance(record, expectedSourceIdentity) {
  validateSourceIdentity(expectedSourceIdentity);
  if (!record || record.schema !== "logos.g1-b9-6.renderer-provenance/v1" ||
      record.sourceIdentity !== expectedSourceIdentity || record.emsdkCommit !== EXPECTED_EMSDK_COMMIT ||
      record.emscriptenVersion !== EXPECTED_EMSCRIPTEN_VERSION || record.rustVersion !== EXPECTED_RUST_VERSION ||
      record.cargoVersion !== EXPECTED_RUST_VERSION || record.pnpmVersion !== EXPECTED_PNPM_VERSION ||
      !/^[0-9]+\.[0-9]+\.[0-9]+/.test(record.cmakeVersion ?? "") ||
      !/^v[0-9]+\.[0-9]+\.[0-9]+/.test(record.nodeVersion ?? "") ||
      !Array.isArray(record.assets) || record.assets.length !== RENDERER_ASSET_PATHS.length)
    throw new Error("BLOCKED: renderer provenance is missing, stale, or mismatched");
  const seen = new Set();
  for (const asset of record.assets) {
    if (!asset || !RENDERER_ASSET_PATHS.includes(asset.path) || seen.has(asset.path) ||
        !Number.isSafeInteger(asset.bytes) || asset.bytes <= 0 || !/^[a-f0-9]{64}$/i.test(asset.sha256 ?? ""))
      throw new Error("BLOCKED: renderer asset provenance is malformed or duplicated");
    seen.add(asset.path);
  }
  if (RENDERER_ASSET_PATHS.some((assetPath) => !seen.has(assetPath)))
    throw new Error("BLOCKED: renderer provenance omits a required artifact");
  return record;
}

export async function createRendererProvenance(assetDirectory, sourceIdentity, toolchain) {
  validateSourceIdentity(sourceIdentity);
  if (!toolchain || toolchain.emsdkCommit !== EXPECTED_EMSDK_COMMIT ||
      toolchain.emscriptenVersion !== EXPECTED_EMSCRIPTEN_VERSION || toolchain.rustVersion !== EXPECTED_RUST_VERSION ||
      toolchain.cargoVersion !== EXPECTED_RUST_VERSION || toolchain.pnpmVersion !== EXPECTED_PNPM_VERSION ||
      !/^[0-9]+\.[0-9]+\.[0-9]+/.test(toolchain.cmakeVersion ?? "") ||
      !/^v[0-9]+\.[0-9]+\.[0-9]+/.test(toolchain.nodeVersion ?? ""))
    throw new Error("BLOCKED: renderer build toolchain does not match pinned versions");
  const assets = [];
  for (const relativePath of RENDERER_ASSET_PATHS) {
    let bytes;
    try { bytes = await readFile(path.join(assetDirectory, relativePath)); }
    catch { throw new Error("BLOCKED: required renderer artifact is missing: " + relativePath); }
    if (!bytes.length) throw new Error("BLOCKED: required renderer artifact is empty: " + relativePath);
    if (relativePath === "render-wasm.wasm" && !bytes.subarray(0, 4).equals(Buffer.from([0, 97, 115, 109])))
      throw new Error("BLOCKED: renderer WebAssembly artifact has invalid magic bytes");
    if (relativePath === "render-wasm.js" && !bytes.toString("utf8").includes("render-wasm.wasm?version="))
      throw new Error("BLOCKED: renderer loader does not reference the packaged WASM artifact");
    assets.push({ path: relativePath, bytes: bytes.length, sha256: sha256(bytes) });
  }
  return { schema: "logos.g1-b9-6.renderer-provenance/v1", sourceIdentity,
    emsdkCommit: toolchain.emsdkCommit, emscriptenVersion: toolchain.emscriptenVersion,
    rustVersion: toolchain.rustVersion, cargoVersion: toolchain.cargoVersion, pnpmVersion: toolchain.pnpmVersion,
    cmakeVersion: toolchain.cmakeVersion, nodeVersion: toolchain.nodeVersion, assets };
}

export async function verifyRendererAssets(assetDirectory, expectedSourceIdentity, record = null) {
  let provenance = record;
  if (!provenance) {
    try { provenance = JSON.parse(await readFile(path.join(assetDirectory, "render-wasm-provenance.json"), "utf8")); }
    catch { throw new Error("BLOCKED: renderer provenance file is missing or malformed"); }
  }
  validateRendererProvenance(provenance, expectedSourceIdentity);
  for (const asset of provenance.assets) {
    let bytes;
    try { bytes = await readFile(path.join(assetDirectory, asset.path)); }
    catch { throw new Error("BLOCKED: required renderer artifact is missing: " + asset.path); }
    if (!bytes.length || bytes.length !== asset.bytes || sha256(bytes) !== asset.sha256)
      throw new Error("BLOCKED: renderer artifact checksum or size mismatch: " + asset.path);
    if (asset.path === "render-wasm.wasm" && !bytes.subarray(0, 4).equals(Buffer.from([0, 97, 115, 109])))
      throw new Error("BLOCKED: renderer WebAssembly artifact has invalid magic bytes");
  }
  return provenance;
}

export function validateBuildProvenanceRenderer(buildRecord) {
  const renderer = buildRecord?.renderer;
  if (!renderer || renderer.schema !== "logos.g1-b9-6.renderer-provenance/v1" ||
      renderer.sourceIdentity !== buildRecord.sourceIdentity || !Array.isArray(renderer.assets) ||
      !Array.isArray(buildRecord.buildArtifacts))
    throw new Error("BLOCKED: source-bound preview does not identify verified renderer artifacts");
  validateRendererProvenance(renderer, buildRecord.sourceIdentity);
  const built = new Map(buildRecord.buildArtifacts.map((entry) => [entry.path, entry]));
  for (const asset of renderer.assets) {
    const output = built.get("js/" + asset.path);
    if (!output || output.bytes !== asset.bytes || output.sha256 !== asset.sha256)
      throw new Error("BLOCKED: source-bound preview renderer bytes do not match renderer provenance");
  }
  return renderer;
}

export function assertWasmRuntimeEvidence(evidence) {
  const pixels = evidence?.framebufferPixels;
  const validPixels = Array.isArray(pixels) && pixels.length > 0 && pixels.every((rgba) =>
    Array.isArray(rgba) && rgba.length === 4 && rgba.every((channel) => Number.isInteger(channel) && channel >= 0 && channel <= 255));
  if (evidence?.status !== "READY" || evidence?.backend !== "wasm" ||
      evidence?.operationSucceeded !== true || !validPixels)
    throw new Error("BLOCKED: actual READY/WASM operation and framebuffer evidence is required");
  return true;
}

export async function verifyPreviewEndpoints(previewUrl, expectedBuild) {
  const url = new URL(previewUrl);
  if (url.protocol !== "http:" || !["127.0.0.1", "localhost", "[::1]", "::1"].includes(url.hostname) ||
      url.username || url.password || url.pathname !== "/g1-b9-5-editor-performance.html" || url.search || url.hash)
    throw new Error("BLOCKED: renderer endpoint preflight accepts only credential-free loopback HTTP");
  const provenanceResponse = await fetch(new URL("/g1-b9-6-build-provenance.json", url));
  if (provenanceResponse.status !== 200 || new URL(provenanceResponse.url).origin !== url.origin)
    throw new Error("BLOCKED: source-bound preview provenance endpoint failed or redirected");
  const buildRecord = await provenanceResponse.json();
  if (JSON.stringify(buildRecord) !== JSON.stringify(expectedBuild))
    throw new Error("BLOCKED: served preview provenance differs from the local build record");
  const renderer = validateBuildProvenanceRenderer(buildRecord);
  const responses = [];
  const expectedFiles = new Map(buildRecord.buildArtifacts.map((entry) => [entry.path, entry]));
  const entryArtifact = expectedFiles.get(buildRecord.entry);
  if (buildRecord.entry !== "g1-b9-5-editor-performance.html" || !entryArtifact)
    throw new Error("BLOCKED: expected Canvas smoke entry is absent from source-bound build provenance");
  const requestedFiles = [{ endpoint: "/" + buildRecord.entry, artifact: entryArtifact },
    ...renderer.assets.map((asset) => ({ endpoint: "/js/" + asset.path.split("/").map(encodeURIComponent).join("/"),
      artifact: asset, buildPath: "js/" + asset.path }))];
  for (const item of requestedFiles) {
    const expected = item.buildPath ? expectedFiles.get(item.buildPath) : item.artifact;
    if (!expected || expected.bytes !== item.artifact.bytes || expected.sha256 !== item.artifact.sha256)
      throw new Error("BLOCKED: expected Canvas or renderer endpoint is absent from the source-bound build");
    const endpoint = item.endpoint;
    const response = await fetch(new URL(endpoint, url));
    if (response.status !== 200 || new URL(response.url).origin !== url.origin)
      throw new Error("BLOCKED: renderer endpoint returned an unexpected status or redirected: " + endpoint);
    const body = Buffer.from(await response.arrayBuffer());
    if (body.length !== expected.bytes || sha256(body) !== expected.sha256)
      throw new Error("BLOCKED: served renderer endpoint checksum mismatch: " + endpoint);
    if (item.artifact.path === "render-wasm.wasm" && !body.subarray(0, 4).equals(Buffer.from([0, 97, 115, 109])))
      throw new Error("BLOCKED: served renderer WASM endpoint has invalid magic bytes");
    responses.push({ path: endpoint, status: response.status, bytes: body.length, sha256: sha256(body) });
  }
  return { status: "ENDPOINTS_VERIFIED_NOT_INITIALIZED", sourceIdentity: buildRecord.sourceIdentity, responses };
}

export async function writeRendererProvenance(assetDirectory, emsdkCommit, emscriptenVersion, rustVersion,
  cargoVersion, pnpmVersion, cmakeVersion, nodeVersion, repoRoot) {
  const commitSha = execFileSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" }).trim();
  const treeSha = execFileSync("git", ["rev-parse", "HEAD^{tree}"], { cwd: repoRoot, encoding: "utf8" }).trim();
  const status = execFileSync("git", ["status", "--porcelain", "--untracked-files=all"], { cwd: repoRoot, encoding: "utf8" });
  if (status.trim()) throw new Error("BLOCKED: source worktree must remain clean while recording renderer provenance");
  const record = await createRendererProvenance(assetDirectory, commitSha + ":" + treeSha,
    { emsdkCommit, emscriptenVersion, rustVersion, cargoVersion, pnpmVersion, cmakeVersion, nodeVersion });
  await writeFile(path.join(assetDirectory, "render-wasm-provenance.json"), JSON.stringify(record, null, 2) + "\n",
    { encoding: "utf8", mode: 0o644, flag: "wx" });
  return record;
}
