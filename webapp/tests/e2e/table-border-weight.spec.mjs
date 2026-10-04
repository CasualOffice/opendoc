// Choosing a border's WIDTH, and getting that width (`docs/153`
// `table.border-width-style`, rank 2).
//
// Every border the table chrome wrote was hard-coded to 8 eighth-points — the
// literal `8` passed to `setCellBorderRange` and `setTableBorder` — while both
// engine calls have taken a size argument all along. So a person could not draw a
// hairline or a thick rule, and the gap was entirely the control.
//
// THE GUARD IS THE EFFECT, NOT THE DROPDOWN, and that is the hard part here:
// `cellBorderEdges` is the only read-back the facade offers and it returns a
// four-bit PRESENCE mask, so nothing in JavaScript can ask what width a cell's
// border actually has. A spec that asserted the `<select>` changed value would
// pass over a chrome that still sent 8, which is exactly the failure this file
// exists to prevent.
//
// So the width is read back out of a real EXPORT. Normalized JSON is the typed
// model serialised (`org.casualoffice.normalized-json`), `BorderEdge` carries
// `sizeEighthPoints` through serde, and the export is driven by the product's own
// `file.export.json` command — so what is asserted is the document the engine
// would hand anybody, not a number this test put somewhere. It is also plain text,
// which a DOCX is not: proving the same thing through `word/document.xml` would
// mean inflating a package to read one attribute.
import { readFile } from "node:fs/promises";

import { test, expect, clickIntoFirstPage, runAppMenuCommand } from "./fixtures.mjs";

/** How many border edges in the exported document are at each width.
 *
 *  Every `borders` record the snapshot carries, counted by `w:sz`. A COUNT rather
 *  than a set, because the interesting failures are mixtures: four edges where the
 *  chrome drew at the chosen width and two where it still used its old literal
 *  would satisfy "some edge is 18" and must not satisfy this.
 *
 *  Driven on a BLANK document deliberately. The demo fixture carries tables of its
 *  own, whose borders arrive at whatever width their producer wrote — the first
 *  run of this spec read 2, 4 and 18 and could not say which belonged to the
 *  gesture under test. A blank document plus one inserted table means every edge
 *  in the file is one this test created, which is the difference between asserting
 *  the product's guarantee and asserting what a sample document happened to be. */
async function exportedBorderWidths(page) {
  const download = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.json");
  const artifact = await download;
  const snapshot = JSON.parse((await readFile(await artifact.path())).toString("utf8"));
  const byWidth = new Map();
  const byStyle = new Map();
  const styles = new Set();
  const walk = (node) => {
    if (Array.isArray(node)) return void node.forEach(walk);
    if (!node || typeof node !== "object") return;
    for (const [key, value] of Object.entries(node)) {
      if (key === "borders" && value && typeof value === "object") {
        for (const record of Object.values(value)) {
          if (record && typeof record === "object" && "style" in record) {
            styles.add(record.style);
            // Counted as well as collected, for the same reason the widths are:
            // four edges in the chosen style and two still in the old one would
            // satisfy a set and must not satisfy this.
            byStyle.set(record.style, (byStyle.get(record.style) ?? 0) + 1);
            const size = record.sizeEighthPoints;
            if (size !== undefined) byWidth.set(size, (byWidth.get(size) ?? 0) + 1);
          }
        }
      }
      walk(value);
    }
  };
  walk(snapshot);
  return { byWidth, byStyle, styles: [...styles].sort() };
}

async function insertTableAndOpenBorders(page) {
  // A NEW BLANK document, not the demo one — see `exportedBorderWidths`. `?blank=1`
  // is the no-document state, so `file.new` is what actually produces a page, and
  // driving it from the File menu is also the product's own route to one.
  await page.goto("/editor.html?blank=1");
  await page.waitForFunction(
    () => document.getElementById("status")?.textContent !== "Loading engine…",
    null,
    { timeout: 45_000 },
  );
  await runAppMenuCommand(page, "file", "file.new");
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  await page.locator("#tabTable").click();
  await page.locator("#tableBtn").click();
  await expect(page.locator("#tableMenu")).toBeVisible();
}

test("the pen offers the widths Google Docs and ONLYOFFICE offer, and defaults to Word's", async ({
  page,
}) => {
  await insertTableAndOpenBorders(page);
  const weight = page.locator("#borderWeight");
  await expect(weight).toBeVisible();
  await expect(weight).toBeEnabled();
  // The seven widths all three references agree on, in eighth-points — `w:sz`'s
  // own unit, so nothing converts between the control and the engine.
  await expect(weight.locator("option")).toHaveCount(7);
  expect(await weight.locator("option").evaluateAll((os) => os.map((o) => o.value))).toEqual([
    "4",
    "8",
    "12",
    "18",
    "24",
    "36",
    "48",
  ]);
  // Word's Table Design opens on ½ pt and so does ONLYOFFICE
  // (`TableSettings.js:366` selects `store.at(1)`), and the engine's own
  // `insertTable` already writes 4 — three references, one value. The chrome used
  // to write 8, so a table inserted by the editor had ½ pt gridlines and the first
  // border a person applied silently doubled.
  await expect(weight).toHaveValue("4");
  // It is a PEN, not a reflector: it must carry an accessible name of its own,
  // because "4" read aloud with no label is not an answer.
  await expect(weight).toHaveAttribute("aria-label", /line weight/i);
});

test("a border applied at a chosen width comes back at that width", async ({
  page,
  consoleErrors,
}) => {
  await insertTableAndOpenBorders(page);

  // 2.25 pt — the fourth entry, deliberately NOT the default and NOT adjacent to
  // it, so a chrome that ignored the control and sent its old literal 8 fails
  // rather than landing on the right answer by accident.
  await page.locator("#borderWeight").selectOption("18");
  await page.locator('.border-btn[data-cellborder="box"]').click();
  await expect(page.locator('.border-btn[data-cellborder="box"]')).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  const { byWidth, styles } = await exportedBorderWidths(page);
  // FOUR edges at 2.25 pt: the box preset's top, bottom, start and end on the one
  // cell the caret is in. An exact count, so a chrome that wrote some edges at the
  // chosen width and some at its old literal cannot pass.
  expect(byWidth.get(18), "the box preset writes four cell edges at the chosen width").toBe(4);
  // And the old hard-coded value is GONE rather than merely outnumbered. This is
  // the line the mutation restoring `8` has to fail on.
  expect(byWidth.get(8), "no edge may still carry the old hard-coded 1 pt").toBeUndefined();
  // The six that remain are the table's own gridlines from `insertTable`, at the
  // engine's own default of ½ pt — untouched, because a cell gesture must not
  // rewrite the table.
  expect(byWidth.get(4), "the table's own gridlines are not rewritten").toBe(6);
  // The style is the pen's DEFAULT, because this gesture never touched the style
  // control — `single`, which is Word's and ONLYOFFICE's and the engine's own.
  //
  // This comment used to say something else, and it was true when it was written
  // and is not now: *"there is no style argument anywhere in `setCellBorderRange`
  // … so a line-style control could not be honest yet and is deliberately absent
  // … This line is what will fail, loudly, the day the engine gains that argument
  // and this chrome has not caught up."* The engine gained it, #732 shipped the
  // control, and this line did not fail — because a spec that never moves the
  // control only ever exercises the default. A test that cannot notice the
  // capability it describes arriving is `SKILL` §9 in miniature, so the sentence is
  // corrected here and the capability is asserted in the test below.
  expect(styles).toEqual(["single"]);
  expect(consoleErrors).toEqual([]);
});

test("the pen draws TABLE borders at the chosen width too, not only cell borders", async ({
  page,
  consoleErrors,
}) => {
  await insertTableAndOpenBorders(page);
  // ONE pen for both groups, which is Word's shape: Line Weight sits beside the
  // border painter and applies to whichever border button is pressed next. A
  // second weight for the table chips would be a question a person has to answer
  // twice, and two controls that can disagree about one gesture.
  await page.locator("#borderWeight").selectOption("48");
  await page.locator('[data-tableborder="all"]').click();

  const { byWidth } = await exportedBorderWidths(page);
  // `all` writes the four outer edges plus the two inside gridlines, and it
  // REPLACES the ½ pt set `insertTable` wrote — so six at 6 pt and none left at
  // the default, which is also what proves the table preset reads the same pen the
  // cell presets do rather than a second one.
  expect(byWidth.get(48), "the all preset writes six table edges at the chosen width").toBe(6);
  expect(byWidth.get(4), "the table's old gridlines are replaced, not added to").toBeUndefined();
  expect(byWidth.get(8), "no edge may still carry the old hard-coded 1 pt").toBeUndefined();
  expect(consoleErrors).toEqual([]);
});

// MUTATION PROOF for the test below: dropping the trailing `penStyle()` from
// `setCellBorderRange` in `table_cell_chrome.mjs` — the shape the file had before
// #732, which still compiles and still runs, because the argument is optional and
// absent means `single`:
//
//   Error: the dashed pen drew solid borders
//   expect(received).toEqual(expected)
//     -   "dashed",
//         "single",
//   1 failed
test("a border authored in a chosen line STYLE comes back in that style", async ({
  page,
  consoleErrors,
}) => {
  // The other half of the pen (`docs/153` `table.border-width-style`). Read back
  // out of a real export for the same reason the width is: `cellBorderStyle` can
  // only answer for the caret's cell, and what has to be true is that the
  // DOCUMENT carries the style — which is what any other application will read.
  await insertTableAndOpenBorders(page);

  const style = page.locator("#borderStyle");
  await expect(style).toBeVisible();
  await expect(style).toBeEnabled();
  // Exactly the six renderings `border_pattern` can draw apart. A seventh entry
  // would be a control promising a line the page cannot show.
  expect(await style.locator("option").evaluateAll((os) => os.map((o) => o.value))).toEqual([
    "single",
    "double",
    "dotted",
    "dashed",
    "dotDash",
    "dotDotDash",
  ]);
  await expect(style, "the pen opens on Word's and the engine's own default").toHaveValue("single");
  await expect(style).toHaveAttribute("aria-label", /line style/i);

  // `dashed` — not the default, and not adjacent to it in the list, so a chrome
  // that dropped the argument lands on `single` rather than on the right answer.
  await style.selectOption("dashed");
  await page.locator('.border-btn[data-cellborder="box"]').click();
  await expect(page.locator('.border-btn[data-cellborder="box"]')).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  const { byStyle, styles } = await exportedBorderWidths(page);
  // FOUR edges dashed — the box preset's top, bottom, start and end on the one
  // cell the caret is in — and the table's own SIX gridlines from `insertTable`
  // still single, because a cell gesture must not rewrite the table. Exact counts,
  // so "dashed appears somewhere" cannot pass, and neither can a chrome that
  // dashed the whole table.
  expect(styles, "the dashed pen drew solid borders").toEqual(["dashed", "single"]);
  expect(byStyle.get("dashed"), "the box preset writes four cell edges dashed").toBe(4);
  expect(byStyle.get("single"), "the table's own gridlines are not restyled").toBe(6);
  expect(consoleErrors).toEqual([]);
});
