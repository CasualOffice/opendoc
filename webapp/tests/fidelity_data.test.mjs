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
    "Document protection & forms",
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
  // Legacy form fields are operable (`docs/109` HF-175): a FORMCHECKBOX ticks
  // and a FORMTEXT accepts typing into its result. No other field kind is
  // editable as a field, so partial rather than full - and never "full" while
  // dropdown form fields cannot be operated at all.
  assert.equal(by["Fields"].editable, "partial");
  assert.notEqual(by["Fields"].editable, "full");
  // All four `w:edit` levels are enforced since ADR-052 - `forms` at the
  // facade's mutation choke point, `readOnly`/`comments`/`trackedChanges` at
  // the operation in `casual-doc-edit`. The pin used to say the other three
  // were NOT enforced, which had been false since ADR-052 landed: a guard can
  // pin a lie (SKILL §9 rule 6), so this one now states what keeps the cell off
  // "full" - `w:formatting` style locking is not enforced, and a
  // `trackedChanges`-protected document does not force Suggesting on open.
  assert.equal(by["Document protection & forms"].editable, "partial");
  assert.equal(by["Document protection & forms"].rendered, "none");
  // Round-trip is NOT full, and it never was: the sixteen `AG_Password` /
  // `AG_TransitionalPassword` attributes on `w:documentProtection` and
  // `w:writeProtection` have no home in the model, and `word/settings.xml` is a
  // consumed part the semantic writer regenerates - so a password-protected
  // document saves with the restriction intact and the password gone. The
  // importer now REPORTS every one of those attributes by name
  // (`documentProtection/@hashValue`, ...) so the loss is not silent, which is
  // what makes "partial" the honest grade rather than "full".
  assert.equal(by["Document protection & forms"].roundtrips, "partial");
  assert.notEqual(by["Document protection & forms"].roundtrips, "full");
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
  // SmartArt is preserved, not rendered as a diagram.
  //
  // CORRECTED 2026-10-04: this pair said "Charts / SmartArt are preserved, not
  // rendered as charts/diagrams" and pinned `Charts.rendered` to "preserved".
  // That was a guard holding the public page BELOW the code — `SKILL` §9.6: a
  // grade pinned at a value the code has outgrown is as false as one pinned
  // above it, and understating is also lying. `casual-doc-layout/src/chart.rs`
  // paints all six modeled families (`is_drawable` returns true for every
  // `ChartGroupKind` variant, pie and doughnut through the shared arc/sector
  // geometry), so a chart is drawn, not stood in for. The Charts row is now
  // derived from the painter, the part writer and the host surface by the test
  // at the bottom of this file rather than pinned here.
  assert.equal(by["SmartArt"].rendered, "preserved");
  // Nothing claims "full" editable for content the editor cannot author at all
  // (SmartArt, math) or can only partly author — charts moved into the second
  // group on 2026-10-04: an existing one selects, resizes and deletes through
  // the generic object surface, and the gap is that nothing can CREATE one.
  // Headers and footers are
  // NOT on this list any more — they are a complete editing surface, held to that
  // by the operation × surface matrix and the formatting-toggle audit.
  //
  // CORRECTED 2026-10-01: this comment said images were partial because of "no
  // in-place replace, no rotation, no picture styling". Rotation and flip ARE
  // authored — three surfaces over `setObjectRotation`/`setObjectFlip`, which read
  // and write a picture's own transform — so the reason stands on the other two:
  // no in-place byte replacement and no picture borders, effects or transparency
  // authoring. The GRADE is unchanged and was never the overstatement; the
  // justification was, and a stale justification in a guard is how a grade gets
  // defended with a fact that stopped being true (`105` EV-007).
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
  // This pair was pinned to "none" on the evidence that `grep -ri watermark
  // crates/` found no watermark concept. That has gone false, and a guard
  // holding a public page to a stale claim understates the engine exactly as
  // badly as overstating it (EV-007). What exists now: `SectionBoundary.watermark`
  // in the model, `casual-doc-layout/src/watermark.rs` stamping it after
  // pagination (9 tests in casual-doc-layout/tests/watermark.rs), and
  // `casual-doc-import/src/watermark.rs` lifting Word's header shape onto the
  // section, reading `v:textpath@string`.
  //
  // PARTIAL, not full, and the halves must move separately:
  //  * WordArt itself (`a:prstTxWarp`, and every warped preset but plain text) is
  //    still neither typed nor painted — this family covers both.
  //  * A DrawingML watermark is not recognised on import.
  //  * The semantic DOCX writer does not write the watermark back yet, so
  //    `roundtrips` stays "partial" too.
  // Raise either cell only when the corresponding half actually closes.
  assert.equal(by["Watermarks & WordArt"].modeled, "partial");
  assert.equal(by["Watermarks & WordArt"].rendered, "partial");
  assert.equal(by["Watermarks & WordArt"].roundtrips, "partial");
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
      /<div class="fid-stat-num"[^>]*>(\d+)<\/div>\s*<div class="fid-stat-label"[^>]*>Construct families graded below<\/div>/,
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

// EV-007 in the OTHER direction: a note that still denies something the editor
// ships.
//
// `fidelity.js` carried "Rotation and flip authoring, custom geometry, and
// text-box body properties (internal margins, vertical anchor, autofit) are not"
// on the shapes row, and "authoring rotation/flip or transparency … are not
// there" on the images row, for the whole time after that authoring shipped.
// Understating is the same defect as overstating (`SKILL` §9 rule 6), and it did
// more damage than it looks: those two sentences were being quoted as a reason not
// to count work that was done.
//
// Nothing here re-derives a grade. What it holds is narrower and checkable: a note
// may not DENY a capability whose authoring path is in the tree. Each row pairs a
// denial phrase with the binding that makes the denial false, read out of the Rust
// rather than asserted as a fact about it — so it fails in BOTH directions: a note
// that re-adds the denial, and a binding removed while the note still promises it.
test("no fidelity note denies an authoring path the engine ships", () => {
  const WASM = new URL("../../crates/casual-doc-wasm/src/", import.meta.url);
  const DENIALS = [
    {
      family: "Text boxes & shapes",
      denial: /rotation and flip authoring[^.]*are not\b/i,
      file: "objects.rs",
      declaration: "js_name = setObjectRotation",
    },
    {
      family: "Text boxes & shapes",
      denial: /text-box body properties \([^)]*\) are not\b/i,
      file: "lib.rs",
      declaration: "js_name = setTextBoxBodyProperties",
    },
    {
      family: "Images & inline drawings",
      denial: /authoring rotation\/flip/i,
      file: "objects.rs",
      declaration: "js_name = setObjectFlip",
    },
  ];
  const stale = [];
  for (const { family, denial, file, declaration } of DENIALS) {
    const rust = readFileSync(new URL(file, WASM), "utf8");
    const shipped = rust.includes(declaration);
    const row = FIDELITY.find((entry) => entry.family === family);
    assert.ok(row, `fidelity.js no longer grades ${family}`);
    const denied = denial.test(row.note);
    if (shipped && denied) {
      stale.push(`${family}: the note still denies what ${file} declares as ${declaration}`);
    }
    if (!shipped && !denied) {
      stale.push(`${family}: ${declaration} is gone from ${file} and the note no longer says so`);
    }
  }
  assert.deepEqual(stale, []);
});

// A `modeled` grade is derived from the importer, not from recollection.
//
// `rendered` has been pinned here since the page was first guarded, and
// `modeled` never was — which is how SmartArt carried `modeled: "full"` while
// NOTHING under `word/diagrams` is parsed anywhere in the importer, and Charts
// carried it while six chart families and the whole of `chartex` are untyped.
// Both are §9 rule 3 overstatements by omission, and both reached the published
// page, which is the third time this file has had to catch that.
//
// This asserts the two claims that CAN be derived from the tree rather than
// agreed by hand. It deliberately does not try to grade every family: a guard
// that pretends to check more than it does is the `fidelity_data` failure mode
// §9 rule 6 names, and this file has already been that once.
test("a family the importer does not parse may not claim it is modeled", () => {
  const importer = readFileSync(
    new URL("../../crates/casual-doc-import/src/body.rs", import.meta.url),
    "utf8",
  );
  const by = Object.fromEntries(FIDELITY.map((row) => [row.family, row]));

  // SmartArt: the diagram parts are the only place a typed diagram could come
  // from. No reference to them means the model holds a relationship and nothing
  // else, so anything but "none" is a claim about code that is not there.
  if (!importer.includes("word/diagrams")) {
    assert.equal(
      by["SmartArt"].modeled,
      "none",
      'nothing in the importer reads word/diagrams, so SmartArt cannot be "modeled" at all — ' +
        "it is a preserved reference. Grade it `none` or point at the parser.",
    );
  }

  // The preview bitmap. Both embedded families pass `preview: None` on every
  // path, so a note promising the reader "a preview image shows if present" is
  // describing a branch that cannot be taken. Word writes no preview for a
  // classic chart part either, so this is not a gap waiting on a fixture.
  const previewless = /preview: None/.test(importer);
  for (const family of ["Charts", "SmartArt"]) {
    if (previewless) {
      assert.doesNotMatch(
        by[family].note,
        /preview image shows/i,
        `${family}'s note promises a preview image while the importer passes ` +
          "`preview: None` on every path, so no document can ever produce one",
      );
    }
  }
});

// Every Charts cell is derived from the code that decides it (`109` HF-256).
//
// The Charts row has been wrong in both directions. It carried `roundtrips:
// "full"` while a chart the editor minted saved as a drawing pointing at a part
// the package did not contain, and `rendered: "preserved"` with a note saying "a
// text placeholder stands in" while `casual-doc-layout/src/chart.rs` painted all
// six modeled families. One overstatement, one understatement, both published,
// and `rendered` was PINNED here at the understated value — which is `SKILL` §9.6
// exactly: a guard can hold a page to a claim that has gone false.
//
// So each of the four cells is keyed to a fact read out of the tree, and every
// check fails in BOTH directions: the grade may not rise above the code, and the
// code may not move past the grade. The three bindings are the chart painter
// (`is_drawable`), the chart part writer (`write_chart_part`) and the host
// surface (`insertChart`), plus the importer's own list of families it declines,
// which must appear in the note family by family — §9 rule 3: absence from a
// matrix is an overstatement by omission.
test("every Charts grade is derived from the painter, the writer and the host surface", () => {
  const CRATES = new URL("../../crates/", import.meta.url);
  const read = (path) => readFileSync(new URL(path, CRATES), "utf8");
  const charts = FIDELITY.find((row) => row.family === "Charts");
  assert.ok(charts, "fidelity.js no longer grades Charts");

  // --- rendered: the painter ------------------------------------------------
  // `is_drawable` is the single place the layout engine decides whether a chart
  // group paints at all; a family it returns true for is drawn, so claiming the
  // family is only "preserved" or stood in for by a "placeholder" denies code
  // that is right there.
  // The body is sliced to the first LINE-START `}` — `split("}")[0]` stops at
  // the brace inside `ChartGroupKind::Bar { .. }`, which is how the first
  // version of this guard read one family, skipped its own `rendered` check and
  // stayed green while the grade was put back to "preserved" by hand.
  const painter = read("casual-doc-layout/src/chart.rs");
  const opens = painter.indexOf("pub fn is_drawable");
  assert.notEqual(
    opens,
    -1,
    "casual-doc-layout/src/chart.rs no longer defines is_drawable — re-derive " +
      "Charts.rendered from whatever decides it now rather than leaving this blind",
  );
  const closes = painter.indexOf("\n}", opens);
  assert.ok(
    closes > opens,
    "is_drawable has no line-start closing brace, so this guard cannot tell " +
      "where the function ends and is reading an arbitrary prefix",
  );
  const drawable = painter.slice(opens, closes);
  const DRAWN = ["Bar", "Line", "Area", "Scatter", "Pie", "Doughnut"];
  // Only the arm that yields `true` counts. Matching the whole function body
  // counted a family whose arm returned FALSE — mutating `Pie` to
  // `=> false` left this green, because the substring `ChartGroupKind::Pie` was
  // still there. So the body is split at `=> true` and a family named after it
  // is treated as a guard that can no longer read the function, not as a
  // family that paints.
  const yields = drawable.indexOf("=> true");
  assert.ok(
    yields > 0,
    "is_drawable no longer has a `=> true` arm; this guard reads which " +
      "families paint by position relative to it and cannot read the new shape",
  );
  const paints = DRAWN.filter((family) =>
    drawable.slice(0, yields).includes(`ChartGroupKind::${family}`),
  );
  const afterwards = DRAWN.filter((family) =>
    drawable.slice(yields).includes(`ChartGroupKind::${family}`),
  );
  assert.deepEqual(
    afterwards,
    [],
    `is_drawable names ${afterwards.join(", ")} AFTER its \`=> true\` arm, so ` +
      "there is an arm this guard cannot read — a family may have stopped " +
      "painting. Re-derive Charts.rendered from the new shape; do not relax this",
  );
  assert.equal(
    paints.length,
    DRAWN.length,
    `is_drawable names ${paints.length} of the ${DRAWN.length} chart families ` +
      `(${paints.join(", ") || "none"}). Either a family stopped painting — in ` +
      "which case say so in the note before touching this number — or this " +
      "guard is reading the wrong text, which is worse than no guard",
  );
  // "preserved", "placeholder" and "none" each say nothing is painted. One
  // family drawing is enough to make all three false.
  assert.ok(
    !["preserved", "placeholder", "none"].includes(charts.rendered),
    `casual-doc-layout/src/chart.rs paints ${paints.join(", ")}, so ` +
      `Charts.rendered may not be "${charts.rendered}" — that grade says ` +
      "nothing is painted and a text placeholder stands in",
  );
  assert.doesNotMatch(
    charts.note,
    /not rendered as a live chart|a text placeholder stands in\b/i,
    "the note denies the painter in casual-doc-layout/src/chart.rs",
  );

  // --- modeled: the importer's declined families, enumerated ----------------
  // `out_of_scope_family` is the importer's own list of the families it refuses
  // to project. Each must be named in the note: a reader who sees "bar, line,
  // area, pie, doughnut, scatter are typed" and no list of what is not infers
  // coverage that does not exist.
  const importer = read("casual-doc-import/src/chart.rs");
  const scope = importer.slice(importer.indexOf("fn out_of_scope_family"));
  const declined = [
    ...scope.slice(0, scope.indexOf("\n}")).matchAll(/b"(\w+Chart)"/g),
  ].map((match) => match[1]);
  assert.ok(
    declined.length >= 5,
    `out_of_scope_family named ${declined.length} families and this guard ` +
      "expected the five it has carried since docs/155 §4.3 (surface, stock, " +
      "radar, bubble, ofPie) — it is reading the wrong function",
  );
  for (const family of declined) {
    assert.match(
      charts.note,
      new RegExp(family, "i"),
      `the importer declines c:${family} and the note does not say so — ` +
        "SKILL §9 rule 3: absence from a support matrix is an overstatement " +
        "by omission, so enumerate the families, not the successes",
    );
  }
  assert.ok(
    declined.length === 0 || charts.modeled !== "full",
    "the importer declines whole chart families, so Charts cannot be fully modeled",
  );
  // The 3-D rule is a suffix match rather than a list, so it has to be checked
  // for separately or a note that dropped it would still pass the loop above.
  if (scope.includes('ends_with(b"3DChart")')) {
    assert.match(
      charts.note,
      /3DChart/,
      "the importer declines every `*3DChart` by suffix and the note must say so",
    );
  }

  // --- roundtrips: the part writer ------------------------------------------
  // Before HF-256 every `word/charts/...` write lived in a `#[cfg(test)]`
  // module, so a minted chart saved as a relationship pointing at nothing while
  // this cell read "full". The cell is now tied to the writer being real: with
  // a writer, "none"/"preserved" understates it; with declined families, "full"
  // overstates it, because those survive only through retention.
  const exporter = read("casual-doc-export/src/chart.rs");
  // Anchored on the opening parenthesis: an unanchored
  // /fn write_chart_part/ also matches `write_chart_part_renamed`, so renaming
  // the writer away left this check green. Found by mutating it.
  const writes = /pub\(crate\) fn write_chart_part\(/.test(exporter);
  assert.ok(
    writes,
    "casual-doc-export/src/chart.rs no longer exposes write_chart_part — if the " +
      "chart part writer was removed, Charts.roundtrips must go back to " +
      '"preserved" and this guard must be rewritten, not deleted',
  );
  assert.ok(
    !["none", "preserved"].includes(charts.roundtrips),
    "a real chart part writer ships, so Charts.roundtrips may not say the family " +
      "only survives as preserved bytes",
  );
  assert.notEqual(
    charts.roundtrips,
    "full",
    `${declined.length} chart families decline projection, so a semantic-mode ` +
      "save keeps them only through retention — partial, not full",
  );

  // --- editable: the host surface -------------------------------------------
  // §9 rule 4: "built" is not "reachable". `insertChart` exists on the engine
  // and nothing in the host calls it, so a reader cannot create a chart. The
  // grade must not claim creation, and the moment a surface lands it must stop
  // denying it.
  const host = readFileSync(
    new URL("../src/main.js", import.meta.url),
    "utf8",
  );
  const engineInserts = /js_name = insertChart/.test(
    read("casual-doc-wasm/src/lib.rs"),
  );
  const hostInserts = host.includes("insertChart");
  assert.ok(
    engineInserts,
    "the engine's insertChart is gone; re-derive this cell from whatever " +
      "replaced it rather than leaving the grade unguarded",
  );
  if (!hostInserts) {
    assert.notEqual(
      charts.editable,
      "full",
      "no host surface calls insertChart, so chart editing cannot be full",
    );
    assert.match(
      charts.note,
      /no host surface calls the engine's `insertChart`/,
      "the engine can insert a chart and the host cannot reach it — the note has " +
        "to say which of the two is true, or the page claims the engine's " +
        "capability as the product's (SKILL §9 rule 4)",
    );
  } else {
    assert.doesNotMatch(
      charts.note,
      /no host surface calls the engine's `insertChart`/,
      "a host surface now calls insertChart and the note still denies it",
    );
  }
  // And the other half of "partial": an existing chart is published as an
  // object with resize grips, so the grade may not be "none" either.
  const publishesChart = /kind: "chart"/.test(read("casual-doc-wasm/src/lib.rs"));
  if (publishesChart) {
    assert.notEqual(
      charts.editable,
      "none",
      'the engine publishes a chart as an ObjectBox with kind: "chart" and ' +
        "embedded-object capabilities, so it selects, resizes and deletes " +
        'through the generic object surface — "none" understates that',
    );
  } else {
    // The other direction, so losing the capability cannot quietly relax the
    // guard: with no chart ObjectBox and no host insert path there is nothing
    // left to author, and the grade has to say so.
    assert.equal(
      charts.editable,
      "none",
      "the engine no longer publishes a chart as a selectable object, so " +
        `"${charts.editable}" claims editing that nothing provides`,
    );
  }
});
