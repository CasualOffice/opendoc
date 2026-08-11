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
    note: "Bold, italic, underline, strike, color, highlight, size, font, super/subscript, small-caps, and run-level shading (w:shd) render and edit. Colored underlines and typed underline styles (double/thick/dotted/dashed/dot-dash/wavy) render and can be authored; words-only underline is preserved but currently paints through spaces. Rare effects (emphasis marks, outline/shadow/emboss, run border) are preserved for export but not painted.",
    modeled: "full", rendered: "partial", editable: "full", roundtrips: "full",
  },
  {
    family: "Paragraph & named styles",
    note: "Apply a paragraph style from the ribbon gallery and reflect the caret\u2019s style. Updating a style from the selection, and creating a new named style, are not there.",
    modeled: "full", rendered: "full", editable: "partial", roundtrips: "full",
  },
  {
    family: "Tables",
    note: "Insert, row/column, merge/split, sort, formula, style, borders, sizing. Exact art/compound borders still partial.",
    modeled: "full", rendered: "partial", editable: "full", roundtrips: "full",
  },
  {
    family: "Lists & numbering",
    note: "Numbering is fully modeled: multiLevelType, per-level restart, level→pStyle links, numStyleLink/styleLink indirection, full per-instance level/start overrides, and the numFmt vocabulary (incl. spelled-out cardinalText/ordinalText) are typed and round-trip. Rendering now resolves per-instance level/start overrides, numStyleLink/styleLink indirection, and lvlRestart, and paints spelled-out formats — so multi-level and style-based lists label correctly. The only render remainder is niche bullet-picture glyphs (lvlPicBulletId); multilevel-gallery/checklist authoring is an editing (not rendering) gap.",
    modeled: "full", rendered: "full", editable: "partial", roundtrips: "full",
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
    note: "Editor sidebar with anchored highlights; add, reply, resolve/reopen, edit, delete; Open/Resolved/All filtering; valid thread ids on export. Comments can be added in any surface, including headers, footers, notes and text boxes. Not Word/Docs parity — single-paragraph ranges only, and comments are not part of printed page output.",
    modeled: "full", rendered: "full", editable: "partial", roundtrips: "full",
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
