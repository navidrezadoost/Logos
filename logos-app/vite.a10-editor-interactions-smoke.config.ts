import { mergeConfig } from "vite";
import appConfig from "./vite.config";

export default mergeConfig(appConfig, {
  define: { __LOGOS_G1_B9_SHADOW_BUILD__: "true" },
  build: {
    outDir: "build-a10-editor-interactions-smoke",
    emptyOutDir: true,
    rollupOptions: { input: "test-harness/a10-editor-interactions-smoke.html" },
  },
});
