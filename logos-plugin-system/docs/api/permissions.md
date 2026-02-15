# Permissions Reference

Logos plugins operate under a **capability-based security model**. Plugins must declare required permissions in their manifest, and all host API calls are checked against the granted permission set at runtime.

---

## Permission Kinds

| Permission | Description | Grants Access To |
|-----------|-------------|------------------|
| `DocumentRead` | Read document structure | `getDocumentInfo`, `getLayers`, `getLayer`, `getLayerCount`, `getSelection` |
| `DocumentWrite` | Modify document structure | `createRect`, `createPath`, `deleteLayer`, `setSelection`, `clearSelection`, `undo`, `redo` |
| `Network` | HTTP/WebSocket access | Fetch API, WebSocket connections (domain-restricted) |
| `FileRead` | Read files from disk | File system read operations (path-restricted) |
| `FileWrite` | Write files to disk | File system write operations (path-restricted) |
| `Clipboard` | Access system clipboard | Read and write clipboard content |
| `Notifications` | Show system notifications | System-level notification API |
| `UserPreferences` | Access user settings | Read/write user preference storage |
| `Background` | Run in background | Execute after panel close, periodic tasks |

---

## Declaring Permissions

Permissions are declared in the plugin manifest:

```json
{
  "id": "com.example.my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "permissions": {
    "document": ["read", "write"],
    "network": {
      "domains": ["api.example.com", "cdn.example.com"]
    },
    "fileSystem": {
      "read": ["~/Documents/exports/"],
      "write": ["~/Documents/exports/"]
    },
    "ui": ["panel"],
    "clipboard": true,
    "notifications": true
  }
}
```

---

## Permission Presets

For convenience, common permission sets are predefined:

### `none`
No permissions. Plugin can only log and check timeout.

### `read_only`
- `DocumentRead`

### `document_full`
- `DocumentRead`
- `DocumentWrite`

---

## Runtime Permission Checks

Every host function call goes through a `PermissionGuard`. If a permission is denied:

1. The call returns an error with a descriptive message
2. The denial is logged to an audit trail
3. The plugin continues execution (denials are non-fatal)

### Denial Audit Log

```rust
struct PermissionDenial {
    permission: PermissionKind,
    context: String,        // e.g., "createRect"
    timestamp: Instant,
}
```

Denials are collected per-plugin and can be inspected by the host application for debugging and security auditing.

---

## Network Domain Restrictions

When `Network` permission is granted, it is restricted to declared domains:

```json
{
  "permissions": {
    "network": {
      "domains": ["api.example.com"]
    }
  }
}
```

Attempting to access an undeclared domain will be blocked.

---

## File Path Restrictions

File access is restricted to declared paths:

```json
{
  "permissions": {
    "fileSystem": {
      "read": ["/tmp/logos-exports/"],
      "write": ["/tmp/logos-exports/"]
    }
  }
}
```

Path checks use prefix matching — the plugin can access any file under the declared directories.

---

## Runtime Permission Modification

The host application can grant or revoke permissions at runtime:

```rust
// Grant additional permissions
guard.runtime_grant(PermissionKind::Network);

// Revoke a permission
guard.runtime_revoke(PermissionKind::FileWrite);
```

This enables scenarios like:
- User approves a permission prompt at runtime
- Temporary elevated permissions for specific operations
- Emergency permission revocation for misbehaving plugins

---

## UI Permissions

Panel UI operations have their own permission layer:

| UI Permission | Grants |
|--------------|--------|
| `Render` | Create panels, send messages |
| `ReadDocument` | Access document data from UI context |
| `WriteDocument` | Modify document from UI context |
| `Network` | Network requests from UI context |

```rust
let ui_perms = UiPermissionSet::render_only(); // Only Render
let ui_perms = UiPermissionSet::full();        // All UI permissions
```

---

## Security Model

### Principle of Least Privilege
Plugins should request only the permissions they need. The Logos marketplace shows permissions to users before installation.

### Defense in Depth
1. **Manifest declaration** — permissions declared at build time
2. **Runtime guard** — every call checked
3. **Domain/path restrictions** — network and file access narrowed
4. **Audit trail** — all denials logged
5. **Rate limiting** — prevents abuse via high-frequency calls

### Sandbox Isolation
Each plugin runs in its own sandbox with:
- **Memory limit:** 50MB (configurable)
- **Execution time:** 10ms per invocation (configurable)
- **Stack depth:** 256 frames
- **Host call limit:** 10,000 calls per execution
- **Output limit:** 1MB

Exceeding any limit terminates the execution with a descriptive `RuntimeError`.
