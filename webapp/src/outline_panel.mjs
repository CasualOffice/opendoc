// The Outline panel's heading tree — a real `role="tree"` since ADR-049.
//
// `documentOutline()` returns `"{level}\t{node}\t{collapsed}\t{text}"` rows in
// document order and this turns them into the panel's rows. Extracted from
// `main.js` (`module_seams`) because the outline is the same enumeration the
// References tab is about and it decided nothing that needed the application;
// what stays in `main.js` is putting the caret somewhere and painting it.
//
// It takes its vocabulary as INPUT (`emptyText`, `labels`), so this module holds
// no user-facing English and the extraction added no unrouted-string debt — the
// discipline `ruler.mjs` was extracted under.
//
// ---- WHAT CHANGED, AND WHY IT IS NOT A FLAT LIST ANY MORE -------------------
//
// This was a flat list of `<button class="outline-item lvl-N">` with
// `aria-current="location"`: no disclosure, no `aria-expanded`, no `role="tree"`.
// A reader could navigate the outline and could not FOLD anything, which was
// half of why ADR-049 had been specified three times without a user ever being
// able to fold a heading.
//
// It is now ARIA's tree pattern: `role="tree"` on the container, `role="treeitem"`
// per row, `aria-level` / `aria-expanded` / `aria-posinset` / `aria-setsize`, a
// roving `tabindex` so the whole tree is ONE tab stop, and a real focusable
// `<button>` for the disclosure.
//
// **ONLYOFFICE is the floor here and explicitly not the ceiling.** Their generic
// `TreeView` does set `aria-expanded` and `aria-level`
// (`web-apps/apps/common/main/lib/component/TreeView.js:243`, `:253`) — but
// `role="tree"` has **zero hits** in `web-apps/apps`, so the attributes sit on
// elements that are not a tree, and their caret is a bare
// `<div class="tree-caret">` (`TreeView.js:181`) with no role and no `tabindex`,
// which cannot satisfy an `aria-expanded` requirement and cannot be reached by
// keyboard at all. And none of it folds anything: `grep -rinI "collaps"
// sdkjs/word` is 0 hits across their whole Word engine.
//
// The keyboard model and the row-visibility rule live in `fold_view.mjs`, where
// `node` can run them without a browser. This module does the DOM and nothing
// else: every decision it appears to make is a function call.

import { OUTLINE_MAX_LEVEL, parseOutlineRows, siblingPositions, treeKeyAction, visibleOutlineRows } from "./fold_view.mjs";

/** Marks the row whose node the caret is in, and unmarks every other.
 *
 *  `aria-current="location"` and not `aria-selected`: the rows navigate, and
 *  "location" is the value for "this is where you are in the document". It is
 *  deliberately kept now that the rows are `treeitem`s — `aria-selected` would
 *  claim a selection model this tree does not have, and the caret's heading is a
 *  location rather than a selected item. */
export function reflectOutlineActive(body, activeNode) {
  for (const item of body.querySelectorAll(".outline-item")) {
    const active = item.dataset.node === activeNode;
    item.classList.toggle("is-active", active);
    if (active) item.setAttribute("aria-current", "location");
    else item.removeAttribute("aria-current");
  }
}

/** Moves the roving tabindex to `index` and focuses that row.
 *
 *  A tree is ONE tab stop: exactly one row is `tabindex="0"` and the rest are
 *  `-1`, so Tab leaves the tree rather than walking every heading in the
 *  document. Without this a 400-heading outline is 400 tab stops, which is the
 *  accessibility failure a flat list of buttons has by construction. */
function rove(tree, index) {
  const rows = [...tree.querySelectorAll(".outline-item")];
  for (const [at, row] of rows.entries()) {
    row.tabIndex = at === index ? 0 : -1;
  }
  rows[index]?.focus();
}

/** The index of the row the roving tabindex is on, or 0. */
function roved(tree) {
  const rows = [...tree.querySelectorAll(".outline-item")];
  const at = rows.findIndex((row) => row.tabIndex === 0);
  return at < 0 ? 0 : at;
}

/**
 * Rebuilds `body` from `rows`.
 *
 * O(rows): one pass, no lookup-by-id inside it. `documentOutline` used to call
 * `paragraph_properties` per node and each of those was a linear scan, which is
 * how this panel came to cost 1.3M x 1.3M block visits on a real file — the
 * reason `main-thread-budget.spec.mjs` still has a row for opening it.
 *
 * `onPick(node)` is what a row does when it is chosen. `onToggle(node,
 * collapsed)` folds or unfolds; when it is **absent** every disclosure renders
 * `aria-disabled` with `toggleWithheldReason` as its title, because a control
 * that looks live and does nothing is worse than one that says why it cannot
 * (`SKILL` §10). `emptyText` and `labels` are passed in so this module holds no
 * user-facing English.
 *
 * @param {HTMLElement} body
 * @param {string[]} rows engine `documentOutline()` output
 * @param {{
 *   emptyText: string,
 *   onPick: (node: string) => void,
 *   activeNode?: string,
 *   onToggle?: (node: string, collapsed: boolean) => void,
 *   toggleWithheldReason?: string,
 *   labels?: {tree?: string, collapse?: string, expand?: string},
 * }} options
 */
export function renderOutline(
  body,
  rows,
  { emptyText, onPick, activeNode = "", onToggle = null, toggleWithheldReason = "", labels = {} },
) {
  body.replaceChildren();
  const parsed = parseOutlineRows(rows);
  if (!parsed.length) {
    const empty = document.createElement("div");
    empty.className = "outline-empty";
    empty.textContent = emptyText;
    body.append(empty);
    return;
  }
  const visible = visibleOutlineRows(parsed);
  const positions = siblingPositions(visible);

  const tree = document.createElement("div");
  tree.className = "outline-tree";
  tree.setAttribute("role", "tree");
  if (labels.tree) tree.setAttribute("aria-label", labels.tree);

  for (const [at, row] of visible.entries()) {
    const indent = Math.min(OUTLINE_MAX_LEVEL, row.level);
    const item = document.createElement("div");
    item.className = "outline-row";
    item.setAttribute("role", "treeitem");
    item.setAttribute("aria-level", String(row.level));
    item.setAttribute("aria-posinset", String(positions[at].posinset));
    item.setAttribute("aria-setsize", String(positions[at].setsize));
    // `aria-expanded` belongs ONLY on a node that can be opened: putting
    // `false` on a leaf tells a screen reader there is something to open and
    // then there is not.
    if (row.hasChildren) item.setAttribute("aria-expanded", String(!row.collapsed));

    // The disclosure: a real focusable `<button>`, which is the whole point.
    // It is rendered for a leaf too — as an inert spacer with no role — so the
    // text of every row at a level starts at the same x and the tree does not
    // ripple as headings gain and lose children.
    const twisty = document.createElement(row.hasChildren ? "button" : "span");
    twisty.className = "outline-twisty";
    if (row.hasChildren) {
      twisty.type = "button";
      const label = row.collapsed ? labels.expand : labels.collapse;
      if (label) {
        twisty.title = label;
        twisty.setAttribute("aria-label", label);
      }
      twisty.tabIndex = -1;
      if (!onToggle) {
        // Disabled WITH A REASON, never a dead control. `aria-disabled` rather
        // than `disabled` so the reason stays reachable: a `disabled` button is
        // skipped by assistive technology and its title never read.
        twisty.setAttribute("aria-disabled", "true");
        if (toggleWithheldReason) twisty.title = toggleWithheldReason;
      } else {
        twisty.addEventListener("click", (event) => {
          // The disclosure is not the row: clicking it folds, and must not also
          // move the caret.
          event.stopPropagation();
          onToggle(row.node, !row.collapsed);
        });
      }
      // The chevron is decoration; the button's accessible name is its label.
      const glyph = document.createElement("span");
      glyph.className = "ms";
      glyph.setAttribute("aria-hidden", "true");
      glyph.textContent = row.collapsed ? "chevron_right" : "expand_more";
      twisty.append(glyph);
    } else {
      twisty.setAttribute("aria-hidden", "true");
    }

    // The row's own activator. A `button` so Space and Enter work the way every
    // other control in this editor works; the tree's key handler sits on the
    // container and routes the rest.
    const label = document.createElement("button");
    label.type = "button";
    label.className = `outline-item lvl-${indent}`;
    label.dataset.node = row.node;
    label.dataset.level = String(row.level);
    label.dataset.collapsed = row.collapsed ? "1" : "0";
    label.dataset.children = row.hasChildren ? "1" : "0";
    label.textContent = row.text;
    label.title = row.text;
    label.tabIndex = at === 0 ? 0 : -1;
    label.addEventListener("click", () => {
      rove(tree, at);
      onPick(row.node);
    });

    item.append(twisty, label);
    tree.append(item);
  }

  tree.addEventListener("keydown", (event) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const index = roved(tree);
    const action = treeKeyAction(event.key, { index, visible });
    if (action.kind === "none") return;
    event.preventDefault();
    switch (action.kind) {
      case "focus":
        rove(tree, action.index);
        break;
      case "activate":
        onPick(action.node);
        break;
      case "collapse":
      case "expand":
        if (onToggle) onToggle(action.node, action.kind === "collapse");
        break;
      case "expandSiblings":
        if (onToggle) for (const node of action.nodes) onToggle(node, false);
        break;
      default:
        break;
    }
  });

  body.append(tree);
  reflectOutlineActive(body, activeNode);
}
