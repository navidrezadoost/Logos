import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: "build-p05-browser-persistence",
    emptyOutDir: true,
    rollupOptions: {
      input: "test-harness/p05-browser-persistence-smoke.html",
    },
  },
});
