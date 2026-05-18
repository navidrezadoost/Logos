# Logos Plugin System

The Logos plugin system allows third-party scripts to read and manipulate the canvas
document via a secure, permission-gated API. Each plugin runs in a sandboxed `<iframe>`
with no same-origin access to the host document.

---

## Architecture

```
Host (logos-app)                    Plugin (sandboxed <iframe>)
─────────────────────────────────   ─────────────────────────────────
bridge.ts                           bootstrap (injected by bridge)
  ├─ connectPlugin()                  └─ window.logos = { call, on }
  ├─ disconnectPlugin()
  ├─ broadcastEvent()               hello-world.ts  ← your plugin
  └─ onMessage()                       └─ logos.call("getSelection")
       └─ handlePluginCall()
            └─ api.ts (dispatch)
```

### Wire protocol

All messages are plain JSON posted via `window.postMessage`.

| Direction      | Message shape                                                   |
|----------------|-----------------------------------------------------------------|
| Host → Plugin  | `{ type: "REQUEST", id, method, params }`                       |
| Host → Plugin  | `{ type: "EVENT", event, payload }` (push)                      |
| Plugin → Host  | `{ type: "RESPONSE", id, success, data?, error? }` (via `__call`) |
| Host → Plugin  | `{ type: "CALL_RESULT", id, success, data?, error? }`           |

---

## Permissions

Permissions are granted at `connectPlugin()` time and enforced host-side.  
A plugin cannot escalate its own permissions.

| Token              | What it unlocks                                  |
|--------------------|--------------------------------------------------|
| `"read"`           | `getPage()`, `getSelection()`, `getShape()`      |
| `"content"`        | `createRect()`, `createEllipse()`, `updateShape()`, `deleteShape()` |
| `"allow:network"`  | `fetch()` from inside the sandbox (future)       |
| `"allow:clipboard"`| `navigator.clipboard` (future)                   |

---

## Plugin API (`window.logos`)

The bridge injects a `logos` global into every plugin iframe before the plugin
script runs.

```typescript
// Call a host API method
logos.call(method: string, params?: unknown): Promise<unknown>

// Listen for host-push events
logos.on(event: "selectionChange" | "pageChange" | "documentChange", fn): void
```

### Available methods

| Method           | Params                               | Returns          | Permission |
|------------------|--------------------------------------|------------------|------------|
| `getPage`        | —                                    | `PluginPage`     | `read`     |
| `getSelection`   | —                                    | `PluginShape[]`  | `read`     |
| `getShape`       | `{ id }`                             | `PluginShape \| null` | `read` |
| `updateShape`    | `{ id, patch }`                      | `void`           | `content`  |
| `createRect`     | `{ x, y, width, height, name? }`     | `string` (id)    | `content`  |
| `createEllipse`  | `{ x, y, width, height, name? }`     | `string` (id)    | `content`  |
| `deleteShape`    | `{ id }`                             | `void`           | `content`  |

---

## Writing a plugin

```typescript
// my-plugin.ts
declare const logos: {
  call<T = unknown>(method: string, params?: unknown): Promise<T>;
  on(event: string, fn: (payload: unknown) => void): void;
};

// Read selection on load
const shapes = await logos.call("getSelection");
console.log("Selected shapes:", shapes);

// React to selection changes
logos.on("selectionChange", async () => {
  const sel = await logos.call("getSelection");
  console.log("New selection:", sel);
});

// Create a shape
const id = await logos.call("createRect", { x: 100, y: 100, width: 200, height: 100 });
console.log("Created:", id);
```

Bundle the file to a self-contained JS (no imports):

```bash
npx esbuild my-plugin.ts --bundle --outfile=public/plugins/my-plugin.js --platform=browser
```

---

## Loading a plugin (host side)

```typescript
import { connectPlugin, disconnectPlugin, broadcastEvent } from "./plugins/bridge";

// Load with read + content permissions
const handle = await connectPlugin(
  "/plugins/hello-world.js",
  ["read", "content"]
);

// Push an event to the plugin
broadcastEvent({
  type: "EVENT",
  event: "selectionChange",
  payload: { ids: ["abc123"] },
});

// Disconnect
handle.disconnect();
```

---

## Sample plugin

See [`sample/hello-world.ts`](./sample/hello-world.ts) — it:

1. Reads the current selection and logs each shape's name, type, and bounds.
2. Listens for `selectionChange` events and re-logs.
3. After 1 second, creates a "Hello from plugin!" rectangle and reads it back.

Build it:
```bash
cd logos-app
npx esbuild src/plugins/sample/hello-world.ts --bundle --outfile=../public/plugins/hello-world.js --platform=browser
```
