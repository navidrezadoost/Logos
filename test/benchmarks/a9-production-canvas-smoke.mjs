import { writeFile } from "node:fs/promises";
import { chromium } from "playwright";

const [url, outputPath] = process.argv.slice(2);
const executablePath = process.env.CHROME_BIN;
if (!url || !outputPath || !executablePath) {
  throw new Error("Usage: CHROME_BIN=/path/to/chrome node a9-production-canvas-smoke.mjs <url> <dom-output>");
}

const browser = await chromium.launch({
  executablePath,
  headless: true,
  args: [
    "--no-sandbox",
    "--disable-dev-shm-usage",
    "--enable-webgl",
    "--disable-features=WebGPU",
    "--ignore-gpu-blocklist",
    "--use-gl=angle",
    "--use-angle=swiftshader",
    "--enable-unsafe-swiftshader",
  ],
});

let pageErrors = [];
try {
  const page = await browser.newPage();
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.goto(url, { waitUntil: "load", timeout: 30_000 });
  await page.waitForFunction(() => {
    const status = document.querySelector("#a9-status")?.dataset.status;
    return status === "passed" || status === "failed";
  }, undefined, { timeout: 90_000 });

  const dom = await page.content();
  await writeFile(outputPath, dom);
  const result = await page.locator("#a9-status").evaluate((element) => ({
    status: element.dataset.status,
    report: JSON.parse(element.textContent || "{}"),
  }));

  console.log(`A9 production Canvas browser status: ${result.status}`);
  console.log(JSON.stringify(result.report, null, 2));
  if (pageErrors.length > 0) {
    console.error(`Browser page errors: ${JSON.stringify(pageErrors)}`);
    process.exitCode = 1;
  }
  if (result.status !== "passed") process.exitCode = 1;

  const measurements = result.report.environment?.measurements ?? [];
  const timingFields = [
    "projectionValidationMs",
    "wasmUploadMs",
    "nativeRendererMs",
    "synchronousMainThreadMs",
    "browserFrameLatencyMs",
  ];
  for (const field of timingFields) {
    if (measurements.some((sample) => !Number.isFinite(sample[field]) || sample[field] < 0)) {
      console.error(`Invalid A9 timing sample for ${field}`);
      process.exitCode = 1;
    }
  }
  if (!measurements.some((sample) => sample.browserFrameLatencyMs > 0)) {
    console.error("Browser frame timing clock did not advance; performance observations are unusable.");
    process.exitCode = 1;
  }
} finally {
  await browser.close();
}
