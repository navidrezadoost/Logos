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
        background: "#1e1e2e",
        border: "1px solid #313244",
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
              background: isActive ? "rgba(203,166,247,0.15)" : "transparent",
              color: isActive ? "#cba6f7" : "#cdd6f4",
              fontSize: 13,
              cursor: "pointer",
              textAlign: "left",
            }}
          >
            {/* Icon slot */}
            <span style={{ width: 20, textAlign: "center", fontSize: 15, flexShrink: 0 }}>
              {tool.icon}
            </span>

            {/* Label */}
            <span style={{ flex: 1 }}>{tool.label}</span>

            {/* Shortcut hint */}
            {tool.shortcut && (
              <span
                style={{
                  fontSize: 11,
                  color: "#585b70",
                  fontFamily: "monospace",
                  flexShrink: 0,
                }}
              >
                {tool.shortcut}
              </span>
            )}

            {/* Active indicator */}
            {isActive && (
              <span style={{ fontSize: 10, color: "#cba6f7", marginLeft: 4 }}>●</span>
            )}
          </button>
        );
      })}
    </div>
  );
}
