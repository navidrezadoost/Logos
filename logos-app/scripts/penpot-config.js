// Static dist config — version must match the compiled JS bundles (Penpot production sync).
// Mismatch triggers app.config "stale JS" reload loop (white screen).

globalThis.CLOSURE_UNCOMPILED_DEFINES = Object.assign(
  globalThis.CLOSURE_UNCOMPILED_DEFINES || {},
  {
    "shadow.cljs.devtools.client.env.enabled": false,
    "shadow.cljs.devtools.client.env.worker_client_id": 0,
    "shadow.cljs.devtools.client.env.log": false,
  }
);

var logosFlags = "enable-plugins enable-storybook disable-email-verification disable-onboarding disable-secure-session-cookies";
var penpotFlags = logosFlags;
var penpotVersion = "2.15.3";
var penpotVersionTag = "2.15.3-1778764938";
var penpotBuildDate = "Thu, 14 May 2026 13:22:18 +0000";
// workspace.html sets __logosPublicURI (origin only) before this script loads.
// Must NOT include /workspace.html — Penpot joins API paths against public_uri.
var penpotPublicURI =
  typeof globalThis.__logosPublicURI === "string"
    ? globalThis.__logosPublicURI
    : "http://127.0.0.1:8888";
var penpotRasterizerURI = "http://127.0.0.1:8888";
var penpotWorkerURI = "/js/worker/main.js";
var penpotThemes = null;
var penpotTermsOfServiceURI = null;
var penpotPrivacyPolicyURI = null;
var penpotGridHelpURI = "https://help.penpot.app/user-guide/flexible-layouts/";
var penpotPluginsListUri = "https://penpot.app/penpothub/plugins";
var penpotPluginsWhitelist = [];
var penpotTemplatesUri = "https://penpot.github.io/penpot-files/";
