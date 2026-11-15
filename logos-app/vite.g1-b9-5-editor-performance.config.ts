import { mergeConfig } from "vite";
import appConfig from "./vite.config";

export default mergeConfig(appConfig, {
  define: { __LOGOS_G1_B9_5_PERF_BUILD__: "true" },
  build: {
    outDir: "build-g1-b9-5-editor-performance",
    emptyOutDir: true,
    rollupOptions: { input: "g1-b9-5-editor-performance.html" },
  },
});
