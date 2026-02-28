---
title: Accessibility Checker
desc: Validate WCAG 2.1 compliance — contrast ratios, touch targets, color blindness simulation, and readability.
eleventyNavigation:
  key: Accessibility Checker
  parent: AI Assistant
  order: 3
---

# Accessibility Checker

The **Accessibility Checker** ensures your designs meet **WCAG 2.1** standards, making them usable for everyone.

<img src="/img/ai-accessibility-checker.webp" alt="Accessibility Checker panel" style="border: 1px solid #ddd; border-radius: 4px; margin: 20px 0;">

---

## What It Checks

### 1. Contrast Ratios

**WCAG 2.1 Success Criterion 1.4.3** — Text must have sufficient contrast with its background.

| Level | Normal Text | Large Text* |
|-------|-------------|-------------|
| **A** | 3:1 | 3:1 |
| **AA** | 4.5:1 | 3:1 |
| **AAA** | 7:1 | 4.5:1 |

*\*Large text = 18pt+ (24px+) or 14pt+ bold (18.5px+)*

**Example:**

```
Foreground: #888888 (gray)
Background: #FFFFFF (white)
Contrast Ratio: 2.84:1 ❌ Fails WCAG AA

Suggestion: "Increase contrast to 4.5:1 — try #595959"
```

**How to fix:**
1. Select the text layer
2. Open **AI Assistant → Accessibility**
3. Click **"Fix Contrast"**
4. Logos automatically adjusts text color to meet AA (or prompts you to choose a darker shade)

---

### 2. Touch Targets

**WCAG 2.1 Success Criterion 2.5.5** — Interactive elements must be at least **44×44px** (AAA) or **24×24px** (AA).

**Example:**

```
Button: 36×36px ⚠️ Below AAA (44×44px)

Suggestion: "Button too small — expand to 44×44px or add padding"
```

**How to fix:**
1. Select the button
2. AI Assistant suggests: (A) Increase button size, or (B) Add 4px padding around existing button
3. Click option → Button resized automatically

**Presets:**

- **WCAG AAA:** 44×44px (recommended for mobile)
- **Material Design:** 48×48px
- **Apple HIG:** 44×44px

---

### 3. Color Blindness Simulation

Simulate how your design appears to users with color vision deficiencies:

| Type | Prevalence | Description |
|------|------------|-------------|
| **Protanopia** | 1% males | Red-blind (missing L-cones) |
| **Deuteranopia** | 1.3% males | Green-blind (missing M-cones) |
| **Tritanopia** | 0.002% | Blue-blind (missing S-cones) |
| **Achromatopsia** | 0.003% | Total color blindness |

**How to use:**

1. Open **View → Color Blindness Simulator** (or press `Ctrl+Shift+K`)
2. Select a filter: Protanopia, Deuteranopia, Tritanopia, Achromatopsia
3. Canvas updates in real-time to show simulated colors
4. AI warns if critical elements become indistinguishable

**Example:**

```
Red button (#FF0000) and green button (#00FF00) on same screen

⚠️ Warning: "Red and green buttons are indistinguishable for 
deuteranopia users (1.3% of males). Add text labels or icons."
```

<p class="advice">
💡 <strong>Best Practice:</strong> Never rely on color alone to convey information. Always add text labels, icons, or patterns.
</p>

---

### 4. Readability

Checks font size, line length, and line height per **WCAG 1.4.8** (Visual Presentation).

#### Font Size

| Text Type | Minimum | Recommended | Purpose |
|-----------|---------|-------------|---------|
| **Body** | 14px | 16px | Main content |
| **Caption** | 11px | 12px | Secondary info, footnotes |
| **Heading** | 20px | 24px+ | Section titles |

**Example:**

```
Paragraph text: 11px ⚠️ Below recommended 16px for body text

Suggestion: "Increase font size to 16px for better readability"
```

#### Line Length

- **Recommended:** 50-75 characters per line (CPL)
- **Maximum:** 80 CPL

**Why it matters:** Lines that are too long cause eye strain and reduce reading speed.

**Example:**

```
Text block: 120 characters per line ⚠️ Exceeds 80 CPL

Suggestion: "Reduce line length to 75 CPL — resize text frame to 600px wide"
```

#### Line Height

- **Recommended:** 1.4× font size (WCAG 1.4.12)
- **Minimum:** 1.25×

**Example:**

```
16px font with 18px line-height (1.125×) ⚠️ Below 1.4× recommended

Suggestion: "Increase line-height to 22px (1.375×)"
```

---

## Running an Accessibility Audit

### Option 1: Automatic (Real-Time)

Accessibility checks run automatically as you design. Warnings appear as **badges** next to layers in the Layers panel.

**Badge colors:**
- 🔴 Red: Critical issue (WCAG A failure)
- 🟡 Yellow: Warning (WCAG AA failure, AAA pass)
- 🔵 Blue: Info (AAA failure, AA pass)

### Option 2: Manual (Full Document Scan)

1. Open **AI Assistant** panel (Ctrl+Shift+A)
2. Click **"Run Accessibility Audit"** (or Ctrl+Shift+K)
3. Review findings in the **Accessibility** tab
4. Click **"Fix All"** to apply all suggested fixes at once

---

## Accessibility Report

After running a full audit, Logos generates a **downloadable report**:

### Summary

```
✅ 45 checks passed
⚠️ 3 warnings (WCAG AA issues)
❌ 2 errors (WCAG A failures)

Overall Grade: B+ (87%)
```

### Details

```
CONTRAST RATIOS
  ❌ Text layer "Login" — 2.84:1 (requires 4.5:1 for AA)
  ✅ Button "Submit" — 12.1:1 (passes AAA)

TOUCH TARGETS
  ⚠️ Icon button — 36×36px (below 44×44px AAA)
  ✅ Primary CTA — 48×48px (passes AAA)

COLOR BLINDNESS
  ⚠️ Red/green indicators indistinguishable for deuteranopia users
  ✅ All text readable in grayscale

READABILITY
  ❌ Body text — 12px (below 16px recommended)
  ✅ Line length — 68 CPL (within 50-75 range)
```

**Export formats:** PDF, HTML, JSON

---

## Configuring Standards

**Preferences → AI Assistant → Accessibility:**

```
Target Level: [AA ▼] (A / AA / AAA)

Checks:
  ✅ Contrast Ratios
  ✅ Touch Targets (44×44px min)
  ✅ Color Blindness Simulation
  ✅ Font Sizes
  ✅ Line Length (80 CPL max)
  ✅ Line Height

Warnings:
  ✅ Show real-time badges
  ✅ Toast notifications for critical issues
  ⬜ Verbose mode (explain each issue)
```

---

## Contrast Ratio Calculator

**Built-in tool** for testing color combinations:

1. Open **Tools → Contrast Checker** (Ctrl+Alt+C)
2. Pick foreground and background colors
3. See real-time contrast ratio and WCAG compliance

**Example:**

```
Foreground: #3B82F6 (blue)
Background: #FFFFFF (white)

Contrast Ratio: 5.32:1
✅ WCAG AA Normal Text (4.5:1)
✅ WCAG AAA Large Text (4.5:1)
❌ WCAG AAA Normal Text (7:1)

Suggestion: "Darken blue to #2563EB for AAA compliance (7.14:1)"
```

**Quick Actions:**
- **Auto-Fix:** Adjusts color automatically to meet selected level
- **Swap Colors:** Inverts foreground and background
- **Save Pair:** Adds to design tokens for reuse

---

## Color Blindness Filters

### Testing Your Design

1. Select artboard or frame
2. Right-click → **Simulate Color Blindness → Protanopia**
3. Canvas updates to show simulated view
4. AI highlights any indistinguishable elements

### Comparing Filters

**View → Color Blindness Grid**

Shows 2×2 grid:
- Original
- Protanopia (red-blind)
- Deuteranopia (green-blind)
- Tritanopia (blue-blind)

<img src="/img/cvd-grid.webp" alt="Color blindness comparison grid" style="border: 1px solid #ddd; border-radius: 4px; margin: 20px 0;">

---

## Examples

### Example 1: Fixing Low Contrast

**Before:**

```
Text: #999999 on #FFFFFF
Contrast: 2.84:1 ❌ Fails AA
```

**AI Suggestion:** *"Darken text to #595959 for 7.0:1 (AAA)"*

**After:**

```
Text: #595959 on #FFFFFF
Contrast: 7.0:1 ✅ Passes AAA
```

---

### Example 2: Expanding Touch Targets

**Before:**

```
Close icon: 32×32px ⚠️ Below AAA (44×44px)
```

**AI Suggestion:** *"Add 6px padding around icon"*

**After:**

```
Close button: 44×44px (icon + padding) ✅ Passes AAA
```

---

### Example 3: Color Blindness Safety

**Before:**

```
Status indicators:
  🔴 Red = Error
  🟢 Green = Success
```

**AI Warning:** *"Red/green are indistinguishable for deuteranopia users (1.3% of males)"*

**After:**

```
Status indicators:
  ❌ Red cross icon = Error
  ✅ Green checkmark icon = Success
```

Now distinguishable by shape, not just color.

---

## Performance

- **Contrast Check:** ~10µs per layer
- **Touch Target Check:** ~5µs per layer
- **Full Audit (100 layers):** ~50ms
- **Color Blindness Simulation:** ~20µs per color

---

## API Access

Plugin developers can use the Accessibility Checker API:

```rust
use logos_ai::{AccessibilityChecker, WcagLevel};

let checker = AccessibilityChecker::new();
let result = checker.check_contrast(fg_color, bg_color);

if !result.passes(WcagLevel::AA) {
    println!("⚠️ Contrast ratio {:.2}:1 fails WCAG AA", result.ratio);
}
```

See [API Reference — Accessibility](/api-reference/logos-ai/#accessibility-checking) for full details.

---

## Frequently Asked Questions

**Q: Does Logos support WCAG 2.2 or 3.0?**  
A: Currently WCAG 2.1 Level AAA. We'll add 2.2 (Focus Appearance, Dragging Movements) and 3.0 (APCA contrast) in future releases.

**Q: Can I export an accessibility report for compliance audits?**  
A: Yes. After running an audit, click **Export → PDF/HTML/JSON**.

**Q: Does it check keyboard navigation?**  
A: Not yet. Keyboard navigation (tab order, focus states) is on our roadmap for Phase 14.

**Q: What about screen reader support?**  
A: Logos exports accessible HTML with ARIA labels. The checker validates that text alternatives exist for all images.

**Q: Can I customize the contrast thresholds?**  
A: WCAG ratios are fixed (AA=4.5:1, AAA=7:1), but you can target different levels in Preferences.

---

## Next Steps

- [Color Palettes →](/user-guide/ai-assistant/color-palettes/) — Generate accessible color schemes
- [Design Suggestions →](/user-guide/ai-assistant/design-suggestions/) — Fix layout issues
- [API Reference →](/api-reference/logos-ai/#accessibility-checking) — Use in plugins

**Learn more:** [WCAG 2.1 Guidelines](https://www.w3.org/WAI/WCAG21/quickref/)
