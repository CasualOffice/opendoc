// The off-screen structural mirror of the document (docs/67 row 9).
//
// Extracted from `main.js` (`109` HF-085): it is a self-contained projection of
// one engine call into one element, it shares no state with the rest of the
// editor beyond the caret's block, and it is the OTHER windowed view of a large
// document — the page band windows what is painted (`docs/113` §8.6), this
// windows what is read aloud. Keeping the two windows in two modules keeps
// either one legible.

/**
 * Rebuilds the read-only, off-screen structural mirror of the document from the
 * engine's `accessibilityTree()` projection so a screen reader can read the
 * canvas (which paints pixels only, exposing no structure). Headings become
 * `h1`–`h6` (levels 7–9 clamp to `h6`), list items group into `ul`/`ol`,
 * tables become real `table`/`tr`/`td`, form checkboxes become
 * `role="checkbox"` with an `aria-checked` state and a name (`docs/120`), and
 * everything else is a `p`. This is never an editing surface — the model stays
 * the source of truth (docs/67 Open Risks). Rebuilt on the same coalesced
 * content-change frame as the outline.
 */
/** How many top-level blocks the accessibility mirror projects at once.
 *
 *  Large enough that an ordinary document (sample.docx is 433 blocks) is still
 *  mirrored whole, small enough that a million-block document costs the same as
 *  a small one. */
const A11Y_WINDOW_BLOCKS = 600;

/** Where the current mirror window starts, so it stays put when there is no
 *  caret to anchor it to (a freshly opened document). */
let a11yWindowStart = 0;

/** What a form checkbox is announced as when the DOCUMENT gives it no name.
 *
 *  An operable control with no accessible name is a WCAG 4.1.2 failure, so the
 *  projection can never leave one unnamed — but it must not invent a specific
 *  name either, because a wrong name is worse than a generic one (`docs/120`
 *  §5). The engine reports the name the document supports and omits it when
 *  there is none; this is the floor. */
const UNNAMED_CHECKBOX = "Check box";

/**
 * Appends one form checkbox to `parent`.
 *
 * `role="checkbox"` plus `aria-checked` is the ARIA 1.2 checkbox contract, and
 * both are required: the control's document content is a PRIVATE-USE code
 * point (`U+F0A3` / `U+F052` in Wingdings 2 on the owner's form), which a
 * screen reader reads as nothing at all, so without this the eight controls on
 * that form were silence. `aria-checked` is written as the string `"true"` /
 * `"false"` the specification requires, and it is re-derived from the model on
 * every rebuild, so ticking the box on the canvas moves what is announced.
 *
 * It is deliberately NOT focusable and carries no click handler: this mirror
 * is a read-only projection (the container says so in its own label) and the
 * control is operated in the document, by click or Space, since #578. Making
 * the mirror operable is a separate piece of work — `109` HF-178 — because it
 * needs focus to survive the rebuild that the edit itself triggers.
 */
function appendCheckbox(parent, node) {
  const box = document.createElement("span");
  box.setAttribute("role", "checkbox");
  box.setAttribute("aria-checked", node.checked === true ? "true" : "false");
  const name = typeof node.name === "string" ? node.name.trim() : "";
  box.setAttribute("aria-label", name || UNNAMED_CHECKBOX);
  parent.appendChild(box);
}

export function renderAccessibilityMirror(doc, focusNode) {
  const a11yDocument = document.getElementById("a11yDocument");
  if (!a11yDocument) return;
  if (!doc) {
    a11yDocument.replaceChildren();
    return;
  }
  // A WINDOW of the document, not all of it. The mirror used to project every
  // block: for a 16,000-paragraph document that was 16,384 DOM nodes and 87% of
  // the time to open it (6.9 s of 7.9 s), and a 65,000-paragraph one exhausted
  // wasm32 memory and killed the tab — the whole document was serialized to
  // JSON, marshalled, parsed and materialized (docs/104 HF-158). Page canvases
  // were virtualized for exactly this reason; this never was.
  //
  // The window follows the caret, so assistive technology reads the part of the
  // document being edited, and the engine only projects those blocks.
  let nodes = [];
  let total = 0;
  let windowStart = 0;
  try {
    const caretBlock = focusNode ? doc.blockIndexOf(focusNode) : -1;
    const anchor = caretBlock >= 0 ? caretBlock : a11yWindowStart;
    windowStart = Math.max(0, anchor - Math.floor(A11Y_WINDOW_BLOCKS / 2));
    const payload = JSON.parse(doc.accessibilityTreeWindow(windowStart, A11Y_WINDOW_BLOCKS));
    total = Number(payload.total) || 0;
    windowStart = Number(payload.start) || 0;
    nodes = Array.isArray(payload.blocks) ? payload.blocks : [];
    a11yWindowStart = windowStart;
  } catch {
    nodes = [];
  }
  const frag = document.createDocumentFragment();
  // A list is a STACK of open lists, one per depth, because screen readers
  // announce nesting from the lists they are given: a level-1 item has to sit in
  // a list inside the level-0 item above it. Emitting every item as a sibling —
  // which this did before `level` reached the projection — told assistive
  // technology that an indented list was flat, even though the engine tracks the
  // depth correctly and Tab really does demote into it.
  let listStack = []; // [{ el, ordered }], innermost last
  const flushList = () => {
    if (listStack.length > 0) {
      frag.appendChild(listStack[0].el);
      listStack = [];
    }
  };
  for (const node of Array.isArray(nodes) ? nodes : []) {
    if (node.kind === "listItem") {
      const depth = Math.max(0, Number(node.level) || 0);
      const ordered = !!node.ordered;
      // Leaving a level closes every list below it; switching between ordered and
      // unordered at the same depth starts a new list, as it always did.
      if (listStack.length > depth + 1) listStack.length = depth + 1;
      if (listStack.length > 0 && listStack[listStack.length - 1].ordered !== ordered) {
        listStack.length -= 1;
      }
      // Entering a deeper level nests the new list inside the last item of the
      // level above; a gap in depth (level 2 with no level 1) is filled so the
      // markup stays well formed rather than dropping the item.
      while (listStack.length < depth + 1) {
        const el = document.createElement(ordered ? "ol" : "ul");
        const parent = listStack[listStack.length - 1];
        if (parent) {
          const host = parent.el.lastElementChild ?? parent.el.appendChild(document.createElement("li"));
          host.appendChild(el);
        }
        listStack.push({ el, ordered });
      }
      const li = document.createElement("li");
      li.textContent = String(node.text ?? "");
      listStack[listStack.length - 1].el.appendChild(li);
      continue;
    }
    flushList();
    if (node.kind === "heading") {
      const level = Math.min(6, Math.max(1, Number(node.level) || 1));
      const heading = document.createElement(`h${level}`);
      heading.textContent = String(node.text ?? "");
      frag.appendChild(heading);
    } else if (node.kind === "image") {
      // A figure the engine found in the document. Reaching the `else` below
      // would render it as an empty `<p>` — a picture announced as SILENCE,
      // which is worse than announcing it badly.
      //
      // `alt` is the author's own description (the same text the object
      // inspector writes). Without one the graphic is still announced, because a
      // reader needs to know something is there that they are not being told
      // about; a decorative `alt=""` would hide it entirely, and this engine
      // cannot know the author meant that.
      const image = document.createElement("img");
      const alt = typeof node.alt === "string" ? node.alt.trim() : "";
      image.setAttribute("src", "data:,");
      image.setAttribute("alt", alt || "Image without a description");
      frag.appendChild(image);
    } else if (node.kind === "checkbox") {
      // A form control in an ordinary paragraph. It follows that paragraph, the
      // way a figure does, because the paragraph is usually its visible label
      // and the engine has already used that text to name it.
      const wrap = document.createElement("p");
      appendCheckbox(wrap, node);
      frag.appendChild(wrap);
    } else if (node.kind === "table") {
      const table = document.createElement("table");
      // A table's header geometry is what lets a reader say "Revenue, Q3" while
      // moving through cells instead of reading a bare grid of numbers. The
      // engine now reports which rows are headers (`w:tblHeader`, `cnfStyle`, or
      // `tblLook`) and whether the first column heads its row.
      const headerRows = new Set(
        (Array.isArray(node.headerRows) ? node.headerRows : []).map(Number),
      );
      const rowHeaderColumn = node.rowHeaderColumn === true;
      if (typeof node.caption === "string" && node.caption.trim()) {
        const caption = document.createElement("caption");
        caption.textContent = node.caption;
        table.appendChild(caption);
      }
      if (typeof node.description === "string" && node.description.trim()) {
        table.setAttribute("aria-description", node.description);
      }
      const thead = document.createElement("thead");
      const tbody = document.createElement("tbody");
      const rows = Array.isArray(node.rows) ? node.rows : [];
      for (const [index, row] of rows.entries()) {
        const tr = document.createElement("tr");
        const isHeaderRow = headerRows.has(index);
        for (const [column, cell] of (Array.isArray(row) ? row : []).entries()) {
          // A header ROW heads its column; a header COLUMN heads its row.
          const heads = isHeaderRow || (rowHeaderColumn && column === 0);
          const el = document.createElement(heads ? "th" : "td");
          if (heads) el.setAttribute("scope", isHeaderRow ? "col" : "row");
          // A cell is `{ text, checkboxes }`: a form puts its controls in
          // cells, and a string can carry a control's glyph but not its role,
          // its state or its name. Older payloads sent a bare string.
          el.textContent = String((typeof cell === "string" ? cell : cell?.text) ?? "");
          for (const box of Array.isArray(cell?.checkboxes) ? cell.checkboxes : []) {
            appendCheckbox(el, box);
          }
          tr.appendChild(el);
        }
        (isHeaderRow ? thead : tbody).appendChild(tr);
      }
      if (thead.childElementCount > 0) table.appendChild(thead);
      table.appendChild(tbody);
      frag.appendChild(table);
    } else {
      const paragraph = document.createElement("p");
      paragraph.textContent = String(node.text ?? "");
      frag.appendChild(paragraph);
    }
  }
  flushList();
  // Say so when the mirror is a window, so a screen reader is not told a
  // 6,000-block document is 300 blocks long.
  if (total > nodes.length) {
    const note = document.createElement("p");
    note.className = "sr-only";
    note.textContent =
      `Showing blocks ${windowStart + 1} to ${windowStart + nodes.length} of ${total}. ` +
      "Move the cursor to read another part of the document.";
    frag.insertBefore(note, frag.firstChild);
  }
  a11yDocument.replaceChildren(frag);
}
