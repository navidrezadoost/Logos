---
title: 3.05. Frontend Guide
desc: "Logos Frontend Guide: TypeScript + React SPA, dev server, state management, and debugging."
---

# Frontend Guide

The Logos frontend is a TypeScript/React application in `logos-app/`. This guide covers
the development workflow, architecture, state management patterns, and debugging tools.

---

## Quick Start

```bash
cd logos-app
npm ci               # install dependencies (uses lockfile)
npm run dev          # dev server at http://localhost:3449 (hot-reload)
npx tsc --noEmit     # type-check (zero errors required)
npx vitest run       # unit tests
npm run build        # production build → logos-app/dist/
```

The dev server proxies all `/api` requests to `http://localhost:6060` (the Go backend).
Make sure the backend is running before starting the frontend.

---

## Project Structure

```
logos-app/
├── src/
│   ├── design/           Design canvas components (WebGPU renderer bridge)
│   ├── workspace/        Layer panel, properties inspector, design tokens panel
│   ├── collab/           Real-time collaboration (WebSocket + OT client)
│   ├── ai/               AI assistant (cloud MCP + local LLM inference)
│   ├── plugins/          Plugin sandbox runtime and panel
│   ├── dashboard/        File/project/team management views
│   ├── auth/             Login, register, profile pages
│   ├── types/
│   │   └── rust-generated/  Auto-generated TypeScript types from rust/logos-types/
│   └── main.tsx          Application entry point
├── workers/              Web workers (GPU offscreen, file processing)
├── public/               Static assets
├── vite.config.ts        Build configuration + API proxy
└── tsconfig.json         TypeScript configuration
```

---

## State Management

Logos uses [Zustand](https://github.com/pmndrs/zustand) for global application state
and React's built-in `useState`/`useReducer` for local component state.

### Store organization

```typescript
// Example: workspace store
import { create } from "zustand";

interface WorkspaceStore {
  selectedIds: Set<string>;
  zoom: number;
  setZoom: (z: number) => void;
  toggleSelect: (id: string) => void;
}

export const useWorkspaceStore = create<WorkspaceStore>((set) => ({
  selectedIds: new Set(),
  zoom: 1,
  setZoom: (zoom) => set({ zoom }),
  toggleSelect: (id) =>
    set((state) => {
      const ids = new Set(state.selectedIds);
      ids.has(id) ? ids.delete(id) : ids.add(id);
      return { selectedIds: ids };
    }),
}));
```

---

## WebGPU Renderer Bridge

The WebGPU render engine is compiled from Rust to WASM (`render-wasm/pkg/`).
The TypeScript bridge in `logos-app/src/design/` calls into it via the
`wasm-bindgen`-generated JS bindings:

```typescript
import init, { RenderEngine } from "@/render-wasm/pkg/render_wasm";

await init();
const engine = RenderEngine.new(canvas);
engine.set_zoom(2.0);
engine.render_frame();
```

When you modify the Rust renderer, rebuild with:

```bash
cd render-wasm && wasm-pack build --target web --release
```

The Vite build picks up the new WASM automatically.

---

## Real-time Collaboration

The collaboration client in `logos-app/src/collab/` maintains a WebSocket connection
to the Go backend and handles:

1. Sending local change-sets to `POST /api/rpc/command/update-file`
2. Receiving broadcasted changes via `ws://…/ws/file/:fileId`
3. Applying remote changes to the local document via the OT client

The OT client in TypeScript mirrors the Go `internal/rebase` engine — both implement
the same 5×5 conflict matrix.

---

## Plugin Runtime

Plugins run in a sandboxed iframe with a restricted global scope. The bridge
between the plugin sandbox and the main canvas is in `logos-app/src/plugins/`.

The plugin API is exposed as `window.logos` inside the sandbox:

```typescript
// Inside a plugin
logos.selection.forEach(shape => {
  shape.fills = [{ type: "solid", color: { r: 1, g: 0, b: 0, a: 1 } }];
});
logos.ui.open("My Plugin", "http://localhost:3001");
```

For full plugin development documentation see [Plugin Guide](../../plugin-guide/).

---

## Debugging

### Browser DevTools

All TypeScript source maps are enabled in development mode. Set breakpoints
directly in the TypeScript source files via DevTools → Sources.

### Debug page

Navigate to `http://localhost:3449/dbg` during development for:
- Feature flag toggles (per-team)
- State inspector
- WebSocket traffic monitor
- Render engine diagnostics

### Logging

```typescript
// Use the structured logger for important events
import { logger } from "@/utils/logging";

logger.debug("collab", "rebase applied", { fileId, revn });
logger.warn("renderer", "WASM not initialized");
```

Log output appears in the browser console grouped by namespace.

### TypeScript type errors

```bash
cd logos-app
npx tsc --noEmit 2>&1 | head -50
```

All type errors must be resolved before merging. `strict: true` is enabled.

---

## Production Build

```bash
cd logos-app
npm run build
# Output: logos-app/dist/
```

The build produces:
- `dist/index.html` — SPA entry point with `logosFlags` config injection point
- `dist/js/` — Vite-chunked JS bundles (immutable hash filenames)
- `dist/css/` — extracted stylesheets
- `dist/assets/` — fonts, images, WASM binary

Serve `dist/` with any static file server. The Nginx configuration in the self-hosting
guide includes the correct `try_files` fallback for SPA routing.

---

## Testing

```bash
cd logos-app

# Unit tests (Vitest + jsdom)
npx vitest run

# Watch mode
npx vitest

# Coverage report
npx vitest run --coverage
```

Tests live alongside their source files: `src/utils/color.test.ts` tests `src/utils/color.ts`.
