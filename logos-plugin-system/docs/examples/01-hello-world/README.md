# Example 1: Hello World Plugin

The simplest possible Logos plugin. Demonstrates panel creation, buttons, notifications, and basic document access.

## What You'll Learn

- Creating a plugin manifest
- Building a UI panel with components
- Responding to button clicks
- Reading document information
- Packaging and installing

## Files

- `manifest.json` — Plugin metadata and permissions
- `plugin.js` — Plugin code

## Try It

1. Package: `logos plugin package .`
2. Install: `logos plugin install hello-world.logos-plugin`
3. Open a document — the panel appears on the right

## Screenshot

```
┌─────────────────────────┐
│ Hello World        [×]  │
├─────────────────────────┤
│ 👋 Hello from Logos!    │
│                         │
│ Document: Page 1        │
│ Layers: 3               │
│ ─────────────────────── │
│ [  Count Layers      ]  │
│ [  Create Rectangle  ]  │
│ [  Show Selection    ]  │
└─────────────────────────┘
```
