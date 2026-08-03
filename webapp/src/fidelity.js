// Data-grounded format and DOCX fidelity support matrices (rendered by fidelity.html).
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
    note: "Typing, IME, selection, navigation, undo/redo, alignment, indentation, spacing, keepNext/keepLines, widow/orphan and contextualSpacing all render and edit. Document-grid line/character snapping and before/after autospacing sizing remain approximate.",
    modeled: "full",
    rendered: "partial",
    editable: "full",
    roundtrips: "full",
  },
  {
    family: "Character / run formatting",
    note: "Bold, italic, underline, strike, color, highlight, size, font, super/subscript render and edit. Typed underline styles (double/wavy/dotted/thick) and colored underlines paint as a plain single line, and rare effects (double-strike, emphasis marks, outline/shadow, run border) are preserved for export but not painted.",
    modeled: "full", rendered: "partial", editable: "full", roundtrips: "full",
  },
  {
    family: "Paragraph & named styles",
    note: "Apply a paragraph style and reflect it; no style gallery or update-style-from-selection yet.",
    modeled: "full",
    rendered: "full",
    editable: "partial",
    roundtrips: "full",
  },
  {
    family: "Tables",
    note: "Insert, row/column, merge/split, sort, formula, style, borders, sizing. Exact art/compound borders still partial.",
    modeled: "full",
    rendered: "partial",
    editable: "full",
    roundtrips: "full",
  },
  {
    family: "Lists & numbering",
    note: "Bullet/numbered toggle, level, and style-inherited numbering render and edit. Per-instance level/start overrides, numStyleLink/styleLink indirection, and per-level restart are not yet resolved, and spelled-out formats (cardinalText/ordinalText) fall back to decimal. No multilevel gallery or checklist authoring.",
    modeled: "partial",
    rendered: "partial",
    editable: "partial",
    roundtrips: "full",
  },
  {
    family: "Images & inline drawings",
    note: "PNG/JPEG render as true in-flow boxes and round-trip; other formats (EMF/WMF, SVG, GIF, BMP, TIFF) and undecodable images show a placeholder. No insert-image or image-edit surface.",
    modeled: "full",
    rendered: "partial",
    editable: "none",
    roundtrips: "full",
  },
  {
    family: "Text boxes & shapes",
    note: "Anchored DrawingML shapes retain bounded preset identities/adjustments; rectangles, lines, ellipses, rounded rectangles, triangles, right triangles, and diamonds render distinctly. Other preset/custom paths, non-text inline shapes, gradients, rotation, vertical text, and linked boxes remain partial. Not editable.",
    modeled: "partial",
    rendered: "partial",
    editable: "none",
    roundtrips: "partial",
  },
  {
    family: "Headers & footers",
    note: "Render with per-section widths, first/even/default inheritance, nested blocks, tables, images, and page fields. Not an editing surface yet.",
    modeled: "full",
    rendered: "full",
    editable: "none",
    roundtrips: "full",
  },
  {
    family: "Footnotes & endnotes",
    note: "Modeled and round-tripped; reference/body placement in layout is still partial. Not editable.",
    modeled: "full",
    rendered: "partial",
    editable: "none",
    roundtrips: "full",
  },
  {
    family: "Sections, columns & page setup",
    note: "Multi-section geometry and columns render; page size/margins/orientation/columns are editable. No section insert/split; column balancing partial.",
    modeled: "full",
    rendered: "partial",
    editable: "partial",
    roundtrips: "full",
  },
  {
    family: "Fields",
    note: "PAGE / NUMPAGES recompute; other fields use cached results and do not soft-wrap. Not editable as fields.",
    modeled: "full",
    rendered: "partial",
    editable: "none",
    roundtrips: "full",
  },
  {
    family: "Math (OMML)",
    note: "Raw OMML is preserved for export. Rows/text, fractions, sub/superscripts, radicals, and delimiters have a bounded typed model and deterministic inline rendering; other constructs use an explicit text placeholder. Not editable.",
    modeled: "partial",
    rendered: "partial",
    editable: "none",
    roundtrips: "full",
  },
  {
    family: "Charts",
    note: "Modeled as first-class references and preserved byte-for-byte on export. Not rendered as a live chart — an embedded preview image shows if the file provides one, otherwise a text placeholder.",
    modeled: "full",
    rendered: "preserved",
    editable: "none",
    roundtrips: "full",
  },
  {
    family: "SmartArt",
    note: "Modeled as references and preserved for export. Not rendered as a diagram — a preview image shows if present, otherwise a text placeholder.",
    modeled: "full",
    rendered: "preserved",
    editable: "none",
    roundtrips: "full",
  },
  {
    family: "VML pictures & shapes",
    note: "Legacy VML pictures render via the shared drawing path; CSS positioning, exact paths, and gradients are partial. Not editable.",
    modeled: "full",
    rendered: "partial",
    editable: "none",
    roundtrips: "full",
  },
  {
    family: "Comments",
    note: "Editor sidebar with anchored highlights; add, reply, resolve/reopen, edit, delete; valid thread ids on export. Not Word/Docs parity — single-paragraph ranges only, no bulk accept/filter surface; not part of printed page output.",
    modeled: "full",
    rendered: "full",
    editable: "partial",
    roundtrips: "full",
  },
  {
    family: "Tracked changes",
    note: "Inline markup with per-author color; suggesting mode, accept/reject single/group/all, round-trip with numeric ids. Not Word/Docs parity — no Final/Original/simple-markup view toggle, and only inline (not structural: paragraph/table/list) changes can be authored.",
    modeled: "full",
    rendered: "partial",
    editable: "partial",
    roundtrips: "full",
  },
  {
    family: "Bookmarks & hyperlinks",
    note: "Links render, activate, and drive TOC navigation; insert/edit/remove a link. Bookmarks navigate only (no create/rename/delete).",
    modeled: "full",
    rendered: "full",
    editable: "partial",
    roundtrips: "full",
  },
  {
    family: "Content controls (w:sdt)",
    note: "SDT wrappers model and round-trip; their text content flows and edits as ordinary paragraphs. Control chrome and checked/checkbox and date-picker states are not rendered.",
    modeled: "full",
    rendered: "partial",
    editable: "partial",
    roundtrips: "full",
  },
];

const FORMAT_SUPPORT = [
  {
    format: "DOCX",
    note: "Bounded OOXML admission, semantic import/export, and the existing browser editor path are implemented and tested.",
    validation: "full",
    import: "full",
    export: "full",
    host: "full",
  },
  {
    format: "Normalized JSON",
    note: "Strict bounded schema-v1 validation and deterministic compact export are implemented in the internal registry; stable generic SDK/WASM entry points are pending.",
    validation: "full",
    import: "full",
    export: "full",
    host: "none",
  },
  {
    format: "Plain text",
    note: "Bounded UTF-8 import, canonical LF export, exact retained unchanged bytes, and loss reporting are implemented internally; stable generic SDK/WASM entry points are pending.",
    validation: "full",
    import: "full",
    export: "full",
    host: "none",
  },
  {
    format: "ODT",
    note: "ODF 1.2–1.4 package admission and internal registry dispatch are complete. The bounded importer and deterministic ODF 1.4 writer cover core paragraphs, headings, text, spaces, tabs, and line breaks. Styles, complex structures, edit-tolerant preservation, stable host APIs, and browser support remain in progress.",
    validation: "full",
    import: "partial",
    export: "partial",
    host: "none",
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

function renderFormatSupport() {
  const body = document.getElementById("format-support-body");
  if (!body) return;
  const frag = document.createDocumentFragment();
  for (const row of FORMAT_SUPPORT) {
    const tr = document.createElement("tr");
    const th = document.createElement("th");
    th.setAttribute("scope", "row");
    th.textContent = row.format;
    const small = document.createElement("small");
    small.textContent = row.note;
    th.appendChild(small);
    tr.appendChild(th);

    for (const stage of ["validation", "import", "export", "host"]) {
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

if (typeof document !== "undefined") {
  renderFormatSupport();
  renderFidelity();
}

// Expose the data (not the DOM render) to a Node drift check.
if (typeof module !== "undefined" && module.exports) {
  module.exports = { FIDELITY, FIDELITY_STAGE, FORMAT_SUPPORT };
}
