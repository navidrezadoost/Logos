import { mergeConfig } from "vite";
import appConfig from "./vite.config";

export default mergeConfig(appConfig, {
  build: {
    outDir: "build-hierarchy-renderer-smoke",
    emptyOutDir: true,
    rollupOptions: {
      input: "test-harness/a8-hierarchical-renderer-smoke.html",
    },
  },
});
