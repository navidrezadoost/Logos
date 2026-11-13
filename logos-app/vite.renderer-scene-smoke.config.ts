import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const appRoot = fileURLToPath(new URL(".", import.meta.url));

/** Isolated bundle check for the React-to-Rust scene adapter (not a browser pixel test). */
export default defineConfig({
  root: appRoot,
  build: {
    outDir: ".renderer-scene-smoke",
    emptyOutDir: true,
    lib: {
      entry: "src/render-wasm/scene.ts",
      formats: ["es"],
      fileName: "renderer-scene",
    },
  },
});
