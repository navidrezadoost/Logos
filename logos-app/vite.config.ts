import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

// Serve the pre-built render-wasm Emscripten artefacts directly from the
// existing frontend public directory during development.
// This avoids duplicating the WASM build output.
const FRONTEND_PUBLIC = path.resolve(
  __dirname,
  "../frontend/resources/public"
);

export default defineConfig({
  plugins: [react()],

  server: {
    port: 5174,
    // Allow Vite to serve files from the sibling frontend resources directory.
    fs: {
      allow: [__dirname, FRONTEND_PUBLIC],
    },
  },

  // During dev, expose the sibling public directory at the root so that
  // `/js/render-wasm.js` and `/js/render-wasm.wasm` resolve correctly.
  publicDir: FRONTEND_PUBLIC,

  // Mark the Emscripten WASM artefact as external so Vite doesn't try to
  // bundle it.  The browser will fetch it lazily at runtime.
  build: {
    rollupOptions: {
      external: [/render-wasm\.(js|wasm)$/],
    },
  },

  // Pass the WASM asset paths to the application at build time.
  define: {
    __RENDER_WASM_JS__: JSON.stringify("/js/render-wasm.js"),
    __RENDER_WASM_WASM__: JSON.stringify("/js/render-wasm.wasm"),
    __LOGOS_LAYOUT_WASM__: JSON.stringify("/js/logos_layout_wasm.wasm"),
  },
});
