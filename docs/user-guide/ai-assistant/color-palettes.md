---
title: Color Palettes
desc: Generate harmonious color schemes based on classic color theory — complementary, triadic, analogous, and more.
eleventyNavigation:
  key: Color Palettes
  parent: AI Assistant
  order: 4
---

# Color Palettes

The **Color Palette Generator** creates beautiful, harmonious color schemes based on classic color theory. No more guesswork — just pick a base color and let the AI do the math.

<img src="/img/ai-color-palettes.webp" alt="Color Palette Generator" style="border: 1px solid #ddd; border-radius: 4px; margin: 20px 0;">

---

## Harmony Schemes

### 1. Complementary (2 colors)

**Theory:** Colors opposite on the color wheel (180° apart).  
**Use Case:** High contrast, call-to-action buttons.

**Example:**

```
Base: Blue (210°)
→ Complementary: Orange (30°)
```

<div style="display: flex; gap: 10px; margin: 10px 0;">
  <div style="background: hsl(210, 80%, 50%); width: 80px; height: 80px; border-radius: 4px;"></div>
  <div style="background: hsl(30, 80%, 50%); width: 80px; height: 80px; border-radius: 4px;"></div>
</div>

**Best for:** Logos, CTAs, bold designs

---

### 2. Analogous (3 colors)

**Theory:** Colors adjacent on the wheel (±30°).  
**Use Case:** Harmonious, cohesive palettes.

**Example:**

```
Base: Blue (210°)
→ Analogous: Cyan (180°), Purple (240°)
```

<div style="display: flex; gap: 10px; margin: 10px 0;">
  <div style="background: hsl(180, 80%, 50%); width: 60px; height: 60px; border-radius: 4px;"></div>
  <div style="background: hsl(210, 80%, 50%); width: 60px; height: 60px; border-radius: 4px;"></div>
  <div style="background: hsl(240, 80%, 50%); width: 60px; height: 60px; border-radius: 4px;"></div>
</div>

**Best for:** Backgrounds, gradients, calming designs

---

### 3. Triadic (3 colors)

**Theory:** Three colors evenly spaced (120° apart).  
**Use Case:** Vibrant, balanced palettes.

**Example:**

```
Base: Red (0°)
→ Triadic: Green (120°), Blue (240°)
```

<div style="display: flex; gap: 10px; margin: 10px 0;">
  <div style="background: hsl(0, 80%, 50%); width: 60px; height: 60px; border-radius: 4px;"></div>
  <div style="background: hsl(120, 80%, 50%); width: 60px; height: 60px; border-radius: 4px;"></div>
  <div style="background: hsl(240, 80%, 50%); width: 60px; height: 60px; border-radius: 4px;"></div>
</div>

**Best for:** Playful brands, infographics, dashboards

---

### 4. Split-Complementary (3 colors)

**Theory:** Base + two colors adjacent to its complement (base + 150°, base + 210°).  
**Use Case:** Less tension than complementary, more interesting than analogous.

**Example:**

```
Base: Blue (210°)
→ Split-Complementary: Yellow-Orange (30°), Red-Orange (0°)
```

<div style="display: flex; gap: 10px; margin: 10px 0;">
  <div style="background: hsl(210, 80%, 50%); width: 60px; height: 60px; border-radius: 4px;"></div>
  <div style="background: hsl(30, 80%, 50%); width: 60px; height: 60px; border-radius: 4px;"></div>
  <div style="background: hsl(0, 80%, 50%); width: 60px; height: 60px; border-radius: 4px;"></div>
</div>

**Best for:** Sophisticated, nuanced designs

---

### 5. Tetradic / Square (4 colors)

**Theory:** Four colors evenly spaced (90° apart).  
**Use Case:** Rich, complex palettes.

**Example:**

```
Base: Red (0°)
→ Tetradic: Yellow-Green (90°), Cyan (180°), Purple (270°)
```

<div style="display: flex; gap: 10px; margin: 10px 0;">
  <div style="background: hsl(0, 80%, 50%); width: 50px; height: 50px; border-radius: 4px;"></div>
  <div style="background: hsl(90, 80%, 50%); width: 50px; height: 50px; border-radius: 4px;"></div>
  <div style="background: hsl(180, 80%, 50%); width: 50px; height: 50px; border-radius: 4px;"></div>
  <div style="background: hsl(270, 80%, 50%); width: 50px; height: 50px; border-radius: 4px;"></div>
</div>

**Best for:** Data visualizations, multi-category UIs

---

### 6. Pentadic / Star (5 colors)

**Theory:** Five colors evenly spaced (72° apart).  
**Use Case:** Maximum variety, charts with 5+ categories.

**Example:**

```
Base: Red (0°)
→ Pentadic: 72°, 144°, 216°, 288°
```

<div style="display: flex; gap: 10px; margin: 10px 0;">
  <div style="background: hsl(0, 80%, 50%); width: 45px; height: 45px; border-radius: 4px;"></div>
  <div style="background: hsl(72, 80%, 50%); width: 45px; height: 45px; border-radius: 4px;"></div>
  <div style="background: hsl(144, 80%, 50%); width: 45px; height: 45px; border-radius: 4px;"></div>
  <div style="background: hsl(216, 80%, 50%); width: 45px; height: 45px; border-radius: 4px;"></div>
  <div style="background: hsl(288, 80%, 50%); width: 45px; height: 45px; border-radius: 4px;"></div>
</div>

**Best for:** Complex dashboards, multi-layered branding

---

## How to Generate a Palette

### Method 1: From Selected Layer

1. Select a layer with a color (rectangle, text, etc.)
2. Press **Alt+P** (or right-click → **Generate Palette**)
3. Choose scheme: Complementary, Triadic, etc.
4. Palette appears in **Color Tokens** panel

### Method 2: From Color Picker

1. Open color picker (double-click any fill/stroke color)
2. Click **palette icon** in bottom-right corner
3. Select scheme
4. Generated colors appear as swatches below picker

### Method 3: Command Palette

1. Open Command Palette (Ctrl+Shift+P)
2. Type "Generate Color Palette"
3. Choose base color from document or enter hex code
4. Select scheme

---

## Palette Variations

Generate **multiple variations** with adjusted saturation/lightness:

**Example:**

```
Base: Blue (210°, 80% saturation, 50% lightness)

Variations:
  1. Normal → S=80%, L=50%
  2. Muted → S=50%, L=50%  (lower saturation)
  3. Pastel → S=80%, L=70%  (higher lightness)
  4. Dark → S=80%, L=30%    (lower lightness)
```

**How to generate:**

1. Right-click existing palette → **Generate Variations**
2. Logos creates 4 versions (normal, muted, pastel, dark)
3. Choose the one that fits your design

---

## Color Temperature

Logos automatically classifies colors by **temperature**:

| Temperature | Hue Range | Feel |
|-------------|-----------|------|
| **Warm** | 0-60° | Energetic, passionate (red, orange, yellow) |
| **Neutral** | 60-120°, 300-360° | Balanced (green, yellow-green, magenta) |
| **Cool** | 120-300° | Calm, trustworthy (cyan, blue, purple) |

**Use Case:** Ensure consistent temperature across your palette for cohesive mood.

**Example:**

```
Warm Palette:
  Red (0°), Orange (30°), Yellow (60°)
  → All warm → Energetic, active feel

Mixed Temperature:
  Red (0°), Blue (210°), Green (120°)
  → Warm + cool + neutral → Chaotic, unbalanced
```

**AI Suggestion:** *"Your palette mixes warm and cool tones — consider standardizing temperature for better cohesion."*

---

## Accessibility Integration

All generated palettes are **automatically checked for WCAG contrast**:

**Example:**

```
Generated Triadic Palette:
  #3B82F6 (blue)
  #10B981 (green)
  #F59E0B (orange)

Accessibility Report:
  ✅ Blue on white: 5.32:1 (passes AA)
  ⚠️ Green on white: 3.21:1 (fails AA)
  ✅ Orange on white: 4.87:1 (passes AA)

Suggestion: "Darken green to #059669 for AA compliance (4.52:1)"
```

Click **"Fix Accessibility"** → Logos adjusts colors to meet AA standard while preserving harmony.

---

## Palette Presets

**Popular palettes** based on real-world design systems:

### Material Design

```
Primary: #6200EE (purple)
Secondary: #03DAC6 (teal)
Error: #B00020 (red)
Background: #FFFFFF
Surface: #F5F5F5
```

### Tailwind CSS

```
Blue: #3B82F6
Green: #10B981
Red: #EF4444
Yellow: #F59E0B
Purple: #8B5CF6
```

### Flat UI Colors

```
Turquoise: #1ABC9C
Emerald: #2ECC71
Peter River: #3498DB
Amethyst: #9B59B6
Wet Asphalt: #34495E
```

**To use:** File → New → Color Palette → Choose preset.

---

## Exporting Palettes

### Format Options

1. **Design Tokens** (JSON)
   ```json
   {
     "primary": "#3B82F6",
     "secondary": "#10B981",
     "accent": "#F59E0B"
   }
   ```

2. **CSS Variables**
   ```css
   :root {
     --color-primary: #3B82F6;
     --color-secondary: #10B981;
     --color-accent: #F59E0B;
   }
   ```

3. **Sass Variables**
   ```scss
   $color-primary: #3B82F6;
   $color-secondary: #10B981;
   $color-accent: #F59E0B;
   ```

4. **Tailwind Config**
   ```js
   module.exports = {
     theme: {
       colors: {
         primary: '#3B82F6',
         secondary: '#10B981',
         accent: '#F59E0B',
       }
     }
   }
   ```

5. **Adobe Swatch (.ase)**  
   Import into Photoshop, Illustrator, After Effects.

6. **PNG Image**  
   Visual reference for sharing with clients.

---

## Examples

### Example 1: Brand Palette

**Goal:** Create a professional SaaS brand palette.

**Steps:**

1. Pick brand color: `#3B82F6` (blue)
2. Generate **Split-Complementary** scheme
3. Results:
   - Primary: `#3B82F6` (blue)
   - Secondary: `#F59E0B` (orange)
   - Accent: `#EF4444` (red)
4. Check accessibility → All pass AA ✅
5. Export as CSS variables

---

### Example 2: Dashboard with 5 Categories

**Goal:** Distinct colors for 5 data categories.

**Steps:**

1. Pick neutral base: `#6366F1` (indigo)
2. Generate **Pentadic** scheme
3. Results:
   - Category A: `#6366F1` (indigo)
   - Category B: `#10B981` (green)
   - Category C: `#F59E0B` (yellow)
   - Category D: `#EF4444` (red)
   - Category E: `#8B5CF6` (purple)
4. Test with **Color Blindness Simulator** → All distinguishable ✅

---

### Example 3: Calming Mobile App

**Goal:** Soothing, cohesive palette for meditation app.

**Steps:**

1. Pick base: `#10B981` (green)
2. Generate **Analogous** scheme
3. Results:
   - Primary: `#10B981` (green)
   - Secondary: `#06B6D4` (cyan)
   - Accent: `#3B82F6` (blue)
4. All colors are **cool temperature** → Calming effect ✅
5. Generate **Muted variation** (lower saturation) for gentler look

---

## Harmony Score

Logos calculates a **harmony score** (0-100%) for custom palettes:

**Factors:**

- **Hue spacing:** Are colors evenly distributed on the wheel?
- **Saturation consistency:** Similar saturation levels?
- **Lightness range:** Adequate contrast without extremes?
- **Temperature coherence:** Consistent warm/cool bias?

**Example:**

```
Your Palette:
  #3B82F6 (blue, 210°, S=80%, L=50%)
  #10B981 (green, 150°, S=80%, L=49%)
  #8B5CF6 (purple, 270°, S=81%, L=52%)

Harmony Score: 87%
  ✅ Saturation: Excellent (80-81% — very consistent)
  ✅ Lightness: Good (49-52% — similar values)
  ⚠️ Hue Spacing: Uneven (60°, 60°, 240° — not perfectly balanced)

Suggestion: "Rotate green to 120° for perfect triadic spacing (score → 95%)"
```

---

## Performance

- **Palette Generation:** ~5µs (near-instant)
- **Accessibility Check:** ~30µs per color pair
- **Color Blindness Simulation:** ~20µs per color
- **Max Palette Size:** 50 colors (no practical limit)

---

## API Access

Plugin developers can use the Palette Generator API:

```rust
use logos_ai::{PaletteGenerator, HslColor, HarmonyScheme};

let base = HslColor { h: 210.0, s: 0.8, l: 0.5 };
let generator = PaletteGenerator::new();
let palette = generator.generate(base, HarmonyScheme::Triadic);

for color in palette.to_rgb() {
    println!("Color: #{:02X}{:02X}{:02X}", 
        (color.r * 255.0) as u8,
        (color.g * 255.0) as u8,
        (color.b * 255.0) as u8
    );
}
```

See [API Reference — Color Harmony](/api-reference/logos-ai/#color-harmony) for full details.

---

## Frequently Asked Questions

**Q: Can I edit generated palettes?**  
A: Yes! Generated palettes are fully editable. Adjust individual colors in the Color Tokens panel.

**Q: How does Logos ensure accessibility?**  
A: Every palette is automatically tested against all generated color pairs. Warnings appear if any pair fails WCAG AA.

**Q: Can I import palettes from other tools?**  
A: Yes. File → Import → Color Palette → Supports .ase (Adobe), .aco (Photoshop), .gpl (GIMP), JSON.

**Q: What's the difference between HSL and HSV?**  
A: Logos uses **HSL** (Hue, Saturation, Lightness) internally because it better matches human perception. HSV (Value) is supported for import/export.

**Q: Can I generate gradients from palettes?**  
A: Yes! Right-click palette → **Generate Gradient** → Logos creates a smooth gradient using all colors in order.

---

## Next Steps

- [Accessibility Checker →](/user-guide/ai-assistant/accessibility/) — Ensure palettes meet WCAG standards
- [Design Suggestions →](/user-guide/ai-assistant/design-suggestions/) — Apply consistent colors across designs
- [API Reference →](/api-reference/logos-ai/#color-harmony) — Use in plugins

**Learn more:** [Color Theory Basics](https://www.interaction-design.org/literature/topics/color-theory) (external resource)
