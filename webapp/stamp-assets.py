#!/usr/bin/env python3
"""Stamp a build version onto every script, stylesheet and module the site loads.

Why this exists: GitHub Pages caches `.html` for 10 minutes but `.js`, `.mjs`,
`.css` and `.wasm` for 4 hours, and the site referenced them by fixed URLs. After
every deploy a visitor's browser therefore paired the NEW page with the OLD
script for up to four hours. The Table menu shipped in #546 is how it surfaced:
the new `editor.html` drew a Table button, the cached `main.js` had no Table
rows, and the menu opened empty in production while every test passed.

Three things have to carry the version, because three different mechanisms fetch:

1. `<script src>`, `<link href>`, `modulepreload` and the wasm preload in each
   page — rewritten.
2. Module-to-module imports (`main.js` -> `./keyboard.mjs`, `../pkg/...`) — these
   resolve relative to the importing module and DROP its query string, so a
   versioned `main.js` would still import stale children. An import map in each
   page remaps every module URL to its versioned form.
3. The wasm binary, which wasm-bindgen's glue fetches with
   `new URL('casual_doc_wasm_bg.wasm', import.meta.url)` — also query-less, so the
   literal in the glue is rewritten.

Run on the deploy artifact only (see .github/workflows/pages.yml). It rewrites
files in place, so running it in a working tree would dirty tracked pages;
`webapp/tests/stamp_assets.test.mjs` runs it on a copy.

Usage: stamp-assets.py <webapp-dir> <build-id>
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

GLUE = "pkg/casual_doc_wasm.js"
WASM = "casual_doc_wasm_bg.wasm"


def module_files(root: Path) -> list[str]:
    """Every module a page can reach, as site-relative paths."""
    found = sorted(
        str(p.relative_to(root)).replace("\\", "/")
        for pattern in ("src/*.js", "src/*.mjs")
        for p in root.glob(pattern)
    )
    if (root / GLUE).exists():
        found.append(GLUE)
    return found


def stamp_page(html: str, build: str, modules: list[str]) -> str:
    # 1. Direct references. Only local ./src/ and ./pkg/ assets: an absolute or
    #    third-party URL is not ours to version.
    def versioned(match: re.Match[str]) -> str:
        attr, url = match.group(1), match.group(2)
        base = url.split("?", 1)[0]
        return f'{attr}="{base}?v={build}"'

    html = re.sub(
        r'\b(src|href)="(\./(?:src|pkg)/[^"?#]+\.(?:js|mjs|css|wasm))(?:\?[^"]*)?"',
        versioned,
        html,
    )
    # The wasm is also preloaded, from markup and from an inline script on the
    # homepage. A preload that names a different URL from the one the glue then
    # fetches is a wasted download of a possibly stale binary, so both forms
    # carry the version too.
    html = re.sub(
        r'(["\'])(\./pkg/' + re.escape(WASM) + r')(?:\?v=[^"\']*)?\1',
        lambda m: f"{m.group(1)}{m.group(2)}?v={build}{m.group(1)}",
        html,
    )

    # 2. Import map, placed before the first module script so it governs every
    #    import. Replaces any previous stamp, so re-running is idempotent.
    html = re.sub(r'\s*<script type="importmap" data-stamp>.*?</script>', "", html, flags=re.S)
    if 'type="module"' in html or 'rel="modulepreload"' in html:
        mapping = {f"./{m}": f"./{m}?v={build}" for m in modules}
        block = (
            '\n    <script type="importmap" data-stamp>'
            + json.dumps({"imports": mapping}, separators=(",", ":"))
            + "</script>"
        )
        html = html.replace("<head>", "<head>" + block, 1)
    return html


def stamp_glue(js: str, build: str) -> str:
    pattern = re.compile(r"(['\"])" + re.escape(WASM) + r"(?:\?v=[^'\"]*)?\1")
    stamped, count = pattern.subn(lambda m: f"{m.group(1)}{WASM}?v={build}{m.group(1)}", js)
    if count == 0:
        raise SystemExit(f"stamp-assets: no '{WASM}' literal in {GLUE}; wasm-bindgen output changed")
    return stamped


def main() -> None:
    if len(sys.argv) != 3 or not re.fullmatch(r"[0-9A-Za-z._-]{1,64}", sys.argv[2]):
        raise SystemExit("usage: stamp-assets.py <webapp-dir> <build-id>")
    root, build = Path(sys.argv[1]), sys.argv[2]
    modules = module_files(root)
    pages = sorted(root.glob("*.html"))
    if not pages:
        raise SystemExit(f"stamp-assets: no pages in {root}")
    for page in pages:
        page.write_text(stamp_page(page.read_text(encoding="utf-8"), build, modules), encoding="utf-8")
    glue = root / GLUE
    if glue.exists():
        glue.write_text(stamp_glue(glue.read_text(encoding="utf-8"), build), encoding="utf-8")
    print(f"stamp-assets: {len(pages)} page(s), {len(modules)} module(s), build {build}")


if __name__ == "__main__":
    main()
