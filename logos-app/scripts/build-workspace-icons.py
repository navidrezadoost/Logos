#!/usr/bin/env python3
"""Build js/logos-workspace-icons.js from src/icons/system/*.svg for Logos workspace."""

from __future__ import annotations

import json
import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src" / "icons" / "system"
TOOLBAR = ROOT / "src" / "icons" / "toolbar"
OUT = ROOT / "js" / "logos-workspace-icons.js"

# Toolbar SVGs reused in Logos workspace scripts (key → filename stem).
TOOLBAR_ALIASES = {
    "move": "select",
}


def extract_paths(svg: str) -> str:
    """Return inner SVG markup (paths only, no wrapper)."""
    svg = re.sub(r"<!--.*?-->", "", svg, flags=re.S)
    inner = re.sub(r"^<svg[^>]*>", "", svg.strip(), count=1, flags=re.I)
    inner = re.sub(r"</svg>\s*$", "", inner, count=1, flags=re.I)
    return inner.strip()


def js_string(value: str) -> str:
    return json.dumps(value)


def build_icon_markup(icon_id: str, inner: str) -> str:
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" '
        f'width="500" height="500" aria-hidden="true" '
        f'class="logos-system-icon logos-system-icon--{icon_id}" '
        f'fill="currentColor">{inner}</svg>'
    )


def main() -> int:
    if not SRC.is_dir():
        sys.stderr.write(
            f"Missing {SRC}\nRun: bash scripts/link-system-icons.sh\n"
        )
        return 1

    svgs = sorted(SRC.glob("*.svg"))
    if not svgs and not TOOLBAR.is_dir():
        sys.stderr.write(f"No SVG files in {SRC}\n")
        return 1

    entries: list[str] = []
    seen: set[str] = set()

    for path in sorted(SRC.glob("*.svg")):
        icon_id = path.stem
        seen.add(icon_id)
        inner = extract_paths(path.read_text(encoding="utf-8"))
        markup = build_icon_markup(icon_id, inner)
        entries.append(f"    {icon_id}: {js_string(markup)}")

    for icon_id, toolbar_stem in TOOLBAR_ALIASES.items():
        if icon_id in seen:
            continue
        path = TOOLBAR / f"{toolbar_stem}.svg"
        if not path.is_file():
            continue
        inner = extract_paths(path.read_text(encoding="utf-8"))
        markup = build_icon_markup(icon_id, inner)
        entries.append(f"    {icon_id}: {js_string(markup)}")
        seen.add(icon_id)

    for path in sorted(TOOLBAR.glob("*.svg")):
        icon_id = path.stem
        if icon_id in seen:
            continue
        if icon_id not in {"hand", "scale", "chevronDown"}:
            continue
        inner = extract_paths(path.read_text(encoding="utf-8"))
        markup = build_icon_markup(icon_id, inner)
        entries.append(f"    {icon_id}: {js_string(markup)}")
        seen.add(icon_id)

    body = "\n".join(
        [
            "/**",
            " * Logos system icons for Logos workspace (Font Awesome sharp-solid).",
            " * AUTO-GENERATED — run: python scripts/build-workspace-icons.py",
            " */",
            "(function (global) {",
            '  "use strict";',
            "  global.LogosSystemIcons = {",
            ",\n".join(entries),
            "  };",
            "})(typeof globalThis !== 'undefined' ? globalThis : window);",
            "",
        ]
    )

    OUT.write_text(body, encoding="utf-8")
    for base in (ROOT / "build" / "js", ROOT / "dist" / "js"):
        base.mkdir(parents=True, exist_ok=True)
        shutil.copy2(OUT, base / OUT.name)

    print(f"Wrote workspace icons → {OUT} ({len(seen)} icons)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
