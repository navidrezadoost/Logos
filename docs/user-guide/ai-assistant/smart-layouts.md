---
title: Smart Layouts
desc: Automatically detect grids, alignment rails, consistent spacing, and aspect ratio locks.
eleventyNavigation:
  key: Smart Layouts
  parent: AI Assistant
  order: 5
---

# Smart Layouts

The **Smart Layouts** engine analyzes your design and automatically detects spatial patterns — grids, alignment rails, equal spacing, aspect ratios — then suggests converting them to **Auto Layout constraints**.

<img src="/img/ai-smart-layouts.webp" alt="Smart Layouts panel" style="border: 1px solid #ddd; border-radius: 4px; margin: 20px 0;">

---

## What It Detects

### 1. Alignment Rails

**Definition:** Multiple elements aligned to a common edge or centerline.

**Example:**

```
Element A: left edge at x=100px
Element B: left edge at x=102px
Element C: left edge at x=99px

→ Detection: "3 elements aligned to x=100 rail (±2px tolerance)"
```

**Detected Rail Types:**

| Rail | Description |
|------|-------------|
| **Left** | Left edges aligned vertically |
| **Right** | Right edges aligned vertically |
| **Top** | Top edges aligned horizontally |
| **Bottom** | Bottom edges aligned horizontally |
| **Center X** | Horizontal centers aligned vertically |
| **Center Y** | Vertical centers aligned horizontally |

**What Logos suggests:**

*"Convert to Auto Layout with left alignment pin?"*

**After accepting:**  
All 3 elements are grouped in an Auto Layout stack with `align: left` constraint. Moving the stack moves all elements together.

---

### 2. Equal Spacing

**Definition:** Consistent gaps between consecutive elements.

**Example:**

```
Card A  ← 20px →  Card B  ← 20px →  Card C  ← 20px →  Card D
```

**Detection:** *"4 cards with equal horizontal spacing (20px)"*

**What Logos suggests:**

*"Convert to Auto Layout horizontal stack with 20px gap?"*

**After accepting:**  
Cards are grouped in a horizontal Auto Layout. Adding a new card automatically maintains 20px spacing.

**Detected Spacing Types:**

- **Horizontal:** Left-to-right gaps (for rows)
- **Vertical:** Top-to-bottom gaps (for columns)

---

### 3. Grid Detection

**Definition:** Elements arranged in rows and columns with consistent x/y positions.

**Example:**

```
[Card 1]  [Card 2]  [Card 3]
[Card 4]  [Card 5]  [Card 6]

Row 1 at y=100px
Row 2 at y=180px
Col 1 at x=50px, Col 2 at x=220px, Col 3 at x=390px
```

**Detection:** *"6 cards arranged in 2 rows × 3 cols grid"*

**What Logos suggests:**

*"Convert to Auto Layout grid with 20px horizontal gap, 80px vertical gap?"*

**After accepting:**  
Cards are organized in a responsive grid. Adding a 7th card starts a new row automatically.

**Grid Parameters:**

- Rows and columns count
- X positions (column alignment)
- Y positions (row alignment)
- Horizontal and vertical gaps

---

### 4. Aspect Ratio Locks

**Definition:** Layers with dimensions matching common aspect ratios.

**Detected Ratios:**

| Ratio | Use Case |
|-------|----------|
| **1:1** | Squares (avatars, icons, Instagram posts) |
| **16:9** | Widescreen video (YouTube, TV) |
| **4:3** | Standard display (older monitors, presentations) |
| **3:2** | Photography (DSLR, 35mm film) |
| **21:9** | Ultra-wide (cinematic) |
| **2:1** | Univisium (cinema) |
| **9:16** | Portrait video (TikTok, Stories) |
| **3:4** | Portrait photo |

**Example:**

```
Video thumbnail: 1920px × 1080px
→ Ratio: 16:9 (1.777...)

Detection: "Video thumbnail matches 16:9 aspect ratio"
```

**What Logos suggests:**

*"Lock aspect ratio to 16:9? Prevents accidental distortion when resizing."*

**After accepting:**  
Resizing the thumbnail maintains 16:9 ratio. Dragging a corner scales proportionally.

---

### 5. Responsive Breakpoints

**Definition:** Element clusters that change layout at specific widths (advanced feature).

**Example:**

```
Desktop (>768px): 3-column grid
Mobile (≤768px): 1-column stack

Detection: "Responsive breakpoint detected at 768px"
```

**What Logos suggests:**

*"Apply responsive constraints? Auto-switch to mobile layout below 768px."*

**After accepting:**  
Canvas preview automatically adjusts layout when resizing artboard below 768px.

<p class="advice">
💡 <strong>Coming Soon:</strong> Full responsive design support is planned for Phase 14. Current detection is preview-only.
</p>

---

## How Detection Works

### Clustering Algorithm

Smart Layouts uses **clustering** to group nearby values:

**Example:**

```
Element edges at x positions: 100, 102, 99, 104, 98

Step 1: Sort → 98, 99, 100, 102, 104
Step 2: Cluster within tolerance (2px) → [98, 99, 100, 102, 104]
Step 3: Center of cluster → 101 (average)

Result: Rail detected at x=101 with 5 aligned elements
```

**Tolerance Settings:**

- **Default:** 2px (balanced)
- **Strict:** 0.5px (pixel-perfect designs)
- **Relaxed:** 4px (rough wireframes)

---

### Confidence Scoring

Each detection includes a **confidence score** (0-100%):

| Score | Meaning | Example |
|-------|---------|---------|
| 90-100% | Definitely a pattern | 6+ elements perfectly aligned |
| 70-89% | Likely a pattern | 3-4 elements with ±1px variance |
| 50-69% | Possible pattern | 2 elements aligned (could be coincidence) |
| <50% | Low confidence | Not shown by default |

**Adjust threshold:** Preferences → AI Assistant → Smart Layouts → Min Confidence.

---

## Applying Constraints

### Option 1: Accept Individual Suggestions

Click **green checkmark** next to detected pattern in AI Assistant panel.

### Option 2: Batch Accept

Select multiple patterns (Shift+Click), then click **"Apply All"**.

### Option 3: Preview Before Applying

Hover over suggestion → **Preview Mode** shows ghost outlines of constrained positions.

### Option 4: Convert to Auto Layout

Right-click detected pattern → **"Convert to Auto Layout"** → Logos creates a frame with appropriate constraints.

---

## Configuration

**Preferences → AI Assistant → Smart Layouts:**

```
Tolerance Preset: [Default ▼] (Strict / Default / Relaxed)

Custom Tolerances:
  Alignment Tolerance: [___2___] px
  Spacing Tolerance:   [___10__] %

Minimum Elements: [__2__]
  (Patterns require at least this many elements)

Checks:
  ✅ Alignment Rails
  ✅ Equal Spacing
  ✅ Grid Detection
  ✅ Aspect Ratios
  ⬜ Responsive Breakpoints (experimental)
```

---

## Examples

### Example 1: Aligning a Navigation Menu

**Before:**

```
[Home]   x=100
[About]  x=103
[Contact] x=99
```

**Detection:** *"3 labels aligned to x=101 rail"*

**After Accepting:**

```
[Auto Layout Stack]
  ├─ [Home]    ← left pin
  ├─ [About]   ← left pin
  └─ [Contact] ← left pin
```

Now dragging the stack moves all items together.

---

### Example 2: Card Grid

**Before:** 6 cards manually positioned

```
Card 1 (50, 100)    Card 2 (220, 100)   Card 3 (390, 100)
Card 4 (50, 250)    Card 5 (220, 250)   Card 6 (390, 250)
```

**Detection:** *"Grid: 2 rows × 3 cols, 170px H-gap, 150px V-gap"*

**After Accepting:**

```
[Auto Layout Grid]
  rows: 2, cols: 3
  gap-x: 170px, gap-y: 150px
  ├─ Card 1, Card 2, Card 3
  └─ Card 4, Card 5, Card 6
```

Adding Card 7 automatically starts a new row.

---

### Example 3: Aspect Ratio Lock

**Before:**

```
Image: 1920×1080px (manual sizing)
```

**Detection:** *"Image matches 16:9 aspect ratio"*

**After Accepting:**

```
Image: 16:9 locked
  Resizing width → height auto-adjusts
  Resizing height → width auto-adjusts
```

No more accidental distortion.

---

## Integration with Auto Layout

Smart Layouts detects **implicit** patterns and converts them to **explicit** Auto Layout constraints.

### Before (Manual Positioning)

```
Frame
  ├─ Text (x=20, y=20)
  ├─ Button (x=20, y=70)
  └─ Icon (x=20, y=130)
```

### After (Auto Layout with Smart Layouts)

```
Frame (Auto Layout, vertical, 30px gap)
  ├─ Text
  ├─ Button
  └─ Icon
```

**Benefits:**

- **Auto-spacing:** Gaps maintained when adding/removing items
- **Responsive:** Frame grows/shrinks with content
- **Easier edits:** Change gap size globally (no manual repositioning)

---

## Performance

- **Analysis Speed:** <2ms for 100 layers
- **Real-time:** Runs automatically as you design
- **Max Layers:** Tested up to 10,000 elements

---

## API Access

Plugin developers can access Smart Layouts via API:

```rust
use logos_ai::{ConstraintInferrer, InferrerConfig};

let config = InferrerConfig::default();
let inferrer = ConstraintInferrer::new(config);
let constraints = inferrer.infer_all(&elements);

for constraint in constraints {
    match constraint {
        InferredConstraint::GridDetected { rows, cols, .. } => {
            println!("Grid: {} rows × {} cols", rows, cols);
        }
        InferredConstraint::AlignmentRail { axis, value, indices } => {
            println!("Rail: {:?} at {:.1}px ({} elements)", axis, value, indices.len());
        }
        _ => {}
    }
}
```

See [API Reference — Smart Constraints](/api-reference/logos-ai/#smart-constraints) for full details.

---

## Frequently Asked Questions

**Q: Can I manually adjust detected patterns?**  
A: No, patterns are auto-detected. But you can dismiss incorrect suggestions and manually create Auto Layout.

**Q: Does it work with rotated elements?**  
A: Currently only axis-aligned elements (0° rotation). Rotated element support is planned for Phase 14.

**Q: What if I don't want Auto Layout?**  
A: You can dismiss suggestions. Smart Layouts is non-intrusive — it only suggests, never forces.

**Q: Can it detect nested grids?**  
A: Yes! If a grid contains sub-grids, Smart Layouts detects both levels recursively.

**Q: Does it understand padding/margins?**  
A: Yes. Detected spacing accounts for element bounds, not visual appearance. So a 10px gap with 5px padding = 15px total.

**Q: Can I export constraint data?**  
A: Yes. File → Export → Layout Constraints → JSON. Useful for handing off to developers.

---

## Next Steps

- [Component Recommendations →](/user-guide/ai-assistant/components/) — Find repeated patterns to componentize
- [Design Suggestions →](/user-guide/ai-assistant/design-suggestions/) — Fix alignment issues before applying constraints
- [API Reference →](/api-reference/logos-ai/#smart-constraints) — Use in plugins

**Learn more:** [Auto Layout Guide](/user-guide/designing/auto-layout/)
