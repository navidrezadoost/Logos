#!/usr/bin/env python3
"""Write dist/workspace.html — Penpot/Logos design shell (full editor UI)."""

from __future__ import annotations

import json
import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "dist"
CONFIG = DIST / "js" / "config.js"
OUT = DIST / "workspace.html"

SPRITE_FILES = (
    "icons.svg",
    "cursors.svg",
    "embedded-assets.svg",
)

# Sprites are out-of-flow; #app is viewport-fixed so flex/scroll cannot shift it off-screen.
SHELL_LAYOUT_CSS = """
      html, body {
        margin: 0;
        padding: 0;
        overflow: hidden;
        width: 100%;
        height: 100%;
      }
      #app {
        position: fixed !important;
        inset: 0 !important;
        width: auto !important;
        height: auto !important;
        overflow: hidden !important;
      }
      #penpot-sprites {
        position: fixed;
        width: 0;
        height: 0;
        overflow: hidden;
        pointer-events: none;
        visibility: hidden;
      }
      #modal {
        position: fixed;
        inset: 0;
        width: 0;
        height: 0;
        overflow: visible;
        pointer-events: none;
        z-index: 9999;
      }
      #modal > * {
        pointer-events: auto;
      }
"""


def read_sprite_markup() -> str:
    """One hidden SVG sheet — symbols stay in DOM for <use href=\"#icon-…\">."""
    sprite_dir = DIST / "images" / "sprites" / "symbol"
    symbol_chunks: list[str] = []

    for name in SPRITE_FILES:
        path = sprite_dir / name
        if not path.is_file():
            sys.stderr.write(f"Warning: missing sprite {path}\n")
            continue
        raw = path.read_text(encoding="utf-8").strip()
        # Some sprite files were accidentally appended with workspace.html body;
        # keep only the first SVG document.
        svg_end = raw.lower().find("</svg>")
        if svg_end >= 0:
            raw = raw[: svg_end + len("</svg>")]
        inner = re.sub(r"^<svg[^>]*>", "", raw, count=1, flags=re.I)
        inner = re.sub(r"</svg>\s*$", "", inner, count=1, flags=re.I)
        symbol_chunks.append(inner.strip())

    if not symbol_chunks:
        return ""

    symbols = "\n".join(symbol_chunks)
    return (
        '<div id="penpot-sprites" aria-hidden="true">'
        '<svg width="0" height="0" style="position:absolute" '
        'xmlns="http://www.w3.org/2000/svg">'
        f"{symbols}"
        "</svg></div>"
    )


PENPOT_MODULES = (
    "main-auth.js",
    "main-dashboard.js",
    "main-settings.js",
    "main-viewer.js",
    "main-workspace.js",
    "main.js",
    "rasterizer.js",
    "render-wasm.js",
    "render.js",
    "shared.js",
    "util-highlight.js",
)


LOGOS_ASSET_SOURCES = (
    ROOT / "css" / "logos-theme.css",
    ROOT / "css" / "logos-toolbar-position.css",
    ROOT / "css" / "logos-toolbar-move-tools.css",
    ROOT / "js" / "logos-workspace-icons.js",
    ROOT / "js" / "logos-workspace-debug.js",
    ROOT / "js" / "logos-toolbar-position.js",
    ROOT / "js" / "logos-toolbar-move-tools.js",
)


def logos_revision() -> str:
    """Short content hash so Logos overrides bust cache independently of Penpot tag."""
    import hashlib

    digest = hashlib.sha256()
    for path in LOGOS_ASSET_SOURCES:
        if path.is_file():
            digest.update(path.name.encode("utf-8"))
            digest.update(path.read_bytes())
    return digest.hexdigest()[:12]


def read_version_tag() -> str:
    if CONFIG.is_file():
        text = CONFIG.read_text(encoding="utf-8", errors="replace")
        m = re.search(r'penpotVersionTag\s*=\s*"([^"]+)"', text)
        if m:
            return m.group(1)
        m = re.search(r'penpotVersion\s*=\s*"([^"]+)"', text)
        if m:
            return m.group(1)
    return "2.15.3"


def build_html(tag: str, sprites: str, logos_rev: str) -> str:
    importmap = {
        "imports": {f"./js/{name}": f"./js/{name}?version={tag}" for name in PENPOT_MODULES}
    }
    importmap_json = json.dumps(importmap, separators=(",", ":"))
    sprite_block = f"\n    {sprites}\n" if sprites else ""

    return f"""<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta http-equiv="x-ua-compatible" content="ie=edge" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <base href="/" />
    <title>Logos — Design</title>
    <meta name="description" content="Logos design workspace">
    <script>
      (function () {{
        if ("scrollRestoration" in history) {{
          history.scrollRestoration = "manual";
        }}
        window.scrollTo(0, 0);
        globalThis.__logosPublicURI = location.origin;
        // Broken legacy URLs (#/workspace/{{file}}/{{file}}) → query form Penpot expects
        var hash = location.hash;
        var legacy = hash.match(/^#\\/workspace\\/([^/?#]+)\\/([^/?#]+)(\\?([^#]*))?$/);
        if (legacy && legacy[1] === legacy[2]) {{
          var params = new URLSearchParams(legacy[3] ? legacy[3].slice(1) : "");
          params.set("file-id", legacy[2]);
          if (!params.get("team-id")) {{
            try {{
              var xhr = new XMLHttpRequest();
              xhr.open("POST", "/api/rpc/command/get-profile", false);
              xhr.setRequestHeader("Content-Type", "application/json");
              xhr.withCredentials = true;
              xhr.send("{{}}");
              if (xhr.status === 200) {{
                var profile = JSON.parse(xhr.responseText);
                if (profile && profile["default-team-id"]) {{
                  params.set("team-id", profile["default-team-id"]);
                }}
              }}
            }} catch (e) {{}}
          }}
          hash = "#/workspace?" + params.toString();
        }}
        // Penpot on_navigate requires origin+pathname === public_uri (e.g. http://host:8888/).
        // Keeping /workspace.html in the address bar makes valid_location false → instant 404.
        // Normalize to / + hash; dashboard → workspace still gets a proper history entry via assign().
        if (hash && hash.charAt(0) === "#") {{
          var target = "/" + hash;
          if (location.pathname + location.hash !== target) {{
            history.replaceState(null, "", target);
          }}
        }}
        // Reload of /#/workspace only hits GET / — serve-frontend uses this cookie to return workspace.html.
        document.cookie = "logos-penpot-shell=1; path=/; SameSite=Lax";
        // Chrome tab discard / bfcache restores stale WebGL workers — reload cleanly.
        window.addEventListener("pageshow", function (e) {{
          if (e.persisted) {{
            location.reload();
          }}
        }});
        var logosHiddenAt = 0;
        document.addEventListener("visibilitychange", function () {{
          if (document.hidden) {{
            logosHiddenAt = Date.now();
            return;
          }}
          if (!logosHiddenAt || Date.now() - logosHiddenAt < 120000) {{
            return;
          }}
          var canvas = document.querySelector("canvas");
          if (!canvas) {{
            return;
          }}
          var gl = canvas.getContext("webgl2") || canvas.getContext("webgl");
          if (gl && gl.isContextLost && gl.isContextLost()) {{
            location.reload();
          }}
        }});
      }})();
    </script>
    <script src="./js/logos-workspace-debug.js?version={tag}&logos={logos_rev}"></script>
    <!-- Warm critical JS before CSS/fonts consume HTTP/1.1 connection slots (6/host). -->
    <link rel="modulepreload" href="./js/shared.js?version={tag}" />
    <link rel="modulepreload" href="./js/libs.js?version={tag}" />
    <link rel="modulepreload" href="./js/main.js?version={tag}" />
    <link rel="preload" href="./js/worker/main.js?version={tag}" as="script" />
    <link id="theme" href="css/main.css?version={tag}" rel="stylesheet" type="text/css" />
    <link href="css/ui.css?ts={tag}" rel="stylesheet" type="text/css" />
    <link href="css/logos-theme.css?version={tag}&logos={logos_rev}" rel="stylesheet" type="text/css" />
    <link href="css/logos-toolbar-position.css?version={tag}&logos={logos_rev}" rel="stylesheet" type="text/css" />
    <link href="css/logos-toolbar-move-tools.css?version={tag}&logos={logos_rev}" rel="stylesheet" type="text/css" />
    <style>{SHELL_LAYOUT_CSS}
    </style>
    <link rel="icon" href="images/favicon.png?version={tag}" />
    <script src="./js/config.js?version={tag}"></script>
    <script type="importmap">{importmap_json}</script>
    <script src="./js/polyfills.js?version={tag}"></script>
    <script type="module">
      globalThis.logosVersion = "{tag.split('-')[0]}";
      globalThis.logosVersionTag = "{tag}";
      globalThis.logosBuildDate = "";
      globalThis.logosWorkerURI = "./js/worker/main.js?version={tag}&logos-worker=5";
    </script>
  </head>
  <body>
    <div id="app"></div>
    <section id="modal"></section>{sprite_block}
    <script type="module" src="./js/libs.js?version={tag}"></script>
    <script type="module">
      import {{ init }} from "./js/main.js";
      import defaultTranslations from "./js/translation.en.js?version={tag}";
      init({{ defaultTranslations }});
    </script>
    <script type="module" src="./js/logos-penpot-bridge.js?version={tag}&logos={logos_rev}"></script>
    <script src="./js/logos-workspace-icons.js?version={tag}&logos={logos_rev}" defer></script>
    <script src="./js/logos-toolbar-position.js?version={tag}&logos={logos_rev}" defer></script>
    <script src="./js/logos-toolbar-move-tools.js?version={tag}&logos={logos_rev}" defer></script>
  </body>
</html>
"""


def sync_logos_assets() -> None:
    """Copy Logos workspace overrides into dist/ and build/ (serve-frontend uses build/)."""
    copies = (
        (ROOT / "css" / "logos-theme.css", "css/logos-theme.css"),
        (ROOT / "css" / "logos-toolbar-position.css", "css/logos-toolbar-position.css"),
        (ROOT / "css" / "logos-toolbar-move-tools.css", "css/logos-toolbar-move-tools.css"),
        (ROOT / "js" / "logos-workspace-icons.js", "js/logos-workspace-icons.js"),
        (ROOT / "js" / "logos-penpot-bridge.js", "js/logos-penpot-bridge.js"),
        (ROOT / "js" / "logos-toolbar-position.js", "js/logos-toolbar-position.js"),
        (ROOT / "js" / "logos-toolbar-move-tools.js", "js/logos-toolbar-move-tools.js"),
        (ROOT / "js" / "logos-workspace-debug.js", "js/logos-workspace-debug.js"),
    )
    for src, rel in copies:
        if not src.is_file():
            sys.stderr.write(f"Warning: missing Logos asset {src}\n")
            continue
        for base in (DIST, ROOT / "build"):
            dest = base / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dest)


def write_shell(tag: str, html: str) -> None:
    OUT.write_text(html, encoding="utf-8")
    build_out = ROOT / "build" / "workspace.html"
    if build_out.parent.is_dir():
        build_out.write_text(html, encoding="utf-8")


def main() -> int:
    if not (DIST / "js" / "main.js").is_file():
        sys.stderr.write(
            "Missing Penpot bundles in dist/js/. Run sync-prod-bundles.sh first,\n"
            "or ensure dist/js/main.js exists.\n"
        )
        return 1

    tag = read_version_tag()
    logos_rev = logos_revision()
    sprites = read_sprite_markup()
    build_icons = ROOT / "scripts" / "build-workspace-icons.py"
    if build_icons.is_file():
        import subprocess
        subprocess.run([sys.executable, str(build_icons)], check=False)
    html = build_html(tag, sprites, logos_rev)
    write_shell(tag, html)
    sync_logos_assets()

    aux = ROOT / "scripts" / "sync-penpot-aux-html.py"
    if aux.is_file():
        import subprocess
        subprocess.run([sys.executable, str(aux)], check=False)

    config_src = ROOT / "scripts" / "penpot-config.js"
    if config_src.is_file():
        text = config_src.read_text(encoding="utf-8")
        (DIST / "js" / "config.js").write_text(text, encoding="utf-8")
        build_config = ROOT / "build" / "js" / "config.js"
        if build_config.parent.is_dir():
            build_config.write_text(text, encoding="utf-8")

    print(
        f"Wrote Penpot workspace shell → {OUT} "
        f"(version {tag}, sprites {len(sprites):,} bytes)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
