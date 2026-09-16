// Data-grounded DOCX fidelity support matrix (rendered by fidelity.html).
//
// This array is the single source of truth for the public matrix, kept in sync
// with docs/18-SUPPORT-MATRIX.md, docs/14-EXECUTION-TRACKER.md (P1A-* model,
// P1F-* render, P1G-* editor rows), and the docs/55/60 fidelity audits. Each
// stage value must be honest: a construct is only "full" where it is truly
// implemented and exercised, only "editable" where the current editor can
// actually change it. When support advances, update the cell here.
//
// Stage vocabulary (see fidelity.html legend):
//   full        ● implemented and exercised
//   partial     ◐ common cases work; documented gaps remain
//   placeholder ▢ a visible stand-in, not the real content
//   preserved   ⊟ kept losslessly for export but not painted (retention floor)
//   none        ○ not implemented
const FIDELITY = [
  {
    family: "Paragraphs & text",
    note: "Typing, IME, selection, navigation, undo/redo, alignment, indentation, spacing (incl. line-rule atLeast/exact), keepNext/keepLines, widow/orphan and contextualSpacing all render and edit. The only remainders are niche/bounded: document-grid (CJK) line/character snapping is not applied, and before/after autospacing sizing is a font-size approximation.",
    modeled: "full", rendered: "full", editable: "full", roundtrips: "full",
  },
  {
    family: "Character / run formatting",
    note: "Bold, italic, underline, strike, color, highlight, size, font, super/subscript, small-caps, and run-level shading (w:shd) render and edit. Colored underlines and typed underline styles (double/thick/dotted/dashed/dot-dash/wavy/words-only) render and can be authored. Rare effects (emphasis marks, outline/shadow/emboss, run border) are preserved for export but not painted.",
    modeled: "full", rendered: "partial", editable: "full", roundtrips: "full",
  },
  {
    family: "Paragraph & named styles",
    note: "Apply a paragraph style from the ribbon gallery and reflect the caret\u2019s style. Updating a style from the selection, and creating a new named style, are not there.",
    modeled: "full", rendered: "full", editable: "partial", roundtrips: "full",
  },
  {
    family: "Tables",
    note: "Insert, row/column, merge/split, sort, formula, style, borders, sizing. Not yet reaching layout: floating tables (`w:tblpPr`, which render inline), cell `noWrap`, `fitText`, `hideMark` and cell `textDirection`, style-provided row properties/margins/spacing, and exact art/compound borders (which fall back to solid).",
    modeled: "full", rendered: "partial", editable: "full", roundtrips: "full",
  },
  {
    family: "Lists & numbering",
    note: "Numbering is fully modeled: multiLevelType, per-level restart, level→pStyle links, numStyleLink/styleLink indirection, full per-instance level/start overrides, and the numFmt vocabulary (incl. spelled-out cardinalText/ordinalText) are typed and round-trip. Rendering now resolves per-instance level/start overrides, numStyleLink/styleLink indirection, and lvlRestart, and paints spelled-out formats — so multi-level and style-based lists label correctly. The remainder is picture bullets: `w:lvlPicBulletId` and the `w:numPicBullet` definitions it points at are not modeled at all, so they are dropped on import without a finding and cannot be written back — a model and round-trip gap, not just a render one. Multilevel-gallery/checklist authoring is a separate editing gap.",
    modeled: "partial", rendered: "full", editable: "partial", roundtrips: "full",
  },
  {
    family: "Images & inline drawings",
    note: "PNG, JPEG, GIF, BMP, TIFF, and WEBP decode and render as true in-flow boxes (with crop/scale) and round-trip. SVG vector paths/shapes rasterize on the native build; SVG text — and all SVG on the browser (WASM) build — falls back to a placeholder, as do EMF/WMF metafiles and undecodable images. A picture can be inserted (from a file or the clipboard), selected, resized, moved, wrapped, reordered, cropped by dragging its handles, described (alt text), and deleted. Replacing an existing picture\u2019s bytes in place, authoring rotation/flip, and picture borders and effects are not there.",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Text boxes & shapes",
    note: "Shape geometry (bounded presets + adjustments), fill (solid and multi-stop gradient), outline (color/width/dash/arrowheads), rotation/flip, and the tight/through wrap contour are typed and round-trip. Custom geometry (custGeom paths) is retained verbatim, not typed — so semantic-mode round-trip stays partial for those. Rendering paints preset shapes with solid/gradient fills, outlines (dash + head/tail arrowheads), rotation/flip, and picture-frame borders; text is contained/clipped to the box. Custom (custGeom) paths, vertical text, linked boxes, and rotated text-box content remain unpainted. Editing: a text box or preset shape can be inserted, selected, moved, resized, wrapped, reordered and deleted; box CONTENT edits with the full text pipeline (inline, floating, and inside a shape group); a shape\u2019s fill and outline colour/weight are editable. Rotation and flip authoring, custom geometry, and text-box body properties (internal margins, vertical anchor, autofit) are not.",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "partial",
  },
  {
    family: "Headers & footers",
    note: "Render with per-section widths, first/even/default inheritance, nested blocks, tables, images, and page fields. Fully editable: double-click the band (or the hover marker) to enter, type and format with the same pipeline as the body — including comments and tracked changes — create a header or footer where none exists, and toggle the different-first-page and different-odd-even variants. Link-to-previous is reference absence, per Word\u2019s model.",
    modeled: "full", rendered: "full", editable: "full", roundtrips: "full",
  },
  {
    family: "Footnotes & endnotes",
    note: "Reference markers, page-bottom note bands, the separator rule (short for a fresh note, full-width for a continuation), and the in-body auto-number glyph all render — with space reservation, per-column bands, cross-page continuation, and end-of-document endnote placement. A footnote or endnote can be inserted from the Insert tab, the menu or the palette, and its body edits like any other surface. Converting a footnote to an endnote, number-format and restart options, and separator customization are not there.",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Sections, columns & page setup",
    note: "Multi-section geometry and columns render; page size/margins/orientation/columns are editable. No section insert/split; column balancing partial.",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Fields",
    note: "PAGE / NUMPAGES recompute; other fields use cached results and do not soft-wrap. Not editable as fields.",
    modeled: "full", rendered: "partial", editable: "none", roundtrips: "full",
  },
  {
    family: "Math (OMML)",
    note: "The full OMML math element set is typed — rows/text, fractions, sub/superscripts and pre-scripts, radicals, delimiters, functions, n-ary operators, matrices, equation arrays, accents, bars, limits, group-characters, and box/border-box wrappers — and the raw OMML is preserved verbatim (rare constructs like phantom spacing stay raw-retained, still lossless). Rendering paints the common arms inline; box/border-box border rules and full Word-parity typesetting remain partial. Not editable.",
    modeled: "full", rendered: "partial", editable: "none", roundtrips: "full",
  },
  {
    family: "Charts",
    note: "Modeled as first-class references and preserved byte-for-byte on export. Not rendered as a live chart — an embedded preview image shows if the file provides one, otherwise a text placeholder.",
    modeled: "full", rendered: "preserved", editable: "none", roundtrips: "full",
  },
  {
    family: "SmartArt",
    note: "Modeled as references and preserved for export. Not rendered as a diagram — a preview image shows if present, otherwise a text placeholder.",
    modeled: "full", rendered: "preserved", editable: "none", roundtrips: "full",
  },
  {
    family: "VML pictures & shapes",
    note: "Legacy VML pictures and shapes render via the shared drawing path, including linear/radial gradient fills; CSS positioning and exact custom paths remain partial. Not editable.",
    modeled: "full", rendered: "partial", editable: "none", roundtrips: "full",
  },
  {
    family: "Comments",
    note: "Editor sidebar with anchored highlights; add, reply, resolve/reopen, edit, delete; Open/Resolved/All filtering; valid thread ids on export. Comments can be added in any surface, including headers, footers, notes and text boxes. Not Word/Docs parity — single-paragraph ranges only. The engine paints a comment highlight only under the read-only markup review view; in the default editing view the highlight and sidebar are host/DOM chrome, so comments contribute nothing to the display list or to printed page output.",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Tracked changes",
    note: "Inline markup with per-author color; suggesting mode, accept/reject single/group/all, a review sidebar with Open/Resolved/All filtering and next/previous navigation, and round-trip with numeric ids. Changes can be authored in any surface, including headers, footers, notes and text boxes. Not Word/Docs parity — the view control is a binary show/hide of markup rather than Word\u2019s Simple Markup / Original views, and only inline (not structural: paragraph/table/list) changes can be authored.",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Bookmarks & hyperlinks",
    note: "Links render, activate, and drive TOC navigation; insert/edit/remove a link. Bookmarks have a manager surface — create, rename, delete, and go to.",
    modeled: "full", rendered: "full", editable: "partial", roundtrips: "full",
  },
  {
    family: "Content controls (w:sdt)",
    note: "SDT wrappers model and round-trip; content flows and edits as ordinary paragraphs, and checkbox content-controls paint their checked/unchecked state glyph. Control bounding chrome, placeholder/prompt text, and dropdown/combo/date-picker chrome are not rendered.",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Fonts, fallback & color glyphs",
    note: "24 Latin faces ship bundled; hosts can register more through the font registry (the desktop story is OS fonts, the browser story is network-fetched faces). Whole-face substitution is name-based and deliberately metric-compatible with LibreOffice's choices (Arial\u2192Liberation Sans, Calibri\u2192Carlito, Cambria\u2192Caladea), and per-glyph coverage fallback runs through the face index. Color glyphs render: sbix/CBDT bitmap strikes and COLR v0/v1 paint graphs are tried before monochrome outlines. Fonts embedded in the document (`w:embedRegular` and its bold/italic slots, the obfuscated `.odttf` parts) are de-obfuscated and registered under the document\u2019s own family name, so they outrank the metric substitute; a corrupt or missing embedded face falls back to substitution and is reported. One real gap: the modeled PANOSE/altName/signature hints are not consulted \u2014 substitution is name-string only. No CJK, Arabic, or Indic face is bundled, so those scripts need a host-registered font on the browser build.",
    modeled: "full", rendered: "full", editable: "none", roundtrips: "full",
  },
  {
    family: "Hyphenation",
    note: "Not implemented. There is no hyphenator, no dictionary, and no consumer for `w:autoHyphenation`, `w:hyphenationZone`, or `w:hyphenationRules`; only `w:suppressAutoHyphens` is cascaded, and it has nothing to suppress. A document authored with automatic hyphenation on will break its lines differently from Word and therefore paginate differently. The settings round-trip.",
    modeled: "partial", rendered: "none", editable: "none", roundtrips: "partial",
  },
  {
    family: "Line numbering (w:lnNumType)",
    note: "Painted in the margin by a post-pagination pass (`line_number.rs`), honouring start, count-by, distance, restart per page / section / continuous, and `w:suppressLineNumbers` from a paragraph or its style. Numbers are page furniture, not text: a click in the margin lands in the body and copying a paragraph never copies its number. One deliberate gap: lines inside table cells are not numbered yet (Word numbers them), because that needs a row-wise ordering rule rather than a guess. Not authorable from the editor. Common in legal pleadings and contract redlines.",
    modeled: "full", rendered: "partial", editable: "none", roundtrips: "full",
  },
  {
    family: "Watermarks & WordArt",
    note: "Not modeled as a watermark. A Word watermark is a header VML or DrawingML shape carrying warped text (`v:textpath` / `a:prstTxWarp`); neither text-path form is typed, so the shape box can paint but its text does not. Preserved for export where it lands in the retained/opaque path.",
    modeled: "none", rendered: "none", editable: "none", roundtrips: "partial",
  },
  {
    family: "Bidi, RTL & CJK grid",
    note: "Paragraph base direction (`w:bidi`) drives the alignment edge, `w:bidiVisual` mirrors table grid/margin/border geometry, and the Unicode bidi algorithm runs per line through the shaper. Two known limits: the shaper exposes no way to force a paragraph's base level, so an RTL paragraph whose text carries no strong RTL character reorders LTR; and the resolved level is collapsed to a single RTL flag, discarding embedding depth. `w:docGrid` line pitch reaches layout with exact/paragraph/table precedence and is gated by `w:snapToGrid`; character-grid snapping (`w:charSpace`, linesAndChars) is not applied. `w:jc=\"distribute\"` currently collapses to ordinary justification, and there is no kashida or CJK inter-character distribution.",
    modeled: "partial", rendered: "partial", editable: "partial", roundtrips: "partial",
  },
  {
    family: "Vertical & rotated text",
    note: "Not implemented. `w:textDirection` (tbRl/btLr on sections and table cells) and DrawingML `bodyPr` vertical writing are never consumed \u2014 there is only one writing-mode axis through flow, composition, hit-testing, and the caret. Rotated text-box content is likewise unpainted (the box itself rotates). Values round-trip.",
    modeled: "partial", rendered: "none", editable: "none", roundtrips: "full",
  },
  {
    family: "Drop caps",
    note: "Implemented: a dropped cap becomes a real paragraph-float exclusion that following lines flow around, and margin-position drop caps offset instead. Gated to the Word-shaped case \u2014 a single-character drop-cap paragraph followed by a body paragraph. Not authorable from the editor.",
    modeled: "full", rendered: "full", editable: "none", roundtrips: "full",
  },
];

const FIDELITY_STAGE = {
  full: { glyph: "●", label: "Full" }, // ●
  partial: { glyph: "◐", label: "Partial" }, // ◐
  placeholder: { glyph: "▢", label: "Placeholder" }, // ▢
  preserved: { glyph: "⊟", label: "Preserved" }, // ⊟
  none: { glyph: "○", label: "Not yet" }, // ○
};

/** Renders the FIDELITY data into the #fidelity-body table body. No-op outside
 *  a browser (so the data can be imported by a Node drift check). */
function renderFidelity() {
  const body = document.getElementById("fidelity-body");
  if (!body) return;
  const frag = document.createDocumentFragment();
  for (const row of FIDELITY) {
    const tr = document.createElement("tr");

    const th = document.createElement("th");
    th.setAttribute("scope", "row");
    th.textContent = row.family;
    if (row.note) {
      const small = document.createElement("small");
      small.textContent = row.note;
      th.appendChild(small);
    }
    tr.appendChild(th);

    for (const stage of ["modeled", "rendered", "editable", "roundtrips"]) {
      const td = document.createElement("td");
      td.className = "cell";
      const value = row[stage];
      const info = FIDELITY_STAGE[value] || FIDELITY_STAGE.none;
      const mark = document.createElement("span");
      mark.className = `fmark ${value}`;
      const glyph = document.createElement("span");
      glyph.className = "glyph";
      glyph.setAttribute("aria-hidden", "true");
      glyph.textContent = info.glyph;
      mark.append(glyph, document.createTextNode(info.label));
      td.appendChild(mark);
      tr.appendChild(td);
    }
    frag.appendChild(tr);
  }
  body.replaceChildren(frag);
}

// Per-format pipeline support (validation / import / export / browser host).
// Kept deliberately conservative so the fidelity story never overstates a format;
// a drift check asserts ODT stays "partial" until a family is genuinely complete.
const FORMAT_SUPPORT = [
  {
    format: "DOCX",
    note: "Bounded OOXML admission, semantic import/export, and the browser editor path are implemented and tested.",
    validation: "full",
    import: "full",
    export: "full",
    host: "full",
  },
  {
    format: "Normalized JSON",
    note: "Strict bounded schema-v1 validation and deterministic compact export are available through the generic WASM API and the browser Open/Save controls.",
    validation: "full",
    import: "full",
    export: "full",
    host: "full",
  },
  {
    format: "Plain text",
    note: "Bounded UTF-8 import, canonical LF export, exact retained unchanged bytes, and loss reporting are available through the generic WASM API and the browser Open/Save controls.",
    validation: "full",
    import: "full",
    export: "full",
    host: "full",
  },
  {
    format: "ODT",
    note: "ODF 1.2–1.4 admission plus generic WASM/browser Open and Save are implemented. Core text, safe links, bookmarks, bounded direct/named styles, nested bullet/number lists, recursive tables, typed footnotes/endnotes, and authored citation labels map through the normalized model and a matching writer to a byte-exact fixed point, and unsafe merge geometry is visibly projected and reported rather than dropped. Import/export and therefore host support remain partial: advanced list continuation/item overrides and label layout, broader style/table properties, media beyond references, and full conformance remain in progress.",
    validation: "full",
    import: "partial",
    export: "partial",
    host: "partial",
  },
];

if (typeof document !== "undefined") {
  renderFidelity();
}

// Expose the data (not the DOM render) to a Node drift check.
if (typeof module !== "undefined" && module.exports) {
  module.exports = { FIDELITY, FIDELITY_STAGE, FORMAT_SUPPORT };
}
