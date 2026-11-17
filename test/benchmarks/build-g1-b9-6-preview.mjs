import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { createReadStream } from "node:fs";
import { mkdir, readdir, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { assertCleanSourceStatus } from "./g1-b9-6-memory-evidence.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const appRoot = path.join(repoRoot, "logos-app");
const outputRoot = path.join(appRoot, "build-g1-b9-6-memory");

function git(args) {
  return spawnSync("git", args, { cwd: repoRoot, encoding: "utf8" });
}

async function sha256File(filePath) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filePath)) hash.update(chunk);
  return hash.digest("hex");
}

async function listBuildFiles(directory, parent = "") {
  const records = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const relative = parent ? `${parent}/${entry.name}` : entry.name;
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) records.push(...await listBuildFiles(absolute, relative));
    else if (entry.isFile()) {
      const details = await stat(absolute);
      records.push({ path: relative, bytes: details.size, sha256: await sha256File(absolute) });
    } else throw new Error(`Unsupported build output entry: ${relative}`);
  }
  return records;
}

async function main() {
  const status = git(["status", "--porcelain", "--untracked-files=all"]);
  if (status.status !== 0) throw new Error("Unable to inspect Git source state");
  assertCleanSourceStatus(status.stdout);
  const commit = git(["rev-parse", "HEAD"]);
  const tree = git(["rev-parse", "HEAD^{tree}"]);
  if (commit.status !== 0 || tree.status !== 0) throw new Error("Unable to identify Git source revision");
  const commitSha = commit.stdout.trim();
  const treeSha = tree.stdout.trim();
  const sourceIdentity = `${commitSha}:${treeSha}`;
  const vitePath = path.join(appRoot, "node_modules", "vite", "bin", "vite.js");
  const build = spawnSync(process.execPath, [vitePath, "build", "--config", "vite.g1-b9-6-memory.config.ts"], {
    cwd: appRoot, stdio: "inherit", env: { ...process.env, LOGOS_MEMORY_SOURCE_ID: sourceIdentity },
  });
  if (build.error || build.status !== 0) throw new Error("BLOCKED: source-bound Canvas smoke build failed");
  const htmlPath = path.join(outputRoot, "g1-b9-5-editor-performance.html");
  if (!(await stat(htmlPath).catch(() => null))?.isFile()) throw new Error("BLOCKED: expected Canvas smoke HTML missing from build");
  const buildArtifacts = await listBuildFiles(outputRoot);
  if (!buildArtifacts.some((item) => item.path === "g1-b9-5-editor-performance.html"))
    throw new Error("BLOCKED: smoke HTML is not part of the verified build artifact set");
  const provenance = { schema: "logos.g1-b9-6.build-provenance/v1", commitSha, treeSha, sourceIdentity,
    entry: "g1-b9-5-editor-performance.html", buildArtifacts };
  await mkdir(outputRoot, { recursive: true });
  await writeFile(path.join(outputRoot, "g1-b9-6-build-provenance.json"), `${JSON.stringify(provenance, null, 2)}\n`,
    { mode: 0o644, flag: "wx" });
  process.stdout.write(`BUILT ${sourceIdentity}\nServe with: cd logos-app && ./node_modules/.bin/vite preview --host 127.0.0.1 --port 4173 --strictPort --outDir build-g1-b9-6-memory\n`);
}

main().catch((error) => {
  const message = error instanceof Error ? error.message : "BLOCKED: unknown preview build failure";
  process.stderr.write(`${message.startsWith("BLOCKED:") ? "" : "BLOCKED: "}${message}\n`);
  process.exitCode = 1;
});
