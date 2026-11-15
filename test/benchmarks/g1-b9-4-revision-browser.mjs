import { writeFileSync } from "node:fs";
import { chromium } from "./node_modules/playwright/index.mjs";

const executablePath = process.env.G1_B9_4_CHROME;
const outputPath = process.env.G1_B9_4_EVIDENCE_PATH;
if (!executablePath || !outputPath) throw new Error("G1_B9_4_CHROME and G1_B9_4_EVIDENCE_PATH are required");
const browser = await chromium.launch({ headless: true, executablePath, args: ["--no-sandbox", "--disable-dev-shm-usage"] });
try {
  const page = await browser.newPage();
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.stack ?? error.message));
  await page.goto("http://127.0.0.1:4179/g1-b9-4-revision-smoke.html", { waitUntil: "load" });
  await page.waitForFunction(() => document.querySelector("#result")?.dataset.status !== "running", undefined, { timeout: 120_000 });
  const evidence = await page.locator("#result").innerText();
  writeFileSync(outputPath, evidence);
  const status = await page.locator("#result").getAttribute("data-status");
  if (status !== "passed" || pageErrors.length) throw new Error(JSON.stringify({ status, pageErrors, evidence }, null, 2));
  console.log(`G1-B9.4 Rust/WASM Chromium PASS; evidence=${outputPath}`);
} finally { await browser.close(); }
