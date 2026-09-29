// Format ▸ Paragraph ▸ Tab stops, driven as a user (`docs/148` §9 item 7,
// `docs/105` UX-004).
//
// THE POINT OF THIS FILE is not that a dialog opens. It is that the dialog and
// the RULER are two surfaces onto one document state: `ruler.mjs` was the only
// way to place a tab stop, and a second way that disagreed with the first would
// be worse than having one. So every assertion here is made twice — once against
// the list in the dialog, once against the glyphs on the ruler — and the last
// test holds the undo guarantee that "Clear all is one step" is about.
//
// Driven red before being trusted (SKILL §4); the mutations are in the commit
// message.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  expectEditorFocused,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

const COMMAND = "layout.tabStops";
const QUERY = "tab stops";

/** The ruler's tab glyphs, left to right: the letter each one shows. */
async function rulerGlyphs(page) {
  return page.locator(".ruler .tab-glyph").allTextContents();
}

/** The dialog's list rows, top to bottom. */
async function listRows(page) {
  return page.locator("#tabStopsList [data-position]").allTextContents();
}

async function openDialog(page) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill(QUERY);
  const row = page.locator(`#cmdList .cmd-item[data-command-id="${COMMAND}"]`);
  await expect(row, "tab stops should be offered by the palette").toBeVisible();
  await expect(row, "tab stops should be enabled with a caret in a paragraph").toBeEnabled();
  await row.click();
  await expect(page.locator("#tabStopsDialog")).toBeVisible();
}

/** Types a position, picks an alignment and presses Set. */
async function setStop(page, inches, align) {
  await page.locator("#tabStopsPosition").fill(inches);
  await page.locator(`#tabStopsAlign [data-tabalign="${align}"]`).click();
  await page.locator("#tabStopsSet").click();
}

test("a stop set in the dialog is the same stop the ruler draws", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await expect(page.locator(".ruler")).toBeVisible();
  expect(await rulerGlyphs(page)).toEqual([]);

  await openDialog(page);
  // Word opens on the position box, and so does this.
  await expect(page.locator("#tabStopsPosition")).toBeFocused();
  await expect(page.locator("#tabStopsEmpty")).toBeVisible();

  await setStop(page, "1", "start");
  await setStop(page, "2.5", "decimal");

  // Both surfaces, in ascending order, from one document state.
  await expect(page.locator("#tabStopsList [data-position]")).toHaveCount(2);
  expect(await listRows(page)).toEqual([
    expect.stringContaining("1"),
    expect.stringContaining("2.5"),
  ]);
  expect(await rulerGlyphs(page)).toEqual(["L", "."]);
  await expect(page.locator("#tabStopsEmpty")).toBeHidden();

  // The glyphs are laid out in document order too, not merely present.
  const xs = await page.locator(".ruler .tab-glyph").evaluateAll((nodes) =>
    nodes.map((node) => node.getBoundingClientRect().left),
  );
  expect(xs[1]).toBeGreaterThan(xs[0]);

  expect(consoleErrors).toEqual([]);
});

test("retyping a stop changes its kind in both places and does not add a second", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openDialog(page);
  await setStop(page, "1", "start");
  expect(await rulerGlyphs(page)).toEqual(["L"]);

  // Word's list fills the fields when a row is chosen; choosing the same
  // position again and pressing Set is how a stop is RETYPED rather than
  // doubled.
  await page.locator("#tabStopsList [data-position]").first().click();
  await expect(page.locator("#tabStopsPosition")).toHaveValue("1");
  await setStop(page, "1", "center");

  await expect(page.locator("#tabStopsList [data-position]")).toHaveCount(1);
  expect(await rulerGlyphs(page)).toEqual(["C"]);
  expect(consoleErrors).toEqual([]);
});

test("a position that is not a position is refused with a sentence, not clamped", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openDialog(page);

  // Blank, nonsense and off-the-paper are three different mistakes and get
  // three different sentences. None of them may place a stop.
  for (const [typed, sentence] of [
    ["", "Type a position first."],
    ["abc", "That is not a position."],
    ["99", "between 0 and 22 inches"],
  ]) {
    await page.locator("#tabStopsPosition").fill(typed);
    await page.locator("#tabStopsSet").click();
    await expect(page.locator("#tabStopsNote")).toContainText(sentence);
    await expect(page.locator("#tabStopsList [data-position]")).toHaveCount(0);
    expect(await rulerGlyphs(page)).toEqual([]);
  }
  expect(consoleErrors).toEqual([]);
});

test("Clear all is ONE undo step, whatever the paragraph held", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openDialog(page);
  await setStop(page, "1", "start");
  await setStop(page, "2.5", "decimal");
  await setStop(page, "4", "end");
  expect(await rulerGlyphs(page)).toEqual(["L", ".", "R"]);

  await page.locator("#tabStopsClearAll").click();
  await expect(page.locator("#tabStopsList [data-position]")).toHaveCount(0);
  expect(await rulerGlyphs(page)).toEqual([]);
  // Both buttons refuse honestly once there is nothing to act on.
  await expect(page.locator("#tabStopsClearAll")).toBeDisabled();
  await expect(page.locator("#tabStopsClear")).toBeDisabled();

  // Escape is the shared light-dismiss contract, and focus comes back to the
  // document rather than being dropped on <body>.
  await page.keyboard.press("Escape");
  await expect(page.locator("#tabStopsDialog")).toBeHidden();
  await expectEditorFocused(page);

  // ONE press. Three stops came back, which is the whole reason Clear all is
  // `clearTabStops` and not three `removeTabStop` calls.
  await page.keyboard.press(`${MOD}+z`);
  await expect(page.locator(".ruler .tab-glyph")).toHaveCount(3);
  expect(await rulerGlyphs(page)).toEqual(["L", ".", "R"]);
  expect(consoleErrors).toEqual([]);
});

test("Clear removes only the chosen stop", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openDialog(page);
  await setStop(page, "1", "start");
  await setStop(page, "2.5", "decimal");
  // Word leaves the list on the stop that was just set, and so does this.
  await expect(page.locator("#tabStopsList [data-position]").nth(1)).toHaveAttribute(
    "aria-selected",
    "true",
  );

  // Re-opening reads the paragraph fresh and chooses nothing, so Clear has to
  // refuse — disabled, with the reason that names the way out.
  await page.keyboard.press("Escape");
  await openDialog(page);
  await expect(page.locator("#tabStopsList [data-position]")).toHaveCount(2);
  await expect(page.locator("#tabStopsClear")).toBeDisabled();
  await expect(page.locator("#tabStopsClear")).toHaveAttribute("title", /list/i);

  await page.locator("#tabStopsList [data-position]").first().click();
  await expect(page.locator("#tabStopsClear")).toBeEnabled();
  await page.locator("#tabStopsClear").click();

  await expect(page.locator("#tabStopsList [data-position]")).toHaveCount(1);
  expect(await rulerGlyphs(page)).toEqual(["."]);
  expect(consoleErrors).toEqual([]);
});

test("the list is operable from the keyboard alone", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openDialog(page);
  await setStop(page, "1", "start");
  await setStop(page, "2.5", "decimal");

  const rows = page.locator("#tabStopsList [data-position]");
  await rows.first().focus();
  await page.keyboard.press("ArrowDown");
  await expect(rows.nth(1)).toBeFocused();
  // Arrowing CHOOSES as it goes, which is what makes the list usable without a
  // pointer: the fields follow, and Clear now acts on the second stop.
  await expect(rows.nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#tabStopsPosition")).toHaveValue("2.5");

  // The alignment control is a real radio group, arrows and all.
  const bar = page.locator('#tabStopsAlign [data-tabalign="bar"]');
  await page.locator('#tabStopsAlign [data-tabalign="decimal"]').focus();
  await page.keyboard.press("ArrowRight");
  await expect(bar).toBeFocused();
  await expect(bar).toHaveAttribute("aria-checked", "true");
  expect(consoleErrors).toEqual([]);
});

// Every control in this dialog writes. In Viewing mode `runToolbarEdit` refuses
// all three, so a dialog that opened there would be three dead buttons behind a
// modal scrim — which `docs/63` and SKILL §10 forbid more strongly than they
// forbid a missing row. The command is therefore present, disabled, and carries
// the same sentence the editor gives every other refused mutation.
//
// Note this is NOT what the caret predicate guards: the editor establishes a
// selection as soon as a document opens, so "no caret" is a state the palette
// cannot be put in with a document open, and a test asserting it would be one
// that could never fail. This is the refusal that is real.
test("in Viewing mode the command refuses and says why, instead of opening dead", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await setReviewMode(page, "viewing");

  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill(QUERY);
  const row = page.locator(`#cmdList .cmd-item[data-command-id="${COMMAND}"]`);
  await expect(row).toBeVisible();
  await expect(row).toBeDisabled();
  await expect(row.locator(".cmd-hint")).not.toBeEmpty();
  await expect(row.locator(".cmd-hint")).toHaveText(/editing/i);
  await page.keyboard.press("Escape");

  // Back in Editing it is the same row, enabled — the refusal was about the
  // mode, not about the command existing.
  await setReviewMode(page, "editing");
  await openDialog(page);
  expect(consoleErrors).toEqual([]);
});
