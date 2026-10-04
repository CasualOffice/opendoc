// Folding's decisions, guarded where they can be run — ADR-049, `docs/157`.
//
// Everything here is a pure function over plain data, which is the whole reason
// `fold_view.mjs` exists: the keyboard model of a `role="tree"` is the part that
// is hardest to see and easiest to get wrong, this repository has no DOM in its
// unit tests, and a reducer is testable where the same logic spread through
// `keydown` handlers is only testable through a browser. That is not a
// hypothetical cost — it is why ONLYOFFICE's tree caret is a bare `<div>` with
// no role and no `tabindex` (`web-apps/.../TreeView.js:181`) and why
// `role="tree"` has zero hits across their whole app tree.
//
// ---- MUTATION PROOFS ------------------------------------------------------
//
// Each guard names the mutation that reddens it in its own comment. Applied one
// at a time to `webapp/src/fold_view.mjs`, run, restored. Recorded in the
// branch's commit message with the failure text.

import assert from "node:assert/strict";
import test from "node:test";

import { EN_STRINGS } from "../src/en_strings.mjs";
import { FOLD_LEVELS, FOLD_MAX_LEVEL, chevronPlacement, foldCommands, hasChildren, pageCountCaveat, parentIndex, parseOutlineRow, parseOutlineRows, siblingPositions, treeKeyAction, visibleOutlineRows } from "../src/fold_view.mjs";

/** An engine outline row. */
const row = (level, node, collapsed, text) => `${level}\t${node}\t${collapsed ? "1" : "0"}\t${text}`;

/** A small heading tree: H1 Alpha > H2 One, H2 Two > H3 Deep; H1 Omega. */
const TREE = [
  row(1, "n1", false, "Alpha"),
  row(2, "n2", false, "One"),
  row(2, "n3", false, "Two"),
  row(3, "n4", false, "Deep"),
  row(1, "n5", false, "Omega"),
];

// ---- Parsing ---------------------------------------------------------------

test("an outline row parses into level, node, fold state and text", () => {
  const parsed = parseOutlineRow("2\tabc\t1\tThe heading");
  assert.deepEqual(parsed, { level: 2, node: "abc", collapsed: true, text: "The heading" });
});

// Mutation: `parts.length < 4` -> `parts.length < 3`. A three-column row then
// parses, its text becomes the node column's neighbour, and the panel renders a
// `treeitem` whose `data-node` is a heading's TEXT — unfoldable, and silently.
test("a malformed row is skipped rather than rendered as an unfoldable item", () => {
  assert.equal(parseOutlineRow("nonsense"), null);
  assert.equal(parseOutlineRow("1\tn1"), null, "the pre-folding three-column shape is refused");
  assert.equal(parseOutlineRow("1\tn1\t0"), null, "so is a row with no text column");
  assert.equal(parseOutlineRow("x\tn1\t0\tText"), null, "and one with no numeric level");
  assert.equal(parseOutlineRow("1\t\t0\tText"), null, "and one with no node");
  assert.equal(parseOutlineRows(["nonsense", "1\tn1\t0\tReal"]).length, 1);
});

test("a level past Word's nine clamps, and the text keeps any tab-looking content", () => {
  assert.equal(parseOutlineRow("40\tn1\t0\tDeep").level, FOLD_MAX_LEVEL);
  assert.equal(parseOutlineRow("1\tn1\t0\tA\tB").text, "A\tB");
});

// ---- Which rows a tree shows -----------------------------------------------

// Mutation: in `visibleOutlineRows`, `if (row.level > suppress) continue` ->
// `if (row.level >= suppress) continue`. The heading that ENDS a fold then
// disappears too, so folding `Alpha` takes `Omega` with it.
test("a collapsed row hides its subtree and nothing beyond it", () => {
  const rows = parseOutlineRows([
    row(1, "n1", true, "Alpha"),
    row(2, "n2", false, "One"),
    row(3, "n3", false, "Deep"),
    row(1, "n4", false, "Omega"),
  ]);
  assert.deepEqual(
    visibleOutlineRows(rows).map((r) => r.node),
    ["n1", "n4"],
    "the collapsed heading stays — it is the affordance — and the next H1 ends the fold",
  );
});

test("a collapsed row nested inside another needs no stack", () => {
  const rows = parseOutlineRows([
    row(1, "n1", true, "Alpha"),
    row(2, "n2", true, "One"),
    row(3, "n3", false, "Deep"),
    row(1, "n4", false, "Omega"),
  ]);
  assert.deepEqual(
    visibleOutlineRows(rows).map((r) => r.node),
    ["n1", "n4"],
  );
});

test("nothing collapsed shows every row, with its children flag", () => {
  const visible = visibleOutlineRows(parseOutlineRows(TREE));
  assert.deepEqual(
    visible.map((r) => [r.node, r.hasChildren]),
    [
      ["n1", true],
      ["n2", false],
      ["n3", true],
      ["n4", false],
      ["n5", false],
    ],
  );
});

// Mutation: `hasChildren` -> `return true`. Every leaf then gets
// `aria-expanded="false"`, which tells a screen reader there is something to
// open and then there is not, and renders a live disclosure on a row with
// nothing under it.
test("only a row with something under it is a parent", () => {
  const rows = parseOutlineRows(TREE);
  assert.equal(hasChildren(rows, 0), true, "Alpha has an H2 under it");
  assert.equal(hasChildren(rows, 1), false, "One has nothing under it");
  assert.equal(hasChildren(rows, 4), false, "the last row can have no children");
});

// ---- ARIA set positions ----------------------------------------------------

// Mutation: in `siblingPositions`, drop the `counters.delete(level)` reset loop.
// The second H2's `aria-posinset` then keeps counting across parents, so a
// screen reader announces "3 of 2" — a number that cannot be true.
test("aria-posinset and aria-setsize describe the sibling set, not the whole list", () => {
  // TWO parents each with children, which is what makes the reset observable:
  // with one parent the level-2 counter never has to restart, so a fixture like
  // TREE alone passes the mutation that drops the reset. Found by mutation.
  const twoParents = parseOutlineRows([
    row(1, "a", false, "A"),
    row(2, "a1", false, "A1"),
    row(1, "b", false, "B"),
    row(2, "b1", false, "B1"),
    row(2, "b2", false, "B2"),
  ]);
  const across = siblingPositions(visibleOutlineRows(twoParents));
  assert.deepEqual(
    across.map((p) => [p.posinset, p.setsize]),
    [
      [1, 2],
      [1, 1],
      [2, 2],
      [1, 2],
      [2, 2],
    ],
    "B1 is the FIRST of B's two children, not the second H2 in the document",
  );

  const visible = visibleOutlineRows(parseOutlineRows(TREE));
  const positions = siblingPositions(visible);
  const described = visible.map((r, at) => [r.node, positions[at].posinset, positions[at].setsize]);
  assert.deepEqual(described, [
    ["n1", 1, 2], // Alpha: first of two H1s
    ["n2", 1, 2], // One: first of two H2s under Alpha
    ["n3", 2, 2], // Two: second of them
    ["n4", 1, 1], // Deep: the only H3 under Two
    ["n5", 2, 2], // Omega: second of two H1s
  ]);
  for (const [node, posinset, setsize] of described) {
    assert.ok(posinset <= setsize, `${node} announced ${posinset} of ${setsize}`);
  }
});

test("a row's parent is the nearest shallower row above it", () => {
  const visible = visibleOutlineRows(parseOutlineRows(TREE));
  assert.equal(parentIndex(visible, 0), -1, "a top-level row has no parent");
  assert.equal(parentIndex(visible, 1), 0);
  assert.equal(parentIndex(visible, 3), 2, "Deep's parent is Two, not Alpha");
  assert.equal(parentIndex(visible, 4), -1);
});

// ---- The keyboard model ----------------------------------------------------

const keyState = (rows, index) => ({ index, visible: visibleOutlineRows(parseOutlineRows(rows)) });

test("Down and Up walk the visible rows, and stop at the ends", () => {
  assert.deepEqual(treeKeyAction("ArrowDown", keyState(TREE, 0)), { kind: "focus", index: 1 });
  assert.deepEqual(treeKeyAction("ArrowUp", keyState(TREE, 2)), { kind: "focus", index: 1 });
  assert.deepEqual(treeKeyAction("ArrowUp", keyState(TREE, 0)), { kind: "none" }, "no wrap at the top");
  assert.deepEqual(treeKeyAction("ArrowDown", keyState(TREE, 4)), { kind: "none" }, "nor at the bottom");
});

// Mutation: `if (row.level > suppress) continue` in `visibleOutlineRows` plus
// nothing else is enough to redden this too — but the mutation written for THIS
// guard is swapping the ArrowRight arms, so a collapsed row moves instead of
// expanding. The tree then cannot be opened by keyboard at all, which is the
// exact failure ONLYOFFICE ships.
test("Right expands a collapsed row, then descends, and does nothing on a leaf", () => {
  const folded = [row(1, "n1", true, "Alpha"), row(2, "n2", false, "One"), row(1, "n5", false, "Omega")];
  assert.deepEqual(treeKeyAction("ArrowRight", keyState(folded, 0)), {
    kind: "expand",
    node: "n1",
    index: 0,
  });
  // Expanded with children: move to the first child.
  assert.deepEqual(treeKeyAction("ArrowRight", keyState(TREE, 0)), { kind: "focus", index: 1 });
  // A leaf: nothing, and NOT a move — a tree that walks sideways off a leaf
  // loses the reader's place.
  assert.deepEqual(treeKeyAction("ArrowRight", keyState(TREE, 1)), { kind: "none" });
});

// Mutation: drop the `!row.collapsed` term from ArrowLeft's condition. Left then
// re-collapses an already-collapsed row forever and can never reach the parent,
// so the only way out of a subtree is the mouse.
test("Left collapses an expanded row, else moves to the parent", () => {
  assert.deepEqual(treeKeyAction("ArrowLeft", keyState(TREE, 0)), {
    kind: "collapse",
    node: "n1",
    index: 0,
  });
  assert.deepEqual(treeKeyAction("ArrowLeft", keyState(TREE, 1)), { kind: "focus", index: 0 }, "a leaf goes up");
  // An already-collapsed row WITH CHILDREN is the case that matters: it must
  // move to its parent rather than re-collapse itself, or there is no way out of
  // a subtree by keyboard. A fixture whose collapsed row happened to be childless
  // passed the mutation that drops the `!collapsed` term — found by mutation.
  const folded = [row(1, "n1", true, "Alpha"), row(2, "n2", false, "One"), row(1, "n5", false, "Omega")];
  assert.deepEqual(
    treeKeyAction("ArrowLeft", keyState(folded, 0)),
    { kind: "none" },
    "an already-collapsed top-level row has nowhere to go, and does not re-collapse",
  );
  const nested = [row(1, "n1", false, "Alpha"), row(2, "n2", true, "One"), row(3, "n3", false, "Deep")];
  assert.deepEqual(
    treeKeyAction("ArrowLeft", keyState(nested, 1)),
    { kind: "focus", index: 0 },
    "an already-collapsed child goes up to its parent",
  );
});

test("Home and End reach the ends of the visible set", () => {
  assert.deepEqual(treeKeyAction("Home", keyState(TREE, 3)), { kind: "focus", index: 0 });
  assert.deepEqual(treeKeyAction("End", keyState(TREE, 0)), { kind: "focus", index: 4 });
});

// Mutation: drop the `case " ":` arm. Space then does nothing on a row that is a
// `<button>`, which is the one key every button in this editor responds to —
// and the row stops behaving like a control.
test("Enter and Space both activate the row", () => {
  for (const key of ["Enter", " "]) {
    assert.deepEqual(
      treeKeyAction(key, keyState(TREE, 2)),
      { kind: "activate", node: "n3", index: 2 },
      `${JSON.stringify(key)} activates`,
    );
  }
});

// Mutation: have `*` return every collapsed node rather than only the ones at the
// focused row's level. It then expands the whole document from any row, which is
// `expandAll` wearing another key's name.
test("asterisk expands the focused row's collapsed siblings and no others", () => {
  const rows = [
    row(1, "n1", false, "Alpha"),
    row(2, "n2", true, "One"),
    row(2, "n3", true, "Two"),
    row(1, "n5", true, "Omega"),
  ];
  const action = treeKeyAction("*", keyState(rows, 1));
  assert.equal(action.kind, "expandSiblings");
  assert.deepEqual(action.nodes, ["n2", "n3"], "the two collapsed H2s, not the collapsed H1");
});

// Mutation: return something other than `{kind: "none"}` from the default arm.
// The handler then `preventDefault`s Tab and every type-ahead letter, and the
// tree becomes a keyboard trap — the plainest WCAG 2.1.2 failure there is.
test("a key the tree does not handle is left alone", () => {
  for (const key of ["Tab", "a", "PageDown", "F6", "Escape"]) {
    assert.deepEqual(treeKeyAction(key, keyState(TREE, 0)), { kind: "none" }, key);
  }
  assert.deepEqual(treeKeyAction("ArrowDown", { index: 99, visible: [] }), { kind: "none" });
});

// ---- The level picker ------------------------------------------------------

test("the level picker offers all levels plus Word's nine, from one table", () => {
  assert.equal(FOLD_LEVELS.length, FOLD_MAX_LEVEL + 1);
  assert.deepEqual(FOLD_LEVELS[0], { level: 0, id: "all", key: "fold.level.all" });
  assert.deepEqual(FOLD_LEVELS.at(-1), { level: 9, id: "l9", key: "fold.level.n" });
});

// ---- The command rows ------------------------------------------------------

const t = (key, params = {}) => {
  // The catalogue's own English, with the plural family resolved the way
  // `i18n.mjs` does, so a key this test passes is a key the build accepts.
  const plural = params.count === 1 ? `${key}.one` : `${key}.other`;
  const template = EN_STRINGS[key] ?? EN_STRINGS[plural];
  assert.ok(template, `missing catalogue key: ${key}`);
  return String(template).replace(/\{(\w+)\}/g, (_, name) => String(params[name] ?? ""));
};

const commandDeps = (overrides = {}) => ({
  t,
  reason: () => "",
  caretHeading: () => ({ node: "n1", collapsed: false }),
  toggle: () => {},
  foldAll: () => {},
  unfoldAll: () => {},
  pickLevel: () => {},
  ...overrides,
});

test("every fold command exists, in the View group, with a catalogue label", () => {
  const commands = foldCommands(commandDeps());
  const ids = commands.map((command) => command.id);
  assert.ok(ids.includes("view.fold.toggle"));
  assert.ok(ids.includes("view.fold.all"));
  assert.ok(ids.includes("view.fold.none"));
  assert.ok(
    ids.includes("view.fold.level.all") && ids.includes("view.fold.level.l1"),
    `the level picker's rungs are commands too: ${ids.join(", ")}`,
  );
  for (const command of commands) {
    assert.equal(command.group, "View");
    assert.ok(command.label.length > 0, `${command.id} has a label`);
    assert.ok(command.kw.length > 0, `${command.id} is searchable`);
  }
});

// Mutation: in `foldCommands`, set `enabled: true` unconditionally. Every
// command then ships as a live control that throws from the engine, which is the
// dead control `SKILL` §10 forbids — and the reader is told nothing.
test("a withheld command is disabled WITH a reason, never dead", () => {
  const withheld = foldCommands(commandDeps({ reason: () => "Windowed, so no folding." }));
  for (const command of withheld) {
    assert.equal(command.enabled, false, `${command.id} is disabled`);
    assert.equal(command.disabledReason, "Windowed, so no folding.", `${command.id} says why`);
  }
});

// Mutation: make `toggleReason` fall back to the document-level `withheld`
// string when there is no heading. The reader who pressed the shortcut inside a
// paragraph is then told about a limitation that does not apply to them.
test("no heading at the caret is its own refusal, not the document's", () => {
  const commands = foldCommands(commandDeps({ caretHeading: () => null }));
  const toggle = commands.find((command) => command.id === "view.fold.toggle");
  assert.equal(toggle.enabled, false);
  assert.equal(toggle.disabledReason, EN_STRINGS["fold.noHeadingAtCaret"]);
  const all = commands.find((command) => command.id === "view.fold.all");
  assert.equal(all.enabled, true, "folding the document is still available");
});

test("the toggle names the direction it will go", () => {
  const collapse = foldCommands(commandDeps()).find((c) => c.id === "view.fold.toggle");
  assert.equal(collapse.label, EN_STRINGS["fold.collapseHeading"]);
  const expand = foldCommands(commandDeps({ caretHeading: () => ({ node: "n1", collapsed: true }) })).find(
    (c) => c.id === "view.fold.toggle",
  );
  assert.equal(expand.label, EN_STRINGS["fold.expandHeading"]);
});

// ---- The page-count sentence -----------------------------------------------

// Mutation: `if (count <= 0) return ""` -> `return ""` always. The editor then
// shows a page count that is not the printed one and says nothing about it,
// which is the silent-difference class `151` §6.5 refuses for a tile index.
test("while anything is folded the editor says the page count is not the printed one", () => {
  assert.equal(pageCountCaveat(0, t), "", "nothing folded, nothing to say");
  const one = pageCountCaveat(1, t);
  assert.match(one, /One heading is collapsed/);
  assert.match(one, /not the printed one/);
  const many = pageCountCaveat(4, t);
  assert.match(many, /^4 headings are collapsed/);
  assert.match(many, /always include collapsed content/);
});

// The rule a clamp got wrong. The chevron is an absolutely-positioned button, so
// one pixel over the text captures the tap that should place the caret.
test("the chevron never covers the text: shown only when the margin holds it whole", () => {
  const WIDTH = 18;
  const GUTTER = 20;
  // Desktop: a real page margin, so it is shown and sits entirely left of the text.
  const wide = chevronPlacement({ x: 96 }, GUTTER);
  assert.equal(wide.show, true);
  assert.ok(
    wide.left + WIDTH <= 96,
    `shown at left ${wide.left} with width ${WIDTH} would reach ${wide.left + WIDTH}, ` +
      "which is over a text edge at 96",
  );

  // Phone reflow: a 16px inset is narrower than the chevron needs, so withheld.
  assert.deepEqual(chevronPlacement({ x: 16 }, GUTTER), { show: false, left: 0 });

  // The boundary is exact: a margin of exactly the gutter still fits.
  assert.deepEqual(chevronPlacement({ x: 20 }, GUTTER), { show: true, left: 0 });
  assert.equal(chevronPlacement({ x: 19 }, GUTTER).show, false);
});

test("whenever it is shown, it is left of the text — at every margin, not just two", () => {
  const GUTTER = 20;
  for (let x = 0; x <= 200; x += 1) {
    const p = chevronPlacement({ x }, GUTTER);
    if (!p.show) continue;
    assert.ok(p.left >= 0, `x=${x}: left ${p.left} is outside the window`);
    assert.ok(p.left + 18 <= x, `x=${x}: the chevron reaches ${p.left + 18}, over the text`);
  }
});

// Surface beside the measure is somewhere to paint. Two
// individually-correct rules used to compose into an unreachable capability: the
// chevron needs more margin than a reflow tile's 16px gutter, and it is only
// shown while the outline panel is open — so the view where folding matters most
// had no in-body disclosure at all. The premise behind the first rule was that
// beyond the page box lies the DESK; in reflow the surface is one colour edge to
// edge, so beyond the tile is the same surface the text is on.
test("in reflow the chevron borrows the surface beside the measure, and only ever leftwards", () => {
  const GUTTER = 20;
  const WIDTH = 18;
  const REFLOW_INSET = 16; // REFLOW_GUTTER_PX — the tile's whole left margin.

  // On paper, unchanged, and this is the precondition rather than a decoration:
  // with no surface to borrow the refusal must still be a refusal, which is what
  // keeps the clamp defect closed where it was real (a phone, a 390px tile).
  assert.deepEqual(chevronPlacement({ x: REFLOW_INSET }, GUTTER, 0), { show: false, left: 0 });

  // In reflow at a desktop width there are hundreds of px of surface each side.
  const onSurface = chevronPlacement({ x: REFLOW_INSET }, GUTTER, 470);
  assert.equal(onSurface.show, true, "a reflow heading still has no disclosure");
  assert.equal(onSurface.left, REFLOW_INSET - GUTTER, "the left edge is not the gutter rule");
  assert.ok(
    onSurface.left + WIDTH <= REFLOW_INSET,
    `reaches ${onSurface.left + WIDTH}, over text that starts at ${REFLOW_INSET}`,
  );

  // THE GUARANTEE, not the two cases above: a wider surface NEVER moves the
  // chevron rightwards. It can only turn a refusal into the placement the gutter
  // rule already implied, so no amount of surface can put a button on a glyph.
  for (let x = 0; x <= 200; x += 1) {
    for (const overhang of [0, 1, 4, 16, 40, 470, 9_999]) {
      const p = chevronPlacement({ x }, GUTTER, overhang);
      if (!p.show) continue;
      assert.equal(p.left, x - GUTTER, `x=${x} overhang=${overhang}: left moved`);
      assert.ok(p.left + WIDTH <= x, `x=${x} overhang=${overhang}: over the text`);
      assert.ok(p.left >= -overhang, `x=${x} overhang=${overhang}: past the surface`);
    }
  }

  // A junk or negative overhang cannot widen the reach — a host reporting -100
  // must get the paged answer, not a chevron 120px into nowhere.
  for (const bad of [-100, Number.NaN, undefined, null, "lots"]) {
    assert.equal(
      chevronPlacement({ x: REFLOW_INSET }, GUTTER, bad).show,
      false,
      `overhang ${String(bad)} was treated as surface`,
    );
  }
});
