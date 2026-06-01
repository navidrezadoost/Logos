#!/usr/bin/env python3
"""Align rasterizer.html / render.html version tags with dist/js/config.js."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "dist"
CONFIG = DIST / "js" / "config.js"
HTML_FILES = ("rasterizer.html", "render.html")


def read_version_tag() -> tuple[str, str, str]:
    text = CONFIG.read_text(encoding="utf-8", errors="replace") if CONFIG.is_file() else ""
    ver = re.search(r'penpotVersion\s*=\s*"([^"]+)"', text)
    tag = re.search(r'penpotVersionTag\s*=\s*"([^"]+)"', text)
    build = re.search(r'penpotBuildDate\s*=\s*"([^"]+)"', text)
    version = ver.group(1) if ver else "2.15.3"
    version_tag = tag.group(1) if tag else version
    build_date = build.group(1) if build else ""
    return version, version_tag, build_date


def patch_html(path: Path, version: str, tag: str, build: str) -> bool:
    if not path.is_file():
        return False
    html = path.read_text(encoding="utf-8", errors="replace")
    updated = html
    updated = re.sub(r"\?version=develop\b", f"?version={tag}", updated)
    updated = re.sub(r'\?ts=develop\b', f"?ts={tag}", updated)
    updated = re.sub(
        r'globalThis\.logosVersion\s*=\s*"[^"]*"',
        f'globalThis.logosVersion = "{version}"',
        updated,
    )
    updated = re.sub(
        r'globalThis\.logosVersionTag\s*=\s*"[^"]*"',
        f'globalThis.logosVersionTag = "{tag}"',
        updated,
    )
    updated = re.sub(
        r'globalThis\.logosBuildDate\s*=\s*"[^"]*"',
        f'globalThis.logosBuildDate = "{build}"',
        updated,
    )
    updated = re.sub(r"logos-worker=\d+", "logos-worker=5", updated)
    updated = re.sub(
        r'\n?\s*<script src="\./js/dev-debug\.js[^"]*"></script>',
        "",
        updated,
    )
    if updated != html:
        path.write_text(updated, encoding="utf-8")
        return True
    return False


def main() -> int:
    version, tag, build = read_version_tag()
    changed = 0
    for name in HTML_FILES:
        for directory in (DIST, ROOT / "build"):
            path = directory / name
            if patch_html(path, version, tag, build):
                print(f"Patched {path} → version {tag}")
                changed += 1
    if changed == 0:
        print(f"No aux HTML changes (already at {tag})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
