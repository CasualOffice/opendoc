// The view and text extras (`editor_view_text.mjs`): the pure rules beside the
// chrome — the zoom Multiple pages drops to, which importer a file is inserted
// through, and that the exported id list is the palette's.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import {
  VIEW_TEXT_COMMAND_IDS,
  insertFormatFor,
  multiplePagesZoom,
  rowWidthFor,
} from "../src/editor_view_text.mjs";
import { PAGE_GAP_PX } from "../src/page_scroll.mjs";

const LETTER_WIDTH_TWIP = 12240; // 816 px at 100%

test("Multiple pages zooms out until two pages sit side by side, never in", () => {
  const zoom = multiplePagesZoom({ viewportWidth: 1280, pageWidthTwip: LETTER_WIDTH_TWIP, currentZoom: 1 });
  assert.ok(zoom < 1, "a 1280px window cannot hold two Letter pages at 100%");
  assert.ok(2 * 816 * zoom + PAGE_GAP_PX <= rowWidthFor(1280), "two pages fit the row at that zoom");
  assert.equal(
    multiplePagesZoom({ viewportWidth: 4000, pageWidthTwip: LETTER_WIDTH_TWIP, currentZoom: 0.5 }),
    0.5,
    "already small enough: the reader's zoom stands",
  );
  assert.equal(
    multiplePagesZoom({ viewportWidth: 200, pageWidthTwip: LETTER_WIDTH_TWIP, currentZoom: 1 }),
    0.25,
    "never below the floor",
  );
});

test("a Markdown file is inserted as Markdown; anything else is detected from its bytes", () => {
  assert.equal(insertFormatFor("notes.md"), "text.markdown");
  assert.equal(insertFormatFor("README.MARKDOWN"), "text.markdown");
  assert.equal(insertFormatFor("report.docx"), "");
  assert.equal(insertFormatFor("plain.txt"), "");
  assert.equal(insertFormatFor("page.html"), "text.html");
});

test("the exported command ids are exactly the palette rows the module declares", () => {
  const source = readFileSync(new URL("../src/editor_view_text.mjs", import.meta.url), "utf8");
  const body = source.slice(source.indexOf("function paletteRows("));
  const declared = [...body.matchAll(/\{ id: "([\w.]+)"/g)].map((match) => match[1]);
  assert.deepEqual(declared, [...VIEW_TEXT_COMMAND_IDS]);
});
