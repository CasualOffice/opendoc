// The Outline panel's heading tree.
//
// `documentOutline()` returns `"level\tnode\ttext"` rows in document order and
// this turns them into the panel's rows. Extracted from `main.js` to pay for the
// captions and cross-references work under the line ratchet (`module_seams`), and
// it is the right thing to take out for it: the outline is the same enumeration
// the References tab is about, and it decided nothing that needed the
// application — what stays in `main.js` is putting the caret somewhere and
// painting it, which is about the caret and not about headings.
//
// It takes its vocabulary as INPUT (`emptyText`), so extracting it moved no
// English out of the catalogue's reach and added no unrouted-string debt of its
// own — the same discipline `ruler.mjs` was extracted under.

/** Word's outline goes to nine levels; the panel styles six. Clamping here
 *  rather than at the call site is what stops a `lvl-7` class nothing styles. */
const MAX_LEVEL = 6;

/** Marks the row whose node the caret is in, and unmarks every other.
 *
 *  `aria-current="location"` and not `aria-selected`: the rows are buttons that
 *  navigate, not options in a listbox, and "location" is the value for "this is
 *  where you are in the document". */
export function reflectOutlineActive(body, activeNode) {
  for (const item of body.querySelectorAll(".outline-item")) {
    const active = item.dataset.node === activeNode;
    item.classList.toggle("is-active", active);
    if (active) item.setAttribute("aria-current", "location");
    else item.removeAttribute("aria-current");
  }
}

/** Rebuilds `body` from `rows`.
 *
 *  O(rows): one pass, no lookup-by-id inside it. `documentOutline` used to call
 *  `paragraph_properties` per node and each of those was a linear scan, which is
 *  how this panel came to cost 1.3M × 1.3M block visits on a real file — the
 *  reason `main-thread-budget.spec.mjs` still has a row for opening it.
 *
 *  `onPick(node)` is what a row does when it is chosen. `emptyText` is shown
 *  when the document has no headings; it is passed in rather than written here
 *  so this module holds no user-facing English. */
export function renderOutline(body, rows, { emptyText, onPick, activeNode = "" }) {
  body.replaceChildren();
  if (!rows.length) {
    const empty = document.createElement("div");
    empty.className = "outline-empty";
    empty.textContent = emptyText;
    body.append(empty);
    return;
  }
  for (const row of rows) {
    const tab = row.indexOf("\t");
    const tab2 = row.indexOf("\t", tab + 1);
    const level = Math.min(MAX_LEVEL, Math.max(1, Number(row.slice(0, tab)) || 1));
    const node = row.slice(tab + 1, tab2);
    const text = row.slice(tab2 + 1);
    const item = document.createElement("button");
    item.type = "button";
    item.className = `outline-item lvl-${level}`;
    item.dataset.node = node;
    item.textContent = text;
    item.title = text;
    item.addEventListener("click", () => onPick(node));
    body.append(item);
  }
  reflectOutlineActive(body, activeNode);
}
