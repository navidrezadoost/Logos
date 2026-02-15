# Example 2: Layer Counter

A real-time layer counter that shows document statistics and updates live as layers are added or removed.

## What You'll Learn

- Reading all layers from the document
- Filtering layers by type
- Reacting to document changes in real-time
- Updating panel UI dynamically
- Using groups to organize components

## Files

- `manifest.json` — Plugin metadata
- `plugin.js` — Plugin code with live updates

## Screenshot

```
┌─────────────────────────┐
│ Layer Counter      [×]  │
├─────────────────────────┤
│ ▼ Document Stats        │
│   Total: 12             │
│   Selected: 3           │
│ ─────────────────────── │
│ ▼ By Type               │
│   Rectangles: 5         │
│   Text: 3               │
│   Frames: 2             │
│   Components: 1         │
│   Groups: 1             │
│ ─────────────────────── │
│ [ Select All Rects   ]  │
│ [ Clear Selection    ]  │
│ [ Refresh            ]  │
└─────────────────────────┘
```
