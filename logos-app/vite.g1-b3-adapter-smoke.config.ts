import { mergeConfig } from "vite";
import appConfig from "./vite.config";

export default mergeConfig(appConfig, {
  build: {
    outDir: "build/g1-b3-adapter-smoke",
    emptyOutDir: true,
    rollupOptions: {
      input: "g1-b3-adapter-smoke.html",
    },
  },
});
