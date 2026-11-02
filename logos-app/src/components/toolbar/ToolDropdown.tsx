/**
 * components/toolbar/ToolDropdown.tsx
 *
 * A thin vertical dropdown that appears to the right of a toolbar group button.
 *
 * Renders a radio-style list of tools. Clicking an item:
 *  1. Marks it as selected (activeToolId).
 *  2. Fires onSelect(toolId).
 *  3. Closes the dropdown (caller's responsibility via onClose).
 *
 * Closes automatically when the user clicks outside the dropdown.
 */

import React, { useEffect, useRef } from "react";
import { theme } from "../../theme/colors";
import { ToolbarIcon } from "./toolbarIcons";
import type { ToolDef } from "../../stores/toolbarStore";
import type { Tool } from "../../stores/uiStore";

interface ToolDropdownProps {
  tools: ToolDef[];
  activeToolId: Tool;
  /** Top position (px) of the dropdown, anchored to the triggering button. */
  topOffset: number;
  onSelect: (toolId: Tool) => void;
  onClose: () => void;
}

export function ToolDropdown({
  tools,
  activeToolId,
  topOffset,
  onSelect,
  onClose,
}: ToolDropdownProps): React.ReactElement {
  const ref = useRef<HTMLDivElement>(null);

  // Close on click-outside
  useEffect(() => {
    function handlePointerDown(e: PointerEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onClose();
      }
    }
    // Capture phase so this fires before any button's onClick
    document.addEventListener("pointerdown", handlePointerDown, true);
    return () => document.removeEventListener("pointerdown", handlePointerDown, true);
  }, [onClose]);

  return (
    <div
      ref={ref}
      role="menu"
      aria-label="Tool options"
      style={{
        position: "fixed",
        left: 52,          // just to the right of the 48px-wide toolbar
        top: topOffset,
        width: 200,
        background: theme.panel,
        border: `1px solid ${theme.border}`,
        borderRadius: 8,
        boxShadow: "0 8px 24px rgba(0,0,0,0.5)",
        padding: "4px 0",
        zIndex: 1000,
      }}
    >
      {tools.map((tool) => {
        const isActive = tool.id === activeToolId;
        return (
          <button
            key={tool.id}
            role="menuitemradio"
            aria-checked={isActive}
            title={tool.shortcut ? `${tool.label} (${tool.shortcut})` : tool.label}
            onClick={() => {
              onSelect(tool.id);
              onClose();
            }}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 10,
              width: "100%",
              padding: "0 12px",
              height: 32,
              border: "none",
              background: isActive ? theme.accentMuted : "transparent",
              color: isActive ? theme.accent : theme.text,
              fontSize: 13,
              cursor: "pointer",
              textAlign: "left",
            }}
          >
            {/* Checkmark when active (grouped) */}
            <span
              style={{
                width: 14,
                display: "flex",
                justifyContent: "center",
                flexShrink: 0,
                fontSize: 12,
                color: theme.accent,
              }}
              aria-hidden
            >
              {isActive ? "✓" : ""}
            </span>

            {/* Icon */}
            <span style={{ width: 20, display: "flex", justifyContent: "center", flexShrink: 0 }}>
              <ToolbarIcon name={tool.icon} size={16} />
            </span>

            {/* Label */}
            <span style={{ flex: 1 }}>{tool.label}</span>

            {/* Shortcut hint */}
            {tool.shortcut && (
              <span
                style={{
                  fontSize: 11,
                  color: theme.textDim,
                  fontFamily: "monospace",
                  flexShrink: 0,
                }}
              >
                {tool.shortcut}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
