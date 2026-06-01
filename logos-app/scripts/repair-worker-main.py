#!/usr/bin/env python3
"""Repair truncated worker/main.js by appending tail modules from cljs-runtime."""

from __future__ import annotations

import json
import os
import re
import sys

WORKER_DIR = os.path.join(os.path.dirname(__file__), "..", "dist", "js", "worker")
MAIN_JS = os.path.join(WORKER_DIR, "main.js")
BACKUP_JS = MAIN_JS + ".truncated"
MANIFEST = os.path.join(WORKER_DIR, "manifest.json")
RUNTIME = os.path.join(WORKER_DIR, "cljs-runtime")

RESUME_FROM = "module$node_modules$rxjs$dist$cjs$internal$operators$repeatWhen.js"


def manifest_to_eval_name(source: str) -> str:
    if source.startswith("node_modules/"):
        return "module$" + source.replace("/", "$")
    name = source.replace("/", ".")
    for ext in (".cljs", ".cljc"):
        if name.endswith(ext):
            return name[: -len(ext)] + ".js"
    return name


def runtime_candidates(name: str) -> list[str]:
    """Map shadow evalLoad names to on-disk cljs-runtime filenames."""
    c: list[str] = [name]

    if name.startswith("app.common.weak.impl_"):
        c.append("module$app$common$weak$" + name.removeprefix("app.common.weak."))

    if name == "shadow.js.shim.module$$logos$mousetrap$default.js":
        c.append("shadow.js.shim.module$$penpot$mousetrap$default.js")

    normalized = name
    normalized = normalized.replace("@zip_DOT_js$zip.js", "$zip_DOT_js$zip_js")
    normalized = normalized.replace("@logos$mousetrap", "$penpot$mousetrap")
    normalized = normalized.replace("eventsource-parser", "eventsource_parser")
    normalized = normalized.replace("-", "_")
    normalized = re.sub(
        r"\.(development|browser)(?=\.js$)",
        r"_\1",
        normalized,
    )
    normalized = normalized.replace(".development", "_development")
    normalized = normalized.replace(".browser", "_browser")
    c.append(normalized)

    if "$zip_DOT_js$zip_js" in normalized:
        c.append(normalized.replace("$zip_DOT_js$zip_js", "$$zip_DOT_js$zip_js"))

    # De-dupe while preserving order.
    return list(dict.fromkeys(c))


def resolve_runtime_file(name: str) -> str | None:
    for candidate in runtime_candidates(name):
        path = os.path.join(RUNTIME, candidate)
        if os.path.isfile(path):
            return candidate
    return None


def js_escape(code: str) -> str:
    return (
        code.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
    )


def stub_code(name: str) -> str:
    ns = name.replace(".js", "").replace("/", ".")
    return f'goog.provide("{ns}");\n'


def main() -> int:
    source_main = BACKUP_JS if os.path.isfile(BACKUP_JS) else MAIN_JS
    manifest = json.load(open(MANIFEST))
    sources = manifest[0]["sources"]

    with open(source_main, "r", encoding="utf-8", errors="replace") as f:
        content = f.read()

    if "isNotifierComplet" in content[-200:]:
        prefix = content.rsplit("\n", 1)[0] + "\n"
    else:
        prefix = content

    start_idx = next(
        i
        for i, s in enumerate(sources)
        if manifest_to_eval_name(s) == RESUME_FROM
    )

    tail_parts: list[str] = []
    missing: list[str] = []

    for source in sources[start_idx:]:
        name = manifest_to_eval_name(source)
        runtime_name = resolve_runtime_file(name)
        if runtime_name:
            with open(os.path.join(RUNTIME, runtime_name), "r", encoding="utf-8") as f:
                code = f.read()
        elif name in (
            "app.common.svg.path.parser.js",
            "app.render_wasm.api.shared.js",
        ):
            code = stub_code(name)
            print(f"  stubbed: {name}")
        else:
            missing.append(name)
            continue
        tail_parts.append(
            f'SHADOW_ENV.evalLoad("{name}", true, "{js_escape(code)}");\n'
        )

    tail_parts.append(
        "\n// logos repair: tail restored from cljs-runtime (static dist)\n"
    )

    out = prefix + "".join(tail_parts)

    if not os.path.isfile(BACKUP_JS):
        with open(BACKUP_JS, "w", encoding="utf-8") as f:
            f.write(content)

    with open(MAIN_JS, "w", encoding="utf-8") as f:
        f.write(out)

    print(f"repaired {MAIN_JS} from {source_main}")
    print(f"  appended modules: {len(tail_parts) - 1}")
    print(f"  missing (no cljs-runtime file): {len(missing)}")
    for name in missing:
        print(f"    - {name}")
    return 0 if not missing else 2


if __name__ == "__main__":
    raise SystemExit(main())
