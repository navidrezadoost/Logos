---
title: Component Recommendations
desc: Automatically detect repeated design patterns and suggest creating reusable components to save time and maintain consistency.
eleventyNavigation:
  key: Component Recommendations
  parent: AI Assistant
  order: 6
---

# Component Recommendations

The **Component Recommender** analyzes your design for **repeated patterns** and suggests creating **reusable components** to:

- **Save time:** Update once, apply everywhere
- **Maintain consistency:** Changes propagate automatically
- **Reduce file size:** Instances reference a single definition

<img src="/img/ai-component-recommendations.webp" alt="Component Recommendations panel" style="border: 1px solid #ddd; border-radius: 4px;margin: 20px 0;">

---

## What It Detects

### 1. Repeated Patterns

**Definition:** Elements with the same label and similar size appearing multiple times.

**Example:**

```
Document contains:
  - "CTA Button" (120×40px) — 7 instances across 3 pages
  - "Close Icon" (24×24px) — 12 instances
  - "Card Title" (300×32px) — 8 instances
```

**AI Suggestion:** *"7 'CTA Button' instances found — create component to save ~140 nodes?"*

**Why it matters:** Manual copy-paste creates independent layers. If you later change button color, you must update all 7 manually. With components, update once → all instances reflect the change.

---

### 2. Identical Styles

**Definition:** Elements with the same visual appearance (fills, strokes, effects, fonts) regardless of label.

**Example:**

```
5 rectangles with identical style:
  - Fill: #3B82F6 (blue)
  - Stroke: 2px #1E40AF
  - Shadow: 0px 4px 6px rgba(0,0,0,0.1)
  - Corner radius: 8px
```

**AI Suggestion:** *"5 elements share identical style — create style component?"*

**After accepting:**  
Style is saved as a **Shared Style**. Changes to the style (e.g., border color) propagate to all 5 rectangles.

---

### 3. Shared Groups

**Definition:** Multiple layers with the same group name.

**Example:**

```
Groups named "Icon + Label":
  - Page 1: [Icon] + [Label]
  - Page 2: [Icon] + [Label]
  - Page 3: [Icon] + [Label]
```

**AI Suggestion:** *"3 'Icon + Label' groups found — create component?"*

**After accepting:**  
Groups are replaced with **component instances**. Editing the main component updates all 3 instances.

---

### 4. Composite Recommendations

**Definition:** Elements matching multiple criteria (label + style + group).

**Example:**

```
"Social Icon" elements:
  - Same label: "Social Icon"
  - Same size: 48×48px
  - Same style: Circle with brand color fill
  - Same group: "Footer Links"
```

**Confidence:** **95%** (highest confidence — multiple signals converge)

---

## How It Works

### Detection Algorithm

1. **Parse document:** Extract all layers with labels, sizes, styles, groups
2. **Fuzzy matching:** Group elements with similar names/sizes (10px bucket tolerance)
3. **Style hashing:** Hash fills, strokes, effects for identical-style detection
4. **Confidence scoring:** More instances = higher confidence
5. **Node savings calculation:** Estimate layers saved by componentizing

**Example:**

```
Button instances: 7
Average nodes per button: 3 (frame + text + icon)
Total nodes: 21
Component approach: 1 main component (3 nodes) + 7 instances (0 extra nodes)
Node savings: 21 - 3 = 18 nodes (86% reduction)
```

---

### Confidence Scoring

| Instances | Confidence | Reason |
|-----------|------------|--------|
| 2 | 50% | Could be coincidence |
| 3 | 70% | Likely intentional |
| 4-6 | 85% | Definitely a pattern |
| 7+ | 95% | Strong pattern |

**Note:** Identical styles + same label boost confidence by 10-20%.

---

## Accepting Recommendations

### Option 1: Create Component

Click **"Create Component"** → Logos:

1. Converts first instance to **Main Component**
2. Converts remaining instances to **Component Instances** linked to main
3. Places main component in **Assets** panel

**Result:** Editing main component updates all instances.

### Option 2: Create Shared Style

(For style-only matches)

Click **"Create Shared Style"** → Logos:

1. Extracts fills, strokes, effects into **Shared Style**
2. Applies style to all matching elements
3. Adds style to **Styles** panel for reuse

**Result:** Changing the style updates all elements using it.

### Option 3: Dismiss

Click **"Dismiss"** → Recommendation disappears. You can undo this in **History** panel.

---

## Configuration

**Preferences → AI Assistant → Component Recommendations:**

```
Minimum Instances: [__2__]
  (Patterns require at least this many occurrences)

Min Confidence: [__50__] %
  (Only show recommendations above this threshold)

Size Tolerance: [__10__] px
  (Elements within ±10px bucket are considered similar size)

Checks:
  ✅ Repeated Patterns (by label + size)
  ✅ Identical Styles (by fill/stroke/effects)
  ✅ Shared Groups (by group name)
  ⬜ Cross-page analysis (slower, more thorough)
```

---

## Examples

### Example 1: CTA Button Componentization

**Before:**

```
Page 1: "Sign Up" button (120×40px, blue fill, "Montserrat Bold 16px")
Page 2: "Get Started" button (120×40px, blue fill, "Montserrat Bold 16px")
Page 3: "Join Now" button (120×40px, blue fill, "Montserrat Bold 16px")
```

**Total:** 3 buttons × 2 layers each (frame + text) = **6 nodes**

**AI Suggestion:** *"3 'CTA Button' instances found (confidence: 70%) — save 4 nodes"*

**After Accepting:**

```
Assets Panel:
  └─ CTA Button [Main Component]
       ├─ Frame (blue fill)
       └─ Text (placeholder: "Button Text")

Page 1: CTA Button instance (text override: "Sign Up")
Page 2: CTA Button instance (text override: "Get Started")
Page 3: CTA Button instance (text override: "Join Now")
```

**Total:** 1 main component (2 nodes) + 3 instances (0 extra nodes) = **2 nodes**

**Savings:** 66% reduction (6 → 2 nodes)

**Benefit:** Change button color once → all 3 update instantly.

---

### Example 2: Icon Grid

**Before:**

```
12 social media icons:
  - Instagram (48×48px, pink circle)
  - Twitter (48×48px, blue circle)
  - Facebook (48×48px, blue circle)
  - LinkedIn (48×48px, blue circle)
  - ... (8 more)
```

Each icon = 3 nodes (circle + logo + shadow) = **36 total nodes**

**AI Suggestion:** *"12 'Social Icon' instances with shared style (95% confidence) — save 24 nodes"*

**After Accepting:**

```
Assets Panel:
  └─ Social Icon [Main Component]
       ├─ Circle (brand color — swappable)
       ├─ Logo (icon — swappable)
       └─ Shadow

12 instances with brand color overrides
```

**Total:** 1 main (3 nodes) + 12 instances (0 extra) = **3 nodes**

**Savings:** 92% reduction (36 → 3 nodes)

---

### Example 3: Card Layout

**Before:**

```
8 product cards (identical layout, different content):
  ├─ Image (300×200px)
  ├─ Title (text)
  ├─ Description (text)
  ├─ Price (text)
  └─ "Add to Cart" button
```

Each card = 5 nodes × 8 cards = **40 nodes**

**AI Suggestion:** *"8 'Product Card' instances detected (85% confidence) — save 32 nodes"*

**After Accepting:**

```
Assets Panel:
  └─ Product Card [Main Component with slots]
       ├─ Image (swappable)
       ├─ Title (text override)
       ├─ Description (text override)
       ├─ Price (text override)
       └─ Button (component nested)

8 instances with content overrides
```

**Total:** 1 main (5 nodes) + nested button component (2 nodes) + 8 instances (0 extra) = **7 nodes**

**Savings:** 82% reduction (40 → 7 nodes)

**Benefit:** Redesign card once (e.g., add review stars) → all 8 update.

---

## Node Savings Calculation

**Formula:**

```
Total Nodes (Before) = Instances × NodesPerInstance
Total Nodes (After)  = NodesInMainComponent
Savings              = (Before - After) / Before × 100%
```

**Example:**

```
10 buttons, 3 nodes each
Before: 10 × 3 = 30 nodes
After: 3 nodes (main component)
Savings: (30 - 3) / 30 × 100% = 90%
```

**Why it matters:**

- **File size:** Smaller .logos files (better performance)
- **Rendering:** Fewer nodes = faster canvas updates
- **Memory:** Lower RAM usage on large documents

---

## Advanced: Fuzzy Size Matching

Elements don't need **exact** sizes to be grouped — Smart matching uses **10px buckets**:

**Example:**

```
Button A: 120×40px → Bucket (12, 4)
Button B: 123×41px → Bucket (12, 4)  ← Same bucket!
Button C: 130×50px → Bucket (13, 5)  ← Different bucket
```

**Result:** A and B are considered "similar size", C is not.

**Why:** Real designs often have ±2-3px variances due to manual positioning. Fuzzy matching catches these.

**Adjust tolerance:** Preferences → Component Recommendations → Size Tolerance.

---

## Integration with Design Systems

Component recommendations work seamlessly with **Design Systems**:

1. AI suggests component
2. You accept → Component created
3. **Auto-publish to design system** (if enabled in Preferences)
4. Other team members can now use the component

**Result:** Consistent components across the entire organization.

---

## Performance

- **Analysis Speed:** <1ms for 50 elements
- **Real-time:** Runs after every edit (debounced by 500ms)
- **Max Elements:** Tested up to 5,000 layers

---

## API Access

Plugin developers can use the Component Recommender API:

```rust
use logos_ai::{ComponentRecommender, DesignElement, RecommenderConfig};

let elements = document.layers.iter().map(|layer| {
    DesignElement {
        index: layer.index,
        label: layer.name.clone(),
        width: layer.bounds.width,
        height: layer.bounds.height,
        style_hash: compute_style_hash(&layer),
        group: layer.group_name.clone(),
    }
}).collect();

let config = RecommenderConfig::default();
let recommender = ComponentRecommender::new(config);
let summary = recommender.recommend_all(&elements);

for rec in summary.recommendations {
    println!("Component '{}': {} instances, saves {} nodes",
        rec.name,
        rec.instances.len(),
        rec.node_savings
    );
}
```

See [API Reference — Component Recommendations](/api-reference/logos-ai/#component-recommendations) for full details.

---

## Comparison with Manual Approach

### Manual Component Creation

1. Spot repeated pattern visually
2. Select first instance
3. Create Component → Assets Panel → **Create Component**
4. Manually find and replace remaining instances one by one
5. **Time:** 2-5 minutes per component

### AI-Powered Component Recommendations

1. AI automatically detects all patterns
2. Review suggestions in AI Assistant panel
3. Click **"Create Component"**
4. AI replaces all instances automatically
5. **Time:** 5 seconds per component

**Speedup:** 24-60× faster

---

## Recommendation Summary

After analyzing your document, Logos shows a **summary**:

```
Component Recommendations: 8 total

  1. "CTA Button" — 7 instances, 18 nodes saved (95% confidence)
  2. "Icon" — 12 instances, 24 nodes saved (85% confidence)
  3. "Card" — 8 instances, 32 nodes saved (85% confidence)
  4. "Badge" — 5 instances, 10 nodes saved (70% confidence)
  5. "Header" — 3 instances, 9 nodes saved (70% confidence)
  6. "Footer Link" — 4 instances, 8 nodes saved (50% confidence)
  7. "Modal" — 2 instances, 20 nodes saved (50% confidence)
  8. "Input Field" — 6 instances, 12 nodes saved (85% confidence)

Total Potential Savings: 133 nodes (72% reduction)
Estimated Time Saved: 18 minutes (vs. manual componentization)

Actions:
  [Accept All (8)]  [Dismiss All]  [Review Individually]
```

---

## Frequently Asked Questions

**Q: Can I rename components after creation?**  
A: Yes. Double-click the component in Assets panel → Rename. All instances remain linked.

**Q: What if I want different variants (e.g., primary/secondary button)?**  
A: Use **Component Variants**. After creating the component, right-click → **Add Variant** → Configure states (e.g., "type: primary | secondary").

**Q: Can I override individual properties in instances?**  
A: Yes! Text, fills, strokes, visibility, and nested instances are all overridable. See [Components Guide](/user-guide/design-systems/components/).

**Q: Does it detect components across different files?**  
A: Not yet. Cross-file analysis is planned for Phase 15 (Design System enhancements).

**Q: What if AI suggests a component I don't want?**  
A: Dismiss it. Dismissed recommendations won't re-appear for that document (unless you manually trigger **"Analyze Again"**).

**Q: Can I export component recommendations for handoff?**  
A: Yes. File → Export → Component Report → JSON. Useful for showing developers what should be implemented as reusable components.

---

## Best Practices

### 1. Review Before Accepting

**Why:** AI might group similar-looking elements that serve different purposes.

**Example:** "Submit" button and "Delete" button look identical but have different semantics. Creating a single component might be wrong.

**Solution:** Review each recommendation. If semantic meaning differs, dismiss and create separate components manually.

---

### 2. Use Descriptive Labels

**Before AI:**

```
Rectangle 1, Rectangle 2, Rectangle 3
```

**AI:** Can't detect pattern (labels don't match)

**After AI-friendly labeling:**

```
Card, Card Copy, Card Copy 2
```

**AI:** Detects 3 "Card" instances (fuzzy label matching)

**Tip:** Rename layers descriptively **before** running AI analysis.

---

### 3. Accept High-Confidence Recommendations First

**Strategy:**

1. Sort by confidence (descending)
2. Accept 90-100% recommendations immediately
3. Review 70-89% recommendations carefully
4. Dismiss <70% unless obviously correct

**Why:** High-confidence = less risk of false positives.

---

### 4. Leverage Auto-Publish

**Workflow:**

1. Enable **Preferences → Design Systems → Auto-Publish Components**
2. Accept AI recommendation
3. Component automatically published to team library
4. Other designers can use it immediately

**Benefit:** Accelerates design system growth.

---

## Next Steps

- [Design Systems → Components](/user-guide/design-systems/components/) — Deep dive into component features
- [Smart Layouts →](/user-guide/ai-assistant/smart-layouts/) — Combine with constraint detection for powerful workflows
- [API Reference →](/api-reference/logos-ai/#component-recommendations) — Use in plugins

**Video Tutorial:** [Componentizing a Design in 60 Seconds with AI](https://www.youtube.com/watch?v=...) (Coming Soon)
