#!/usr/bin/env python3
"""Stamp a build version onto every script, stylesheet and module the site loads.

Why this exists: GitHub Pages caches `.html` for 10 minutes but `.js`, `.mjs`,
`.css` and `.wasm` for 4 hours, and the site referenced them by fixed URLs. After
every deploy a visitor's browser therefore paired the NEW page with the OLD
script for up to four hours. The Table menu shipped in #546 is how it surfaced:
the new `editor.html` drew a Table button, the cached `main.js` had no Table
rows, and the menu opened empty in production while every test passed.

Four things have to carry the version, because four different mechanisms fetch:

1. `<script src>`, `<link href>`, `modulepreload` and the wasm preload in each
   page — rewritten.
2. Module-to-module imports (`main.js` -> `./keyboard.mjs`, `../pkg/...`) — these
   resolve relative to the importing module and DROP its query string, so a
   versioned `main.js` would still import stale children. An import map in each
   page remaps every module URL to its versioned form.
3. The wasm binary, which wasm-bindgen's glue fetches with
   `new URL('casual_doc_wasm_bg.wasm', import.meta.url)` — also query-less, so the
   literal in the glue is rewritten.
4. Static files — the logo, the touch icon, fonts — referenced from page markup
   and from `url()` inside the stylesheets. The first version of this script left
   them out, and the new logo shipped in #548 kept showing as the OLD one for four
   hours: `opendoc-mark.svg` is served `max-age=14400` from a fixed URL, exactly
   the defect this script exists to prevent, one file type over.

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
# The lazily loaded PDF module (`docs/182` §17a): imported by `pdf_module.mjs`,
# so it is in the import map, and its glue fetches its own wasm.
PDF_GLUE = "pkg-pdf/casual_doc_wasm_pdf.js"
PDF_WASM = "casual_doc_wasm_pdf_bg.wasm"

# Static file types a page or stylesheet can reference. Page links (`.html`) are
# deliberately absent: they are navigations with canonical URLs, not assets, and
# Pages already caches HTML for only ten minutes.
STATIC = r"svg|png|jpe?g|gif|webp|ico|woff2?|ttf|otf"


def module_files(root: Path) -> list[str]:
    """Every module a page can reach, as site-relative paths."""
    found = sorted(
        str(p.relative_to(root)).replace("\\", "/")
        for pattern in ("src/*.js", "src/*.mjs")
        for p in root.glob(pattern)
    )
    if (root / GLUE).exists():
        found.append(GLUE)
    if (root / PDF_GLUE).exists():
        found.append(PDF_GLUE)
    return found


# A static import or re-export at the start of a statement: `import x from "./a.mjs"`,
# `import { a,\n b } from "./a.mjs"`, `export { a } from "./a.mjs"`, `import "./a.mjs"`.
# Anchored at a line start so comments and strings that merely mention an import do not
# count, and it never matches `import(` — a dynamic import is fetched on demand, by
# design, and preloading it would defeat that.
STATIC_IMPORT = re.compile(
    r"^[ \t]*(?:import|export)\b(?:[^;\"'`(]*?\bfrom)?\s*([\"'])(\.{1,2}/[^\"']+)\1",
    re.M,
)


def static_graph(root: Path, entries: list[str]) -> list[str]:
    """Every module statically reachable from `entries`, site-relative, in discovery order.

    O(modules + imports); reads each module once."""
    seen: list[str] = []
    queue = list(entries)
    while queue:
        rel = queue.pop(0)
        if rel in seen:
            continue
        path = root / rel
        if not path.is_file():
            continue
        seen.append(rel)
        for match in STATIC_IMPORT.finditer(path.read_text(encoding="utf-8")):
            target = (path.parent / match.group(2).split("?", 1)[0]).resolve()
            try:
                queue.append(str(target.relative_to(root.resolve())).replace("\\", "/"))
            except ValueError:
                continue  # outside the site: not ours to preload
    return seen


def module_preloads(html: str, build: str, root: Path | None) -> str:
    """3. One flat `modulepreload` for every module the page's scripts import statically.

    Without it the browser discovers the graph a level at a time: it must fetch and
    parse `main.js` before it learns of the ~110 modules main.js imports, fetch those
    before it learns of theirs, and so on — five sequential round trips over ~310
    modules before the editor's script can run. Measured on the deployed editor
    (wasm blocked, so the module graph alone), DOMContentLoaded fell from a median
    of ~9.6 s to ~6.0 s with the flat list (docs/109 HF-421). The hrefs are the
    versioned URLs the import map resolves to, so each preload is the very response
    the import then uses."""
    html = re.sub(r'\s*<link rel="modulepreload" data-stamp [^>]*>', "", html)
    if root is None:
        return html
    entries = re.findall(r'<script type="module" src="\./((?:src|pkg)/[^"?#]+)', html)
    if not entries:
        return html
    already = set(re.findall(r'<link rel="modulepreload" href="\./([^"?#]+)', html))
    graph = [m for m in static_graph(root, entries) if m not in already and m not in entries]
    if not graph:
        return html
    block = "".join(
        f'\n    <link rel="modulepreload" data-stamp href="./{m}?v={build}" />' for m in graph
    )
    # After the page's own preloads (main.js, the glue, the wasm), so the entry
    # module and the multi-megabyte engine are requested first; otherwise right
    # after the import map, which must precede every module fetch.
    head_end = html.find("</head>")
    own = [
        m
        for m in re.finditer(r'<link rel="(?:module)?preload"(?! data-stamp)[^>]*>', html)
        if head_end < 0 or m.start() < head_end
    ]
    anchor = own[-1] if own else re.search(r'<script type="importmap" data-stamp>.*?</script>', html, flags=re.S)
    at = anchor.end() if anchor else html.index("<head>") + len("<head>")
    return html[:at] + block + html[at:]


def stamp_page(html: str, build: str, modules: list[str], root: Path | None = None) -> str:
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
    # Static files named in markup: the favicon, the touch icon, the logo image.
    html = re.sub(
        r'\b(src|href)="(\./[^"?#]+\.(?:' + STATIC + r'))(?:\?[^"]*)?"',
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
    return module_preloads(html, build, root)


def stamp_stylesheet(css: str, build: str) -> str:
    """Versions every relative `url()` to a static file inside a stylesheet.

    The stylesheet itself is versioned from the page, but a `url()` inside it
    resolves against the stylesheet's path and drops its query, exactly like a
    module import — so `url("../opendoc-mark.svg")` fetched the cached logo."""

    def versioned(match: re.Match[str]) -> str:
        quote, path = match.group(1), match.group(2)
        return f"url({quote}{path}?v={build}{quote})"

    return re.sub(
        r"url\((['\"]?)((?!data:|https?:|//)[^'\")?#]+\.(?:" + STATIC + r"))(?:\?v=[^'\")]*)?\1\)",
        versioned,
        css,
    )


def stamp_glue(js: str, build: str, wasm: str = WASM, glue: str = GLUE) -> str:
    pattern = re.compile(r"(['\"])" + re.escape(wasm) + r"(?:\?v=[^'\"]*)?\1")
    stamped, count = pattern.subn(lambda m: f"{m.group(1)}{wasm}?v={build}{m.group(1)}", js)
    if count == 0:
        raise SystemExit(f"stamp-assets: no '{wasm}' literal in {glue}; wasm-bindgen output changed")
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
        page.write_text(stamp_page(page.read_text(encoding="utf-8"), build, modules, root), encoding="utf-8")
    stylesheets = sorted((root / "src").glob("*.css"))
    for sheet in stylesheets:
        sheet.write_text(stamp_stylesheet(sheet.read_text(encoding="utf-8"), build), encoding="utf-8")
    glue = root / GLUE
    if glue.exists():
        glue.write_text(stamp_glue(glue.read_text(encoding="utf-8"), build), encoding="utf-8")
    pdf_glue = root / PDF_GLUE
    if pdf_glue.exists():
        pdf_glue.write_text(
            stamp_glue(pdf_glue.read_text(encoding="utf-8"), build, PDF_WASM, PDF_GLUE),
            encoding="utf-8",
        )
    print(
        f"stamp-assets: {len(pages)} page(s), {len(stylesheets)} stylesheet(s), "
        f"{len(modules)} module(s), build {build}"
    )


if __name__ == "__main__":
    main()
