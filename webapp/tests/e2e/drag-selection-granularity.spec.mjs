// Double-click-and-drag selects by WORD; triple-click-and-drag by PARAGRAPH.
//
// The gesture every word processor has: Word documents it as "double-click and
// drag to select by word", Google Docs matches it, and ONLYOFFICE carries it as
// a document-level selection TYPE that persists for the gesture
// (`AscWord.TEXT_SELECTION_TYPE`, `sdkjs/word/Editor/Document.js:82-86`, chosen
// from the click count at `Document.js:10489-10494` and applied on every
// selection update at `sdkjs/word/Editor/Paragraph.js:408-427`).
//
// This editor had neither, and the failure mode was worse than absence: the
// browser's `dblclick` fires on the SECOND pointer-up — after the drag has
// finished — and the handler then selected the word under the pointer's FINAL
// position, overwriting everything the drag had built. So a double-click-and-drag
// selected exactly one word however far the pointer travelled, and the triple-
// click `click` listener did the same to a paragraph drag.
//
// Every test here drives a REAL gesture: `mouse.down({ clickCount: 2 })` with
// the button held, then `mouse.move`, then `mouse.up` — which is the sequence
// that produces the clobbering `dblclick`. A spec that called
// `click({ clickCount: 2 })` and then dragged would be testing two separate
// gestures and would have stayed green while this was broken.
//
// The ground truth for "what is selected" is the native copy payload — what the
// ENGINE believes is selected — not the painted highlight, because the painted
// highlight is only a picture of it.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
} from "./fixtures.mjs";

/** Three words whose byte offsets are known because they are pure ASCII:
 *    aardvark  0..8    (space at 8)
 *    bandicoot 9..18   (space at 18)
 *    capybara  19..27  (space at 27)
 *    dormouse  28..36
 *  Deliberately not real sentence text: a repeated character would give the
 *  shaper no word boundary to find, and a word that also occurs in the fixture
 *  would make "is this the right word?" unanswerable. */
const WORDS = "aardvark bandicoot capybara dormouse ";

async function caretBox(page) {
  return page.locator(".overlay .caret").first().evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, height: r.height };
  });
}

/** The client point the caret occupies at byte `offset` of the first paragraph.
 *
 *  Measured from the painted caret rather than computed from a font metric: the
 *  spec needs the pixel a user would aim at, and only the renderer knows it.
 *
 *  Polled until the rect STOPS MOVING, which is not pedantry: read mid-repaint
 *  it answers the previous offset's x, and a drag between two points that are
 *  really the same point selects one word and looks exactly like the defect
 *  this file is about. That is a flake that would have been read as a
 *  regression. */
async function pointAtOffset(page, offset) {
  await moveCaretToDocStart(page);
  for (let i = 0; i < offset; i += 1) await page.keyboard.press("ArrowRight");
  let previous = null;
  let box = null;
  await expect
    .poll(async () => {
      const next = await caretBox(page);
      const settled = previous !== null && Math.abs(next.x - previous.x) < 0.5;
      previous = next;
      box = next;
      return settled;
    })
    .toBe(true);
  return { x: box.x + 1, y: box.y + box.height / 2 };
}

/** Two points a drag can actually travel between. A spec that drags between two
 *  points the renderer happened to answer identically proves nothing. */
function expectApart(from, to, what) {
  expect(
    Math.abs(to.x - from.x) + Math.abs(to.y - from.y),
    `${what}: the two ends of the drag resolved to the same pixel`,
  ).toBeGreaterThan(10);
}

/** The text the ENGINE believes is selected: the native copy payload. */
async function selectedText(page) {
  return page.evaluate(() => {
    const dt = new DataTransfer();
    document.dispatchEvent(
      new ClipboardEvent("copy", { clipboardData: dt, bubbles: true, cancelable: true }),
    );
    return dt.getData("text/plain");
  });
}

/** A press of `clickCount` clicks that then DRAGS to `to` before releasing.
 *
 *  The button stays down between the final `down` and the `up`, which is the
 *  whole point: this is one gesture, and `dblclick` (or the third `click`) fires
 *  at the end of it. */
async function clickAndDrag(page, from, through, clickCount) {
  await page.mouse.move(from.x, from.y);
  for (let n = 1; n < clickCount; n += 1) {
    await page.mouse.down({ clickCount: n });
    await page.mouse.up({ clickCount: n });
  }
  await page.mouse.down({ clickCount: clickCount });
  for (const point of [through].flat()) {
    await page.mouse.move(point.x, point.y, { steps: 14 });
  }
  await page.mouse.up({ clickCount: clickCount });
}

/** The known words at the start of the document, and the first paragraph's own
 *  text left after them so a selection can be told apart from "everything". */
async function seedWords(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type(WORDS);
  await moveCaretToDocStart(page);
  await expect(page.locator(".overlay .caret").first()).toBeVisible();
}

test("double-click-and-drag selects whole words, not the one under the pointer", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await seedWords(page);

  // Press inside "aardvark" (offset 3) and drag to inside "capybara" (offset 22).
  const from = await pointAtOffset(page, 3);
  const to = await pointAtOffset(page, 22);
  expectApart(from, to, "aardvark to capybara");
  await clickAndDrag(page, from, to, 2);

  const selected = await selectedText(page);
  // The guarantee a reader experiences: the three words they dragged across,
  // whole. Before this shipped, the answer was "capybara" — the single word the
  // pointer happened to finish on.
  expect(
    selected,
    `a double-click-and-drag across three words selected ${JSON.stringify(selected)}`,
  ).toBe("aardvark bandicoot capybara");

  expect(consoleErrors).toEqual([]);
});

test("a word drag grows the ANCHOR end too, so the first word stays whole", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await seedWords(page);

  // Press in the MIDDLE of "bandicoot" (offset 13) and drag BACKWARD into
  // "aardvark" (offset 4). ONLYOFFICE's `checkWordSelection`
  // (`sdkjs/word/Editor/Paragraph.js:8606-8626`) snaps the anchor to its word's
  // END when the selection runs backward; without that rule the selection would
  // start mid-word at "bandi" and the word the gesture began on would be cut.
  const from = await pointAtOffset(page, 13);
  const to = await pointAtOffset(page, 4);
  expectApart(from, to, "bandicoot back to aardvark");
  await clickAndDrag(page, from, to, 2);

  const selected = await selectedText(page);
  expect(
    selected,
    `a backward word drag selected ${JSON.stringify(selected)}`,
  ).toBe("aardvark bandicoot");

  expect(consoleErrors).toEqual([]);
});

test("a word drag that reverses past its own press keeps the word it started on", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await seedWords(page);

  // Press in "bandicoot" (13), drag BACKWARD into "aardvark" (4), then FORWARD
  // again past the press point into "capybara" (22), and release.
  //
  // This is the case that decides whether the anchor may be read back from the
  // selection. Dragging backward puts the anchor on "bandicoot"'s END offset,
  // which is the space after it — and `word_bounds` answers `[]` for a space,
  // so an anchor re-snapped from the live selection STAYS on that space and the
  // word the gesture began on is silently dropped from the selection. Keeping
  // the raw press position is what makes the reversal recoverable.
  const press = await pointAtOffset(page, 13);
  const back = await pointAtOffset(page, 4);
  const forward = await pointAtOffset(page, 22);
  expectApart(press, back, "bandicoot back to aardvark");
  expectApart(press, forward, "bandicoot forward to capybara");
  await clickAndDrag(page, press, [back, forward], 2);

  const selected = await selectedText(page);
  expect(
    selected,
    `a reversed word drag selected ${JSON.stringify(selected)}`,
  ).toBe("bandicoot capybara");

  expect(consoleErrors).toEqual([]);
});

test("a bare double-click still selects exactly the word under it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await seedWords(page);

  // The precondition for the two tests above: the gesture that does NOT drag is
  // unchanged. The guard that stops `dblclick` overwriting a drag must not also
  // stop it doing its own job.
  const at = await pointAtOffset(page, 13);
  await page.mouse.click(at.x, at.y, { clickCount: 2 });

  expect(await selectedText(page)).toBe("bandicoot");
  expect(consoleErrors).toEqual([]);
});

test("a single-click drag is still character-granular", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await seedWords(page);

  // Granularity must be chosen by the click count and nothing else: a plain
  // drag that happens to start inside a word still selects characters, or
  // selecting part of a word would have become impossible.
  const from = await pointAtOffset(page, 2);
  const to = await pointAtOffset(page, 5);
  expectApart(from, to, "three characters");
  await clickAndDrag(page, from, to, 1);

  expect(await selectedText(page)).toBe("rdv");
  expect(consoleErrors).toEqual([]);
});

test("triple-click-and-drag selects whole paragraphs", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  // Two paragraphs whose first words are unambiguous, with the fixture's own
  // text trailing the second one so "the whole paragraph" is distinguishable
  // from "the words I typed".
  await page.keyboard.type("Aardvark paragraph one.");
  await page.keyboard.press("Enter");
  await page.keyboard.type("Bandicoot paragraph two. ");
  await moveCaretToDocStart(page);
  await expect(page.locator(".overlay .caret").first()).toBeVisible();

  const from = await pointAtOffset(page, 4); // inside "Aardvark", paragraph one
  await moveCaretToDocStart(page);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Home");
  for (let i = 0; i < 4; i += 1) await page.keyboard.press("ArrowRight");
  const second = await caretBox(page);
  const to = { x: second.x + 1, y: second.y + second.height / 2 };

  expectApart(from, to, "paragraph one to paragraph two");
  await clickAndDrag(page, from, to, 3);

  const selected = await selectedText(page);
  // Both paragraphs, each whole. Before this shipped the triple-click `click`
  // listener fired after the drag and pulled the selection back to the single
  // paragraph under the pointer.
  expect(
    selected.startsWith("Aardvark paragraph one."),
    `a paragraph drag selected ${JSON.stringify(selected.slice(0, 80))}`,
  ).toBe(true);
  expect(selected).toContain("Bandicoot paragraph two.");
  expect(
    selected.split("\n").length,
    "a paragraph drag across two paragraphs must span both",
  ).toBeGreaterThan(1);

  expect(consoleErrors).toEqual([]);
});

test("a bare triple-click still selects exactly its paragraph", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("Aardvark paragraph one.");
  await page.keyboard.press("Enter");
  await page.keyboard.type("Bandicoot paragraph two. ");
  await moveCaretToDocStart(page);

  const at = await pointAtOffset(page, 4);
  await page.mouse.click(at.x, at.y, { clickCount: 3 });

  const selected = await selectedText(page);
  expect(selected).toBe("Aardvark paragraph one.");
  expect(consoleErrors).toEqual([]);
});
