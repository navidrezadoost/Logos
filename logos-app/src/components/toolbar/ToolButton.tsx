/**
 * components/toolbar/ToolButton.tsx
 *
 * A single toolbar button with an svger-cli SVG icon.
 */

import React from "react";
import { theme } from "../../theme/colors";
import { ToolbarIcon, type ToolbarIconName } from "./toolbarIcons";

interface ToolButtonProps {
  icon: ToolbarIconName;
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
        background: active ? theme.accent : "transparent",
        color: active ? theme.onAccent : theme.text,
        cursor: "pointer",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        transition: "background 0.1s",
        flexShrink: 0,
      }}
    >
      <ToolbarIcon name={icon} size={18} />
      {hasDropdown && (
        <span
          aria-hidden
          style={{
            position: "absolute",
            bottom: 2,
            right: 2,
            lineHeight: 1,
            color: active ? theme.onAccent : theme.textDim,
            pointerEvents: "none",
            userSelect: "none",
          }}
        >
          <ToolbarIcon name="chevronDown" size={8} />
        </span>
      )}
    </button>
  );
}
