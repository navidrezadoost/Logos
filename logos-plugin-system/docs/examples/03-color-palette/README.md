# Example 3: Color Palette

A color palette plugin that lets users pick colors and apply them to create new rectangles. Demonstrates document modification and the color picker component.

## What You'll Learn

- Using the ColorPicker component
- Creating shapes with `Logos.createRect()`
- Using NumberInput for precise control
- Toggle components for options
- Undo/redo integration

## Files

- `manifest.json` — Plugin metadata
- `plugin.js` — Plugin code with color management

## Screenshot

```
┌─────────────────────────┐
│ Color Palette      [×]  │
├─────────────────────────┤
│ ▼ Color                 │
│   Fill: [████████████]  │
│   Opacity: ━━━━━━╋━ 100 │
│ ─────────────────────── │
│ ▼ Size                  │
│   Width:  ━━━━━━╋━ 200  │
│   Height: ━━━━━━╋━ 200  │
│ ─────────────────────── │
│ ▼ Position              │
│   X: ━━╋━━━━━━━━ 100    │
│   Y: ━━╋━━━━━━━━ 100    │
│ ─────────────────────── │
│ ☑ Create at center      │
│ ─────────────────────── │
│ [ Create Shape       ]  │
│ [ Undo Last          ]  │
│ [ Create Grid (3×3)  ]  │
└─────────────────────────┘
```
