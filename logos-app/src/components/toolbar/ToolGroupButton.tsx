/**
 * grouped grouped tool button: one visible icon + subtle chevron.
 *
 * - Click icon area → activate the currently displayed tool.
 * - Click chevron → open the tool list.
 * - Right-click anywhere on the control → open the tool list.
 */

import React, { useState } from "react";
import { theme } from "../../theme/colors";
import { ToolbarIcon, type ToolbarIconName } from "./toolbarIcons";

interface ToolGroupButtonProps {
  icon: ToolbarIconName;
  label: string;
  shortcut?: string;
  /** True when any tool in this group is the active canvas tool. */
  active?: boolean;
  /** True when the group's dropdown menu is open. */
  menuOpen?: boolean;
  onActivate: () => void;
  onOpenMenu: () => void;
}

export function ToolGroupButton({
  icon,
  label,
  shortcut,
  active = false,
  menuOpen = false,
  onActivate,
  onOpenMenu,
}: ToolGroupButtonProps): React.ReactElement {
  const [hoverZone, setHoverZone] = useState<"main" | "chevron" | null>(null);
  const title = shortcut ? `${label} (${shortcut})` : label;

  const bg =
    active
      ? theme.accent
      : hoverZone
        ? theme.accentMuted
        : "transparent";

  const fg = active ? theme.onAccent : theme.text;
  const chevronColor = active ? theme.onAccent : hoverZone === "chevron" ? theme.text : theme.textDim;

  return (
    <div
      role="group"
      aria-label={label}
      onContextMenu={(e) => {
        e.preventDefault();
        onOpenMenu();
      }}
      style={{
        position: "relative",
        width: 36,
        height: 36,
        flexShrink: 0,
      }}
    >
      {/* Main icon hit area — upper ~75% of the button */}
      <button
        type="button"
        title={title}
        aria-label={title}
        aria-pressed={active}
        onClick={onActivate}
        onMouseEnter={() => setHoverZone("main")}
        onMouseLeave={() => setHoverZone(null)}
        style={{
          position: "absolute",
          inset: 0,
          padding: "0 0 8px 0",
          borderRadius: 6,
          border: "none",
          background: bg,
          color: fg,
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          transition: "background 0.1s",
        }}
      >
        <ToolbarIcon name={icon} size={18} />
      </button>

      {/* Chevron hit area — bottom-right corner */}
      <button
        type="button"
        title={`${label} tools`}
        aria-label={`Show ${label} tools`}
        aria-expanded={menuOpen}
        onClick={(e) => {
          e.stopPropagation();
          onOpenMenu();
        }}
        onMouseEnter={() => setHoverZone("chevron")}
        onMouseLeave={() => setHoverZone(null)}
        style={{
          position: "absolute",
          right: 0,
          bottom: 0,
          width: 16,
          height: 14,
          padding: 0,
          border: "none",
          borderRadius: "0 0 6px 0",
          background: hoverZone === "chevron" && !active ? theme.accentMuted : "transparent",
          color: chevronColor,
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          lineHeight: 1,
          zIndex: 1,
        }}
      >
        <span
          style={{
            display: "flex",
            transform: menuOpen ? "rotate(180deg)" : undefined,
            transition: "transform 0.15s ease",
          }}
        >
          <ToolbarIcon name="chevronDown" size={8} />
        </span>
      </button>
    </div>
  );
}
