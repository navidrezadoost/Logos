/**
 * Hello World — sample Logos plugin.
 *
 * This file runs inside a sandboxed <iframe>. It has access only to the
 * `logos` global injected by the bridge bootstrap.
 *
 * To compile this for use with the bridge (which fetches raw JS):
 *   npx esbuild src/plugins/sample/hello-world.ts --bundle --outfile=public/plugins/hello-world.js --platform=browser
 *
 * Then load it in the app:
 *   import { connectPlugin } from "./plugins/bridge";
 *   const handle = await connectPlugin("/plugins/hello-world.js", ["read", "content"]);
 */

// The `logos` global is injected by the bridge bootstrap before this script runs.
declare const logos: {
  call<T = unknown>(method: string, params?: unknown): Promise<T>;
  on(event: string, fn: (payload: unknown) => void): void;
};

// ─── Initial run: log selection ──────────────────────────────────────────────

async function logSelection(): Promise<void> {
  const selection = await logos.call<{ id: string; type: string; name: string; x: number; y: number; width: number; height: number }[]>(
    "getSelection"
  );

  if (selection.length === 0) {
    console.log("[hello-world] No shapes selected.");
    return;
  }

  console.log(`[hello-world] ${selection.length} shape(s) selected:`);
  for (const shape of selection) {
    console.log(
      `  • ${shape.name} (${shape.type}) — id=${shape.id}  pos=(${shape.x}, ${shape.y})  size=${shape.width}×${shape.height}`
    );
  }
}

logSelection().catch((err) => {
  console.error("[hello-world] Error:", err.message);
});

// ─── React to future selection changes ───────────────────────────────────────

logos.on("selectionChange", (payload) => {
  console.log("[hello-world] Selection changed:", payload);
  logSelection().catch(console.error);
});

// ─── Demo: create a greeting rect after 1 second ─────────────────────────────

setTimeout(async () => {
  try {
    const id = await logos.call<string>("createRect", {
      x: 50,
      y: 50,
      width: 200,
      height: 80,
      name: "Hello from plugin!",
    });
    console.log(`[hello-world] Created greeting rect, id=${id}`);

    // Read it back to verify
    const shape = await logos.call("getShape", { id });
    console.log("[hello-world] Shape read-back:", shape);
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    console.error("[hello-world] Failed to create rect:", msg);
  }
}, 1000);
