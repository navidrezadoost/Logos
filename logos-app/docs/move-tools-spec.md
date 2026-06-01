# Figma Move Tools — Specification (Logos implementation reference)

This document defines how the **Move tool group** (Move, Hand, Scale) behaves in Logos.
It mirrors Figma’s toolbar presentation and interaction model.

---

## 1. Toolbar structure

The move tools occupy **one toolbar slot**, not three separate buttons.

| Visible element | Meaning |
|----------------|---------|
| Main icon | The **currently active** move tool (Move, Hand, or Scale) |
| Chevron (▾) | Subtle indicator in the bottom-right; more tools in this group |

Only one icon is shown at a time. The icon updates when the user picks a different tool from the dropdown or presses a shortcut.

**Implementation:** `ToolGroupButton` + `toolbarStore.activeToolInGroup.move`

---

## 2. Interaction model

| Action | Result |
|--------|--------|
| **Click main icon area** | Activates the tool currently shown on the button (`activeToolInGroup.move`). Does not open the menu. |
| **Click chevron** | Opens/closes the vertical tool list. |
| **Right-click** anywhere on the control | Opens the tool list. |
| **Pick tool from list** | Closes list, updates toolbar icon, activates that tool, stores it as the group’s displayed tool. |

Frame and Shape groups use the same `ToolGroupButton` pattern for consistency.

---

## 3. Dropdown menu

Compact vertical list. Each row:

```
[ ✓ if active ] [ icon ] [ label ] [ shortcut ]
```

| Tool | Label | Shortcut | Logos `Tool` id |
|------|-------|----------|-----------------|
| Move | Move | `V` | `select` |
| Hand | Hand | `H` | `hand` |
| Scale | Scale | `K` | `scale` |

**Implementation:** `ToolDropdown.tsx`

---

## 4. Keyboard shortcuts

| Key | Tool | Notes |
|-----|------|-------|
| `V` | Move | Always selects Move (default), not last-used Hand/Scale |
| `H` | Hand | |
| `K` | Scale | |
| `Esc` | Move | Clears selection and returns to Move |

Shortcuts update the toolbar icon and `uiStore.activeTool` immediately.

---

## 5. Tool behaviors (canvas)

### Move (`select`, `V`)

- **Cursor:** Pointer arrow.
- **Click shape:** Select topmost shape under cursor. `Shift` adds to selection. `Ctrl`/`Cmd` toggles.
- **Click empty canvas:** Deselect all.
- **Drag selected shape:** Move by delta; snap when enabled. `Shift` constrains to 0°/45°/90°.
- **Drag resize handles:** Resize; corners keep aspect ratio by default. `Shift` inverts constraint. `Alt`/`Option` resize from center.
- **Drag rotation handle:** Rotate; `Shift` → 15° steps.
- **Marquee on empty canvas:** Select intersecting shapes. `Shift` add, `Ctrl`/`Cmd` toggle.
- **Double-click:** Enter edit mode (text, path, drill into group/frame) when implemented.

### Hand (`hand`, `H`)

- **Cursor:** Open hand; closed hand while dragging.
- **Drag:** Pan viewport (same as Space + drag in Move mode).
- **Click without drag:** No selection change.
- **Scroll / pinch:** Zoom (global, all tools).

### Scale (`scale`, `K`)

- **Cursor:** Scale/crosshair style when implemented.
- **Click shape:** Select and prepare for uniform scale.
- **Drag:** Uniform scale from shape center; aspect ratio preserved.
- **Corner handles only** in Scale mode (no edge non-uniform resize in pure Scale mode).

*Full canvas behavior is implemented incrementally in `Canvas.tsx` and related handlers.*

---

## 6. Persistence rules

| Scenario | Toolbar icon after |
|----------|-------------------|
| User picks Hand from dropdown, then Rectangle tool | Rectangle active; move slot still **shows Hand** until changed |
| User presses `V` after using Rectangle | **Move** icon and Move tool active |
| User presses `H` | **Hand** icon and Hand tool active |
| User presses `K` | **Scale** icon and Scale tool active |
| User clicks move slot icon while Hand is displayed | **Hand** re-activated (last displayed move tool) |

The toolbar icon always reflects the **active move tool** while a move tool is active. When another group is active (e.g. Rectangle), the move slot shows the **last chosen** move tool without activating it.

---

## 7. Mutual exclusivity

Only one tool active globally. Activating any move tool deactivates drawing, text, pen, frame, slice, dev, etc., and vice versa.

**Source of truth:** `uiStore.activeTool`  
**Group display state:** `toolbarStore.activeToolInGroup`

---

## 8. Visual states

| State | Appearance |
|-------|------------|
| Default | Group icon + subtle chevron |
| Group active | Accent background on main button |
| Hover (main) | Muted accent background |
| Hover (chevron) | Slightly brighter chevron / muted background |
| Menu open | Chevron rotated 180° |

---

## 9. Code map

| File | Role |
|------|------|
| `src/components/toolbar/ToolGroupButton.tsx` | Split hit targets: icon vs chevron |
| `src/components/toolbar/ToolDropdown.tsx` | Figma-style menu rows |
| `src/components/toolbar/Toolbar.tsx` | Groups, shortcuts, wiring |
| `src/stores/toolbarStore.ts` | `TOOL_GROUPS`, `activeToolInGroup` |
| `src/stores/uiStore.ts` | `activeTool` ground truth |

---

## 10. Differences from legacy Logos toolbar

| Before | After (Figma-aligned) |
|--------|----------------------|
| Entire group button opened dropdown | Icon activates tool; chevron opens menu |
| Same UX, less precise | Right-click opens menu |
| Bullet (●) for active row | Checkmark (✓) left of row |
