import { mergeConfig } from "vite";
import appConfig from "./vite.config";

export default mergeConfig(appConfig, {
  define: {
    // Force the production loader down its normal, safe Canvas 2D fallback path.
    __RENDER_WASM_JS__: JSON.stringify("/__a7-render-wasm-not-installed.js"),
    __RENDER_WASM_WASM__: JSON.stringify("/__a7-render-wasm-not-installed.wasm"),
  },
  build: {
    outDir: "build-renderer-compatibility-smoke",
    emptyOutDir: true,
    rollupOptions: {
      input: "test-harness/a7-renderer-compatibility-smoke.html",
    },
  },
});
