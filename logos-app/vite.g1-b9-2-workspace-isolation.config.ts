import { mergeConfig } from "vite";
import appConfig from "./vite.config";

export default mergeConfig(appConfig, {
  build: {
    outDir: "build-g1-b9-2-workspace-isolation",
    emptyOutDir: true,
    rollupOptions: { input: "test-harness/g1-b9-2-workspace-isolation.html" },
  },
});
