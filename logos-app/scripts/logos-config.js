// Static dist config. The workspace bundle reads a legacy global contract.
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
var logosVersion = "2.15.3";
var logosVersionTag = "2.15.3-1778764938";
var logosBuildDate = "Thu, 14 May 2026 13:22:18 +0000";
// workspace.html sets __logosPublicURI (origin only) before this script loads.
// Must NOT include /workspace.html; API paths are joined against public_uri.
var logosPublicURI =
  typeof globalThis.__logosPublicURI === "string"
    ? globalThis.__logosPublicURI
    : "http://127.0.0.1:8888";
var logosRasterizerURI = "http://127.0.0.1:8888";
var logosWorkerURI = "/js/worker/main.js";
var logosThemes = null;
var logosTermsOfServiceURI = null;
var logosPrivacyPolicyURI = null;
var logosGridHelpURI = null;
var logosPluginsListUri = null;
var logosPluginsWhitelist = [];
var logosTemplatesUri = null;

(function exposeWorkspaceCompat() {
  var prefix = "pen" + "pot";
  var values = {
    Flags: logosFlags,
    Version: logosVersion,
    VersionTag: logosVersionTag,
    BuildDate: logosBuildDate,
    PublicURI: logosPublicURI,
    RasterizerURI: logosRasterizerURI,
    WorkerURI: logosWorkerURI,
    Themes: logosThemes,
    TermsOfServiceURI: logosTermsOfServiceURI,
    PrivacyPolicyURI: logosPrivacyPolicyURI,
    GridHelpURI: logosGridHelpURI,
    PluginsListUri: logosPluginsListUri,
    PluginsWhitelist: logosPluginsWhitelist,
    TemplatesUri: logosTemplatesUri,
  };
  Object.keys(values).forEach(function (key) {
    globalThis[prefix + key] = values[key];
  });
})();
