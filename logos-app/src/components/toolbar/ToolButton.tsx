/**
 * components/toolbar/ToolButton.tsx
 *
 * A single toolbar button.
 *
 * - When `hasDropdown` is true, renders a small ▾ chevron in the bottom-right
 *   corner to signal that clicking opens a dropdown of related tools.
 * - `active` — whether this button's tool is the currently selected tool.
 * - `onClick` — called when the main button area is clicked.
 */

import React from "react";

interface ToolButtonProps {
  icon: string;
  label: string;
  shortcut?: string;
  active?: boolean;
  hasDropdown?: boolean;
  onClick: () => void;
}

export function ToolButton({
  icon,
  label,
  shortcut,
  active = false,
  hasDropdown = false,
  onClick,
}: ToolButtonProps): React.ReactElement {
  const title = shortcut ? `${label} (${shortcut})` : label;

  return (
    <button
      title={title}
      onClick={onClick}
      style={{
        position: "relative",
        width: 36,
        height: 36,
        borderRadius: 6,
        border: "none",
        background: active ? "#cba6f7" : "transparent",
        color: active ? "#1e1e2e" : "#cdd6f4",
        fontSize: 16,
        cursor: "pointer",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        transition: "background 0.1s",
        flexShrink: 0,
      }}
    >
      {icon}
      {hasDropdown && (
        <span
          aria-hidden
          style={{
            position: "absolute",
            bottom: 3,
            right: 3,
            fontSize: 6,
            lineHeight: 1,
            color: active ? "#1e1e2e" : "#6c7086",
            pointerEvents: "none",
            userSelect: "none",
          }}
        >
          ▾
        </span>
      )}
    </button>
  );
}
