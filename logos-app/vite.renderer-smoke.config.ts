import { mergeConfig } from "vite";
import appConfig from "./vite.config";

export default mergeConfig(appConfig, {
  build: {
    outDir: "build-renderer-smoke",
    emptyOutDir: true,
    rollupOptions: {
      input: "test-harness/p04-react-renderer-smoke.html",
    },
  },
});
