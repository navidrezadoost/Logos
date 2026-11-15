import { writeFile } from "node:fs/promises";
import { chromium } from "playwright";

const url = process.argv[2] ?? "http://127.0.0.1:4176/test-harness/dt-c3-token-workflow.html";
const browser = await chromium.launch({
  headless: true,
  ...(process.env.CHROME_BIN ? { executablePath: process.env.CHROME_BIN } : {}),
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

try {
  const page = await browser.newPage({ viewport: { width: 800, height: 500 }, deviceScaleFactor: 1 });
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.goto(url, { waitUntil: "networkidle" });
  await page.waitForFunction(() => {
    const status = document.querySelector("#dt-c3-status")?.getAttribute("data-status");
    return status === "passed" || status === "failed";
  }, null, { timeout: 90000 });

  const html = await page.content();
  const text = await page.locator("#dt-c3-status").innerText();
  const status = await page.locator("#dt-c3-status").getAttribute("data-status");
  const report = JSON.parse(text);
  const evidence = {
    status,
    url,
    browser: await page.evaluate(() => navigator.userAgent),
    rendererBackend: report.rendererBackend ?? "unknown",
    report,
    pageErrors,
  };
  if (process.env.DT_C3_EVIDENCE_PATH) await writeFile(process.env.DT_C3_EVIDENCE_PATH, `${JSON.stringify(evidence, null, 2)}\n`);
  if (process.env.DT_C3_HTML_PATH) await writeFile(process.env.DT_C3_HTML_PATH, `${html}\n`);
  process.stdout.write(`${JSON.stringify(evidence, null, 2)}\n`);

  if (status !== "passed" || pageErrors.length > 0) process.exitCode = 1;
} finally {
  await browser.close();
}
