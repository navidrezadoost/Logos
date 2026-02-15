# Example 4: Export Helper

An export helper plugin that generates a text-based report of the document structure—layer tree, statistics, and metadata. Demonstrates document traversal and text output.

## What You'll Learn

- Traversing the document layer tree
- Gathering aggregated statistics
- Text output and reporting
- Select dropdown component
- Multi-format output concept

## Files

- `manifest.json` — Plugin metadata
- `plugin.js` — Document analysis and export

## Screenshot

```
┌─────────────────────────┐
│ Export Helper       [×]  │
├─────────────────────────┤
│ ▼ Export Options         │
│   Format: [JSON     ▾]  │
│   ☑ Include metadata    │
│   ☑ Include positions   │
│ ─────────────────────── │
│ ▼ Document Summary      │
│   Name: My Design       │
│   Total layers: 15      │
│   Rectangles: 8         │
│   Text layers: 4        │
│   Frames: 3             │
│ ─────────────────────── │
│ [ Generate Report    ]  │
│ [ Copy to Clipboard  ]  │
│ [ Export Selection   ]  │
└─────────────────────────┘
```
