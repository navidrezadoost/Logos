import { defineConfig, mergeConfig } from "vite";
import path from "node:path";
import baseConfig from "./vite.config";

const sourceIdentity = process.env.LOGOS_MEMORY_SOURCE_ID;
if (!sourceIdentity || !/^[a-f0-9]{40}:[a-f0-9]{40}$/i.test(sourceIdentity)) {
  throw new Error("LOGOS_MEMORY_SOURCE_ID must contain the clean Git commit and tree SHA");
}

export default mergeConfig(baseConfig, defineConfig({
  define: { __G1_B9_6_SOURCE_ID__: JSON.stringify(sourceIdentity) },
  build: {
    outDir: "build-g1-b9-6-memory",
    emptyOutDir: true,
    rollupOptions: {
      input: path.resolve(__dirname, "g1-b9-5-editor-performance.html"),
    },
  },
}));
