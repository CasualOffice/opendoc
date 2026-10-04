// The border PEN's line style: that a border authored as dashed is dashed.
//
// `docs/153` `table.border-width-style`. The three setters have taken a trailing
// style argument since the engine gained one, and before #732 nothing in the
// chrome passed it — so every border this editor drew was solid whatever the
// document or the person wanted. `tests/e2e/table-border-weight.spec.mjs` proved
// the WIDTH reaches the document and, in the same breath, recorded that the style
// could not: *"there is no style argument anywhere in `setCellBorderRange` …
// `border_edge` hard-codes `single` … This line is what will fail, loudly, the day
// the engine gains that argument and this chrome has not caught up."* The engine
// gained it, the chrome caught up, and nothing failed — because the style was only
// ever exercised at its default.
//
// ---- WHAT IS ASSERTED, AND WHY IT IS THE EFFECT ---------------------------
//
// Not "`penStyle()` was the eighth argument". That is the mechanism, and a guard
// on an argument list passes a chrome that sends the right value to the wrong
// setter, or sends it and then clears it. What is asserted is the READ-BACK:
// `cellBorderStyle` is the facade's own accessor for "what line style does this
// cell's border have", `reflect()` already depends on it, and it is the same
// question a reader asks by looking at the page.
//
// The fake document is therefore the engine's real border machinery, transcribed
// from `casual-doc-wasm/src/lib.rs`:
//
//   * `border_style_or_default` — `None`/`undefined` means `"single"`, which is
//     exactly why dropping the argument is SILENT rather than a type error;
//   * `border_style_token` — the six renderable styles plus the two aliases a
//     document can arrive carrying (`dashSmallGap` -> `dashed`,
//     `dashDotStroked` -> `dotDash`), refusing anything else with a sentence;
//   * `set_table_borders_preset` — `box` sets the four outer edges, `all` adds
//     the gridlines, `none` clears, and a single edge TOGGLES;
//   * `cell_border_style` — top ?? bottom ?? start ?? end, canonicalised, and
//     `""` for a cell with no border at all.
//
// A fake that stored whatever it was handed would let the mutation through: the
// whole defect is that `undefined` is a legal value meaning `single`.
//
// ---- MUTATION PROOF -------------------------------------------------------
//
// Dropping the trailing `penStyle()` from BOTH calls in `bindCellFormatMenu` —
// `setCellBorderRange(a, f, b.dataset.cellborder, r, g, bl, penWeight())` and
// `setTableBorder(node, b.dataset.tableborder, r, g, bl, penWeight())`, which is
// what the file looked like before #732 — applied, run, restored. It COMPILES and
// it RUNS, because the eighth argument is optional and absent means `single`;
// that is the whole reason this guard has to assert the effect. What it printed:
//
//   not ok 2 - a cell border authored as dashed reads back as dashed
//     error: |-
//       a border drawn with the dashed pen came back solid
//       'single' !== 'dashed'
//   not ok 3 - the pen draws TABLE borders in the chosen style too
//     error: |-
//       Expected values to be strictly equal:
//   not ok 4 - every style the renderer can draw apart survives the round trip
//     error: |-
//       double
//       'single' !== 'double'
//   not ok 6 - a cell with NO border leaves the pen where the person left it ("" is not "single")
//     error: |-
//       Expected values to be strictly equal:
//       'single' !== 'dotted'
//   # tests 9 / # pass 5 / # fail 4
//
// Four of the nine, and the five survivors are the point: the style list, the
// unknown-token fallback, the read-back of a style ALREADY in the document, the
// focus rule and every weight assertion are all blind to it. That is how a style
// control ships looking correct and drawing solid lines — and it is why test 6
// reddening matters more than it looks: it does not fail on the pen's value, it
// fails on the DOCUMENT, because a pen left correctly at `dotted` that draws
// `single` is the lying control in its most convincing form.
import assert from "node:assert/strict";
import test from "node:test";

import { EN_STRINGS } from "../src/en_strings.mjs";
import { setCatalogue, setLocale } from "../src/i18n.mjs";

setCatalogue("en", { ...EN_STRINGS });
setLocale("en");

// `bindCellFormatMenu` fills the weight `<select>` with `document.createElement`,
// so a `document` has to exist before the module is imported.
const activeElement = { current: null };
globalThis.document = {
  createElement: () => ({ value: "", textContent: "" }),
  get activeElement() {
    return activeElement.current;
  },
};

const {
  BORDER_STYLES,
  BORDER_WEIGHTS_EIGHTH_POINTS,
  DEFAULT_BORDER_STYLE,
  DEFAULT_BORDER_WEIGHT_EIGHTH_POINTS,
  bindCellFormatMenu,
  borderWeightLabel,
} = await import("../src/table_cell_chrome.mjs");

/** `border_style_token`: the canonical `w:val` for a style the renderer can draw
 *  distinguishably, or `null`. The two aliases are the ones a document arrives
 *  carrying, and they canonicalise rather than being refused. */
function styleToken(style) {
  return (
    {
      single: "single",
      double: "double",
      dotted: "dotted",
      dashed: "dashed",
      dashSmallGap: "dashed",
      dotDash: "dotDash",
      dashDotStroked: "dotDash",
      dotDotDash: "dotDotDash",
    }[style] ?? null
  );
}

/** `border_style_or_default`: `undefined` means `single`. THE LINE THE MUTATION
 *  WALKS THROUGH — an omitted style is not an error, it is a solid border. */
function styleOrDefault(style) {
  const token = styleToken(style ?? "single");
  if (token === null) throw new Error(`“${style}” isn't a border line style.`);
  return token;
}

/**
 * A document with one table of one cell, carrying the engine's own border rules.
 *
 * Edges are stored as `{ style, size }` records the way `BorderEdge` is, so what
 * `cellBorderStyle` reports is a consequence of what was written rather than a
 * value this test put somewhere.
 */
function fakeDoc() {
  const cell = { top: null, bottom: null, start: null, end: null, insideH: null, insideV: null };
  const table = { top: null, bottom: null, start: null, end: null, insideH: null, insideV: null };

  /** `set_table_borders_preset`. A single edge TOGGLES; the presets assign. */
  function preset(borders, edges, mk) {
    const toggle = (slot) => (borders[slot] = borders[slot] === null ? mk() : null);
    switch (edges) {
      case "none":
        for (const slot of ["top", "bottom", "start", "end", "insideH", "insideV"]) {
          borders[slot] = null;
        }
        break;
      case "box":
        for (const slot of ["top", "bottom", "start", "end"]) borders[slot] = mk();
        break;
      case "all":
        for (const slot of ["top", "bottom", "start", "end", "insideH", "insideV"]) {
          borders[slot] = mk();
        }
        break;
      case "top":
        toggle("top");
        break;
      case "bottom":
        toggle("bottom");
        break;
      case "left":
        toggle("start");
        break;
      case "right":
        toggle("end");
        break;
      default:
        break;
    }
  }

  /** `cell_border_style`: the first edge that exists, canonicalised, else "". */
  const firstStyle = (borders) => {
    const edge = borders.top ?? borders.bottom ?? borders.start ?? borders.end;
    return edge ? (styleToken(edge.style) ?? "single") : "";
  };

  return {
    cellBorders: cell,
    tableBorders: table,
    setCellBorderRange(anchor, focus, edges, r, g, b, size, style) {
      const token = styleOrDefault(style);
      preset(cell, edges, () => ({ style: token, size: Math.min(96, Math.max(2, size)), r, g, b }));
      return true;
    },
    setTableBorder(node, edges, r, g, b, size, style) {
      const token = styleOrDefault(style);
      preset(table, edges, () => ({ style: token, size: Math.min(96, Math.max(2, size)), r, g, b }));
      return true;
    },
    cellBorderStyle: () => firstStyle(cell),
    tableBorderStyle: () => firstStyle(table),
    cellBorderEdges: () =>
      (cell.top ? 1 : 0) | (cell.bottom ? 2 : 0) | (cell.start ? 4 : 0) | (cell.end ? 8 : 0),
    cellShadingAt: () => -1,
    cellVerticalAlignAt: () => "top",
    setCellShadingRange: () => true,
    setCellVerticalAlignRange: () => true,
    /** A document whose cell already carries a style the chrome did not choose —
     *  an imported file, or a second surface. Written through the same records. */
    seedCell(style) {
      const token = styleOrDefault(style);
      for (const slot of ["top", "bottom", "start", "end"]) cell[slot] = { style: token, size: 8 };
    },
  };
}

/** The elements of `#tableFmtMenu` this module touches, as plain objects. */
function fakeMarkup() {
  const press = new Map();
  const button = (dataset) => ({ dataset, attrs: new Map() });
  const cellButtons = ["none", "box", "top", "bottom", "left", "right"].map((k) =>
    button({ cellborder: k }),
  );
  const tableButtons = ["none", "box", "all"].map((k) => button({ tableborder: k }));
  for (const b of [...cellButtons, ...tableButtons]) {
    b.setAttribute = (name, value) => void b.attrs.set(name, value);
    b.getAttribute = (name) => b.attrs.get(name) ?? null;
  }
  const menu = {
    querySelectorAll: (selector) =>
      selector === ".border-btn"
        ? cellButtons
        : selector === "[data-tableborder]"
          ? tableButtons
          : [],
  };
  return { menu, cellButtons, tableButtons, press };
}

/** A `<select>` with the two properties the module uses. */
function fakeSelect(value = "") {
  const options = [];
  return { options, value, append: (option) => void options.push(option) };
}

function build() {
  const markup = fakeMarkup();
  const doc = fakeDoc();
  const borderStyle = fakeSelect(DEFAULT_BORDER_STYLE);
  const borderWeight = fakeSelect();
  const handlers = new Map();
  const shade = { value: "#ffffff", addEventListener: () => {} };
  const api = bindCellFormatMenu({
    menu: markup.menu,
    shade,
    shadeNone: {},
    vAlign: {},
    cellBorderColor: { value: "#112233" },
    tableBorderColor: { value: "#445566" },
    borderWeight,
    borderStyle,
    doc: () => doc,
    caretNode: () => "p1",
    formatRange: (apply) => apply("p1", "p1"),
    runNodeEdit: (thunk) => thunk("p1"),
    // The press is recorded against the element, so a test drives the REAL
    // listener the module installed rather than a seam added for it.
    onButton: (element, handler) => void handlers.set(element, handler),
    bindRadioGroup: () => ({ reflect: () => {} }),
    hexToRgb: (hex) => [
      Number.parseInt(hex.slice(1, 3), 16),
      Number.parseInt(hex.slice(3, 5), 16),
      Number.parseInt(hex.slice(5, 7), 16),
    ],
  });
  const pressCell = (edges) =>
    handlers.get(markup.cellButtons.find((b) => b.dataset.cellborder === edges))();
  const pressTable = (edges) =>
    handlers.get(markup.tableButtons.find((b) => b.dataset.tableborder === edges))();
  return { api, doc, borderStyle, borderWeight, pressCell, pressTable, ...markup };
}

// ---------------------------------------------------------------------------
// The effect: the style chosen is the style the document carries.
// ---------------------------------------------------------------------------

test("the pen offers exactly the styles the renderer can draw apart", () => {
  // Six, because `border_pattern` resolves six distinguishable renderings. A
  // seventh entry would be a control that promises a line the page cannot show —
  // which is the ONLYOFFICE `Wavy`-looks-like-`Single` defect the facade refuses.
  assert.deepEqual(BORDER_STYLES, [
    "single",
    "double",
    "dotted",
    "dashed",
    "dotDash",
    "dotDotDash",
  ]);
  assert.equal(DEFAULT_BORDER_STYLE, "single");
  for (const style of BORDER_STYLES) {
    assert.equal(styleToken(style), style, `${style} is not a canonical engine token`);
  }
});

test("a cell border authored as dashed reads back as dashed", () => {
  // THE CENTRAL CASE. Dashed is chosen deliberately: it is not the default and
  // not adjacent to it, so a chrome that dropped the argument lands on `single`
  // rather than on the right answer by accident.
  const h = build();
  h.borderStyle.value = "dashed";
  h.borderWeight.value = "18";
  h.pressCell("box");
  assert.equal(
    h.doc.cellBorderStyle("p1"),
    "dashed",
    "a border drawn with the dashed pen came back solid",
  );
  // All four edges, not just the one the accessor happens to read first.
  for (const slot of ["top", "bottom", "start", "end"]) {
    assert.equal(h.doc.cellBorders[slot].style, "dashed", slot);
    assert.equal(h.doc.cellBorders[slot].size, 18, `${slot} lost the chosen width`);
  }
  // And a cell gesture does not touch the table's own borders.
  assert.equal(h.doc.tableBorderStyle("p1"), "");
});

test("the pen draws TABLE borders in the chosen style too", () => {
  // ONE pen for both groups, which is Word's shape. A second style for the table
  // chips would be a question answered twice and two controls that can disagree
  // about one gesture.
  const h = build();
  h.borderStyle.value = "dotDotDash";
  h.borderWeight.value = "48";
  h.pressTable("all");
  assert.equal(h.doc.tableBorderStyle("p1"), "dotDotDash");
  for (const slot of ["top", "bottom", "start", "end", "insideH", "insideV"]) {
    assert.equal(h.doc.tableBorders[slot].style, "dotDotDash", slot);
    assert.equal(h.doc.tableBorders[slot].size, 48, slot);
  }
});

test("every style the renderer can draw apart survives the round trip", () => {
  // Each of the six, because a mapping is where a vocabulary drifts: five correct
  // entries and one that silently resolves to `single` is exactly the shape a
  // single-value test cannot see.
  for (const style of BORDER_STYLES) {
    const h = build();
    h.borderStyle.value = style;
    h.pressCell("box");
    assert.equal(h.doc.cellBorderStyle("p1"), style, style);
  }
});

test("a style the markup and the table disagree about falls back rather than throwing", () => {
  // `penStyle` refuses an unrecognised value HERE instead of sending it for the
  // engine to refuse, so the stroke still lands: a border is what the person
  // asked for, and refusing to draw one because a dropdown could not be read is
  // the worse answer. `wave` is a real `w:val` the model can hold and the
  // renderer cannot distinguish.
  const h = build();
  h.borderStyle.value = "wave";
  h.pressCell("box");
  assert.equal(h.doc.cellBorderStyle("p1"), "single", "the border was not drawn at all");
});

// ---------------------------------------------------------------------------
// `reflect()`: the two cases that are easy to "simplify" away.
// ---------------------------------------------------------------------------

test('a cell with NO border leaves the pen where the person left it ("" is not "single")', () => {
  // `cellBorderStyle` returns "" for a cell with no border, and that is not a
  // reason to move the control: clearing a border and drawing a new one must not
  // silently revert the pen to solid. The obvious `host.borderStyle.value =
  // doc.cellBorderStyle(node)` is the simplification this guards.
  const h = build();
  h.borderStyle.value = "dotted";
  assert.equal(h.doc.cellBorderStyle("p1"), "", "precondition: the cell has no border");
  h.api.reflect();
  assert.equal(h.borderStyle.value, "dotted", "clearing a border reverted the pen to solid");

  // Drawing with it, clearing it, and drawing again must give the same line both
  // times — which is the behaviour that rule exists for.
  h.pressCell("box");
  assert.equal(h.doc.cellBorderStyle("p1"), "dotted");
  h.pressCell("none");
  h.api.reflect();
  assert.equal(h.borderStyle.value, "dotted");
  h.pressCell("box");
  assert.equal(h.doc.cellBorderStyle("p1"), "dotted");
});

test("the style ALREADY IN FORCE is read back onto the pen", () => {
  // The other half of the same rule: a dropdown that only ever wrote would be a
  // write-only switch, so a cell that arrives carrying `dashSmallGap` must move
  // the control to its canonical `dashed`.
  const h = build();
  h.doc.seedCell("dashSmallGap");
  h.api.reflect();
  assert.equal(h.borderStyle.value, "dashed", "an imported alias did not canonicalise");
});

test("a pen the person has OPEN is never moved under them", () => {
  // `document.activeElement === host.borderStyle` is the same rule the weight
  // list follows by never being refilled: rewriting a `<select>` a keyboard user
  // is reading loses their place mid-choice, and the caret can move while the
  // popover is open.
  const h = build();
  h.doc.seedCell("double");
  h.borderStyle.value = "dotDash";
  activeElement.current = h.borderStyle;
  h.api.reflect();
  assert.equal(h.borderStyle.value, "dotDash", "the pen moved while the person had it open");
  activeElement.current = null;
  h.api.reflect();
  assert.equal(h.borderStyle.value, "double", "and it catches up once they let go");
});

// ---------------------------------------------------------------------------
// The weight half, so a style change cannot quietly cost it.
// ---------------------------------------------------------------------------

test("the weight list is the seven all three references agree on, at Word's default", () => {
  const h = build();
  assert.deepEqual(BORDER_WEIGHTS_EIGHTH_POINTS, [4, 8, 12, 18, 24, 36, 48]);
  assert.equal(
    h.borderWeight.options.length,
    BORDER_WEIGHTS_EIGHTH_POINTS.length,
    "the pen was not filled",
  );
  assert.equal(h.borderWeight.value, String(DEFAULT_BORDER_WEIGHT_EIGHTH_POINTS));
  assert.equal(borderWeightLabel(18), "2.25 pt");
  // An unreadable control still draws, at the default rather than at NaN.
  h.borderWeight.value = "";
  h.pressCell("box");
  assert.equal(h.doc.cellBorders.top.size, DEFAULT_BORDER_WEIGHT_EIGHTH_POINTS);
  assert.equal(h.doc.cellBorders.top.style, DEFAULT_BORDER_STYLE);
});
