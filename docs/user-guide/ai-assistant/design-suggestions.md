---
title: Design Suggestions
desc: Automatic detection of alignment issues, spacing inconsistencies, overlaps, and layout problems.
eleventyNavigation:
  key: Design Suggestions
  parent: AI Assistant
  order: 2
---

# Design Suggestions

The **Design Suggestions** engine analyzes your layout for common design issues and offers one-click fixes.

<img src="/img/ai-design-suggestions.webp" alt="Design Suggestions panel" style="border: 1px solid #ddd; border-radius: 4px; margin: 20px 0;">

---

## What It Detects

### 1. Alignment Issues

**Problem:** Elements with edges nearly aligned (within 1-8px tolerance) but not perfectly aligned.

**Example:**
```
Button 1: x = 100px
Button 2: x = 103px  ← 3px off
Button 3: x = 98px   ← 2px off
```

**Suggestion:** *"3 elements nearly aligned vertically (left edge) — align to x=100?"*

**Fix:** Click "Accept" → All 3 buttons snap to x=100.

**Why it matters:** Subtle misalignments create visual noise and look unprofessional.

---

### 2. Spacing Inconsistencies

**Problem:** Elements with varying gaps between them.

**Example:**
```
Card 1 ← 20px gap → Card 2 ← 24px gap → Card 3 ← 19px gap → Card 4
```

**Suggestion:** *"4 cards have inconsistent horizontal spacing (19-24px) — normalize to 21px average?"*

**Fix:** Click "Accept" → All gaps become 21px.

**Why it matters:** Consistent spacing creates visual rhythm and makes designs feel cohesive.

---

### 3. Overlapping Elements

**Problem:** Two or more layers occupying the same space (unintentionally).

**Example:**
```
Icon overlaps button by 12px (8% of button area)
```

**Suggestion:** *"Icon overlaps 'Submit' button — move icon 12px left?"*

**Fix:** Click "Accept" → Icon moves to avoid overlap.

**Why it matters:** Overlaps can hide content or cause rendering issues.

**Note:** Intentional overlaps (e.g., badge on avatar) are detected but shown with lower confidence. You can permanently dismiss these.

---

### 4. Out of Bounds

**Problem:** Elements positioned outside the canvas or artboard boundaries.

**Example:**
```
Text layer extends 50px beyond canvas right edge
```

**Suggestion:** *"Text extends beyond canvas — resize or reposition?"*

**Fix:** Click "Accept" → Text is automatically wrapped or repositioned within bounds.

**Why it matters:** Off-canvas content won't appear in exports.

---

### 5. Hierarchy Issues

**Problem:** Parent/child size relationships that violate visual hierarchy.

**Example:**
```
Parent frame: 200×100px
Child text: 250×50px (overflows parent)
```

**Suggestion:** *"Child text larger than parent frame — resize parent or child?"*

**Fix:** Choose option: (1) Expand parent to fit child, or (2) Shrink child to fit parent.

**Why it matters:** Broken hierarchy confuses users and breaks Auto Layout.

---

### 6. Grouping Opportunities

**Problem:** Dense clusters of elements that should be grouped for easier organization.

**Example:**
```
8 layers within 100px radius, no shared parent group
```

**Suggestion:** *"8 nearby layers detected — group as 'Icon Set'?"*

**Fix:** Click "Accept" → Layers are grouped, renamed "Icon Set".

**Why it matters:** Groups reduce layer clutter and enable batch operations.

---

## Confidence Scores

Suggestions include a **confidence percentage** (0-100%) indicating how certain the AI is:

| Range | Meaning | Example |
|-------|---------|---------|
| 90-100% | Definitely an issue | 3px misalignment with 5 elements |
| 70-89% | Likely an issue | Inconsistent spacing (variance > 20%) |
| 50-69% | Possible issue | Small overlap (< 5% area) |
| < 50% | Low confidence | Not shown by default |

You can adjust the **minimum confidence threshold** in Preferences → AI Assistant → Design Suggestions → Min Confidence.

---

## Configuring Strictness

**Preferences → AI Assistant → Design Suggestions:**

### Presets

- **Strict** (1px alignment tolerance, 5% spacing tolerance)  
  Use for pixel-perfect designs, enterprise UIs.

- **Default** (4px alignment, 15% spacing)  
  Balanced for most projects.

- **Relaxed** (8px alignment, 30% spacing)  
  For sketches, rough wireframes.

### Custom Settings

```
Alignment Tolerance: [____4____] px
Spacing Tolerance:   [____15___] %
Min Confidence:      [____30___] %

Checks:
  ✅ Alignment
  ✅ Spacing
  ✅ Overlaps
  ✅ Out of Bounds
  ✅ Hierarchy
  ✅ Grouping
```

---

## Accepting Suggestions

### Method 1: Click Accept

Click the **green checkmark** next to the suggestion in the AI Assistant panel.

### Method 2: Keyboard Shortcut

With suggestion selected, press **Ctrl+Enter**.

### Method 3: Batch Accept

Select multiple suggestions (Shift+Click), then click **Accept All**.

<p class="advice">
💡 <strong>Tip:</strong> Review proposed fixes in the <strong>Preview</strong> pane before accepting. Hover over a suggestion to see a ghost outline of the corrected position.
</p>

---

## Dismissing Suggestions

### Temporary Dismiss (24 hours)

Click the **gray X** → Suggestion disappears for 24 hours.

### Permanent Dismiss

Right-click suggestion → **Never Show Again** → This specific instance is ignored forever.

### Disable Check Type

Right-click suggestion → **Disable All [Alignment] Checks** → That entire category is turned off.

---

## Examples

### Example 1: Aligning a Button Grid

**Before:**

```
[Button 1]  x=100, y=50
[Button 2]  x=104, y=50   ← 4px misaligned
[Button 3]  x=98,  y=50   ← 2px misaligned
```

**Suggestion:** *"3 buttons nearly aligned (left edge) — snap to x=100?"*

**After Accepting:**

```
[Button 1]  x=100, y=50
[Button 2]  x=100, y=50   ✅ Aligned
[Button 3]  x=100, y=50   ✅ Aligned
```

---

### Example 2: Normalizing Card Spacing

**Before:**

```
Card 1 ← 20px → Card 2 ← 28px → Card 3 ← 22px → Card 4
```

**Suggestion:** *"4 cards have inconsistent spacing (20-28px) — normalize to 23px average?"*

**After Accepting:**

```
Card 1 ← 23px → Card 2 ← 23px → Card 3 ← 23px → Card 4
```

---

### Example 3: Fixing Overlap

**Before:**

```
[Icon]
    └─ Overlaps [Button] by 15px (10% of button area)
```

**Suggestion:** *"Icon overlaps button — move icon 15px left?"*

**After Accepting:**

```
[Icon]  ← 5px gap →  [Button]   ✅ No overlap
```

---

## Performance

- **Analysis Speed:** <1ms for 50 layers
- **UI Update:** Real-time (suggestions appear as you edit)
- **Max Layers:** Tested up to 10,000 layers with no slowdown

---

## Advanced: Proposed Fixes

Some suggestions include **proposed fixes** (new positions/sizes). These are available via the [API](/api-reference/logos-ai/#design-suggestions) for plugin developers:

```rust
use logos_ai::{DesignAnalyzer, DesignContext};

let analyzer = DesignAnalyzer::new(config);
let suggestions = analyzer.analyze(&context);

for suggestion in suggestions {
    if !suggestion.proposed_fix.is_empty() {
        // Apply proposed positions
        for (i, new_rect) in suggestion.proposed_fix.iter().enumerate() {
            update_layer(suggestion.affected_indices[i], *new_rect);
        }
    }
}
```

---

## Frequently Asked Questions

**Q: Why don't I see any suggestions?**  
A: Your design might already be well-aligned! Try the "Run Full Analysis" button (Ctrl+Shift+A) to force a deep scan.

**Q: Can I undo an accepted suggestion?**  
A: Yes, use Ctrl+Z immediately after accepting.

**Q: How does the AI differ from smart guides?**  
A: Smart guides help you align while dragging. Design Suggestions analyze the entire layout and suggest batch fixes.

**Q: Can plugins access these suggestions?**  
A: Yes! See [Plugin Guide — AI APIs](/plugin-guide/#ai-apis).

**Q: Does it work with Auto Layout?**  
A: Yes. The AI understands Auto Layout constraints and won't suggest fixes that conflict with existing constraints.

---

## Next Steps

- [Accessibility Checker →](/user-guide/ai-assistant/accessibility/)
- [Color Palettes →](/user-guide/ai-assistant/color-palettes/)
- [Smart Layouts →](/user-guide/ai-assistant/smart-layouts/)

**Questions?** Join the [Logos Community Forum](https://community.logos.app).
