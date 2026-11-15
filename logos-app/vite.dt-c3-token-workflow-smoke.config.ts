import { mergeConfig } from "vite";
import appConfig from "./vite.config";

export default mergeConfig(appConfig, {
  build: {
    outDir: "build-dt-c3-token-workflow-smoke",
    emptyOutDir: true,
    rollupOptions: { input: "test-harness/dt-c3-token-workflow.html" },
  },
});
