import { defineConfig } from "vite";

export default defineConfig({
  base: "./",
  build: {
    outDir: "build/g1-b9-3-compatibility-smoke",
    emptyOutDir: true,
    rollupOptions: { input: "g1-b9-3-compatibility-smoke.html" },
  },
});
