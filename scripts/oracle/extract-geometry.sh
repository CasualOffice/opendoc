#!/usr/bin/env bash
# Oracle geometry extraction (docs/94 H2): render a .docx with a *pinned*
# LibreOffice, then reduce the resulting PDF to the per-page geometry the
# casual-doc-render `oracle_geometry` test compares against.
#
# Output (stdout, and written to fixtures/oracle/<id>.geom.json by the caller):
#   { "pages": [ { "sizeTwips": [w,h], "contentBboxTwips": [x0,y0,x1,y1]|null } ] }
#
# Coordinates: PDF points (1/72in), origin top-left, converted to twips (×20) so
# they match the engine's twip geometry. The content bbox is the union of the
# page's word boxes (pdftotext -bbox), i.e. the inked text region.
#
# Determinism (docs/94 §Determinism): pin the LibreOffice version AND install
# ONLY the bundled metric-compatible faces (Liberation/Carlito/Caladea) so the
# oracle shapes with the same metrics the engine does. The re-bless CI job
# (.github/workflows/oracle-geometry.yml) provides that environment; run this
# there, not on an ad-hoc workstation, or the reference will not be reproducible.
#
# Usage: extract-geometry.sh <input.docx>   # prints the JSON to stdout
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <input.docx>" >&2
  exit 2
fi
input="$1"
if [[ ! -f "$input" ]]; then
  echo "no such file: $input" >&2
  exit 2
fi

for tool in soffice pdftotext python3; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "required tool not found: $tool" >&2
    exit 3
  }
done

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

# 1. .docx -> PDF (headless, deterministic profile).
soffice --headless --norestore --nolockcheck \
  --convert-to pdf --outdir "$workdir" "$input" >/dev/null

pdf="$workdir/$(basename "${input%.*}").pdf"
[[ -f "$pdf" ]] || {
  echo "LibreOffice produced no PDF for $input" >&2
  exit 4
}

# 2. PDF -> per-word bounding boxes (XHTML with <page>/<word> elements).
pdftotext -bbox "$pdf" "$workdir/bbox.html"

# 3. Reduce the word boxes to per-page {size, content bbox} in twips.
python3 - "$workdir/bbox.html" <<'PY'
import sys, re, json, html.parser

class BBox(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.pages = []          # list of dict(w,h, x0,y0,x1,y1|None)
        self._cur = None
    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "page":
            self._cur = {"w": float(a["width"]), "h": float(a["height"]),
                         "x0": None, "y0": None, "x1": None, "y1": None}
            self.pages.append(self._cur)
        elif tag == "word" and self._cur is not None:
            x0, y0 = float(a["xmin"]), float(a["ymin"])
            x1, y1 = float(a["xmax"]), float(a["ymax"])
            c = self._cur
            c["x0"] = x0 if c["x0"] is None else min(c["x0"], x0)
            c["y0"] = y0 if c["y0"] is None else min(c["y0"], y0)
            c["x1"] = x1 if c["x1"] is None else max(c["x1"], x1)
            c["y1"] = y1 if c["y1"] is None else max(c["y1"], y1)

parser = BBox()
with open(sys.argv[1], encoding="utf-8") as fh:
    parser.feed(fh.read())

def twips(pt):
    # PDF points (1/72in) -> twips (1/1440in); round to the nearest twip.
    return int(round(pt * 20.0))

out = {"pages": []}
for p in parser.pages:
    has_ink = p["x0"] is not None
    out["pages"].append({
        "sizeTwips": [twips(p["w"]), twips(p["h"])],
        "contentBboxTwips": (
            [twips(p["x0"]), twips(p["y0"]), twips(p["x1"]), twips(p["y1"])]
            if has_ink else None
        ),
    })
print(json.dumps(out, indent=2))
PY
