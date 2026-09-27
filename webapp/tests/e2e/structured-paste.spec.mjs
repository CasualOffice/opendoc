// docs/67 audit row 7 ("clipboard structure"): an internal copy of a table or a
// list must survive paste as real structure, not flatten to plain text. Rich
// run/paragraph/link paste already worked; this covers the STRUCTURED case.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  MOD,
} from "./fixtures.mjs";

// Dispatches a real clipboard event through the editor's own handlers and
// returns the payloads the handler wrote (for copy) so a later paste can replay
// them — the same helper shape the clipboard-rich suite uses.
async function clipboardEvent(page, type, data = {}) {
  return page.evaluate(
    ({ type, data }) => {
      const dt = new DataTransfer();
      for (const [mime, value] of Object.entries(data)) dt.setData(mime, value);
      const event = new ClipboardEvent(type, {
        clipboardData: dt,
        bubbles: true,
        cancelable: true,
      });
      document.dispatchEvent(event);
      return { html: dt.getData("text/html"), text: dt.getData("text/plain") };
    },
    { type, data },
  );
}

test("an internal copy of a table pastes back as a real table, not flattened text", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Insert a 2x2 table; the caret lands in its first cell.
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  // Let the editor surface take focus with the caret settled in the first cell.
  await page.waitForTimeout(200);

  // Type into the first cell and select it — a text range whose endpoints are
  // both inside the table, so the structured copy captures the whole table.
  await page.keyboard.type("CELLTEXT");
  await page.keyboard.press("Shift+Home");
  const clip = await clipboardEvent(page, "copy");
  // The internal marker now carries a structured block fragment (a table), not
  // only flat runs.
  expect(clip.html).toMatch(/^<!--opendoc-clipboard-runs:/);
  const decoded = Buffer.from(
    clip.html.match(/runs:([A-Za-z0-9+/=]+)-->/)[1],
    "base64",
  ).toString("utf8");
  expect(decoded).toContain('"type":"table"');

  // Move the caret to a body paragraph (document start) and paste the table.
  await moveCaretToDocStart(page);
  await expect(page.locator("#tabTable")).toBeDisabled(); // caret is in prose now
  await clipboardEvent(page, "paste", { "text/html": clip.html, "text/plain": clip.text });

  // The paste reconstructed a real table: the caret landed inside it, so the
  // contextual Table ribbon activates. A flattened text paste would leave the
  // caret in a paragraph with the Table tab disabled.
  await expect(page.locator("#tabTable")).toBeEnabled();

  // The cell content survived and now exists twice (original + pasted).
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill("CELLTEXT");
  await expect(page.locator("#findStatus")).toHaveText(/ of 2$/);
  await page.keyboard.press("Escape");

  // One undoable action removes the pasted table (Table tab disables again).
  // Use the keyboard: the caret is inside the pasted table, so the ribbon shows
  // its contextual Table tab and the Home tab's Undo button is not visible.
  await page.keyboard.press(`${MOD}+z`);
  await expect(page.locator("#tabTable")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});

test("an internal copy of a numbered list pastes back as a list, preserving numbering", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Build three clean paragraphs at the top: two list items and a prose
  // destination below them, none entangled with the fixture's first heading.
  await page.keyboard.press("Enter");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.type("ITEMALPHA");
  await page.keyboard.press("Enter");
  await page.keyboard.type("ITEMBETA");
  await page.keyboard.press("Enter");
  await page.keyboard.type("PROSEDEST");

  // Select the two items (end of BETA up to start of ALPHA) and number them.
  await moveCaretToDocStart(page);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("End");
  await page.keyboard.press("Shift+Home");
  await page.keyboard.press("Shift+ArrowUp");
  await page.keyboard.press("Shift+Home");
  await page.locator("#numberedList").click();
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");
  // Let the list-formatting edit commit and restore the range before copying.
  await page.waitForTimeout(200);

  const clip = await clipboardEvent(page, "copy");
  const decoded = Buffer.from(
    clip.html.match(/runs:([A-Za-z0-9+/=]+)-->/)[1],
    "base64",
  ).toString("utf8");
  expect(decoded).toContain('"numbering"');

  // Collapse to the prose destination (third paragraph) and confirm it is not a
  // list, then paste the copied list there.
  await moveCaretToDocStart(page);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Home");
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "false");
  await clipboardEvent(page, "paste", { "text/html": clip.html, "text/plain": clip.text });

  // The caret landed in the first pasted list item: numbering survived (a
  // flattened paste would drop into plain prose with the button unpressed).
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");

  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill("ITEMALPHA");
  await expect(page.locator("#findStatus")).toHaveText(/ of 2$/);
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

// Dispatches a paste carrying a freshly-encoded PNG file, the way the OS
// clipboard yields a copied image. The same helper `insert-image.spec.mjs` uses.
async function pasteGeneratedImage(page) {
  await page.evaluate(async () => {
    const canvas = new OffscreenCanvas(6, 4);
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = "#cc3322";
    ctx.fillRect(0, 0, 6, 4);
    const blob = await canvas.convertToBlob({ type: "image/png" });
    const file = new File([blob], "red.png", { type: "image/png" });
    const dt = new DataTransfer();
    dt.items.add(file);
    document.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }),
    );
  });
}

// The silent-loss defect, in the browser (`docs/128` §2). A logo in a table cell
// is the ordinary shape of a letterhead, and the structured clipboard carries the
// picture VERBATIM — `copyStructured` serialises the selected body blocks with no
// inline filter at all. The paste then rebuilt each paragraph through a four-arm
// `sanitize_inlines` with a `_ => {}`, so 25 of `InlineNode`'s 29 kinds were
// discarded with nothing refused and nothing reported.
//
// The picture is looked for by COPYING THE PASTED TABLE BACK OUT, through the
// editor's own clipboard surface, and checking that the payload carries a drawing
// whose node id is not the original's. That asserts both halves of the fix — the
// picture arrived, and it arrived with a fresh identity rather than a second
// reference to the node it was copied from — against the model the renderer draws
// rather than a host flag reading back what was written.
//
// `#a11yDocument` is deliberately NOT the instrument: the accessibility projection
// renders a table as its rows of text, so a picture inside a cell is not an `<img>`
// in the mirror at all. That is a real gap and a tracker row of its own, not
// something to paper over by asserting the wrong thing here.
//
// Mutation: restore the `_ => {}` arm in `clone_inlines`
// (`crates/casual-doc-edit/src/clone.rs`) and rebuild — the re-copied payload
// carries no drawing at all.
test("an internal copy of a table carries the picture in its cell", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  await page.waitForTimeout(200);

  // A logo in the first cell, then text after it so the copy range has an
  // endpoint inside the same cell.
  await pasteGeneratedImage(page);
  await expect(page.locator("#status")).toContainText("Picture inserted");

  await page.keyboard.type("LOGOCELL");
  await page.keyboard.press("Shift+Home");
  const clip = await clipboardEvent(page, "copy");
  const decoded = Buffer.from(
    clip.html.match(/runs:([A-Za-z0-9+/=]+)-->/)[1],
    "base64",
  ).toString("utf8");
  // The CLIPBOARD carries the drawing, so any loss from here on is the paste's.
  expect(decoded).toContain('"type":"drawing"');
  const drawingIdIn = (payload) => {
    const cell = JSON.parse(payload).blocks[0].rows[0].cells[0].blocks[0];
    return cell.inlines.find((inline) => inline.type === "drawing")?.id;
  };
  const sourceDrawing = drawingIdIn(decoded);

  await moveCaretToDocStart(page);
  await expect(page.locator("#tabTable")).toBeDisabled();
  await clipboardEvent(page, "paste", { "text/html": clip.html, "text/plain": clip.text });

  // A real table arrived (the contextual Table tab activates) and the caret is in
  // it, so copying from here copies the PASTED table rather than the original.
  await expect(page.locator("#tabTable")).toBeEnabled();
  await page.keyboard.press("Shift+End");
  const again = await clipboardEvent(page, "copy");
  const pasted = Buffer.from(
    again.html.match(/runs:([A-Za-z0-9+/=]+)-->/)[1],
    "base64",
  ).toString("utf8");
  expect(pasted).toContain('"type":"drawing"');
  // Carried, and re-minted: a shared id would be one node in two places.
  expect(drawingIdIn(pasted)).not.toBe(sourceDrawing);

  // A paste that carried everything says nothing about loss.
  await expect(page.locator("#status")).not.toContainText("cannot be duplicated");

  expect(consoleErrors).toEqual([]);
});

// The other half of the rule: what genuinely cannot come across is REPORTED.
// A comment anchor is unique per comment — one comment has one anchored range —
// so a second copy of it is a defect rather than a copy. The engine drops the
// three comment markers and names the family; the host has to say so, because a
// paste that quietly loses a reviewer's comment anchor is the silent loss
// `AGENTS.md` forbids.
//
// Mutation: delete the `if (loss) setStatus(loss, "error")` line from
// `runStructuredPaste` in `main.js` — the status line then reads "Copied 8
// characters" and the user is never told.
test("a structured paste that cannot carry a comment anchor says so", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  await page.waitForTimeout(200);

  // Comment the cell text, so the copied table carries the comment's range
  // markers and its reference.
  await page.keyboard.type("REMARKED");
  await page.keyboard.press("Shift+Home");
  await page.locator('[data-tab="review"]').click();
  await expect(page.locator("#reviewCommentBtn")).toBeEnabled();
  await page.locator("#reviewCommentBtn").click();
  const composer = page.locator("#reviewSidebar textarea, .review-composer textarea").first();
  await expect(composer).toBeVisible();
  await composer.fill("a remark");
  await page
    .locator("#reviewSidebar button", { hasText: /Comment|Add|Post/ })
    .first()
    .click();
  await expect(page.locator(".review-comment-marker")).toHaveCount(1);
  await page.locator('[data-tab="insert"]').click();

  // Re-select the commented text and copy the table.
  await page.keyboard.press("Shift+Home");
  const clip = await clipboardEvent(page, "copy");
  const decoded = Buffer.from(
    clip.html.match(/runs:([A-Za-z0-9+/=]+)-->/)[1],
    "base64",
  ).toString("utf8");
  expect(decoded).toContain("comment_range_start");

  await moveCaretToDocStart(page);
  await clipboardEvent(page, "paste", { "text/html": clip.html, "text/plain": clip.text });

  // The table came across, and the loss was reported rather than swallowed —
  // through the status channel, which also means the assertive region and the
  // toast (`status_policy.mjs`: only `"error"` escalates).
  await expect(page.locator("#tabTable")).toBeEnabled();
  await expect(page.locator("#status")).toContainText("comments");
  await expect(page.locator("#status")).toContainText("cannot be duplicated");
});
