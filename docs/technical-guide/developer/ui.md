---
title: 3.08. UI Guide
desc: "Logos UI Guide: design system components, styling with CSS Modules + Sass, and React patterns."
---

# UI Guide

This guide covers how to build and extend UI components in `logos-app/`. The frontend
uses **React 19**, **TypeScript**, **CSS Modules**, and **Sass**.

For the component library in isolation see the [Storybook](#storybook) section.

---

## Design Principles

| Principle | How We Apply It |
|---|---|
| **Composable** | Components accept `children` and slot props instead of embedding behavior |
| **Typed** | Every prop is typed; no `any`; `strict: true` in `tsconfig.json` |
| **Isolated styles** | CSS Modules per component — no global class leakage |
| **Accessible** | ARIA attributes, keyboard navigation, focus management |
| **Small** | If a component exceeds ~150 lines, extract sub-components |

---

## Component Structure

```
logos-app/src/
├── design/
│   └── components/         Reusable canvas-level components (shapes, overlays)
├── workspace/
│   └── components/         Design panel UI (fills, strokes, effects, layers)
├── dashboard/
│   └── components/         File browser, project cards, team pages
└── shared/
    └── ui/                 Generic UI primitives (Button, Input, Modal, Toast, …)
        ├── Button.tsx
        ├── Button.module.scss
        └── Button.stories.tsx
```

---

## Writing a Component

### Basic pattern

```tsx
// src/shared/ui/Button/Button.tsx
import styles from "./Button.module.scss";
import clsx from "clsx";

interface ButtonProps {
  label: string;
  variant?: "primary" | "secondary" | "ghost";
  disabled?: boolean;
  onClick?: () => void;
  className?: string;
  children?: React.ReactNode;
}

export function Button({
  label,
  variant = "primary",
  disabled = false,
  onClick,
  className,
  children,
}: ButtonProps) {
  return (
    <button
      className={clsx(styles.button, styles[variant], className)}
      disabled={disabled}
      onClick={onClick}
      aria-label={label}
    >
      {children ?? label}
    </button>
  );
}
```

### CSS Module

```scss
// src/shared/ui/Button/Button.module.scss
.button {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  border-radius: var(--radius-md);
  font: var(--font-body-sm);
  cursor: pointer;
  transition: background 120ms ease;

  &.primary   { background: var(--color-accent); color: #fff; }
  &.secondary { background: transparent; border: 1px solid var(--color-border); }
  &.ghost     { background: transparent; }

  &:disabled  { opacity: 0.4; cursor: not-allowed; }
}
```

---

## Composability

### Children prop

```tsx
// Usage
<Button label="Save">
  <SaveIcon /> Save changes
</Button>
```

### Slot props (for structured composition)

Prefer slot props when the parent needs to control the structure of a specific piece:

```tsx
interface PanelProps {
  header: React.ReactNode;
  footer?: React.ReactNode;
  children: React.ReactNode;
}

function Panel({ header, footer, children }: PanelProps) {
  return (
    <div className={styles.panel}>
      <div className={styles.header}>{header}</div>
      <div className={styles.body}>{children}</div>
      {footer && <div className={styles.footer}>{footer}</div>}
    </div>
  );
}
```

---

## Styling with CSS Modules + Sass

### Class composition

```tsx
import styles from "./Thing.module.scss";
import clsx from "clsx";

// Conditional classes
<div className={clsx(styles.root, isActive && styles.active, className)} />
```

### CSS custom properties (design tokens)

All colors, spacing, typography, and radius values are CSS custom properties defined
in `logos-app/src/styles/tokens/`. Use them everywhere instead of hardcoded values:

```scss
.card {
  background: var(--color-surface);
  padding: var(--sp-4) var(--sp-6);
  border-radius: var(--radius-lg);
  font: var(--font-body-md);
  color: var(--color-text-primary);
}
```

### Variant patterns

```scss
// Data-attribute driven variants (avoids class proliferation)
.badge {
  padding: var(--sp-1) var(--sp-2);
  border-radius: var(--radius-sm);

  &[data-level="info"]    { background: var(--color-info-subtle); }
  &[data-level="success"] { background: var(--color-success-subtle); }
  &[data-level="warning"] { background: var(--color-warning-subtle); }
  &[data-level="error"]   { background: var(--color-error-subtle); }
}
```

```tsx
<span className={styles.badge} data-level={level}>
  {message}
</span>
```

---

## State Management Patterns

### Local state (single component)

Use `useState` or `useReducer`:

```tsx
function ColorPicker() {
  const [hex, setHex] = useState("#000000");
  // ...
}
```

### Shared UI state (panel/section-level)

Use Zustand stores defined in the relevant feature directory:

```tsx
// src/workspace/store/inspectorStore.ts
import { create } from "zustand";

export const useInspectorStore = create<InspectorStore>((set) => ({
  activeTab: "design",
  setActiveTab: (tab) => set({ activeTab: tab }),
}));

// In a component
const { activeTab, setActiveTab } = useInspectorStore();
```

### Server state (API data)

Use React Query or direct `fetch` with `useEffect` + `useState`. Keep API calls in
dedicated hook files, not inside components:

```tsx
// src/dashboard/hooks/useFiles.ts
export function useFiles(projectId: string) {
  const [files, setFiles] = useState<FileEntry[]>([]);
  useEffect(() => {
    fetch(`/api/rpc/command/get-project-files`, { method: "POST", body: JSON.stringify({ projectId }) })
      .then(r => r.json())
      .then(setFiles);
  }, [projectId]);
  return files;
}
```

---

## Accessibility

- All interactive elements (buttons, inputs, toggles) must be focusable and have an accessible label
- Use semantic HTML (`<button>`, `<nav>`, `<main>`, `<dialog>`) over `<div>` with `onClick`
- Canvas overlays and custom controls must implement `role` + `aria-*` attributes
- Keyboard navigation: `Tab`, `Shift+Tab`, `Enter`, `Space`, `Escape` must work for all interactive elements

---

## Storybook

Storybook shows every UI component in isolation.

```bash
cd logos-app
npm run storybook       # dev server at http://localhost:6006
npm run build-storybook # static build → storybook-static/
```

Add a story for every new shared UI component:

```tsx
// src/shared/ui/Button/Button.stories.tsx
import type { Meta, StoryObj } from "@storybook/react";
import { Button } from "./Button";

const meta: Meta<typeof Button> = {
  component: Button,
  tags: ["autodocs"],
};
export default meta;

export const Primary: StoryObj<typeof Button> = {
  args: { label: "Save", variant: "primary" },
};

export const Ghost: StoryObj<typeof Button> = {
  args: { label: "Cancel", variant: "ghost" },
};
```

---

## Icons

Icons are SVG React components in `src/assets/icons/`. Use them directly:

```tsx
import { IconTrash } from "@/assets/icons";

<button aria-label="Delete">
  <IconTrash size={16} />
</button>
```

Do not use `<img src="icon.svg">` — SVG components allow CSS color inheritance via `currentColor`.

---

## Typography Scale

Use the typographic tokens defined in `src/styles/tokens/typography.scss`:

| Token | Usage |
|---|---|
| `--font-display-lg` | Section headers, empty state titles |
| `--font-heading-md` | Panel headers, dialog titles |
| `--font-body-md` | Default body text |
| `--font-body-sm` | Labels, secondary text, captions |
| `--font-mono-sm` | Code, hex values, coordinates |

---

## Testing UI Components

Use [Vitest](https://vitest.dev/) + [Testing Library](https://testing-library.com/):

```tsx
// Button.test.tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Button } from "./Button";

test("calls onClick when clicked", async () => {
  const onClick = vi.fn();
  render(<Button label="Save" onClick={onClick} />);
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(onClick).toHaveBeenCalledOnce();
});
```
