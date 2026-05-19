/**
 * components/ui/MultiSelectChips.tsx
 *
 * A chip-based multi-select control supporting three layout/overflow modes:
 *
 *   wrap     — chips wrap onto as many lines as needed (default)
 *   scroll   — all chips stay on one line; container scrolls horizontally
 *   truncate — shows up to `maxVisible` chips; overflow shown as "+N more" chip
 *              that expands on click to reveal the rest
 *
 * Usage:
 *
 *   const [selected, setSelected] = useState<string[]>([]);
 *   <MultiSelectChips
 *     options={["rect", "ellipse", "text", "path"]}
 *     selected={selected}
 *     onChange={setSelected}
 *     scrollMode="wrap"
 *     getLabel={(v) => v}
 *   />
 */

import React, { useRef, useState } from "react";

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

export type ScrollMode = "wrap" | "scroll" | "truncate";

export interface MultiSelectChipsProps<T extends string = string> {
  /** All available options. */
  options: T[];
  /** Currently selected values. */
  selected: T[];
  /** Called when the selection changes. */
  onChange: (next: T[]) => void;
  /** Display label for each value. Defaults to the value itself. */
  getLabel?: (value: T) => string;
  /** Optional icon for each chip. */
  getIcon?: (value: T) => string | undefined;
  /**
   * How to handle overflow:
   * - `wrap`     — multi-line (default)
   * - `scroll`   — single-line, horizontal scroll
   * - `truncate` — clip after `maxVisible` and show "+N more"
   */
  scrollMode?: ScrollMode;
  /**
   * Max chips shown before the "+N more" overflow chip.
   * Only relevant when scrollMode === "truncate". Default: 3.
   */
  maxVisible?: number;
  /** If true, allow de-selecting the last item. Default: false. */
  allowEmpty?: boolean;
  /** Placeholder text shown when nothing is selected (only shown in truncate/scroll modes). */
  placeholder?: string;
  /** Extra CSS class for the outer container. */
  className?: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Chip
// ─────────────────────────────────────────────────────────────────────────────

interface ChipProps {
  label: string;
  icon?: string;
  active: boolean;
  onClick: () => void;
}

function Chip({ label, icon, active, onClick }: ChipProps): React.ReactElement {
  const [hovered, setHovered] = useState(false);

  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 3,
        padding: "2px 8px",
        borderRadius: 12,
        border: active ? "1px solid #cba6f7" : "1px solid #45475a",
        background: active
          ? "#cba6f7"
          : hovered
          ? "#313244"
          : "transparent",
        color: active ? "#1e1e2e" : "#cdd6f4",
        fontSize: 11,
        fontWeight: active ? 600 : 400,
        cursor: "pointer",
        whiteSpace: "nowrap",
        flexShrink: 0,
        transition: "background 0.1s, border-color 0.1s",
        userSelect: "none",
      }}
    >
      {icon && <span style={{ fontSize: 10 }}>{icon}</span>}
      {label}
    </button>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Container — shared style bits
// ─────────────────────────────────────────────────────────────────────────────

const GAP = 4;

// ─────────────────────────────────────────────────────────────────────────────
// MultiSelectChips
// ─────────────────────────────────────────────────────────────────────────────

export function MultiSelectChips<T extends string = string>({
  options,
  selected,
  onChange,
  getLabel = (v) => v,
  getIcon,
  scrollMode = "wrap",
  maxVisible = 3,
  allowEmpty = false,
}: MultiSelectChipsProps<T>): React.ReactElement {
  const [expanded, setExpanded] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  function toggle(value: T) {
    const active = selected.includes(value);
    if (active) {
      if (!allowEmpty && selected.length === 1) return; // keep at least one
      onChange(selected.filter((v) => v !== value));
    } else {
      onChange([...selected, value]);
    }
  }

  // ── scroll mode ──────────────────────────────────────────────────────────
  if (scrollMode === "scroll") {
    return (
      <div
        ref={scrollRef}
        style={{
          display: "flex",
          flexDirection: "row",
          gap: GAP,
          overflowX: "auto",
          overflowY: "hidden",
          scrollbarWidth: "none",
          WebkitOverflowScrolling: "touch",
          paddingBottom: 1,
        }}
      >
        {options.map((opt) => (
          <Chip
            key={opt}
            label={getLabel(opt)}
            icon={getIcon?.(opt)}
            active={selected.includes(opt)}
            onClick={() => toggle(opt)}
          />
        ))}
      </div>
    );
  }

  // ── truncate mode ─────────────────────────────────────────────────────────
  if (scrollMode === "truncate") {
    const visible = expanded ? options : options.slice(0, maxVisible);
    const overflowCount = options.length - maxVisible;
    const hasOverflow = !expanded && overflowCount > 0;

    return (
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          gap: GAP,
        }}
      >
        {visible.map((opt) => (
          <Chip
            key={opt}
            label={getLabel(opt)}
            icon={getIcon?.(opt)}
            active={selected.includes(opt)}
            onClick={() => toggle(opt)}
          />
        ))}
        {hasOverflow && (
          <Chip
            label={`+${overflowCount} more`}
            active={false}
            onClick={() => setExpanded(true)}
          />
        )}
        {expanded && overflowCount > 0 && (
          <Chip
            label="Show less"
            active={false}
            onClick={() => setExpanded(false)}
          />
        )}
      </div>
    );
  }

  // ── wrap mode (default) ───────────────────────────────────────────────────
  return (
    <div
      style={{
        display: "flex",
        flexWrap: "wrap",
        gap: GAP,
      }}
    >
      {options.map((opt) => (
        <Chip
          key={opt}
          label={getLabel(opt)}
          icon={getIcon?.(opt)}
          active={selected.includes(opt)}
          onClick={() => toggle(opt)}
        />
      ))}
    </div>
  );
}
