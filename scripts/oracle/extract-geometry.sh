#!/usr/bin/env bash
# Oracle geometry extraction (docs/94 H2): render a .docx with a *pinned*
# LibreOffice, then reduce the resulting PDF to the per-page geometry the
# casual-doc-render `oracle_geometry` test compares against.
#
# Output (stdout, and written to fixtures/oracle/<id>.geom.json by the caller):
#   { "schema": 2,
#     "pages": [ { "sizeTwips": [w,h],
#                  "contentBboxTwips": [x0,y0,x1,y1]|null,
#                  "excludedLines": n } ] }
#
# Coordinates: PDF points (1/72in), origin top-left, converted to twips (×20) so
# they match the engine's twip geometry.
#
# WHAT THE CONTENT BBOX IS (read before changing it — the two sides of this gate
# have to measure the *same* quantity, and once did not):
#
#   It is the union of the page's `pdftotext -bbox` word boxes, which is the
#   *text region*, NOT the inked region and NOT the block/paragraph boxes.
#   Poppler derives a word box from the text state, not the glyph outlines:
#     x: the pen position before the word's first glyph .. after its last
#     y: baseline − the font descriptor's ascent .. baseline + its descent
#   (Verified against this corpus: LibreOffice's 12pt Liberation Serif word boxes
#   are 13.284pt tall, exactly (1824+443)/2048 × 12pt — lowercase ink is not.)
#   The engine reproduces the same quantity from its shaped runs' pen advances
#   and per-run ascent/descent, so no ink-vs-layout fudge factor is involved.
#
# FONT-PARITY SCOPING (`excludedLines`):
#
#   Determinism rests on both renderers shaping with the same metric-compatible
#   faces — but that only holds for the code points those faces COVER. A run of
#   CJK, Arabic, emoji, … is substituted by whatever each side happens to find,
#   with different advances, and a wider substitute also shifts every word after
#   it on the same line. Such text is not oracle-comparable, so the words are
#   grouped into lines (by vertical overlap) and any line containing an
#   out-of-coverage code point is dropped from the bbox and counted instead. The
#   test applies the mirror-image rule (drop a line holding a run that did not
#   resolve to a bundled metric-compatible face) and compares the counts exactly,
#   so the exclusion cannot silently hide a line.
#
# Determinism (docs/94 §Determinism): pin the LibreOffice version AND install
# ONLY the bundled metric-compatible faces (Liberation/Carlito/Caladea) so the
# oracle shapes with the same metrics the engine does. The re-bless CI job
# (.github/workflows/oracle-geometry.yml) provides that environment; run this
# there, not on an ad-hoc workstation, or the reference will not be reproducible.
#
# SCHEMA: bump `SCHEMA` below (and `ORACLE_SCHEMA` in the test) whenever the
# meaning of a reference field changes. The test then compares only page count
# and size against a reference blessed under the old meaning, until this job has
# regenerated it — references are never hand-edited to make a comparison pass.
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

# 3. Reduce the word boxes to per-page {size, content bbox, excluded lines}.
python3 - "$workdir/bbox.html" <<'PY'
import sys, json, html.parser

SCHEMA = 2

# Code points the pinned faces (Liberation Sans/Serif/Mono, Carlito, Caladea)
# all cover, as inclusive ranges. Deliberately conservative: a range left out
# only excludes more text from the comparison, whereas a range wrongly claimed
# here silently compares glyphs one of the two renderers had to substitute.
PINNED_RANGES = (
    (0x0020, 0x007E),  # Basic Latin (printable)
    (0x00A0, 0x017F),  # Latin-1 Supplement + Latin Extended-A
    (0x2010, 0x2027),  # General Punctuation: dashes, quotes, bullets, ellipsis
    (0x2030, 0x205E),  # General Punctuation: per-mille, primes, spaces, marks
    (0x20AC, 0x20AC),  # Euro sign
)


def is_pinned(text):
    """Whether every code point in `text` is covered by the pinned faces.

    Whitespace is ignored: it carries no glyph of its own, and poppler may hand
    back markup whitespace around a word's character data.
    """
    for ch in text:
        if ch.isspace():
            continue
        cp = ord(ch)
        if not any(lo <= cp <= hi for lo, hi in PINNED_RANGES):
            return False
    return True


class BBox(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.pages = []          # list of dict(w, h, words=[...])
        self._cur = None
        self._word = None
    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "page":
            self._cur = {"w": float(a["width"]), "h": float(a["height"]), "words": []}
            self.pages.append(self._cur)
        elif tag == "word" and self._cur is not None:
            self._word = {
                "x0": float(a["xmin"]), "y0": float(a["ymin"]),
                "x1": float(a["xmax"]), "y1": float(a["ymax"]),
                "text": "",
            }
            self._cur["words"].append(self._word)
    def handle_data(self, data):
        if self._word is not None:
            self._word["text"] += data
    def handle_endtag(self, tag):
        if tag == "word":
            self._word = None


def group_into_lines(words):
    """Groups word boxes into lines by vertical overlap.

    Sort by top edge, then start a new line whenever a word begins at or below
    the running bottom of the current one. Mixed sizes on a line still overlap,
    so they group together; side-by-side content (columns, table cells) can merge
    into one band, which is conservative — it can only widen what one
    unshapeable word excludes.
    """
    lines = []
    bottom = None
    for w in sorted(words, key=lambda w: (w["y0"], w["x0"])):
        if bottom is not None and w["y0"] < bottom:
            lines[-1].append(w)
            bottom = max(bottom, w["y1"])
        else:
            lines.append([w])
            bottom = w["y1"]
    return lines


parser = BBox()
with open(sys.argv[1], encoding="utf-8") as fh:
    parser.feed(fh.read())

def twips(pt):
    # PDF points (1/72in) -> twips (1/1440in); round to the nearest twip.
    return int(round(pt * 20.0))

out = {"schema": SCHEMA, "pages": []}
for p in parser.pages:
    bbox = None
    excluded = 0
    for line in group_into_lines(p["words"]):
        if not all(is_pinned(w["text"]) for w in line):
            excluded += 1
            continue
        for w in line:
            if bbox is None:
                bbox = [w["x0"], w["y0"], w["x1"], w["y1"]]
            else:
                bbox = [min(bbox[0], w["x0"]), min(bbox[1], w["y0"]),
                        max(bbox[2], w["x1"]), max(bbox[3], w["y1"])]
    out["pages"].append({
        "sizeTwips": [twips(p["w"]), twips(p["h"])],
        "contentBboxTwips": [twips(v) for v in bbox] if bbox is not None else None,
        "excludedLines": excluded,
    })
print(json.dumps(out, indent=2))
PY
