# Plugin Manifest Reference

Every Logos plugin requires a `manifest.json` file describing its metadata, permissions, and capabilities.

---

## Minimal Manifest

```json
{
  "name": "Hello World",
  "version": "1.0.0",
  "entry_point": "plugin.js"
}
```

---

## Full Schema

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "My Awesome Plugin",
  "version": "1.2.3",
  "author": "Jane Developer",
  "description": "A plugin that does amazing things with your designs.",
  "entry_point": "plugin.js",
  "ui_entry_point": "ui.js",
  "category": "Layout",
  "license": "MIT",
  "repository": "https://github.com/example/my-plugin",
  "homepage": "https://example.com/my-plugin",
  "icon": "icon.png",
  "icons": {
    "16": "icons/icon-16.png",
    "48": "icons/icon-48.png",
    "128": "icons/icon-128.png"
  },
  "tags": ["layout", "alignment", "spacing"],
  "min_logos_version": "1.0.0",
  "max_execution_time": 10,
  "permissions": {
    "document": ["read", "write"],
    "network": {
      "domains": ["api.example.com"]
    },
    "ui": ["panel"],
    "clipboard": true
  },
  "hooks": [
    "onLoad",
    "onSave",
    "onSelectionChange"
  ],
  "commands": [
    {
      "id": "align-left",
      "label": "Align Left",
      "shortcut": "Ctrl+Shift+L"
    },
    {
      "id": "distribute-spacing",
      "label": "Distribute Spacing",
      "shortcut": "Ctrl+Shift+D"
    }
  ]
}
```

---

## Field Reference

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Display name of the plugin (max 100 characters) |
| `version` | `string` | Semantic version: `major.minor.patch` |
| `entry_point` | `string` | Path to the main JavaScript file |

### Optional Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | `string` | Auto-generated UUID | Unique plugin identifier |
| `author` | `string` | `""` | Author name |
| `description` | `string` | `""` | Plugin description (max 500 characters) |
| `ui_entry_point` | `string` | `null` | Path to UI-specific JavaScript |
| `category` | `string` | `"Other"` | Plugin category |
| `license` | `string` | `null` | SPDX license identifier |
| `repository` | `string` | `null` | Source code URL |
| `homepage` | `string` | `null` | Plugin website URL |
| `icon` | `string` | `null` | Default icon path |
| `icons` | `object` | `{}` | Size-specific icons (16, 48, 128px) |
| `tags` | `array` | `[]` | Searchable tags |
| `min_logos_version` | `string` | `null` | Minimum Logos version required |
| `max_execution_time` | `number` | `10` | Max execution time in milliseconds |
| `permissions` | `object` | `{}` | Required permissions |
| `hooks` | `array` | `[]` | Lifecycle hooks to subscribe to |
| `commands` | `array` | `[]` | User-invocable commands |

---

## Version Format

Versions follow [Semantic Versioning](https://semver.org/):

```
MAJOR.MINOR.PATCH
```

- **MAJOR** — Incompatible changes
- **MINOR** — Backwards-compatible features
- **PATCH** — Backwards-compatible fixes

The `SemVer` type supports comparison and compatibility checking:

```rust
let v1 = SemVer::new(1, 2, 3);
let v2 = SemVer::new(1, 3, 0);
assert!(v1.satisfies(&v2)); // same major version → compatible
```

---

## Categories

| Category | Description |
|----------|-------------|
| `Layout` | Alignment, spacing, grid tools |
| `Color` | Color palettes, gradients, themes |
| `Typography` | Font tools, text formatting |
| `Export` | Export formats, optimization |
| `Accessibility` | A11y checking, contrast tools |
| `Animation` | Motion, transitions, keyframes |
| `Collaboration` | Comments, sharing, review |
| `DevTools` | Developer utilities, inspection |
| `Assets` | Asset management, stock resources |
| `Other` | Uncategorized |

---

## Hooks

Hooks subscribe your plugin to lifecycle events:

| Hook | Trigger |
|------|---------|
| `onLoad` | Plugin is first loaded |
| `onSave` | Document is saved |
| `onSelectionChange` | Selection changes |
| `onFrame` | Each render frame |
| `onLayerCreate` | A layer is created |
| `onLayerDelete` | A layer is deleted |
| `onExport` | Document is exported |

```json
{
  "hooks": ["onLoad", "onSelectionChange"]
}
```

---

## Commands

Commands register actions in the Logos command palette:

```json
{
  "commands": [
    {
      "id": "my-action",
      "label": "Do Something Cool",
      "shortcut": "Ctrl+Shift+K"
    }
  ]
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | `string` | yes | Unique command identifier |
| `label` | `string` | yes | Display name in command palette |
| `shortcut` | `string` | no | Keyboard shortcut |

---

## Validation Rules

The manifest is validated before a plugin can be loaded:

1. `name` must be non-empty and ≤ 100 characters
2. `entry_point` must be specified
3. `description` must be ≤ 500 characters (if provided)
4. `version` must be valid semver
5. `tags` are limited to prevent abuse

Validation errors prevent plugin loading and are reported to the user.

---

## Icon Sizes

Plugins can provide icons at multiple resolutions:

| Size | Pixels | Usage |
|------|--------|-------|
| Small | 16×16 | Toolbar, list items |
| Medium | 48×48 | Plugin browser cards |
| Large | 128×128 | Plugin detail page, marketplace |

Icons should be PNG with transparency support.
