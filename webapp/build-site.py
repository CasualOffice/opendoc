#!/usr/bin/env python3
"""Build-time partial inliner for the flat-static OpenDoc site.

The site is served as flat HTML files by GitHub Pages — no SPA framework and no
client-side routing (see README / build.sh). To keep the shared header and
footer DRY across every page without a runtime, page bodies are authored as
`*.page.html` templates that reference partials with include markers:

    <!-- @include site-header active=docs -->
    <!-- @include site-footer -->

This generator inlines `_partials/<name>.html` at each marker and writes the
final flat `<name>.html` next to the template. The `active=<key>` argument marks
the matching nav link (`data-nav="<key>"`) with `aria-current="page"` so each
page shows its own active state. The marker's own indentation is applied to
every inlined line, so generated HTML stays tidy and, crucially, deterministic:
re-running the generator produces byte-identical output (enforced by `--check`,
which the build and CI run so the committed HTML can never drift from its
template + partials).

Usage:
    ./build-site.py           # regenerate every *.page.html -> *.html
    ./build-site.py --check   # verify committed HTML matches a fresh build
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
PARTIALS = HERE / "_partials"

# One include marker per line: `<!-- @include NAME [key=value ...] -->`.
INCLUDE_RE = re.compile(
    r"^(?P<indent>[ \t]*)<!-- @include (?P<name>[\w-]+)(?P<args>(?:\s+[\w-]+=[^\s>]+)*)\s*-->[ \t]*$",
    re.MULTILINE,
)


def parse_args(raw: str) -> dict[str, str]:
    return dict(pair.split("=", 1) for pair in raw.split())


def load_partial(name: str) -> str:
    path = PARTIALS / f"{name}.html"
    if not path.is_file():
        raise SystemExit(f"build-site: unknown partial '{name}' ({path} not found)")
    return path.read_text(encoding="utf-8").rstrip("\n")


def render_partial(name: str, args: dict[str, str]) -> str:
    html = load_partial(name)
    active = args.get("active")
    if active:
        # Mark the active primary-nav link for styling + assistive tech. The
        # partial tags each link with a stable data-nav key.
        needle = f'data-nav="{active}"'
        if needle not in html:
            raise SystemExit(f"build-site: no nav link data-nav=\"{active}\" in partial '{name}'")
        html = html.replace(needle, f'{needle} aria-current="page"', 1)
    return html


def indent_block(block: str, indent: str) -> str:
    if not indent:
        return block
    return "\n".join(indent + line if line else line for line in block.split("\n"))


def render_page(template: Path) -> str:
    def sub(match: re.Match) -> str:
        name = match.group("name")
        args = parse_args(match.group("args"))
        return indent_block(render_partial(name, args), match.group("indent"))

    return INCLUDE_RE.sub(sub, template.read_text(encoding="utf-8"))


def output_path(template: Path) -> Path:
    # foo.page.html -> foo.html
    return template.with_name(template.name[: -len(".page.html")] + ".html")


def templates() -> list[Path]:
    return sorted(HERE.glob("*.page.html"))


def build(check: bool) -> int:
    drift = []
    for template in templates():
        rendered = render_page(template)
        out = output_path(template)
        if check:
            current = out.read_text(encoding="utf-8") if out.is_file() else None
            if current != rendered:
                drift.append(out.name)
        else:
            out.write_text(rendered, encoding="utf-8")
            print(f"build-site: wrote {out.name} <- {template.name}")
    if check:
        if drift:
            print("build-site --check: generated HTML is stale for: " + ", ".join(drift))
            print("Run ./build-site.py and commit the result.")
            return 1
        print(f"build-site --check: {len(templates())} page(s) up to date.")
    return 0


if __name__ == "__main__":
    sys.exit(build(check="--check" in sys.argv[1:]))
