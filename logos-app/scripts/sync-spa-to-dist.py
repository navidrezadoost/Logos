#!/usr/bin/env python3
"""Copy the React SPA build into dist/ so legacy :8888 static servers load the new UI."""

from __future__ import annotations

import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUILD = ROOT / "build"
DIST = ROOT / "dist"

DASHBOARD_BOOT_SCRIPT = """
    <script>
      // Full Penpot editor: boot from workspace.html (not the React dashboard).
      if (/^#\\/workspace(\\/|\\?)/.test(location.hash)) {
        location.replace("/workspace.html" + location.hash);
      } else {
        document.cookie = "logos-penpot-shell=; path=/; max-age=0";
      }
    </script>"""

WORKSPACE_HASH_RE = r'/^#\\/workspace(\/|\?)/.test(location.hash)'


def inject_dashboard_boot(index_path: Path) -> None:
    html = index_path.read_text(encoding="utf-8")
    if "logos-penpot-shell=; path=/; max-age=0" in html:
        return
    if 'location.replace("/workspace.html"' in html:
        html = html.replace(
            """      if (/^#\\/workspace(\\/|\\?)/.test(location.hash)) {
        location.replace("/workspace.html" + location.hash);
      }""",
            """      if (/^#\\/workspace(\\/|\\?)/.test(location.hash)) {
        location.replace("/workspace.html" + location.hash);
      } else {
        document.cookie = "logos-penpot-shell=; path=/; max-age=0";
      }""",
            1,
        )
        index_path.write_text(html, encoding="utf-8")
        return
    # Replace outdated boot script that only matched #/workspace/…
    old_patterns = (
        'if (/^#\\/workspace\\//.test(location.hash))',
        "if (/^#\\/workspace\\//.test(location.hash))",
    )
    for old in old_patterns:
        if old in html:
            html = html.replace(
                old,
                "if (/^#\\/workspace(\\/|\\?)/.test(location.hash))",
                1,
            )
            index_path.write_text(html, encoding="utf-8")
            return
    marker = "<title>Logos App</title>"
    if marker in html:
        html = html.replace(marker, marker + DASHBOARD_BOOT_SCRIPT, 1)
        index_path.write_text(html, encoding="utf-8")


def main() -> int:
    index = BUILD / "index.html"
    assets = BUILD / "assets"
    if not index.is_file() or not assets.is_dir():
        sys.stderr.write(
            "Missing logos-app/build — run first:\n  npm run build:spa\n"
        )
        return 1

    DIST.mkdir(parents=True, exist_ok=True)
    shutil.copy2(index, DIST / "index.html")
    inject_dashboard_boot(DIST / "index.html")
    inject_dashboard_boot(BUILD / "index.html")

    dist_assets = DIST / "assets"
    if dist_assets.exists():
        shutil.rmtree(dist_assets)
    shutil.copytree(assets, dist_assets)

    print(f"Synced React SPA → {DIST}/ (index.html + assets/)")
    print("WASM/static files under dist/js/ are unchanged.")

    sync_penpot = ROOT / "scripts" / "sync-penpot-workspace.py"
    sync_aux = ROOT / "scripts" / "sync-penpot-aux-html.py"
    if sync_penpot.is_file():
        import subprocess
        subprocess.run([sys.executable, str(sync_penpot)], check=False)
    if sync_aux.is_file():
        import subprocess
        subprocess.run([sys.executable, str(sync_aux)], check=False)

    theme_src = ROOT / "css" / "logos-theme.css"
    logos_assets = (
        (ROOT / "css" / "logos-theme.css", "css/logos-theme.css"),
        (ROOT / "css" / "logos-toolbar-position.css", "css/logos-toolbar-position.css"),
        (ROOT / "css" / "logos-toolbar-move-tools.css", "css/logos-toolbar-move-tools.css"),
        (ROOT / "js" / "logos-workspace-icons.js", "js/logos-workspace-icons.js"),
        (ROOT / "js" / "logos-penpot-bridge.js", "js/logos-penpot-bridge.js"),
        (ROOT / "js" / "logos-toolbar-position.js", "js/logos-toolbar-position.js"),
        (ROOT / "js" / "logos-toolbar-move-tools.js", "js/logos-toolbar-move-tools.js"),
    )
    if theme_src.is_file():
        for src, rel in logos_assets:
            if not src.is_file():
                continue
            for base in (DIST, BUILD):
                dest = base / rel
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dest)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
