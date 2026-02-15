# Example 5: Animation Tool

An advanced plugin that creates animated patterns by placing shapes programmatically. Demonstrates real-time updates, path creation, mathematical transformations, and undo integration.

## What You'll Learn

- Creating complex paths with `Logos.createPath()`
- Programmatic shape placement with math
- Undo/redo for batch operations
- Real-time event handling
- Advanced UI with multiple groups
- Building creative/generative tools

## Files

- `manifest.json` — Plugin metadata
- `plugin.js` — Generative pattern engine

## Screenshot

```
┌─────────────────────────┐
│ Animation Tool     [×]  │
├─────────────────────────┤
│ ▼ Pattern Settings      │
│   Shape: [Circle   ▾]  │
│   Count: ━━━━━━╋━ 12   │
│   Radius: ━━━━╋━━ 150  │
│ ─────────────────────── │
│ ▼ Transform             │
│   Scale: ━━━━━╋━ 1.0   │
│   Rotation: ━━━╋━ 30   │
│   Spacing: ━━━━╋━ 10   │
│ ─────────────────────── │
│ ▼ Center                │
│   X: ━━━━━╋━━━━ 400    │
│   Y: ━━━━━╋━━━━ 400    │
│ ─────────────────────── │
│ ☑ Auto-update preview   │
│ ─────────────────────── │
│ [ Generate Pattern   ]  │
│ [ Spiral Pattern     ]  │
│ [ Undo All           ]  │
│ ─────────────────────── │
│ Generated: 0 shapes     │
└─────────────────────────┘
```
