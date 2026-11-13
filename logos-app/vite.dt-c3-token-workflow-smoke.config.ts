import { mergeConfig } from "vite";
import appConfig from "./vite.config";

export default mergeConfig(appConfig, {
  define: {
    __RENDER_WASM_JS__: JSON.stringify("/__dt-c3-renderer-not-installed.js"),
    __RENDER_WASM_WASM__: JSON.stringify("/__dt-c3-renderer-not-installed.wasm"),
  },
  build: {
    outDir: "build-dt-c3-token-workflow-smoke",
    emptyOutDir: true,
    rollupOptions: { input: "test-harness/dt-c3-token-workflow.html" },
  },
});
