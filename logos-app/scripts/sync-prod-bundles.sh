#!/usr/bin/env bash
# Replace dev ESM loaders with Penpot production bundles (fast load).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"
JS="$DIST/js"
IMAGE="${PENPOT_FRONTEND_IMAGE:-penpotapp/frontend:latest}"
KEEP=(config.js translation.en.js)

echo ">> Extracting production JS from $IMAGE ..."
CID="$(docker create "$IMAGE")"
trap 'docker rm -f "$CID" >/dev/null 2>&1 || true' EXIT

TMP="$(mktemp -d)"
docker cp "$CID:/var/www/app/js/." "$TMP/js/"
docker cp "$CID:/var/www/app/css/." "$TMP/css/" 2>/dev/null || true
docker cp "$CID:/var/www/app/index.html" "$TMP/index.html.penpot" 2>/dev/null || true

BACKUP="$DIST/.dev-js-backup-$(date +%Y%m%d%H%M%S)"
mkdir -p "$BACKUP"
for f in "${KEEP[@]}"; do
  if [[ -f "$JS/$f" ]]; then
    cp "$JS/$f" "$BACKUP/"
  fi
done

echo ">> Backing up dev cljs-runtime to $BACKUP/cljs-runtime ..."
if [[ -d "$JS/cljs-runtime" ]]; then
  mv "$JS/cljs-runtime" "$BACKUP/"
fi
if [[ -d "$JS/worker/cljs-runtime" ]]; then
  mkdir -p "$BACKUP/worker"
  mv "$JS/worker/cljs-runtime" "$BACKUP/worker/"
fi

echo ">> Installing production bundles ..."
mkdir -p "$JS/worker"
cp -a "$TMP/js/"*.js "$JS/" 2>/dev/null || true
cp -a "$TMP/js/worker/"*.js "$JS/worker/" 2>/dev/null || true

for f in "${KEEP[@]}"; do
  if [[ -f "$BACKUP/$f" ]]; then
    cp "$BACKUP/$f" "$JS/$f"
  fi
done

if [[ -f "$ROOT/scripts/penpot-config.js" ]]; then
  cp "$ROOT/scripts/penpot-config.js" "$JS/config.js"
fi

# Logos worker bootstrap (disable shadow devtools in worker scope).
WORKER="$JS/worker/main.js"
if [[ -f "$WORKER" ]] && ! grep -q "Logos static dist: disable shadow-cljs" "$WORKER"; then
  python3 - "$WORKER" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1])
bootstrap = r'''var shadow$provide = {};

// Logos static dist: disable shadow-cljs in the render worker (separate global scope).
(function (w) {
  w.CLOSURE_UNCOMPILED_DEFINES = Object.assign(w.CLOSURE_UNCOMPILED_DEFINES || {}, {
    "shadow.cljs.devtools.client.env.enabled": false,
    "shadow.cljs.devtools.client.env.worker_client_id": 0,
    "shadow.cljs.devtools.client.env.log": false,
  });
  var NativeWS = w.WebSocket;
  function FakeWS(url) {
    var self = this;
    var listeners = {};
    this.url = url;
    this.readyState = NativeWS.CONNECTING;
    this.bufferedAmount = 0;
    this.onopen = this.onmessage = this.onerror = this.onclose = null;
    this.close = function () { self.readyState = NativeWS.CLOSED; };
    this.send = function () {};
    this.addEventListener = function (type, fn) {
      (listeners[type] = listeners[type] || []).push(fn);
    };
    this.removeEventListener = function () {};
    this.dispatchEvent = function (evt) {
      (listeners[evt.type] || []).forEach(function (fn) { fn(evt); });
      return true;
    };
    setTimeout(function () {
      self.readyState = NativeWS.OPEN;
      var evt = { type: "open", target: self, currentTarget: self };
      if (typeof self.onopen === "function") self.onopen(evt);
      (listeners.open || []).forEach(function (fn) { fn(evt); });
    }, 0);
  }
  w.WebSocket = function (url, protos) {
    if (typeof url === "string" && (url.indexOf(":3448") !== -1 || url.indexOf("remote-relay") !== -1)) {
      return new FakeWS(url);
    }
    return protos !== undefined ? new NativeWS(url, protos) : new NativeWS(url);
  };
  w.WebSocket.CONNECTING = NativeWS.CONNECTING;
  w.WebSocket.OPEN = NativeWS.OPEN;
  w.WebSocket.CLOSING = NativeWS.CLOSING;
  w.WebSocket.CLOSED = NativeWS.CLOSED;
})(self);

'''
text = p.read_text(encoding="utf-8", errors="replace")
if text.startswith("var shadow$provide"):
    text = bootstrap + text.split("\n", 1)[1]
else:
    text = bootstrap + text
p.write_text(text, encoding="utf-8")
PY
fi

# Copy wasm assets if present.
if [[ -f "$TMP/js/render-wasm.wasm" ]]; then
  cp "$TMP/js/render-wasm.wasm" "$JS/"
fi

# CSS must match the synced JS bundles (class names are hashed per build).
if [[ -f "$TMP/css/main.css" ]]; then
  mkdir -p "$DIST/css"
  cp "$TMP/css/main.css" "$DIST/css/main.css"
  echo ">> Synced css/main.css ($(wc -c < "$DIST/css/main.css") bytes)"
fi

# Align config.js + index.html version tags with the synced bundles (prevents reload loop).
if [[ -f "$TMP/index.html.penpot" ]]; then
  python3 - "$TMP/index.html.penpot" "$DIST/index.html" "$JS/config.js" <<'PY'
import re, sys
from pathlib import Path

penpot_html = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
index_path = Path(sys.argv[2])
config_path = Path(sys.argv[3])

def grab(pat, default=""):
    m = re.search(pat, penpot_html)
    return m.group(1) if m else default

ver = grab(r'globalThis\.penpotVersion\s*=\s*"([^"]+)"', "2.15.3")
tag = grab(r'globalThis\.penpotVersionTag\s*=\s*"([^"]+)"', ver)
build = grab(r'globalThis\.penpotBuildDate\s*=\s*"([^"]+)"', "")

config = f'''// Static dist config — version must match synced production JS bundles.
globalThis.CLOSURE_UNCOMPILED_DEFINES = Object.assign(
  globalThis.CLOSURE_UNCOMPILED_DEFINES || {{}},
  {{
    "shadow.cljs.devtools.client.env.enabled": false,
    "shadow.cljs.devtools.client.env.worker_client_id": 0,
    "shadow.cljs.devtools.client.env.log": false,
  }}
);

var logosFlags = "enable-plugins enable-storybook disable-email-verification disable-onboarding disable-secure-session-cookies";
var penpotFlags = logosFlags;
var penpotVersion = "{ver}";
var penpotVersionTag = "{tag}";
var penpotBuildDate = "{build}";
var penpotPublicURI = "http://127.0.0.1:8888";
var penpotRasterizerURI = "http://127.0.0.1:8888";
var penpotWorkerURI = "/js/worker/main.js";
var penpotThemes = null;
var penpotTermsOfServiceURI = null;
var penpotPrivacyPolicyURI = null;
var penpotGridHelpURI = "https://help.penpot.app/user-guide/flexible-layouts/";
var penpotPluginsListUri = "https://penpot.app/penpothub/plugins";
var penpotPluginsWhitelist = [];
var penpotTemplatesUri = "https://penpot.github.io/penpot-files/";
'''
config_path.write_text(config, encoding="utf-8")

if index_path.exists():
    html = index_path.read_text(encoding="utf-8", errors="replace")
    html = re.sub(r'\?version=develop', f'?version={tag}', html)
    html = re.sub(r'\?ts=', f'?ts={tag}', html)
    html = re.sub(r'globalThis\.logosVersion\s*=\s*"[^"]*"', f'globalThis.logosVersion = "{ver}"', html)
    html = re.sub(r'globalThis\.logosVersionTag\s*=\s*"[^"]*"', f'globalThis.logosVersionTag = "{tag}"', html)
    html = re.sub(r'globalThis\.logosBuildDate\s*=\s*"[^"]*"', f'globalThis.logosBuildDate = "{build}"', html)
    html = re.sub(r'logos-worker=\d+', 'logos-worker=5', html)
    index_path.write_text(html, encoding="utf-8")

print(f">> Version tags set to {tag}")
PY
fi

echo ">> Done. Dev backup: $BACKUP"
wc -c "$JS/main.js" "$JS/shared.js" "$JS/libs.js" "$JS/main-dashboard.js" "$JS/worker/main.js" 2>/dev/null || true

echo ""
echo ">> Note: sync-prod-bundles updates legacy Penpot JS under dist/js/."
echo ">> The project dashboard is the React app — refresh it with:"
echo ">>   npm run build:spa"
