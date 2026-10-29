// The pure halves of Borders and shading, table positioning and Draw table
// (`docs/153` table.borders-dialog, table.text-wrapping, table.position-dialog,
// table.draw): the edge-diagram model, the form -> patch diffs, and the pencil's
// geometry. The engine halves are `casual-doc-wasm`'s `table_layout` tests.
import assert from "node:assert/strict";
import test from "node:test";

import {
  BORDER_LINES,
  bordersPatch,
  edgesFromState,
  penCss,
  presetEdges,
  presetOf,
  toggleEdge,
} from "../src/table_borders_dialog.mjs";
import {
  formFromPosition,
  inchesField,
  inchesText,
  linkMoveWithText,
  positionPatch,
} from "../src/table_position_dialog.mjs";
import { drawnBox, insideCell, lineOrientation, straightLine } from "../src/table_draw_mode.mjs";
import { COMMAND_CONTRACT, commandContract } from "../src/host_contract.mjs";
import { APP_MENU_SECTIONS, TABLE_MENU_LABELS, TABLE_MENU_LABEL_KEYS, menuCommandIds } from "../src/command_taxonomy.mjs";
import { TABLE_FACES } from "../src/ribbon_faces.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";

const pen = { style: "single", size: 4, color: "#000000" };
const allOn = { top: "on", bottom: "on", left: "on", right: "on", insideH: "on", insideV: "on" };

test("the diagram opens on the engine's reading, and guesses nothing it was not told", () => {
  const edges = edgesFromState({ edges: { top: "on", insideH: "mixed", insideV: "unavailable", left: "bogus" } });
  assert.deepEqual(edges, {
    top: "on",
    bottom: "unavailable",
    left: "unavailable",
    right: "unavailable",
    insideH: "mixed",
    insideV: "unavailable",
  });
  assert.equal(BORDER_LINES.length, 6);
});

test("clicking a line flips it; a mixed line draws whole; an unavailable one never moves", () => {
  const edges = { ...allOn, insideV: "unavailable", insideH: "mixed" };
  assert.equal(toggleEdge(edges, "top").top, "off");
  assert.equal(toggleEdge(toggleEdge(edges, "top"), "top").top, "on");
  assert.equal(toggleEdge(edges, "insideH").insideH, "on");
  assert.equal(toggleEdge(edges, "insideV"), edges, "an inside line of a single column is not a control");
});

test("presets shape the diagram the way Word's Setting column does", () => {
  const start = { ...allOn, insideV: "unavailable" };
  assert.deepEqual(presetEdges(start, "none"), { ...start, top: "off", bottom: "off", left: "off", right: "off", insideH: "off" });
  assert.deepEqual(presetEdges(start, "box"), { ...start, insideH: "off" });
  const inside = presetEdges(presetEdges(start, "none"), "inside");
  assert.equal(inside.insideH, "on");
  assert.equal(inside.top, "off", "Inside leaves the outline alone");
  assert.equal(presetOf(presetEdges(allOn, "box")), "box");
  assert.equal(presetOf(allOn), "all");
  assert.equal(presetOf({ ...allOn, top: "off" }), "", "a custom diagram presses no preset");
});

test("the patch carries what moved: a toggled line, every drawn line when the pen moved, and the fill", () => {
  const initial = { ...allOn };
  const base = { scope: "cells", initial, initialPen: pen, pen, initialShading: "", shading: "" };
  assert.equal(bordersPatch({ ...base, edges: { ...initial } }), null, "nothing changed, nothing sent");

  const off = bordersPatch({ ...base, edges: { ...initial, insideH: "off" } });
  assert.deepEqual(off.edges, { insideH: "off" });
  assert.equal(off.scope, "cells");

  const restyled = bordersPatch({ ...base, edges: { ...initial, insideH: "off" }, pen: { ...pen, style: "double" } });
  assert.deepEqual(Object.keys(restyled.edges).sort(), ["bottom", "insideH", "insideV", "left", "right", "top"]);
  assert.equal(restyled.style, "double");

  const mixed = bordersPatch({ ...base, initial: { ...initial, top: "mixed" }, edges: { ...initial, top: "mixed" }, pen: { ...pen, size: 12 } });
  assert.ok(!("top" in mixed.edges), "an untouched mixed line keeps each cell's own");

  const filled = bordersPatch({ ...base, edges: { ...initial }, shading: "#ffff00" });
  assert.deepEqual(filled, { scope: "cells", shading: "#ffff00" });
  const cleared = bordersPatch({ ...base, edges: { ...initial }, initialShading: "#ffff00", shading: "" });
  assert.equal(cleared.shading, "none");

  // Nothing was drawn before: the first line drawn is sent with the pen shown.
  const first = bordersPatch({ ...base, initial: { ...initial, top: "off" }, edges: { ...initial }, initialPen: null });
  assert.ok(first.edges.top === "on" && first.size === 4 && first.color === "#000000");
});

test("the preview draws the pen the page will", () => {
  assert.equal(penCss({ style: "dashed", size: 12, color: "#ff0000" }), "2px dashed #ff0000");
  assert.equal(penCss({ style: "double", size: 4, color: "#000000" }), "3px double #000000");
  assert.match(penCss({ style: "dotDotDash", size: 4 }), /1px dotted/);
});

const reading = {
  wrap: "around",
  floatable: true,
  hAlign: "left",
  hOffsetTwips: 0,
  hRelative: "column",
  vAlign: "offset",
  vOffsetTwips: 720,
  vRelative: "paragraph",
  topTwips: 0,
  bottomTwips: 0,
  leftTwips: 180,
  rightTwips: 180,
  allowOverlap: true,
};

test("the position form opens on the reading and sends only what changed", () => {
  const form = formFromPosition(reading);
  assert.equal(form.vOffset, "0.5");
  assert.equal(form.left, "0.13");
  assert.equal(form.moveWithText, true, "move with text IS paragraph-relative");
  assert.deepEqual(positionPatch(reading, form), { patch: null });

  const centred = positionPatch(reading, { ...form, hAlign: "center", hRelative: "page", top: "0.1", allowOverlap: false });
  assert.deepEqual(centred.patch, { topTwips: 144, hAlign: "center", hRelative: "page", allowOverlap: false });

  // A stale number in an offset field the axis is not using is never sent.
  assert.deepEqual(positionPatch(reading, { ...form, hOffset: "9" }), { patch: null });
  assert.deepEqual(positionPatch(reading, { ...form, vOffset: "1" }).patch, { vOffsetTwips: 1440 });

  assert.deepEqual(positionPatch(reading, { ...form, left: "-1" }), { invalid: "left" });
  assert.deepEqual(positionPatch(reading, { ...form, vOffset: "abc" }), { invalid: "vOffset" });
  assert.equal(positionPatch(reading, { ...form, vOffset: "-2" }).patch.vOffsetTwips, -2880);
});

test("a file's inline vertical position survives an edit of the other fields", () => {
  const inline = { ...reading, vAlign: "inline" };
  const form = formFromPosition(inline);
  assert.equal(form.vAlign, "top");
  assert.deepEqual(positionPatch(inline, { ...form, right: "0.25" }).patch, { rightTwips: 360 });
});

test("move with text and the vertical anchor are one setting", () => {
  const form = formFromPosition(reading);
  assert.equal(linkMoveWithText({ ...form, moveWithText: false }, "moveWithText").vRelative, "margin");
  assert.equal(linkMoveWithText({ ...form, vRelative: "page", moveWithText: true }, "moveWithText").vRelative, "paragraph");
  assert.equal(linkMoveWithText({ ...form, vRelative: "page" }, "vRelative").moveWithText, false);
  assert.equal(linkMoveWithText({ ...form, vRelative: "margin", moveWithText: false }, "moveWithText").vRelative, "margin");
});

test("inches fields read signed, bounded, and never clamp", () => {
  assert.equal(inchesText(-1440), "-1");
  assert.equal(inchesText(0), "0");
  assert.equal(inchesField(" 1.5 ", -22, 22), 2160);
  assert.equal(inchesField("23", -22, 22), null);
  assert.equal(inchesField("", 0, 22), null);
});

test("the pencil straightens a line along the way it went further, and boxes a drag", () => {
  assert.equal(lineOrientation({ x: 0, y: 0 }, { x: 10, y: 300 }), "vertical");
  assert.equal(lineOrientation({ x: 0, y: 0 }, { x: 300, y: 10 }), "horizontal");
  assert.deepEqual(straightLine({ x: 100, y: 0 }, { x: 110, y: 400 }), [105, 0, 105, 400]);
  assert.deepEqual(straightLine({ x: 0, y: 50 }, { x: 400, y: 60 }), [0, 55, 400, 55]);
  assert.deepEqual(drawnBox({ x: 500, y: 400 }, { x: 100, y: 100 }), { left: 100, top: 100, width: 400, height: 300 });
  assert.equal(insideCell([1, 100, 100, 200, 100], 1, { x: 150, y: 150 }), true);
  assert.equal(insideCell([1, 100, 100, 200, 100], 2, { x: 150, y: 150 }), false, "another page");
  assert.equal(insideCell([], 1, { x: 150, y: 150 }), false);
});

test("Draw table is an Insert command with a face on the ribbon; Borders and shading is a Table menu row", () => {
  assert.equal(commandContract("insert.drawTable")?.requires, "mutate");
  assert.ok(COMMAND_CONTRACT.some((row) => row.id === "insert.drawTable"));
  assert.ok(menuCommandIds("insert").includes("insert.drawTable"));
  assert.ok(APP_MENU_SECTIONS.table, "the Table menu exists");
  assert.ok(menuCommandIds("table").includes("table.borderStyle"));
  assert.equal(TABLE_MENU_LABELS.get("table.borderStyle"), "Borders and shading…");
  assert.equal(EN_STRINGS[TABLE_MENU_LABEL_KEYS.get("table.borderStyle")], "Borders and shading…");
  assert.ok(TABLE_FACES.some((face) => face.select === "#tableBordersBtn" && face.command === "table.borderStyle"));
});
