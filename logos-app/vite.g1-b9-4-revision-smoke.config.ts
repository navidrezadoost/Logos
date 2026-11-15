import { defineConfig } from "vite";

export default defineConfig({
  base: "./",
  build: { outDir: "build/g1-b9-4-revision-smoke", emptyOutDir: true,
    rollupOptions: { input: "g1-b9-4-revision-smoke.html" } },
});
