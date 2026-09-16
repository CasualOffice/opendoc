// Drift + honesty guard for the public DOCX fidelity support matrix
// (webapp/src/fidelity.js, rendered by fidelity.html). This does NOT re-derive
// support state — it locks the shape and a handful of load-bearing "must stay
// honest" cells so a careless edit can't silently overstate public claims or
// drop a construct family.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

// fidelity.js is a classic browser <script> (this package is type=module, so it
// can't be `require`d, and it must stay import/export-free for the browser).
// Evaluate it in a sandbox with a fake `module` and no `document`, so its
// data-export block runs while its DOM render is skipped.
const source = readFileSync(
  new URL("../src/fidelity.js", import.meta.url),
  "utf8",
);
const sandbox = { exports: {} };
new Function("module", source)(sandbox);
const { FIDELITY, FIDELITY_STAGE, FORMAT_SUPPORT } = sandbox.exports;

const STAGES = ["modeled", "rendered", "editable", "roundtrips"];
const VALUES = new Set(["full", "partial", "placeholder", "preserved", "none"]);

test("format pipeline rows are complete and do not overstate ODT", () => {
  assert.deepEqual(
    FORMAT_SUPPORT.map((row) => row.format),
    ["DOCX", "Normalized JSON", "Plain text", "ODT"],
  );
  for (const row of FORMAT_SUPPORT) {
    assert.ok(
      row.note && row.note.trim().length > 0,
      `${row.format} has a note`,
    );
    for (const stage of ["validation", "import", "export", "host"]) {
      assert.ok(
        VALUES.has(row[stage]),
        `${row.format}.${stage} is a known stage value`,
      );
    }
  }
  const odt = FORMAT_SUPPORT.find((row) => row.format === "ODT");
  assert.equal(odt.validation, "full");
  assert.equal(odt.import, "partial");
  assert.equal(odt.export, "partial");
  assert.equal(odt.host, "partial");
  assert.match(odt.note, /nested bullet\/number lists/);
  assert.match(odt.note, /recursive tables/);
  assert.match(odt.note, /typed footnotes\/endnotes/);
  assert.match(odt.note, /authored citation labels/);
  assert.match(odt.note, /matching writer/);
  assert.match(odt.note, /unsafe merge geometry is visibly projected and reported/i);
  assert.match(odt.note, /advanced list continuation\/item overrides/);
  assert.equal(
    FORMAT_SUPPORT.find((row) => row.format === "Normalized JSON").host,
    "full",
  );
  assert.equal(
    FORMAT_SUPPORT.find((row) => row.format === "Plain text").host,
    "full",
  );
});

test("every construct family in the expected set is present exactly once", () => {
  const expected = [
    "Paragraphs & text",
    "Character / run formatting",
    "Paragraph & named styles",
    "Tables",
    "Lists & numbering",
    "Images & inline drawings",
    "Text boxes & shapes",
    "Headers & footers",
    "Footnotes & endnotes",
    "Sections, columns & page setup",
    "Fields",
    "Math (OMML)",
    "Charts",
    "SmartArt",
    "VML pictures & shapes",
    "Comments",
    "Tracked changes",
    "Bookmarks & hyperlinks",
    "Content controls (w:sdt)",
    "Fonts, fallback & color glyphs",
    "Hyphenation",
    "Line numbering (w:lnNumType)",
    "Watermarks & WordArt",
    "Bidi, RTL & CJK grid",
    "Vertical & rotated text",
    "Drop caps",
  ];
  const actual = FIDELITY.map((row) => row.family);
  assert.deepEqual(
    actual,
    expected,
    "construct list drifted — update the page and this guard together",
  );
});

test("every row is well-formed with a note and valid stage values", () => {
  for (const row of FIDELITY) {
    assert.ok(
      row.note && row.note.trim().length > 0,
      `${row.family} has a note`,
    );
    for (const stage of STAGES) {
      assert.ok(
        VALUES.has(row[stage]),
        `${row.family}.${stage} is a known stage value (got ${row[stage]})`,
      );
    }
    // Every stage value the page can render must have a glyph/label.
    for (const stage of STAGES) {
      assert.ok(
        FIDELITY_STAGE[row[stage]],
        `${row.family}.${stage} maps to a legend entry`,
      );
    }
  }
});

test("load-bearing honesty invariants hold (do not overstate public support)", () => {
  const by = Object.fromEntries(FIDELITY.map((row) => [row.family, row]));
  // Images: select/resize/move/wrap/z-order editing ships (inline + body floats);
  // crop/alt-text/insert-image are follow-ups — so partial, not none (never full).
  assert.equal(by["Images & inline drawings"].editable, "partial");
  // Headers/footers ARE an editing surface: enter by double-click or the hover
  // marker, type and format with the same pipeline as the body (including
  // comments and tracked changes), create one where none exists, and toggle the
  // first-page / odd-even variants.
  assert.equal(by["Headers & footers"].editable, "full");
  // Text boxes/shapes: select/resize/move/wrap/z-order ships; box content + shape
  // authoring are follow-ups — partial, not none.
  assert.equal(by["Text boxes & shapes"].editable, "partial");
  // Common shape model (fill/gradient, outline/dash/arrows, rotation/flip, wrap
  // contour, preset geometry) is fully typed as of Layer 1; custGeom stays
  // retained-not-typed, so semantic-mode round-trip remains partial.
  assert.equal(by["Text boxes & shapes"].modeled, "full");
  assert.equal(by["Text boxes & shapes"].roundtrips, "partial");
  // Notes can be inserted and their bodies edited like any other surface;
  // footnote/endnote conversion and number-format options are not there, so
  // partial rather than full.
  assert.equal(by["Footnotes & endnotes"].editable, "partial");
  assert.equal(by["Fields"].editable, "none");
  // Math is fully typed as of Layer 1 (all 20 OMML math elements mapped or
  // raw-retained), but rendering is a bounded subset and it stays read-only.
  assert.equal(by["Math (OMML)"].modeled, "full");
  assert.equal(by["Math (OMML)"].rendered, "partial");
  assert.equal(by["Math (OMML)"].editable, "none");
  // Character rendering stays partial because emphasis/outline/shadow effects
  // remain unpainted. Typed underline styles, including wavy and words-only,
  // render. Paragraphs and Lists render their common surface in full — the
  // remainders (docGrid/autospace on paragraphs, unknown numFmt on lists) are
  // niche/bounded, not common gaps.
  assert.equal(by["Character / run formatting"].rendered, "partial");
  // Numbering renders in full, but it is NOT fully modeled: `w:lvlPicBulletId`
  // and the `w:numPicBullet` definitions it references have zero representation
  // in the model (`grep -rn "numPicBullet" crates/` is empty), so picture
  // bullets are dropped on import without a finding. Upgrading this to "full"
  // requires typing them; the note must stop calling it a render-only gap.
  assert.equal(by["Lists & numbering"].modeled, "partial");
  assert.equal(by["Lists & numbering"].rendered, "full");
  // Comments render only under the read-only markup review view
  // (casual-doc-layout/src/flow.rs:2908-2919 gates the highlight on
  // ReviewView::Markup). In the default editing view the highlight and sidebar
  // are host/DOM chrome and contribute nothing to the display list, so engine
  // rendering is partial. Upgrade only when comment anchors reach the display
  // list in the default view.
  assert.equal(by["Comments"].rendered, "partial");
  // Charts / SmartArt are preserved, not rendered as charts/diagrams.
  assert.equal(by["Charts"].rendered, "preserved");
  assert.equal(by["SmartArt"].rendered, "preserved");
  // Nothing claims "full" editable for content the editor cannot author at all
  // (charts, SmartArt, math) or can only partly author (images: no in-place
  // replace, no rotation, no picture styling). Headers and footers are NOT on
  // this list any more — they are a complete editing surface, held to that by
  // the operation × surface matrix and the formatting-toggle audit.
  for (const family of [
    "Charts",
    "SmartArt",
    "Math (OMML)",
    "Images & inline drawings",
  ]) {
    assert.notEqual(
      by[family].editable,
      "full",
      `${family} must not claim full editability`,
    );
  }

  // Families added by the 2026-09 audit because their absence from this table
  // was itself an overstatement: a reader seeing only full/partial rows infers
  // coverage that does not exist. Each assertion below names the code fact that
  // has to change before the cell may be raised, so the guard fails on a
  // premature upgrade rather than rubber-stamping it.

  // No hyphenator, dictionary, or consumer for w:autoHyphenation /
  // w:hyphenationZone exists; only w:suppressAutoHyphens is cascaded
  // (casual-doc-layout/src/cascade.rs:560-561).
  assert.equal(by["Hyphenation"].rendered, "none");
  // This cell used to be pinned to "none" with the justification "no
  // generator" — a guard holding a public page to a claim that had gone FALSE.
  // `place_line_numbers` runs after pagination (document_layout.rs, the call
  // before compose) and `compose_page` paints the stamps, with 16 integration
  // tests in casual-doc-layout/tests/line_numbering.rs. Partial, not full,
  // because lines inside table cells are deliberately not numbered yet; the
  // grade must move again only when that gap closes.
  assert.equal(by["Line numbering (w:lnNumType)"].modeled, "full");
  assert.equal(by["Line numbering (w:lnNumType)"].rendered, "partial");
  // `grep -ri watermark crates/` finds no watermark concept, and neither
  // v:textpath nor a:prstTxWarp is typed, so warped watermark text cannot paint.
  assert.equal(by["Watermarks & WordArt"].modeled, "none");
  assert.equal(by["Watermarks & WordArt"].rendered, "none");
  // One writing-mode axis only: every layout reference to text_direction is
  // `None` in test scaffolding.
  assert.equal(by["Vertical & rotated text"].rendered, "none");
  // Previously pinned to "partial" on the grounds that embedded .odttf faces
  // were never de-obfuscated. That shipped: `deobfuscate_odttf` and
  // `register_embedded_fonts` (casual-doc-layout/src/font_registry.rs), wired
  // at wasm open, with 7 tests in tests/embedded_fonts.rs. The remaining gap —
  // PANOSE/altName hints not consulted — is a consumption gap, and the hints
  // themselves are typed, so the MODEL side is full.
  assert.equal(by["Fonts, fallback & color glyphs"].modeled, "full");
  // Colour glyphs DO render (sbix/CBDT strikes then COLR v0/v1 in
  // casual-doc-render/src/lib.rs:593-605). This asserts the page does not
  // regress to denying a shipped feature, as it did until this audit.
  assert.equal(by["Fonts, fallback & color glyphs"].rendered, "full");
  // The shaper exposes no paragraph base-direction control and levels collapse
  // to a single RTL flag (casual-doc-layout/src/shape.rs:480-505, :1125).
  assert.equal(by["Bidi, RTL & CJK grid"].rendered, "partial");
  // None of these seven is authorable from the editor today.
  for (const family of [
    "Hyphenation",
    "Line numbering (w:lnNumType)",
    "Watermarks & WordArt",
    "Vertical & rotated text",
    "Drop caps",
  ]) {
    assert.equal(
      by[family].editable,
      "none",
      `${family} is not authorable from the editor`,
    );
  }
});

// EV-002. The "Construct families graded below" tile is a number typed into the
// page by hand. It read 19 against 26 families in `fidelity.js` — a figure that
// contradicted the table printed directly beneath it, on a page that is
// `robots: index,follow`. A number on a public page must be generated from, or
// at minimum checked against, a committed artifact; this is the check.
test("the fidelity page's family-count tile matches the data it renders", () => {
  for (const page of ["../fidelity.page.html", "../fidelity.html"]) {
    const html = readFileSync(new URL(page, import.meta.url), "utf8");
    const match = html.match(
      /<div class="fid-stat-num">(\d+)<\/div>\s*<div class="fid-stat-label">Construct families graded below<\/div>/,
    );
    assert.ok(match, `${page} must carry the families tile`);
    assert.equal(
      Number(match[1]),
      FIDELITY.length,
      `${page} claims ${match[1]} construct families; fidelity.js defines ${FIDELITY.length}`,
    );
  }
});

// EV-001. The same page advertised the grades as "CI-enforced" in its meta
// description long after the body prose was corrected to say the oracle gate is
// manual and inert. The meta is what search results and link previews quote, so
// it is the half of the page most likely to be read.
test("the fidelity page does not advertise a CI gate it does not have", () => {
  for (const page of ["../fidelity.page.html", "../fidelity.html"]) {
    const html = readFileSync(new URL(page, import.meta.url), "utf8");
    const meta = html.match(/<meta\s+name="description"[^>]*content="([^"]*)"/s);
    assert.ok(meta, `${page} must carry a meta description`);
    assert.doesNotMatch(
      meta[1],
      /(?<!not )CI-enforced/,
      `${page}'s meta description claims the grades are CI-enforced; the oracle ` +
        `geometry gate is workflow_dispatch-only and skips every fixture`,
    );
  }
});
