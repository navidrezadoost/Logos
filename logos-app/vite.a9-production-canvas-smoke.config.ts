import { mergeConfig } from "vite";
import appConfig from "./vite.config";

export default mergeConfig(appConfig, {
  build: {
    outDir: "build-a9-production-canvas-smoke",
    emptyOutDir: true,
    rollupOptions: { input: "test-harness/a9-production-canvas-smoke.html" },
  },
});
