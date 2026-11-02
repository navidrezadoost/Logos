---
title: Architecture Overview
desc: Logos Community Edition architecture — Go backend, TypeScript frontend, Rust core.
---

# Architecture Overview

The Logos Community Edition architecture is documented in full in
[`ARCHITECTURE.md`](/ARCHITECTURE.md) at the repository root.

## Quick Reference

| Layer | Technology | Directory |
|---|---|---|
| **Frontend SPA** | TypeScript · React 19 · Zustand · Vite | `logos-app/` |
| **Backend API** | Go · chi · pgx · go-jose | `backend-go/` |
| **Core types** | Rust → generates TypeScript `.d.ts` | `rust/logos-types/` |
| **Layout engine** | Rust (native + WASM) | `rust/logos-layout/` |
| **CRDT rebase** | Go (production) + Rust `rlib` | `backend-go/internal/rebase/` |
| **Render engine** | Rust → WASM · WebGPU shaders | `render-wasm/` |
| **MCP server** | TypeScript · Model Context Protocol | `mcp/` |
| **Plugin SDK** | TypeScript sandbox | `plugins/` |

## Guides

- [Backend Guide](../backend.md) — Go server, RPC handlers, auth, file format
- [Frontend Guide](../frontend.md) — React SPA, state management, WebGPU bridge
- [Shared Types & Codegen](../common.md) — `rust/logos-types/` + TypeScript codegen
- [Abstraction Levels](../abstraction-levels.md) — layered code organization
- [Data Guide](../data-guide.md) — shape model, migrations, file format
- [Dev Environment](../devenv.md) — Docker devenv and native setup
