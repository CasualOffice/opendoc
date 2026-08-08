// The Insert surface: the ribbon Insert tab, the Insert app menu, and the
// command palette must expose the SAME command set, and every one of them must
// work on a document the user has not clicked into yet.
//
// Both halves of this spec exist because both halves shipped broken. Picture,
// Symbol, Emoji, Bookmark and Field landed with an engine op, an app-menu row
// and a passing e2e spec, but the ribbon Insert tab — the surface a user
// actually looks at — kept showing only Table and Link, so there was no visible
// way to insert a picture at all. And every Insert command was gated on
// `selection`, which was only assigned by a click, so a freshly loaded document
// could not be inserted into from ANY surface. The existing insert specs missed
// it by construction: each one calls `clickIntoFirstPage` before exercising the
// feature, so none of them ever asked what a user sees on load.
//
// Accordingly, no test in this file clicks into the page. `gotoEditor` alone is
// the precondition, exactly as it is for a user who has just opened a file.
import { test, expect, gotoEditor, setReviewMode } from "./fixtures.mjs";

// A 1×1 PNG — the smallest thing `createImageBitmap` will decode, so the test
// exercises the real file-picker → decode → insert path without shipping a
// binary fixture.
const ONE_PIXEL_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64",
);

// The rich corpus opens on a level-1 heading, so the off-screen accessibility
// tree's h1 is a model-derived (canvas-independent) read of the first thing in
// the body — which is where the insertion point sits before anyone clicks.
const BODY_START_HEADING = "Rich Document";

function bodyStartHeading(page) {
  return page.locator("#a11yDocument").getByRole("heading", { level: 1 }).first();
}

// The collapsed caret's rounded on-screen position, read from the overlay the
// editor draws from engine geometry (the page itself is an opaque canvas).
function caretPoint(page) {
  return page.evaluate(() => {
    const caret = document.querySelector(".overlay .caret");
    if (!caret) return null;
    return {
      left: Math.round(Number.parseFloat(caret.style.left)),
      top: Math.round(Number.parseFloat(caret.style.top)),
    };
  });
}

async function openInsertTab(page) {
  await page.locator("#tabInsert").click();
  await expect(page.locator("#panelInsert")).toBeVisible();
}

test("the Insert ribbon exposes every Insert command, in Word's group order", async ({
  page,
  consoleErrors,
}) => {
  // Pinned wide: `updateRibbonOverflow` relocates non-pinned `.rgroup`s into
  // #ribbonOverflowMenu at narrow widths, so an unpinned viewport would drop
  // groups from the roster below and fail for a reason that is not a regression.
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);
  await openInsertTab(page);

  // The exact roster, in order. Deleting a button — the regression this spec
  // exists to catch — or quietly reordering the tab fails here.
  const buttonIds = await page
    .locator("#panelInsert .rgroup button")
    .evaluateAll((buttons) => buttons.map((button) => button.id));
  expect(buttonIds).toEqual([
    "insertTableBtn",
    "insertPictureBtn",
    "insertLinkBtn",
    "insertBookmarkBtn",
    "insertFieldBtn",
    "insertSymbolBtn",
    "insertEmojiBtn",
  ]);

  // Word's Insert tab order: Tables ▸ Illustrations ▸ Links ▸ Text ▸ Symbols.
  expect(await page.locator("#panelInsert .rgroup-label").allTextContents()).toEqual([
    "Table",
    "Illustrations",
    "Links",
    "Text",
    "Symbols",
  ]);

  // Each control is labelled for assistive technology. The icon ligature itself
  // is deliberately not asserted: swapping a glyph changes nothing a user can
  // act on, and pinning it would fail a cosmetic refresh while catching nothing.
  const expected = [
    ["#insertTableBtn", "Insert table"],
    ["#insertPictureBtn", "Insert picture"],
    ["#insertLinkBtn", "Add or edit link"],
    ["#insertBookmarkBtn", "Bookmark"],
    ["#insertFieldBtn", "Insert field"],
    ["#insertSymbolBtn", "Insert symbol"],
    ["#insertEmojiBtn", "Insert emoji"],
  ];
  for (const [selector, label] of expected) {
    await expect(page.locator(selector)).toBeVisible();
    await expect(page.locator(selector)).toHaveAttribute("aria-label", label);
  }

  // The ribbon teaches the shortcut the palette already lists.
  await expect(page.locator("#insertLinkBtn")).toHaveAttribute("title", /⌘K/);

  expect(consoleErrors).toEqual([]);
});

test("every Insert ribbon button is live on a freshly loaded document — only Link, which needs text, is not", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await openInsertTab(page);

  // Word and Google Docs never ask you to click into the page before Insert ▸
  // Picture / Symbol / Emoji / Bookmark / Field / Table. Re-adding the old
  // `enabled: !!selection` gate turns every one of these red.
  for (const selector of [
    "#insertTableBtn",
    "#insertPictureBtn",
    "#insertBookmarkBtn",
    "#insertFieldBtn",
    "#insertSymbolBtn",
    "#insertEmojiBtn",
  ]) {
    await expect(page.locator(selector)).toBeEnabled();
  }

  // Link is the one Insert command with a real precondition: it hyperlinks
  // selected text, so with nothing selected it stays disabled — that guard must
  // survive the un-gating of its neighbours.
  await expect(page.locator("#insertLinkBtn")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});

test("the Insert menu and the command palette agree with the ribbon on a freshly loaded document", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // The app menu: same commands, same availability — no "Place the caret…"
  // reason on anything that does not, in fact, need a caret.
  await page.locator('.app-menu-button[data-menu="insert"]').click();
  for (const id of [
    "insert.table",
    "insert.image",
    "insert.bookmark",
    "insert.field",
    "insert.symbol",
    "insert.emoji",
  ]) {
    const item = page.locator(`#appMenuPopover .app-menu-item[data-command="${id}"]`);
    await expect(item).toBeEnabled();
    await expect(item).not.toHaveAttribute("title", /place the caret/i);
  }
  const linkItem = page.locator('#appMenuPopover .app-menu-item[data-command="insert.link"]');
  await expect(linkItem).toBeDisabled();
  await expect(linkItem).toHaveAttribute("title", "Select text to add a link");
  await page.keyboard.press("Escape");

  // The palette: the same commands are runnable, and their hint column no
  // longer teaches a precondition that does not exist.
  await page.locator("#searchTrigger").click();
  await expect(page.locator("#cmdPalette")).toBeVisible();
  for (const label of ["Picture…", "Symbol…", "Emoji…", "Field…", "Bookmark…"]) {
    await page.locator("#cmdInput").fill(label.replace("…", ""));
    const item = page.locator(".cmd-item", { hasText: label }).first();
    await expect(item).toBeVisible();
    await expect(item).toBeEnabled();
    await expect(item.locator(".cmd-hint")).not.toHaveText(/place the caret/i);
  }
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("Insert ▸ Picture inserts a picture from the ribbon with no prior click", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await openInsertTab(page);
  await expect(page.locator("#undoBtn")).toBeDisabled(); // nothing done yet

  const chooser = page.waitForEvent("filechooser");
  await page.locator("#insertPictureBtn").click();
  await (await chooser).setFiles({
    name: "red.png",
    mimeType: "image/png",
    buffer: ONE_PIXEL_PNG,
  });

  await expect(page.locator("#status")).toContainText("Picture inserted");
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", /Insert image/i);

  expect(consoleErrors).toEqual([]);
});

test("Insert ▸ Symbol inserts at the start of the body when no caret was placed", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await openInsertTab(page);
  await expect(bodyStartHeading(page)).toHaveText(BODY_START_HEADING);

  await page.locator("#insertSymbolBtn").click();
  await expect(page.locator("#symbolDialog")).toBeVisible();
  await page.locator('#symbolGrid .glyph-cell[data-glyph="€"]').click();

  // The insertion point on an unclicked document is the start of the body, so
  // the glyph lands ahead of the first heading's text — read from the model-
  // derived accessibility tree, not from the canvas.
  await expect(bodyStartHeading(page)).toHaveText(`€${BODY_START_HEADING}`);
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Paste");

  expect(consoleErrors).toEqual([]);
});

test("Insert ▸ Emoji inserts at the start of the body when no caret was placed", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await openInsertTab(page);

  await page.locator("#insertEmojiBtn").click();
  await expect(page.locator("#emojiDialog")).toBeVisible();
  await page.locator('#emojiGrid .glyph-cell[data-glyph="😀"]').click();

  await expect(bodyStartHeading(page)).toHaveText(`😀${BODY_START_HEADING}`);
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Paste");

  expect(consoleErrors).toEqual([]);
});

test("Insert ▸ Field inserts at the start of the body when no caret was placed", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // A field is a model node, not run text, so it does not appear in the
  // accessibility projection the symbol/emoji tests read — the caret is where
  // its position shows. Read the load-time insertion point first: focusing the
  // surface only makes the existing caret visible, it never moves it (no click,
  // no arrow key), so this is a measurement of where the document already sits.
  await page.locator("#pages").focus();
  const bodyStart = await caretPoint(page);

  await openInsertTab(page);
  await page.locator("#insertFieldBtn").click();
  await expect(page.locator("#fieldDialog")).toBeVisible();
  await page.locator('.field-choice[data-field-kind="author"]').click();
  await expect(page.locator("#fieldDialog")).toBeHidden();
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Field change");
  await expect(page.locator("#status")).toContainText("Inserted Author");

  // The insert leaves the caret immediately after the field, so a field placed
  // at the insertion point puts the caret on the document's very first line and
  // further right than that line started. Both coordinates come from the caret
  // overlay the editor draws from engine geometry, so this is layout-derived,
  // not a canvas read.
  const afterInsert = await caretPoint(page);
  expect(afterInsert.top).toBe(bodyStart.top);
  expect(afterInsert.left).toBeGreaterThan(bodyStart.left);

  expect(consoleErrors).toEqual([]);
});

test("Insert ▸ Table drops a table with no prior click", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await openInsertTab(page);
  // The document renders to a canvas, so the proof a table exists is that the
  // caret is now inside one: the contextual Table tab only enables on inTable().
  await expect(page.locator("#tabTable")).toBeDisabled();

  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();

  await expect(page.locator("#tabTable")).toBeEnabled();
  await expect(page.locator("#undoBtn")).toBeEnabled();

  expect(consoleErrors).toEqual([]);
});

test("Insert ▸ Bookmark opens the bookmark manager with no prior click", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await openInsertTab(page);

  await page.locator("#insertBookmarkBtn").click();
  await expect(page.locator("#bookmarkDialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator("#bookmarkDialog")).toBeHidden();

  expect(consoleErrors).toEqual([]);
});

test("the load-time insertion point exists but paints no caret until the editor surface is focused", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // The insertion point is live (the Insert tab proves it — see above), but the
  // caret is not painted: `#pages` does not have focus yet, so the editor would
  // discard typing, and a blinking cursor there would be a lie.
  await expect(page.locator(".overlay .caret")).toHaveCount(0);

  // Focusing the surface — by keyboard here, no click — makes it the user's
  // caret and paints it.
  await page.locator("#pages").focus();
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  // And it is a real caret: typing goes in at the start of the body.
  await page.keyboard.type("Z");
  await expect(bodyStartHeading(page)).toHaveText(`Z${BODY_START_HEADING}`);

  expect(consoleErrors).toEqual([]);
});

// The drift guard. `INSERT_SURFACE` unifies enablement and activation, but the
// ribbon buttons are authored in editor.html while the menu roster is its own id
// list, so nothing in the code stops a new command reaching one surface and
// missing the other — which is exactly how Picture shipped with no ribbon
// button. Comparing the two rosters through the DOM is what makes that omission
// fail CI: each ribbon button carries the command id it runs.
test("the Insert ribbon's command set is exactly the Insert menu's", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);

  await page.locator('.app-menu-button[data-menu="insert"]').click();
  const menuCommands = await page
    .locator("#appMenuPopover .app-menu-item[data-command]")
    .evaluateAll((items) =>
      items.map((item) => item.dataset.command).filter((id) => id.startsWith("insert.")),
    );
  await page.keyboard.press("Escape");

  await openInsertTab(page);
  const ribbonCommands = await page
    .locator("#panelInsert .rgroup button[data-command]")
    .evaluateAll((buttons) => buttons.map((button) => button.dataset.command));

  expect(menuCommands.length).toBeGreaterThan(0);
  expect([...ribbonCommands].sort()).toEqual([...menuCommands].sort());

  expect(consoleErrors).toEqual([]);
});

// The guards must hold on the path this change created — a ribbon insert with no
// prior click. Every pre-existing read-only spec calls `clickIntoFirstPage`
// first, so none of them covers this state.
test("Viewing mode refuses every ribbon insert on a freshly loaded document", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);
  await setReviewMode(page, "viewing");
  await openInsertTab(page);

  // Each dialog-opening insert refuses BEFORE opening, so the user is never led
  // into picking something that cannot be applied.
  for (const [button, dialog] of [
    ["#insertSymbolBtn", "#symbolDialog"],
    ["#insertEmojiBtn", "#emojiDialog"],
    ["#insertFieldBtn", "#fieldDialog"],
  ]) {
    await page.locator(button).click();
    await expect(page.locator(dialog)).toBeHidden();
    await expect(page.locator("#status")).toContainText("read-only");
  }

  // Nothing entered history: a refused insert is not an edit.
  await expect(page.locator("#undoBtn")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});

// The insertion point must be honest in BOTH directions. Seeding it at open
// while `#pages` already holds focus would leave the surface focused, typing
// accepted, and no caret painted — the user typing blind. Dropping a file onto
// an editor that has been clicked into reaches exactly that state, because HTML5
// drag never moves focus.
test("opening a document into an already-focused surface paints its caret immediately", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await page.locator("#pages").focus();
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  // Re-open a document while #pages still holds focus. `setInputFiles` drives the
  // real open path without moving focus, which is what a file drop also does.
  const status = page.locator("#status");
  await page.locator("#file").setInputFiles("demo.docx");
  // The open path names the file while it works, then clears the strip; waiting
  // for both edges proves the reopen actually completed before we measure.
  await expect(status).toContainText("demo.docx");
  await expect(status).not.toContainText("demo.docx");

  // Focused surface ⇒ the caret is real and painted, not implicit.
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
  await page.keyboard.type("Q");
  await expect(bodyStartHeading(page)).toHaveText(`Q${BODY_START_HEADING}`);

  expect(consoleErrors).toEqual([]);
});
