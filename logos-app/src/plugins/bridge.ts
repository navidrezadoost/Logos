/**
 * Plugin bridge — iframe sandbox + postMessage protocol.
 *
 * Each plugin runs in a sandboxed <iframe> with:
 *   sandbox="allow-scripts"   (no allow-same-origin → strict isolation)
 *
 * Wire format (both directions):
 *   { type, ...fields } — see types.ts
 *
 * Permission model:
 *   Permissions are granted at connect() time and enforced host-side.
 *   The plugin iframe cannot escalate its own permissions.
 */

import {
  HostMessage,
  PendingRequest,
  PluginEvent,
  PluginHandle,
  PluginMessage,
  PluginPermission,
  PluginRequest,
  PluginResponse,
} from "./types";
import { buildPluginApi } from "./api";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const REQUEST_TIMEOUT_MS = 15_000;

/** CSP-safe bootstrap injected into the sandboxed iframe. */
const SANDBOX_BOOTSTRAP = `
(function () {
  'use strict';

  // Map: method name → handler function (registered by the plugin)
  const handlers = Object.create(null);
  // Map: requestId → { resolve, reject }
  const pending = new Map();

  /** Call a host API method and get a Promise back. */
  window.logos = {
    call(method, params) {
      return new Promise((resolve, reject) => {
        const id = crypto.randomUUID();
        pending.set(id, { resolve, reject });
        parent.postMessage({ type: 'RESPONSE', id, success: true, data: { __call: true, method, params } }, '*');
        // Re-use the RESPONSE channel to send calls upward; host differentiates via __call flag.
      });
    },

    /** Register event handler (e.g. logos.on('selectionChange', fn)) */
    on(event, fn) {
      handlers[event] = fn;
    },
  };

  window.addEventListener('message', (evt) => {
    const msg = evt.data;
    if (!msg || typeof msg !== 'object') return;

    if (msg.type === 'REQUEST') {
      // Host is invoking a method inside our plugin (unlikely in simple model, but supported)
      const fn = handlers[msg.method];
      if (fn) {
        Promise.resolve().then(() => fn(msg.params));
      }
      return;
    }

    if (msg.type === 'EVENT') {
      const fn = handlers[msg.event];
      if (fn) fn(msg.payload);
      return;
    }

    if (msg.type === 'CALL_RESULT') {
      const p = pending.get(msg.id);
      if (p) {
        pending.delete(msg.id);
        if (msg.success) p.resolve(msg.data);
        else p.reject(new Error(msg.error || 'Call failed'));
      }
    }
  });
})();
`;

// ---------------------------------------------------------------------------
// Bridge state
// ---------------------------------------------------------------------------

interface ActivePlugin {
  handle: PluginHandle;
  iframe: HTMLIFrameElement;
  permissions: Set<PluginPermission>;
  pending: Map<string, PendingRequest>;
  container: HTMLElement;
}

const activePlugins = new Map<string, ActivePlugin>();

// Single message listener shared across all plugins
let globalListenerInstalled = false;

function ensureGlobalListener(): void {
  if (globalListenerInstalled) return;
  globalListenerInstalled = true;
  window.addEventListener("message", onMessage, false);
}

// ---------------------------------------------------------------------------
// Message handling
// ---------------------------------------------------------------------------

function findPluginByIframe(source: MessageEventSource | null): ActivePlugin | undefined {
  for (const p of activePlugins.values()) {
    if (p.iframe.contentWindow === source) return p;
  }
  return undefined;
}

function onMessage(evt: MessageEvent): void {
  const plugin = findPluginByIframe(evt.source);
  if (!plugin) return;

  const msg = evt.data as PluginMessage & { data?: { __call?: boolean; method?: string; params?: unknown } };
  if (!msg || typeof msg !== "object") return;

  // Plugin → Host: a logos.call() from inside the sandbox.
  // Encoded as a RESPONSE with __call=true (avoids needing a separate message type).
  if (msg.type === "RESPONSE" && msg.data && (msg.data as any).__call === true) {
    const { method, params } = msg.data as { __call: true; method: string; params: unknown };
    handlePluginCall(plugin, msg.id, method, params);
    return;
  }

  // Standard response to a host-initiated REQUEST
  if (msg.type === "RESPONSE") {
    const pending = plugin.pending.get(msg.id);
    if (!pending) return;
    clearTimeout(pending.timeoutId);
    plugin.pending.delete(msg.id);
    if (msg.success) pending.resolve(msg.data);
    else pending.reject(new Error(msg.error ?? "Plugin call failed"));
  }
}

// ---------------------------------------------------------------------------
// Host-side API dispatch
// ---------------------------------------------------------------------------

function hasPermission(plugin: ActivePlugin, required: PluginPermission): boolean {
  return plugin.permissions.has(required);
}

function sendCallResult(
  iframe: HTMLIFrameElement,
  id: string,
  success: boolean,
  data?: unknown,
  error?: string
): void {
  iframe.contentWindow?.postMessage({ type: "CALL_RESULT", id, success, data, error }, "*");
}

async function handlePluginCall(
  plugin: ActivePlugin,
  requestId: string,
  method: string,
  params: unknown
): Promise<void> {
  const api = buildPluginApi(plugin.handle.id);

  try {
    // Permission checks
    const readMethods = ["getSelection", "getPage", "getShape"];
    const contentMethods = ["updateShape", "createRect", "createEllipse", "deleteShape"];

    if (readMethods.includes(method) && !hasPermission(plugin, "read")) {
      throw new Error(`Permission denied: '${method}' requires 'read' permission.`);
    }
    if (contentMethods.includes(method) && !hasPermission(plugin, "content")) {
      throw new Error(`Permission denied: '${method}' requires 'content' permission.`);
    }

    // Dispatch
    const fn = (api as unknown as Record<string, unknown>)[method];
    if (typeof fn !== "function") {
      throw new Error(`Unknown API method: '${method}'`);
    }

    const result = await (fn as (params: unknown) => unknown)(params);
    sendCallResult(plugin.iframe, requestId, true, result);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    sendCallResult(plugin.iframe, requestId, false, undefined, message);
  }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Connect a plugin by URL.
 *
 * @param pluginUrl - URL of the plugin entry-point JS file (will be injected
 *                    into a sandboxed iframe via srcdoc).
 * @param permissions - Permission tokens to grant to this plugin.
 * @param container  - DOM element to append the (invisible) iframe to.
 *                     Defaults to document.body.
 */
export async function connectPlugin(
  pluginUrl: string,
  permissions: PluginPermission[],
  container: HTMLElement = document.body
): Promise<PluginHandle> {
  ensureGlobalListener();

  const pluginId = crypto.randomUUID();
  const permSet = new Set(permissions);

  const iframe = document.createElement("iframe");
  iframe.setAttribute("sandbox", "allow-scripts");
  iframe.setAttribute("title", `plugin-${pluginId}`);
  iframe.style.cssText = "display:none;width:0;height:0;border:none;";

  // Build srcdoc: inject bootstrap + fetch + execute plugin script
  const srcdoc = `<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body>
<script>
${SANDBOX_BOOTSTRAP}
</script>
<script>
// Fetch and eval the plugin script (network allowed only if permission granted server-side)
fetch(${JSON.stringify(pluginUrl)})
  .then(r => r.text())
  .then(code => {
    try { eval(code); }
    catch(e) { console.error('[logos-plugin] Runtime error:', e.message); }
  })
  .catch(e => console.error('[logos-plugin] Failed to load plugin:', e.message));
</script>
</body>
</html>`;

  iframe.srcdoc = srcdoc;

  const pending = new Map<string, PendingRequest>();

  const handle: PluginHandle = {
    id: pluginId,
    url: pluginUrl,
    permissions: permSet as ReadonlySet<PluginPermission>,
    disconnect() {
      disconnectPlugin(pluginId);
    },
  };

  const activePlugin: ActivePlugin = { handle, iframe, permissions: permSet, pending, container };
  activePlugins.set(pluginId, activePlugin);

  container.appendChild(iframe);

  return handle;
}

/**
 * Disconnect a plugin by ID, cleaning up its iframe and pending requests.
 */
export function disconnectPlugin(pluginId: string): void {
  const plugin = activePlugins.get(pluginId);
  if (!plugin) return;

  // Reject all pending requests
  for (const [, p] of plugin.pending) {
    clearTimeout(p.timeoutId);
    p.reject(new Error("Plugin disconnected"));
  }
  plugin.pending.clear();

  // Remove iframe from DOM
  plugin.iframe.srcdoc = "";
  plugin.container.removeChild(plugin.iframe);

  activePlugins.delete(pluginId);
}

/**
 * Send a push event to all connected plugins (or a specific plugin by ID).
 */
export function broadcastEvent(event: PluginEvent, targetPluginId?: string): void {
  for (const [id, plugin] of activePlugins) {
    if (targetPluginId && id !== targetPluginId) continue;
    plugin.iframe.contentWindow?.postMessage(event, "*");
  }
}

/**
 * Send a request to a plugin and await its response.
 * Mainly useful for testing; normal data flow goes through handlePluginCall.
 */
export function sendRequest(
  pluginId: string,
  method: string,
  params: unknown = {}
): Promise<unknown> {
  const plugin = activePlugins.get(pluginId);
  if (!plugin) return Promise.reject(new Error(`Plugin ${pluginId} not connected`));

  const id = crypto.randomUUID();
  const request: PluginRequest = { type: "REQUEST", id, method, params };

  return new Promise((resolve, reject) => {
    const timeoutId = setTimeout(() => {
      plugin.pending.delete(id);
      reject(new Error(`Request '${method}' timed out after ${REQUEST_TIMEOUT_MS}ms`));
    }, REQUEST_TIMEOUT_MS);

    plugin.pending.set(id, { resolve, reject, timeoutId });
    plugin.iframe.contentWindow?.postMessage(request as HostMessage, "*");
  });
}

export { activePlugins };
