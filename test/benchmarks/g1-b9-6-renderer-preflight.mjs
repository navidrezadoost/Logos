import { readFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { verifyPreviewEndpoints, verifyRendererAssets, writeRendererProvenance } from "./g1-b9-6-renderer-artifacts.mjs";
import { assertCleanSourceStatus } from "./g1-b9-6-memory-evidence.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

async function main(args) {
  const [command, ...values] = args;
  if (command === "write-provenance") {
    if (values.length !== 8) throw new Error("Usage: write-provenance <asset-directory> <emsdk-commit> <emscripten-version> <rust-version> <cargo-version> <pnpm-version> <cmake-version> <node-version>");
    const record = await writeRendererProvenance(path.resolve(values[0]), values[1], values[2], values[3],
      values[4], values[5], values[6], values[7], repoRoot);
    process.stdout.write("RENDERER_PROVENANCE_WRITTEN " + JSON.stringify(record) + "\n");
    return;
  }
  if (command === "verify-assets") {
    if (values.length !== 2) throw new Error("Usage: verify-assets <asset-directory> <commit:tree-source-identity>");
    const record = await verifyRendererAssets(path.resolve(values[0]), values[1]);
    process.stdout.write("RENDERER_ASSETS_VERIFIED " + JSON.stringify(record) + "\n");
    return;
  }
  if (command === "verify-preview") {
    if (values.length !== 2) throw new Error("Usage: verify-preview <loopback-preview-url> <build-directory>");
    const localBuild = JSON.parse(await readFile(path.join(path.resolve(values[1]), "g1-b9-6-build-provenance.json"), "utf8"));
    assertCleanSourceStatus(execFileSync("git", ["status", "--porcelain", "--untracked-files=all"], { cwd: repoRoot, encoding: "utf8" }));
    const commitSha = execFileSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" }).trim();
    const treeSha = execFileSync("git", ["rev-parse", "HEAD^{tree}"], { cwd: repoRoot, encoding: "utf8" }).trim();
    if (localBuild.commitSha !== commitSha || localBuild.treeSha !== treeSha ||
        localBuild.sourceIdentity !== commitSha + ":" + treeSha)
      throw new Error("BLOCKED: local preview build provenance is stale for the current clean source revision");
    const result = await verifyPreviewEndpoints(values[0], localBuild);
    process.stdout.write(JSON.stringify(result, null, 2) + "\n");
    return;
  }
  throw new Error("Usage: <write-provenance|verify-assets|verify-preview> ...");
}

main(process.argv.slice(2)).catch((error) => {
  const message = error instanceof Error ? error.message : "BLOCKED: unknown renderer preflight failure";
  process.stderr.write((message.startsWith("BLOCKED:") ? "" : "BLOCKED: ") + message + "\n");
  process.exitCode = 1;
});
