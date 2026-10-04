// Folding, decided: the half of ADR-049's chrome that runs in `node`.
//
// The split is the one `reflow_view.mjs` / `reflow_chrome.mjs` already
// established here, and `spelling.mjs` / `spell_check.mjs` before that: the
// DECISIONS in a module a unit test can run with no browser — how an engine
// outline row parses, which rows a tree shows, what each key does, what the
// level picker offers, what the editor says about the page count — and the
// wiring in `fold_chrome.mjs`. Nothing in here touches the DOM or the engine.
//
// WHY THAT MATTERS HERE RATHER THAN BEING A HABIT: the keyboard model is the
// part of a `role="tree"` that is hardest to see and easiest to get wrong, and
// this repository has no DOM in its unit tests. A reducer over plain data is
// testable; the same logic spread through `keydown` handlers is only testable
// through a browser, which is why ONLYOFFICE's caret has no role and no
// tabindex at all (`TreeView.js:181` — a bare `<div class="tree-caret">`) and
// why `role="tree"` has ZERO hits across `web-apps/apps`. They are the floor.
//
// THE COMPETITIVE POSITION, sourced (`docs/157`): ONLYOFFICE do not fold.
// `grep -rinI "collaps" sdkjs/word` is 0 hits across their 232-file Word engine,
// their exported outline API (`word/Editor/DocumentOutline.js:501-512`) has no
// Collapse or Expand, and they discard `w15:collapsed` structurally. So Word is
// the standard for everything here, and the only thing we take from them is the
// Left/Right direction of their generic tree widget.

/** The deepest outline level Word has, and the deepest the engine reports. */
export const FOLD_MAX_LEVEL = 9;

/** The deepest level the panel has an indent rule for. A row deeper than this
 *  is still a real `treeitem` with its true `aria-level`; only the indent
 *  class clamps, so nothing renders as a `lvl-7` class no stylesheet has. */
export const OUTLINE_MAX_LEVEL = 6;

/**
 * One engine outline row, parsed.
 *
 * `documentOutline()` returns `"{level}\t{node}\t{collapsed}\t{text}"`. The
 * collapsed column arrived with folding, and it is a column rather than a second
 * engine call because two calls can disagree with each other — the panel needs
 * the level, the node and the fold state of the SAME row to build a `treeitem`.
 *
 * Returns `null` for a row that is not well formed, so a malformed row is
 * skipped rather than rendered as a tree item with no node to fold.
 *
 * @param {string} row
 * @returns {{level: number, node: string, collapsed: boolean, text: string}|null}
 */
export function parseOutlineRow(row) {
  const parts = String(row ?? "").split("\t");
  if (parts.length < 4) return null;
  const level = Number(parts[0]);
  if (!Number.isInteger(level) || level < 1) return null;
  const node = parts[1];
  if (node === "") return null;
  return {
    level: Math.min(FOLD_MAX_LEVEL, level),
    node,
    collapsed: parts[2] === "1",
    // The text is LAST and carries no tab (the engine replaces any with a
    // space), so re-joining is belt and braces rather than a parse.
    text: parts.slice(3).join("\t"),
  };
}

/**
 * Every well-formed row of an engine outline, in document order.
 *
 * O(rows), one pass, no lookup-by-id inside it — the shape that made this panel
 * cost 1.3M x 1.3M block visits on a real file (`docs/116`).
 *
 * @param {string[]} rows
 */
export function parseOutlineRows(rows) {
  const out = [];
  for (const row of rows ?? []) {
    const parsed = parseOutlineRow(row);
    if (parsed) out.push(parsed);
  }
  return out;
}

/**
 * The rows a tree SHOWS: every row except those inside a collapsed subtree.
 *
 * The same one-integer suppression state machine the layout filter runs
 * (`casual-doc-layout/src/fold.rs`), for the same reason — a collapsed heading's
 * own row stays visible and everything under it up to the next heading at the
 * same level or higher goes. Running the same rule in both places is what makes
 * the panel agree with the page; running a different one is how everyone else
 * ends up with two fold states.
 *
 * Each returned entry carries its index in the FULL row list, so a caller can
 * map a tree row back to the engine row it came from without a second search.
 *
 * O(rows).
 *
 * @param {{level: number, node: string, collapsed: boolean, text: string}[]} rows
 */
export function visibleOutlineRows(rows) {
  const out = [];
  /** The level of the innermost collapsed row we are inside, or 0 for none. */
  let suppress = 0;
  for (const [index, row] of (rows ?? []).entries()) {
    if (suppress > 0) {
      if (row.level > suppress) continue;
      suppress = 0;
    }
    out.push({ ...row, index, hasChildren: hasChildren(rows, index) });
    if (row.collapsed) suppress = row.level;
  }
  return out;
}

/**
 * Whether the row at `index` has any deeper row under it before the next row at
 * its own level or higher — i.e. whether it is a parent in the heading tree.
 *
 * A row with no children gets no `aria-expanded` at all, because ARIA's tree
 * pattern reserves that attribute for nodes that can be opened: putting
 * `aria-expanded="false"` on a leaf tells a screen reader there is something to
 * open and then there is not.
 *
 * O(1) amortised in practice (it looks at the next row only) — the first
 * following row either is deeper, in which case there are children, or is not,
 * in which case there are none.
 *
 * @param {{level: number}[]} rows
 * @param {number} index
 */
export function hasChildren(rows, index) {
  const row = rows?.[index];
  const next = rows?.[index + 1];
  return Boolean(row && next && next.level > row.level);
}

/**
 * The position of each visible row among its own siblings, as ARIA's tree
 * pattern wants it: `aria-posinset` is 1-based within the set of siblings at the
 * same level under the same parent, and `aria-setsize` is that set's size.
 *
 * Derived rather than counted ad hoc at render time, because the two numbers
 * have to agree with each other and with what is on screen: a screen reader
 * announces "3 of 7", and getting the 7 from the whole list rather than from the
 * sibling set is the bug that makes the announcement a lie.
 *
 * @param {{level: number}[]} visible rows from [`visibleOutlineRows`]
 * @returns {{posinset: number, setsize: number}[]} parallel to `visible`
 */
export function siblingPositions(visible) {
  const rows = visible ?? [];
  const out = rows.map(() => ({ posinset: 1, setsize: 1 }));
  // One pass forward to number each row within its sibling run, then one pass
  // backward to stamp each run's total onto its members. A sibling run ends at
  // the first row whose level is SHALLOWER (a different parent) — a deeper row
  // between two siblings is a child and does not interrupt the run.
  const counters = new Map();
  for (const [index, row] of rows.entries()) {
    // Entering a level resets every deeper level's counter, so the next time a
    // deeper level appears it restarts at 1 under its new parent.
    for (const level of [...counters.keys()]) {
      if (level > row.level) counters.delete(level);
    }
    const next = (counters.get(row.level) ?? 0) + 1;
    counters.set(row.level, next);
    out[index].posinset = next;
  }
  const totals = new Map();
  for (let index = rows.length - 1; index >= 0; index -= 1) {
    const row = rows[index];
    for (const level of [...totals.keys()]) {
      if (level > row.level) totals.delete(level);
    }
    const seen = totals.get(row.level);
    const total = seen === undefined ? out[index].posinset : Math.max(seen, out[index].posinset);
    totals.set(row.level, total);
    out[index].setsize = total;
  }
  return out;
}

/**
 * The index of the parent of visible row `index`, or `-1` at the top level.
 *
 * The nearest preceding visible row with a shallower level — which is what Left
 * moves to when the focused row is already collapsed.
 *
 * @param {{level: number}[]} visible
 * @param {number} index
 */
export function parentIndex(visible, index) {
  const rows = visible ?? [];
  const row = rows[index];
  if (!row) return -1;
  for (let at = index - 1; at >= 0; at -= 1) {
    if (rows[at].level < row.level) return at;
  }
  return -1;
}

/**
 * What a key does on a tree row — the whole keyboard model, as a reducer.
 *
 * ARIA's tree pattern, plus the one direction ONLYOFFICE's generic widget also
 * has (`TreeView.js:345-350`: Right expands, Left collapses). Returned as an
 * intent rather than performed, so the model is testable without a browser and
 * the DOM module has nothing to decide.
 *
 *   * **Down / Up** — move to the next / previous VISIBLE row. A row inside a
 *     collapsed subtree is not visible and is not stepped through, which is the
 *     difference between a tree and the flat list this panel used to be.
 *   * **Right** — expand a collapsed row; on an already-expanded row, move to
 *     its first child; on a leaf, nothing.
 *   * **Left** — collapse an expanded row; on an already-collapsed row or a
 *     leaf, move to its parent.
 *   * **Home / End** — first / last visible row.
 *   * **Enter / Space** — activate: put the caret at that heading. Space is
 *     included because the row is a `button` and a button's Space is its click;
 *     leaving it out would make the row behave unlike every other control.
 *   * **`*`** — expand every sibling at the focused row's level. This is ARIA's
 *     wording and it is DELIBERATELY not ADR-049's ("expands all under the
 *     focused row"): the asterisk is an accessibility convention, a screen
 *     reader user expects the documented behaviour, and the ADR sentence was
 *     written from Word's menu rather than from the tree pattern. Recorded here
 *     rather than left ambiguous, per `AGENTS.md`.
 *
 * Returns `{kind: "none"}` for a key the tree does not handle, so the caller
 * knows not to `preventDefault` it — swallowing Tab or a type-ahead letter is
 * how a tree stops being operable by keyboard.
 *
 * @param {string} key the `KeyboardEvent.key`
 * @param {{index: number, visible: {level: number, node: string, collapsed: boolean, hasChildren: boolean}[]}} state
 * @returns {{kind: string, index?: number, node?: string, nodes?: string[], collapsed?: boolean}}
 */
export function treeKeyAction(key, { index, visible }) {
  const rows = visible ?? [];
  const row = rows[index];
  if (!row) return { kind: "none" };
  const move = (to) => (to === index || to < 0 || to >= rows.length ? { kind: "none" } : { kind: "focus", index: to });
  switch (key) {
    case "ArrowDown":
      return move(index + 1);
    case "ArrowUp":
      return move(index - 1);
    case "Home":
      return move(0);
    case "End":
      return move(rows.length - 1);
    case "ArrowRight":
      if (row.collapsed) return { kind: "expand", node: row.node, index };
      if (row.hasChildren) return move(index + 1);
      return { kind: "none" };
    case "ArrowLeft":
      if (row.hasChildren && !row.collapsed) return { kind: "collapse", node: row.node, index };
      return move(parentIndex(rows, index));
    case "Enter":
    case " ":
      return { kind: "activate", node: row.node, index };
    case "*":
      return {
        kind: "expandSiblings",
        index,
        nodes: rows.filter((other) => other.level === row.level && other.collapsed).map((other) => other.node),
      };
    default:
      return { kind: "none" };
  }
}

/**
 * The outline-level picker's rungs: "all levels" plus levels 1 to 9.
 *
 * One table, so the palette, the menu and the panel cannot offer a level the
 * others do not have — the command-surface parity defect this repository keeps
 * finding (`docs/105` UX-004). `level: 0` is "show all levels", which folds
 * nothing; `level: 1` folds every heading. That is the engine's own
 * `foldToLevel` contract, named once.
 */
export const FOLD_LEVELS = Object.freeze([
  Object.freeze({ level: 0, id: "all", key: "fold.level.all" }),
  ...Array.from({ length: FOLD_MAX_LEVEL }, (_, index) =>
    Object.freeze({ level: index + 1, id: `l${index + 1}`, key: "fold.level.n" }),
  ),
]);

/**
 * The four fold command rows, as data.
 *
 * Declared here rather than in `main.js` for the reason `reflow_chrome.mjs`
 * declares its own: the labels, the state and the refusal belong with the
 * control, and `main.js` spreads the result into its registry. Each row carries
 * `enabled` and `disabledReason`, so a surface that cannot run a command shows
 * it **disabled with a reason** and never as a control that does nothing
 * (`SKILL` §10).
 *
 * `t` is injected rather than imported so this module stays runnable in `node`
 * with no locale boot, which is the whole point of the split.
 *
 * @param {{
 *   t: (key: string, params?: object) => string,
 *   reason: () => string,
 *   caretHeading: () => ({node: string, collapsed: boolean}|null),
 *   toggle: () => void,
 *   foldAll: () => void,
 *   unfoldAll: () => void,
 *   pickLevel: (level: number) => void,
 * }} deps
 */
export function foldCommands({ t, reason, caretHeading, toggle, foldAll, unfoldAll, pickLevel }) {
  const withheld = reason();
  const available = withheld === "";
  const heading = caretHeading();
  // `view.fold.toggle` needs a heading to act on, and "the caret is not in a
  // heading" is a different refusal from "this document cannot be folded". Two
  // withholdings, two sentences: one shared "folding is unavailable" string
  // would tell a reader who pressed the shortcut in a paragraph about a
  // document-level limitation that does not apply to them.
  const toggleReason = available ? (heading ? "" : t("fold.noHeadingAtCaret")) : withheld;
  return [
    {
      id: "view.fold.toggle",
      label: t(heading?.collapsed ? "fold.expandHeading" : "fold.collapseHeading"),
      group: "View",
      kw: "fold unfold collapse expand heading outline section hide show subtree",
      enabled: toggleReason === "",
      disabledReason: toggleReason,
      run: toggle,
    },
    {
      id: "view.fold.all",
      label: t("fold.collapseAll"),
      group: "View",
      kw: "fold collapse all headings outline sections overview",
      enabled: available,
      disabledReason: withheld,
      run: foldAll,
    },
    {
      id: "view.fold.none",
      label: t("fold.expandAll"),
      group: "View",
      kw: "unfold expand all headings outline sections reveal",
      enabled: available,
      disabledReason: withheld,
      run: unfoldAll,
    },
    ...FOLD_LEVELS.map((rung) => ({
      id: `view.fold.level.${rung.id}`,
      label: t(rung.key, { level: rung.level }),
      group: "View",
      kw: `fold level outline show headings depth ${rung.level}`,
      enabled: available,
      disabledReason: withheld,
      run: () => pickLevel(rung.level),
    })),
  ];
}

/**
 * What the editor says about the page count while anything is folded, or `""`
 * when nothing is.
 *
 * **Word says this out loud and so must we.** Collapsed content occupies no
 * pages, so the number on screen is lower than the number that will print —
 * necessarily, and by design: print, PDF and DOCX export are always fully
 * expanded (ADR-049), so the printed numbers are the true ones. A page
 * indicator that silently means something different while folded is the same
 * class of lie as printing a tile index as a page number, which `151` §6.5
 * refuses for the same reason.
 *
 * @param {number} foldedCount how many headings are folded
 * @param {(key: string, params?: object) => string} t
 */
export function pageCountCaveat(foldedCount, t) {
  const count = Number(foldedCount) || 0;
  if (count <= 0) return "";
  return t("fold.pageCountNotPrinted", { count });
}
