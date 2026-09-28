// Three capabilities that already worked but could only be reached one level
// deeper than the competition puts them (`105` UX-004, the command-surface
// parity defect class):
//
//   * Page number and Date were field KINDS, reachable only through Insert ▸
//     Field and then a row in the picker. Word's Insert tab has Page Number in
//     its Header & Footer group and Date & Time in its Text group; ONLYOFFICE's
//     Insert tab has both as their own buttons. They are now direct band
//     buttons — and they run the SAME `insertFieldAtCaret` the picker rows run,
//     declared once in `INSERT_SURFACE`, so there is no second insert path.
//   * Comment was in the Insert MENU and on the Review band, but had no Insert
//     BAND button. In the ribbon chrome there is no menu bar (docs/122), so the
//     Insert tab could not reach commenting at all.
//
// "The button exists" is not "the button works", so every guard here drives the
// real control and then reads a signal the button itself cannot fake. Three
// kinds, weakest to strongest: the status strip naming the kind, the caret
// advancing past the field's inline width and the Undo control naming the single
// "Field change" (render- and history-derived), and the normalized-JSON export
// carrying the field's own OOXML instruction (model-derived — the only one of the
// three that can tell a PAGE field from a NUMPAGES field).
//
// And because the icons are ligatures in a self-hosted font, each new button's
// glyph is MEASURED — a ligature the font does not carry renders as the literal
// word and blows the control's width out, which has shipped here before.
import { readFileSync } from "node:fs";
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  openAppMenu,
  openCommandPalette,
  expectNothingToUndo,
  runAppMenuCommand,
} from "./fixtures.mjs";

/** The collapsed caret's rounded on-screen x, from the overlay the editor draws
 *  from engine geometry. A field inserted at the caret advances it by the
 *  field's inline width — the render-layer proof it laid out inline. */
function caretX(page) {
  return page.evaluate(() => {
    const caret = document.querySelector(".overlay .caret");
    return caret ? Math.round(Number.parseFloat(caret.style.left)) : null;
  });
}

async function openInsertTab(page) {
  await page.locator("#tabInsert").click();
  await expect(page.locator("#panelInsert")).toBeVisible();
}

// The three controls, with the group Word and ONLYOFFICE file each one under.
const NEW_CONTROLS = [
  ["#insertPageNumberBtn", "insert-running", "insert.field.page"],
  ["#insertDateBtn", "insert-text", "insert.field.date"],
  ["#insertCommentBtn", "insert-comments", "review.comment"],
];

test("each promoted control sits in the group Word files it under and names its command", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);
  await openInsertTab(page);

  for (const [selector, group, command] of NEW_CONTROLS) {
    const button = page.locator(selector);
    await expect(button).toBeVisible();
    // The command id, not just the presence of a button: a control that runs
    // something other than the command its surface claims is the drift the
    // `INSERT_SURFACE` / `REVIEW_SURFACE` tables exist to prevent, and the id is
    // stamped from the table row rather than authored in the markup.
    await expect(button).toHaveAttribute("data-command", command);
    await expect(page.locator(`#panelInsert .rgroup[data-group="${group}"] ${selector}`)).toHaveCount(
      1,
    );
  }

  expect(consoleErrors).toEqual([]);
});

// A Material Symbols ligature the self-hosted font does not carry renders as the
// literal word, which silently widens the control — that is how a clipped "100"
// button shipped. Measured, and measured as `scrollWidth` rather than as the
// bounding box: the bounding box is what the flex button CLIPS the span to, so a
// 17-character word inside a 30px button reads as 28px and a guard on it has
// almost no margin. `scrollWidth` is the content's own width, so a resolved glyph
// measures one em box (18px here, the same as every other icon on this band) and
// an unresolved ligature measures an order of magnitude more.
//
// This asserts the icon RESOLVES, not which icon it is: pinning the ligature
// would fail a cosmetic refresh while catching nothing, which is why the roster
// spec deliberately refuses to.
test("every new band icon renders as a glyph, not as its ligature name", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);
  await openInsertTab(page);

  const measured = await page.evaluate(
    (selectors) =>
      selectors.map((selector) => {
        const span = document.querySelector(`${selector} .ms`);
        return { selector, text: span.textContent, width: span.scrollWidth };
      }),
    NEW_CONTROLS.map(([selector]) => selector),
  );

  for (const { selector, text, width } of measured) {
    expect(
      width,
      `${selector} renders "${text}" ${width}px wide — the ligature did not resolve`,
    ).toBeLessThanOrEqual(24);
    expect(width, `${selector} rendered nothing`).toBeGreaterThan(4);
  }

  expect(consoleErrors).toEqual([]);
});

test("Insert ▸ Page number inserts the PAGE field directly, with no picker", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await page.locator("#tabHome").click();
  const undoBtn = page.locator("#undoBtn");
  await expect(undoBtn).toBeDisabled(); // nothing done yet
  const startX = await caretX(page);

  await openInsertTab(page);
  await page.locator("#insertPageNumberBtn").click();

  // Directly: the field picker never opens. That is the whole point of the
  // promotion, and it is what a second insert path would most easily get wrong.
  await expect(page.locator("#fieldDialog")).toBeHidden();
  await expect(page.locator("#status")).toContainText("Inserted Page number");

  // And it is a real field: the line reflowed around its inline width, and the
  // op entered history as the single undoable "Field change".
  await expect.poll(() => caretX(page)).toBeGreaterThan(startX);
  await page.locator("#tabHome").click();
  await expect(undoBtn).toHaveAttribute("aria-label", "Undo Field change");

  // One Undo takes it back out, and nothing else was charged to history.
  await undoBtn.click();
  await expect.poll(() => caretX(page)).toBe(startX);
  await expect(undoBtn).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});

test("Insert ▸ Date inserts the DATE field directly, with no picker", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await page.locator("#tabHome").click();
  const undoBtn = page.locator("#undoBtn");
  await expect(undoBtn).toBeDisabled();
  const startX = await caretX(page);

  await openInsertTab(page);
  await page.locator("#insertDateBtn").click();

  await expect(page.locator("#fieldDialog")).toBeHidden();
  await expect(page.locator("#status")).toContainText("Inserted Date");
  await expect.poll(() => caretX(page)).toBeGreaterThan(startX);
  await page.locator("#tabHome").click();
  await expect(undoBtn).toHaveAttribute("aria-label", "Undo Field change");

  expect(consoleErrors).toEqual([]);
});

// The strongest available statement of "the button works": not that the caret
// moved or that the strip said something, but that the DOCUMENT now holds the two
// field nodes, read back out of the engine through the normalized-JSON export.
// Caret geometry and the Undo label are render- and history-derived and could
// both be satisfied by the wrong kind of field; the instruction string cannot.
test("both promoted buttons put real field nodes in the document model", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await openInsertTab(page);
  await page.locator("#insertPageNumberBtn").click();
  await expect(page.locator("#status")).toContainText("Inserted Page number");
  await page.locator("#insertDateBtn").click();
  await expect(page.locator("#status")).toContainText("Inserted Date");

  await openCommandPalette(page);
  await page.locator("#cmdInput").fill("Normalized JSON");
  const download = page.waitForEvent("download");
  await page.locator(".cmd-item", { hasText: "Normalized JSON" }).first().click();
  const model = JSON.parse(readFileSync(await (await download).path(), "utf8"));

  // Walk the export once and collect every field inline, rather than asking the
  // model for each kind in turn.
  const fields = [];
  const walk = (node) => {
    if (Array.isArray(node)) return node.forEach(walk);
    if (!node || typeof node !== "object") return;
    if (node.type === "field") fields.push({ instruction: node.instruction, kind: node.kind?.type });
    for (const value of Object.values(node)) walk(value);
  };
  walk(model);

  // PAGE carries Word's own field instruction and recomputes at pagination, so
  // it has no cached result text. DATE caches the host-formatted string, because
  // the engine reads no clock (see `field_kinds.mjs`).
  expect(fields).toEqual(
    expect.arrayContaining([
      { instruction: "PAGE \\* MERGEFORMAT", kind: "page" },
      { instruction: "DATE", kind: "date" },
    ]),
  );

  expect(consoleErrors).toEqual([]);
});

test("Insert ▸ Comment needs a range, says so, and opens the one composer", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await openInsertTab(page);

  const button = page.locator("#insertCommentBtn");
  // A caret is not a range, so the control is disabled — and a disabled control
  // has to SAY why, which for a button that takes no events means its title.
  await expect(button).toBeDisabled();
  // The title must INSTRUCT, not describe. It used to carry the margin button's
  // own label — "Add a comment on the selected text" — which says what the
  // control does and leaves a reader who cannot click it none the wiser. A
  // reason starts with the thing to do.
  await expect(button).toHaveAttribute("title", /^select /i);

  // Select something and the same button comes live, from the REVIEW_SURFACE
  // rule the Review band's button reads — one precondition, not two.
  for (let i = 0; i < 6; i++) await page.keyboard.press("Shift+ArrowRight");
  await openInsertTab(page);
  await expect(button).toBeEnabled();

  await button.click();
  const composer = page.locator("#reviewSidebar textarea, .review-composer textarea").first();
  await expect(composer).toBeVisible();

  expect(consoleErrors).toEqual([]);
});

test("Viewing mode refuses both direct field inserts and charges nothing to history", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "viewing");

  // Driven from the Insert MENU, not the band: Viewing mode composes the ribbon
  // away, and the menu is the axis a reader keeps. The rows stay ENABLED — they
  // require a document and a caret, which is what they have — and the refusal is
  // the shared `blockMutationInViewing()` gate speaking in the status line, which
  // is the guarantee this test exists for. A row that went grey here would be
  // proving the button, not the choke point.
  for (const command of ["insert.field.page", "insert.field.date"]) {
    await runAppMenuCommand(page, "insert", command);
    await expect(page.locator("#status")).toContainText("read-only");
  }

  await expectNothingToUndo(page);

  expect(consoleErrors).toEqual([]);
});

// The promoted kinds are commands, not band-only shortcuts: a capability on one
// surface is the defect this whole round is about, so the Insert MENU offers the
// same two rows. `insert-surface.spec.mjs` holds the band and the menu at exact
// parity over the `insert.` namespace; this asserts the rows are RUNNABLE there,
// which set equality alone does not say.
test("the Insert menu offers the same two field rows, enabled", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await openAppMenu(page, "insert");
  for (const id of ["insert.field.page", "insert.field.date"]) {
    const row = page.locator(`#appMenuPopover .app-menu-item[data-command="${id}"]`);
    await expect(row).toBeVisible();
    await expect(row).toBeEnabled();
  }
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});
