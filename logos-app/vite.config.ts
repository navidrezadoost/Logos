import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { svgerVitePlugin } from "svger-cli/vite";
import path from "path";

// Pre-built WASM/static assets live in logos-app/dist/ (committed build output).
const STATIC_ASSETS = path.resolve(__dirname, "dist");

export default defineConfig({
  plugins: [
    svgerVitePlugin({
      source: "./src/icons/toolbar",
      output: "./src/components/icons",
      framework: "react",
      typescript: true,
      hmr: true,
    }),
    svgerVitePlugin({
      source: "./src/icons/system",
      output: "./src/components/icons/system",
      framework: "react",
      typescript: true,
      hmr: true,
    }),
    react(),
  ],

  server: {
    port: 8888,
    host: "127.0.0.1",
    strictPort: true,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8080",
        changeOrigin: true,
      },
      "/assets/by-id": {
        target: "http://127.0.0.1:8080",
        changeOrigin: true,
      },
    },
    fs: {
      allow: [__dirname, STATIC_ASSETS],
    },
  },

  publicDir: STATIC_ASSETS,

  // Mark the Emscripten WASM artefact as external so Vite doesn't try to
  // bundle it.  The browser will fetch it lazily at runtime.
  build: {
    outDir: "build",
    emptyOutDir: true,
    rollupOptions: {
      external: [/render-wasm\.(js|wasm)$/],
    },
  },

  // Pass the WASM asset paths to the application at build time.
  define: {
    __RENDER_WASM_JS__: JSON.stringify("/js/render-wasm.js"),
    __RENDER_WASM_WASM__: JSON.stringify("/js/render-wasm.wasm"),
    __LOGOS_LAYOUT_WASM__: JSON.stringify("/js/logos_layout_wasm.wasm"),
    __LOGOS_VECTOR_WASM__: JSON.stringify("/js/logos_vector_wasm.wasm"),
  },
});
