// The deck's off-screen structural mirror: what a screen reader reads instead of
// pixels (docs/156 Tier 3).
//
// # Why this module exists at all
//
// A rendered slide IS pixels. A `<canvas>` carries no text, no headings and no
// list structure, so a screen reader presented with a painted deck gets nothing —
// while the text has been in the model and shaped into glyph runs the whole time.
// That is the "modelled but unreachable" failure `SKILL` §9.4 names, and the deck
// viewer shipped with it as its sharpest gap. `a11y_mirror.mjs` is the document
// editor's answer to exactly the same problem, and this is the same shape: one
// engine projection, one off-screen container, read-only.
//
// # Why it is a separate module from `slides_page.mjs`
//
// Same reason `a11y_mirror.mjs` was lifted out of `main.js` (`109` HF-085): it is a
// self-contained projection of one engine call into one element and shares no state
// with the rest of the page. It takes the parsed outline and two containers, so it
// can be driven under `node --test` with a document fragment rather than only in a
// browser.

import { t } from "./i18n.mjs";

/// The heading level a slide's title becomes.
///
/// `h3`, not `h1`: the page's own `<h2>` ("The current slide") is the region this
/// mirror sits inside, so a title at `h2` would claim to be a sibling of that
/// heading rather than its content, and one at `h1` would claim to outrank the
/// page. A reader navigating by heading lands on the stage, then on the slide's
/// title, which is the structure the deck actually has.
const TITLE_HEADING = "h3";

/// Builds one shape's text into `parent`.
///
/// # Why depth becomes nesting ONLY when there is depth
///
/// A paragraph states `a:pPr@lvl`, and a reader needs that: a sub-point announced
/// at the same depth as the point above it is a different claim about the slide.
/// So a shape whose paragraphs state any level above zero is nested as a list, one
/// `ul` per depth, which is how `a11y_mirror.mjs` nests the document's own lists and
/// the only structure screen readers announce nesting from.
///
/// A shape with no depth is NOT a list, though — it is paragraphs. Wrapping a
/// single sentence in a `ul` makes a reader hear "list, one item" before every
/// title and every caption in the deck, which is noise standing in for information.
/// And `ul` rather than `ol`: PresentationML's bullet is `a:buChar`/`a:buAutoNum`
/// per level and this build does not resolve it, so claiming an ORDERED list would
/// be a claim about the file this page cannot support.
function appendParagraphs(parent, paragraphs) {
  const deepest = paragraphs.reduce((max, paragraph) => Math.max(max, paragraph.level | 0), 0);
  if (deepest === 0) {
    for (const paragraph of paragraphs) {
      const line = document.createElement("p");
      line.textContent = paragraph.text;
      parent.append(line);
    }
    return;
  }
  // A stack of open lists, innermost last — the same shape the document mirror's
  // list projection uses, and for the same reason: a level-1 item has to sit in a
  // list INSIDE the level-0 item above it, or assistive technology is told an
  // indented outline is flat.
  const stack = [];
  for (const paragraph of paragraphs) {
    const depth = Math.max(0, paragraph.level | 0);
    if (stack.length > depth + 1) stack.length = depth + 1;
    while (stack.length < depth + 1) {
      const list = document.createElement("ul");
      const host = stack[stack.length - 1];
      if (host) {
        // A gap in depth — a level 2 under a level 0, which a real deck states
        // whenever an author deletes the middle row — is filled with an empty item
        // so the markup stays well formed. Dropping the paragraph instead would
        // lose the words, which is the one outcome worse than an odd structure.
        const item = host.lastElementChild ?? host.appendChild(document.createElement("li"));
        item.append(list);
      }
      stack.push(list);
    }
    const item = document.createElement("li");
    item.textContent = paragraph.text;
    stack[stack.length - 1].append(item);
  }
  if (stack[0]) parent.append(stack[0]);
}

/// Builds a projected table into `parent` as a real `table`.
///
/// A real `table`/`tr`/`td` with real `colspan`/`rowspan`, which is the only thing
/// that gives a screen reader a grid it can navigate — `a11y_mirror.mjs` projects
/// the document's tables the same way, and for the same reason. A list of
/// paragraphs would turn a 3x3 grid into nine sentences with no row, no column and
/// no merge.
///
/// `td` and never `th`: `a:tblPr@firstRow`/`@firstCol` say a style treats a band as
/// a header, which is a FORMATTING flag and not a statement that the cells are
/// headers — and this build does not apply a table style's parts at all. Claiming
/// a header row would make a reader hear "Revenue, Q3" against cells that may hold
/// nothing of the kind, which is worse than a plain grid.
function appendTable(parent, shape, rows) {
  const table = document.createElement("table");
  const name = typeof shape.name === "string" ? shape.name.trim() : "";
  if (name !== "") table.setAttribute("aria-label", name);
  for (const row of rows) {
    const tr = document.createElement("tr");
    for (const cell of Array.isArray(row) ? row : []) {
      const td = document.createElement("td");
      // Written only when they are not 1, because `colspan="1"` is the default and
      // an attribute restating a default is noise in the accessibility tree.
      const columns = Math.max(1, cell.columnSpan | 0);
      const rowsSpanned = Math.max(1, cell.rowSpan | 0);
      if (columns > 1) td.setAttribute("colspan", String(columns));
      if (rowsSpanned > 1) td.setAttribute("rowspan", String(rowsSpanned));
      appendParagraphs(td, Array.isArray(cell.paragraphs) ? cell.paragraphs : []);
      tr.append(td);
    }
    table.append(tr);
  }
  parent.append(table);
}

/// Builds one shape of the outline into `parent`.
function appendShape(parent, shape) {
  if (shape.kind === "table") {
    const rows = Array.isArray(shape.rows) ? shape.rows : [];
    if (rows.length === 0) return;
    appendTable(parent, shape, rows);
    return;
  }
  const paragraphs = Array.isArray(shape.paragraphs) ? shape.paragraphs : [];
  if (paragraphs.length === 0) return;
  if (shape.role === "title") {
    // A title is a HEADING, which is the single most useful thing this mirror can
    // say: it is how a reader skims a deck, and `aria-current` on a thumbnail tells
    // them which slide is showing but never what it is about.
    const heading = document.createElement(TITLE_HEADING);
    heading.textContent = paragraphs.map((paragraph) => paragraph.text).join(" ");
    parent.append(heading);
    return;
  }
  // Every other shape is a labelled group. The label is the shape's own
  // `p:cNvPr@name` — "Content Placeholder 2", "Slide Number Placeholder 4" — which
  // is what PowerPoint's selection pane shows and the only thing distinguishing two
  // text boxes for a reader with no visual context. A shape the file left unnamed
  // gets no group rather than an invented name: a wrong name is worse than none.
  const name = typeof shape.name === "string" ? shape.name.trim() : "";
  if (name === "") {
    appendParagraphs(parent, paragraphs);
    return;
  }
  const group = document.createElement("div");
  group.setAttribute("role", "group");
  group.setAttribute("aria-label", name);
  appendParagraphs(group, paragraphs);
  parent.append(group);
}

/// Rebuilds the mirror from one slide's text outline.
///
/// `outline` is `casual_pres_wasm`'s `slideText` projection, parsed:
/// `{shapes:[{id, tier, role, name, kind, paragraphs:[{level, text}],
/// rows:[[{columnSpan, rowSpan, paragraphs}]]}]}`, where `kind` is `"text"` or
/// `"table"` and the other of the two lists is empty. Both containers
/// are replaced wholly rather than patched, because a half-replaced mirror reading
/// one slide's title above another's body is the failure that shape prevents — the
/// same reason `createViewer` keeps one state object.
///
/// # Why the inherited text is a separate region
///
/// Because it is a different kind of thing. A deck's footer, its logo's caption and
/// its slide-number furniture come from the layout and the master and repeat on
/// every slide; the slide's own shapes are its content. Interleaved, a reader hears
/// the same footer between every pair of slides with no way to tell it from the
/// slide's words. The region is hidden outright when the deck has none, since an
/// empty labelled region is announced as a region with nothing in it.
export function renderSlideMirror({ own, inherited, outline }) {
  own.replaceChildren();
  inherited.replaceChildren();
  const shapes = Array.isArray(outline?.shapes) ? outline.shapes : [];
  let inheritedCount = 0;
  for (const shape of shapes) {
    if (shape.tier === "slide") {
      appendShape(own, shape);
    } else {
      appendShape(inherited, shape);
      inheritedCount += 1;
    }
  }
  inherited.hidden = inheritedCount === 0;
  // Re-labelled on every rebuild rather than only in the markup: the count is the
  // one part of this region's name that a translation cannot carry, and a reader
  // deciding whether to walk into it wants to know how much is in there.
  if (inheritedCount > 0) {
    inherited.setAttribute("aria-label", t("slides.inheritedText", { count: inheritedCount }));
  }
  return { own: own.childElementCount, inherited: inheritedCount };
}
