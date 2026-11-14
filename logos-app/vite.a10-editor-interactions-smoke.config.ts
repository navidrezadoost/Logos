import { mergeConfig } from "vite";
import appConfig from "./vite.config";

export default mergeConfig(appConfig, {
  build: {
    outDir: "build-a10-editor-interactions-smoke",
    emptyOutDir: true,
    rollupOptions: { input: "test-harness/a10-editor-interactions-smoke.html" },
  },
});
