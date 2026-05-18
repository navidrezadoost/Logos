/**
 * offline/indicator.tsx — Sync status dot for the Toolbar.
 *
 * Displays a small coloured circle with a label to communicate the current
 * offline/sync state to the user. Mount inside the Toolbar component.
 *
 * Usage:
 *   import { SyncIndicator } from "../offline/indicator";
 *   // ...
 *   <SyncIndicator status={syncStatus} />
 *
 * The `syncStatus` state value should come from `initPersistence` /
 * `SyncManager.onStatus` callbacks in App.tsx.
 */

import React from "react";
import type { SyncStatus } from "./sync";

// ---------------------------------------------------------------------------
// Styles (inline — no CSS module dependency, zero bundle overhead)
// ---------------------------------------------------------------------------

const INDICATOR_CONFIG: Record<
  SyncStatus,
  { color: string; label: string; title: string; pulse: boolean }
> = {
  online:   { color: "#22c55e", label: "Saved",     title: "All changes saved",              pulse: false },
  saved:    { color: "#22c55e", label: "Saved",     title: "All changes saved to device",    pulse: false },
  saving:   { color: "#f59e0b", label: "Saving…",   title: "Saving changes to device…",     pulse: true  },
  syncing:  { color: "#3b82f6", label: "Syncing…",  title: "Syncing changes with server…",  pulse: true  },
  offline:  { color: "#6b7280", label: "Offline",   title: "Working offline. Changes are saved locally.", pulse: false },
  error:    { color: "#ef4444", label: "Error",     title: "Could not save or sync. Will retry on reconnect.", pulse: false },
  conflict: { color: "#f97316", label: "Conflict",  title: "Merge conflict. Server version applied.", pulse: false },
};

// ---------------------------------------------------------------------------
// Keyframe animation injected once
// ---------------------------------------------------------------------------

let pulseInjected = false;

function injectPulseAnimation(): void {
  if (pulseInjected || typeof document === "undefined") return;
  pulseInjected = true;
  const style = document.createElement("style");
  style.textContent = `
    @keyframes logos-sync-pulse {
      0%, 100% { opacity: 1; transform: scale(1); }
      50%       { opacity: 0.5; transform: scale(1.25); }
    }
  `;
  document.head.appendChild(style);
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export interface SyncIndicatorProps {
  status: SyncStatus;
  /** Show the text label next to the dot. Defaults to true. */
  showLabel?: boolean;
  /** Additional CSS class applied to the root element. */
  className?: string;
}

export function SyncIndicator({
  status,
  showLabel = true,
  className,
}: SyncIndicatorProps): React.ReactElement {
  injectPulseAnimation();

  const cfg = INDICATOR_CONFIG[status];

  const dotStyle: React.CSSProperties = {
    display: "inline-block",
    width: 8,
    height: 8,
    borderRadius: "50%",
    backgroundColor: cfg.color,
    flexShrink: 0,
    animation: cfg.pulse
      ? "logos-sync-pulse 1.2s ease-in-out infinite"
      : undefined,
  };

  const containerStyle: React.CSSProperties = {
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
    cursor: "default",
    userSelect: "none",
    fontSize: 11,
    fontWeight: 500,
    color: "#9ca3af",   // muted foreground — adapts to light/dark via CSS var if desired
    lineHeight: 1,
  };

  const labelStyle: React.CSSProperties = {
    whiteSpace: "nowrap",
  };

  return (
    <div
      style={containerStyle}
      className={className}
      title={cfg.title}
      aria-label={cfg.title}
      role="status"
      aria-live="polite"
    >
      <span style={dotStyle} aria-hidden="true" />
      {showLabel && <span style={labelStyle}>{cfg.label}</span>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Hook — subscribe to SyncStatus reactively (optional convenience export)
// ---------------------------------------------------------------------------

/**
 * `useSyncStatus(initialStatus?)` — React hook that manages a `SyncStatus`
 * value. Returns `[status, setStatus]` — pass `setStatus` to `initPersistence`
 * and `SyncManager.onStatus`.
 *
 * @example
 * ```tsx
 * const [syncStatus, setSyncStatus] = useSyncStatus("online");
 * useEffect(() => {
 *   initPersistence(DOC_ID, setSyncStatus);
 *   const mgr = createSyncManager(DOC_ID, setSyncStatus);
 *   mgr.start();
 *   return () => { stopPersistence(); mgr.stop(); };
 * }, []);
 * ```
 */
export function useSyncStatus(
  initialStatus: SyncStatus = "online"
): [SyncStatus, (s: SyncStatus) => void] {
  const [status, setStatus] = React.useState<SyncStatus>(initialStatus);
  return [status, setStatus];
}
