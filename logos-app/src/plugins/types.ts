/**
 * Plugin system types for Logos App.
 *
 * Wire protocol:
 *   Host → Plugin: PluginRequest
 *   Plugin → Host: PluginResponse
 *   Host → Plugin: PluginEvent  (push notifications)
 */

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------

/** Coarse-grained permission tokens granted at plugin load time. */
export type PluginPermission =
  | "read"      // read page/selection data
  | "content"   // create/update shapes
  | "allow:network"  // fetch() from plugin sandbox
  | "allow:clipboard"; // navigator.clipboard access

// ---------------------------------------------------------------------------
// Wire protocol
// ---------------------------------------------------------------------------

/** Host → Plugin: invoke a method in the plugin sandbox */
export interface PluginRequest {
  type: "REQUEST";
  /** Unique request ID for response correlation */
  id: string;
  method: string;
  params: unknown;
}

/** Plugin → Host: response to a PluginRequest */
export interface PluginResponse {
  type: "RESPONSE";
  id: string;
  success: boolean;
  data?: unknown;
  error?: string;
}

/** Host → Plugin: push event (selection changed, page changed, etc.) */
export interface PluginEvent {
  type: "EVENT";
  event: "selectionChange" | "pageChange" | "documentChange";
  payload: unknown;
}

export type HostMessage = PluginRequest | PluginEvent;
export type PluginMessage = PluginResponse;

// ---------------------------------------------------------------------------
// API data shapes (stable interface for plugins)
// ---------------------------------------------------------------------------

export type PluginShapeType =
  | "rect"
  | "ellipse"
  | "text"
  | "path"
  | "group"
  | "frame"
  | "bool"
  | "svg-raw";

export interface PluginShape {
  id: string;
  type: PluginShapeType;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  opacity: number;
  hidden: boolean;
  fills: PluginFill[];
}

export interface PluginFill {
  type: "solid";
  color: string;   // CSS #rrggbb
  opacity: number; // 0–1
}

export interface PluginPage {
  id: string;
  name: string;
  shapes: PluginShape[];
}

// ---------------------------------------------------------------------------
// Plugin handle (returned by bridge.connect)
// ---------------------------------------------------------------------------

export interface PluginHandle {
  readonly id: string;
  readonly url: string;
  readonly permissions: ReadonlySet<PluginPermission>;
  /** Terminate the plugin iframe and clean up */
  disconnect(): void;
}

// ---------------------------------------------------------------------------
// Internal pending-request tracking
// ---------------------------------------------------------------------------

export interface PendingRequest {
  resolve: (data: unknown) => void;
  reject: (err: Error) => void;
  timeoutId: ReturnType<typeof setTimeout>;
}
