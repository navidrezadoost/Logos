import { writeFileSync } from "node:fs";
import { chromium } from "./node_modules/playwright/index.mjs";

const executablePath = process.env.G1_B3_CHROME;
const outputPath = process.env.G1_B3_EVIDENCE_PATH;
if (!executablePath || !outputPath) throw new Error("G1_B3_CHROME and G1_B3_EVIDENCE_PATH are required");

const browser = await chromium.launch({
  headless: true,
  executablePath,
  args: ["--no-sandbox", "--disable-dev-shm-usage", "--enable-precise-memory-info"],
});
try {
  const page = await browser.newPage();
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.stack ?? error.message));
  await page.goto("http://127.0.0.1:4177/g1-b3-adapter-smoke.html", { waitUntil: "load" });
  await page.waitForFunction(
    () => document.querySelector("#result")?.getAttribute("data-status") !== "pending",
    undefined,
    { timeout: 120_000 },
  );
  const html = await page.content();
  const status = await page.locator("#result").getAttribute("data-status");
  const evidence = await page.locator("#result").innerText();
  writeFileSync(outputPath, `${html}\n<!-- G1-B3 evidence JSON:\n${evidence}\n-->\n`);
  if (status !== "passed" || pageErrors.length > 0) {
    throw new Error(JSON.stringify({ status, pageErrors, evidence }, null, 2));
  }
  console.log(`G1-B3 Chromium smoke PASS; evidence=${outputPath}`);
} finally {
  await browser.close();
}
