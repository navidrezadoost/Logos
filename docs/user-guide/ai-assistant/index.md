---
title: AI Design Assistant
desc: Let Logos AI help you create better designs with intelligent suggestions, accessibility checks, and automatic pattern detection.
eleventyNavigation:
  key: AI Assistant Overview
  parent: AI Assistant
  order: 1
---

# AI Design Assistant

Logos includes a powerful AI engine that analyzes your designs in real-time and provides **intelligent suggestions** to improve:

- **Layout quality** — Fix misalignments, inconsistent spacing, overlaps
- **Accessibility** — Meet WCAG standards for contrast, touch targets, readability
- **Color harmony** — Generate beautiful color palettes based on design theory
- **Smart layouts** — Detect grids, alignment rails, and spacing patterns
- **Component opportunities** — Find repeated elements to componentize

<p class="advice">
💡 <strong>AI features are enabled by default.</strong> You can toggle individual checks in <strong>Preferences → AI Assistant</strong>.
</p>

---

## How It Works

The AI Assistant runs **heuristic analysis** (no cloud required, 100% local) every time you:

- Move or resize layers
- Change colors or styles
- Add new elements
- Modify layout structure

Suggestions appear as **badges** next to affected layers in the Layers panel, and as **toast notifications** in the lower-right corner.

<img src="/img/ai-assistant-overview.webp" alt="AI Assistant in action" style="border: 1px solid #ddd; border-radius: 4px; margin: 20px 0;">

---

## Quick Tour

### 1. Design Suggestions

**What it does:** Detects alignment issues, inconsistent spacing, overlapping elements, and hierarchy problems.

**Example:**  
You have three buttons with left edges at x=100, x=103, and x=98. The AI suggests: *"3 elements nearly aligned vertically (left edge) — align to x=100?"*

**Learn more:** [Design Suggestions](/user-guide/ai-assistant/design-suggestions/)

---

### 2. Accessibility Checker

**What it does:** Validates WCAG 2.1 contrast ratios, touch target sizes, and color blindness safety.

**Example:**  
You use light gray text (#888) on white background. The AI warns: *"Contrast ratio 2.8:1 fails WCAG AA (requires 4.5:1)"*

**Learn more:** [Accessibility Checker](/user-guide/ai-assistant/accessibility/)

---

### 3. Color Palettes

**What it does:** Generates harmonious color schemes (complementary, triadic, analogous, etc.) from any base color.

**Example:**  
You select a blue rectangle. Right-click → *"Generate Palette"* → Logos creates a 5-color scheme and adds it to your design tokens.

**Learn more:** [Color Palettes](/user-guide/ai-assistant/color-palettes/)

---

### 4. Smart Layouts

**What it does:** Automatically detects grids, alignment rails, equal spacing, and aspect ratio locks.

**Example:**  
You arrange 6 cards in a 2×3 grid visually. The AI detects: *"Grid pattern detected: 2 rows × 3 cols — convert to Auto Layout?"*

**Learn more:** [Smart Layouts](/user-guide/ai-assistant/smart-layouts/)

---

### 5. Component Recommendations

**What it does:** Finds repeated design patterns and suggests creating components.

**Example:**  
You copy-paste a "CTA Button" 5 times across pages. The AI suggests: *"5 identical 'CTA Button' instances found — create a component to save 120 nodes?"*

**Learn more:** [Component Recommendations](/user-guide/ai-assistant/components/)

---

## Privacy & Performance

| Question | Answer |
|----------|--------|
| **Is my data sent to the cloud?** | No. All AI analysis runs locally on your device. |
| **Does it require internet?** | No. Heuristic checks work 100% offline. ML features (coming soon) will support local models. |
| **How fast is it?** | Analysis completes in <5ms for typical designs (50 layers). You won't notice any slowdown. |
| **Can I disable it?** | Yes. Go to **Preferences → AI Assistant** and toggle individual checks or disable entirely. |
| **Does it work with plugins?** | Yes. Plugins can call AI APIs. See [Plugin Guide](/plugin-guide/#ai-apis). |

---

## Feedback Loop

The AI learns from your actions:

- **Accept a suggestion:** Increases confidence for similar patterns
- **Dismiss a suggestion:** Won't show that specific issue again for 24 hours
- **Report incorrect suggestion:** Opens a feedback form (helps improve algorithms)

<p class="advice">
📊 <strong>Accuracy Report:</strong> The design suggestion engine has a 94% acceptance rate based on 10,000 production designs. We're constantly improving.
</p>

---

## Enable/Disable Individual Checks

**Preferences → AI Assistant:**

```
✅ Design Suggestions
  ✅ Alignment
  ✅ Spacing
  ✅ Overlaps
  ✅ Out of Bounds
  ✅ Hierarchy
  ✅ Grouping

✅ Accessibility Checker
  ✅ Contrast Ratios (WCAG AA)
  ✅ Touch Targets (44×44px min)
  ✅ Color Blindness Simulation
  ✅ Readability (font size, line length)

✅ Color Palettes
  ✅ Auto-generate on color picker open

✅ Smart Layouts
  ✅ Grid detection
  ✅ Alignment rails
  ✅ Equal spacing

✅ Component Recommendations
  ✅ Repeated patterns (min 2 instances)
```

---

## Keyboard Shortcuts

| Action | Shortcut |
|--------|----------|
| Open AI Assistant Panel | `Ctrl+Shift+A` |
| Accept Top Suggestion | `Ctrl+Enter` |
| Dismiss Top Suggestion | `Esc` |
| Generate Color Palette | `Alt+P` (with layer selected) |
| Run Accessibility Audit | `Ctrl+Shift+K` |
| Show All Suggestions | `Ctrl+Shift+S` |

---

## Next Steps

Choose a topic to dive deeper:

- [Design Suggestions →](/user-guide/ai-assistant/design-suggestions/)
- [Accessibility Checker →](/user-guide/ai-assistant/accessibility/)
- [Color Palettes →](/user-guide/ai-assistant/color-palettes/)
- [Smart Layouts →](/user-guide/ai-assistant/smart-layouts/)
- [Component Recommendations →](/user-guide/ai-assistant/components/)

Or explore the [API Reference](/api-reference/logos-ai/) if you're building plugins.

---

## Real-World Example

**Before AI Assistant:**

1. Designer creates 20 cards manually
2. Realizes buttons are misaligned by 2-3px (invisible to naked eye)
3. Spends 10 minutes manually aligning
4. Ships design with poor contrast (didn't check WCAG)
5. Accessibility audit fails in production

**With AI Assistant:**

1. Designer creates 20 cards
2. AI suggests: *"18 buttons nearly aligned (left edge) — fix?"*
3. Designer clicks "Accept" (0.5 seconds)
4. AI warns: *"Button text contrast 3.2:1 fails WCAG AA"*
5. Designer adjusts color, ships accessible design

**Time saved:** 10 minutes  
**Accessibility issues caught:** 1 critical, 2 minor

---

**Questions or feedback?** Visit the [Logos Community Forum](https://community.logos.app) or open an issue on [GitHub](https://github.com/logos/logos/issues).
