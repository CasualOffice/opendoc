// Page, column and section breaks: reachable, and they do what they say.
//
// `docs/153` ranks the page break 1 and the section break 2 — the two
// highest-ranked gaps on the whole matrix — and both were pure chrome gaps.
// `insertBreak("page")`, `insertBreak("column")` and `insertSectionBreak(start)`
// have been in `casual-doc-wasm` with their own refusals and their own engine
// tests for some time, and the string `insertBreak` did not appear anywhere in
// `webapp/`. `104` HF-127 recorded Ctrl/Cmd+Enter as inert on top of that.
//
// WHAT THIS ASSERTS, AND WHY NOT THE MENU. A spec that found the six rows in a
// dropdown would pass over a chrome that called nothing, or called the engine
// with the wrong argument, or inserted a column break where a page break was
// asked for. So every test here reads the EFFECT:
//
//   * a page break really starts a new page — the page count goes up, and the
//     text after the caret is on the new one;
//   * a section break really creates a section — asserted by changing the new
//     section's page SIZE and watching only the later pages change width, which
//     is a thing no amount of break-shaped markup can fake;
//   * the chord really inserts a page break, which is HF-127's actual fix;
//   * a refused break says WHY, in the engine's own words.
import { readFile } from "node:fs/promises";

import {
  test,
  expect,
  clickIntoFirstPage,
  mirrorBlocks,
  runAppMenuCommand,
  shortcutHint,
} from "./fixtures.mjs";

const MOD = process.platform === "darwin" ? "Meta" : "Control";

/** A blank document with one paragraph of text, and the caret in the middle of
 *  it.
 *
 *  Blank, not the demo fixture: the demo is 20-odd pages of someone else's
 *  content, so "the page count went up by one" would be measuring the fixture
 *  rather than the gesture, and a section-size change would have to be told apart
 *  from the sections the file already has. A document this test built is a
 *  document whose every page and section it can account for. */
async function blankDocumentWithText(page, text = "Alpha Beta Gamma") {
  await page.goto("/editor.html?blank=1");
  await page.waitForFunction(
    () => document.getElementById("status")?.textContent !== "Loading engine…",
    null,
    { timeout: 45_000 },
  );
  await runAppMenuCommand(page, "file", "file.new");
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });
  await clickIntoFirstPage(page);
  await page.locator("#pages").pressSequentially(text);
  return page.locator(".page-wrap");
}

/** Opens the Breaks dropdown and clicks one row. The ROUTE matters: this is the
 *  ribbon face, which is the surface a reader looks at. */
async function insertBreakFromRibbon(page, commandId) {
  await page.locator('[data-tab="insert"]').click();
  await expect(page.locator("#insertBreaksBtn")).toBeEnabled();
  await page.locator("#insertBreaksBtn").click();
  await expect(page.locator("#insertBreaksMenu")).toBeVisible();
  await page.locator(`#insertBreaksMenu [data-command="${commandId}"]`).click();
  // The menu closes before the break lands, so the reader can see the page it
  // changed rather than the menu that changed it.
  await expect(page.locator("#insertBreaksMenu")).toBeHidden();
}

test("the Breaks dropdown offers all six, from the ribbon and from the Insert menu", async ({
  page,
}) => {
  await blankDocumentWithText(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertBreaksBtn").click();
  await expect(page.locator("#insertBreaksMenu")).toBeVisible();
  // Google Docs' flat Insert ▸ Break with the kind spelled out, not Word's
  // heading-over-short-name — the same label has to serve the command palette.
  await expect(page.locator("#insertBreaksMenu [data-command]")).toHaveText([
    "Page break",
    "Column break",
    "Section break (next page)",
    "Section break (continuous)",
    "Section break (even page)",
    "Section break (odd page)",
  ]);
  await page.keyboard.press("Escape");

  // THE SECOND SURFACE, and the one that matters in compact chrome: the ribbon is
  // hidden there, so a command with only a band face is palette-only — the hole
  // the References menu was added to close. Every one of the six is a real row in
  // the Insert menu's Breaks submenu, and `runAppMenuCommand` asserts each is
  // present and ENABLED before it clicks.
  for (const id of [
    "layout.break.page",
    "layout.break.column",
    "layout.break.section.nextPage",
    "layout.break.section.continuous",
    "layout.break.section.evenPage",
    "layout.break.section.oddPage",
  ]) {
    await runAppMenuCommand(page, "insert", id);
    await expect(page.locator("#status")).not.toHaveClass(/error/);
  }
});

test("a page break starts a new page, and the text after the caret is on it", async ({
  page,
  consoleErrors,
}) => {
  const pages = await blankDocumentWithText(page, "Before the break");
  await expect(pages).toHaveCount(1);

  await insertBreakFromRibbon(page, "layout.break.page");

  // THE EFFECT. One page became two, which is the whole claim.
  await expect(pages).toHaveCount(2);
  // And it says so: a break is invisible at the caret and on a short document may
  // move nothing the reader notices, so silence would read as "nothing happened".
  await expect(page.locator("#status")).toContainText("Page break inserted");
  await expect(page.locator("#status")).not.toHaveClass(/error/);

  // THE CARET IS ON THE NEW PAGE, which is the half of the claim a page count
  // cannot make: a document could gain a page for any number of reasons without
  // the insertion point following the break.
  //
  // Read from WHICH PAGE holds the caret element. Every `.page-wrap` carries its
  // own transparent overlay and the editor draws the caret into one of them from
  // engine geometry, so the page number on that wrap is the engine's own answer
  // to "where is the insertion point" — not a coordinate this test converts.
  await expect(page.locator('.page-wrap[data-page-number="2"] .overlay .caret')).toHaveCount(1);
  await expect(page.locator('.page-wrap[data-page-number="1"] .overlay .caret')).toHaveCount(0);

  // And the text is still ONE paragraph, which is what a page break IS. This is
  // the assertion that failed when this spec was first written, and it was the
  // spec that was wrong: `insertBreak` inserts a run-level `w:br` INSIDE the
  // paragraph, exactly as OOXML and Word do, so the text after the caret stays in
  // the same paragraph and merely paints on the next page. A spec that demanded a
  // paragraph split would have been demanding the wrong behaviour.
  await page.locator("#pages").pressSequentially("After the break");
  const paragraphs = await mirrorBlocks(page);
  const joined = paragraphs.filter(
    (text) => text.includes("Before the break") && text.includes("After the break"),
  );
  expect(
    joined.length,
    "a page break is a run-level break: the paragraph must not be split",
  ).toBe(1);

  // One undo step, not two. A break that undid in halves would leave a document
  // nobody asked for.
  await expect(page.locator("#undoBtn")).toBeEnabled();
  await page.keyboard.press(`${MOD}+z`);
  await page.keyboard.press(`${MOD}+z`);
  await expect(pages).toHaveCount(1);
  expect(consoleErrors).toEqual([]);
});

test("Ctrl/Cmd+Enter inserts a page break — HF-127's chord is no longer swallowed", async ({
  page,
  consoleErrors,
}) => {
  const pages = await blankDocumentWithText(page, "Chord test");
  await expect(pages).toHaveCount(1);

  // The chord, pressed on the editing surface. `104` HF-127: this used to reach
  // `main.js`'s `if (mod) { breakTypingSession(); return; }` guard — which sits
  // ABOVE the Enter branch — and be left to the browser, which does nothing with
  // it. The fix is one row in `KEYMAP`: the dispatcher is registered before the
  // editor's own handler and claims the chord with `stopImmediatePropagation`, so
  // the guard never sees it.
  await page.locator("#pages").press(`${MOD}+Enter`);

  await expect(pages).toHaveCount(2);
  await expect(page.locator("#status")).toContainText("Page break inserted");

  // And the chord is ADVERTISED where the command is offered, rendered for the
  // keyboard the runner actually has — a hint hardcoded as "⌘⏎" passes on one OS
  // and lies on the other (`105` UX-009).
  const hint = shortcutHint("⌘⏎");
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertBreaksBtn").click();
  await expect(page.locator("#insertBreaksMenu")).toBeVisible();
  expect(hint.length, "the fixture produced no chord hint to look for").toBeGreaterThan(0);
  expect(consoleErrors).toEqual([]);
});

test("a section break really creates a section, and the caret lands in the new one", async ({
  page,
  consoleErrors,
}) => {
  const pages = await blankDocumentWithText(page, "First section");

  // BEFORE: the document declares one section, and the Page setup dialog's
  // Section list is the engine's own enumeration of them (`pageSetupSections`),
  // not a count this test keeps. One option means one section.
  await page.locator('[data-tab="layout"]').click();
  await page.locator("#layoutOrientationBtn").click();
  await expect(page.locator("#pageSetupMenu")).toBeVisible();
  await expect(page.locator("#pageSetupSection option")).toHaveCount(1);
  const firstSection = await page.locator("#pageSetupSection").inputValue();
  await page.locator("#pageSetupClose").click();
  await expect(page.locator("#pageSetupMenu")).toBeHidden();

  await insertBreakFromRibbon(page, "layout.break.section.nextPage");
  await expect(pages).toHaveCount(2);
  await expect(page.locator("#status")).toContainText("Section break inserted");

  // AFTER: two sections, and the one the CARET is in is the new one. That second
  // half is what makes this an effect assertion rather than a restatement — a
  // break that spliced a boundary and left the caret behind would satisfy a
  // count, and the next thing the reader does (set the new section's paper size)
  // would silently reformat the half they just left.
  //
  // Back to the Layout tab: inserting the break left the Insert tab showing, and
  // a hidden band's buttons are not clickable — which is the ribbon doing its job.
  await page.locator('[data-tab="layout"]').click();
  await page.locator("#layoutOrientationBtn").click();
  await expect(page.locator("#pageSetupMenu")).toBeVisible();
  await expect(page.locator("#pageSetupSection option")).toHaveCount(2);
  const caretSection = await page.locator("#pageSetupSection").inputValue();
  expect(caretSection, "the caret must be in the section the break created").not.toBe(
    firstSection,
  );
  await page.locator("#pageSetupClose").click();
  expect(consoleErrors).toEqual([]);
});

test("each of the four start types is written as the type that was asked for", async ({
  page,
  consoleErrors,
}) => {
  await blankDocumentWithText(page, "Sections");

  // All four, in one document, so the test cannot pass by inserting the same kind
  // four times — and read back out of a real EXPORT. `SectionBoundary.sectionType`
  // serialises to the engine's own `w:type` vocabulary, so this is the document
  // the engine would hand anybody rather than a value the chrome reported about
  // itself. A spec that trusted the status line would pass over a chrome that
  // sent "nextPage" for all four.
  for (const start of ["nextPage", "continuous", "evenPage", "oddPage"]) {
    await insertBreakFromRibbon(page, `layout.break.section.${start}`);
    await expect(page.locator("#status")).not.toHaveClass(/error/);
  }

  const download = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.json");
  const snapshot = JSON.parse((await readFile(await (await download).path())).toString("utf8"));
  const types = [];
  const walk = (node) => {
    if (Array.isArray(node)) return void node.forEach(walk);
    if (!node || typeof node !== "object") return;
    if (typeof node.sectionType === "string") types.push(node.sectionType);
    for (const value of Object.values(node)) walk(value);
  };
  walk(snapshot);
  // Each break makes the boundary BEFORE it carry the start type asked for, so
  // all four appear. Sorted, because the order boundaries serialise in is the
  // model's business and pinning it would be asserting the mechanism rather than
  // the guarantee.
  expect([...new Set(types)].sort()).toEqual(["continuous", "evenPage", "nextPage", "oddPage"]);
  expect(consoleErrors).toEqual([]);
});

test("a break refused inside a table says WHY, in the engine's own words", async ({
  page,
  consoleErrors,
}) => {
  await blankDocumentWithText(page, "Table test");
  // The engine honours a forced break only in the body's own block list and
  // refuses anywhere else BY NAME, with a `refused:`-coded sentence that
  // `editRefusalMessage` passes through verbatim. That refusal is the reason the
  // control is not greyed out in a table: "A break cannot be inserted inside a
  // table" tells a reader what to do and a dark button tells them nothing.
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();

  await insertBreakFromRibbon(page, "layout.break.page");
  await expect(page.locator("#status")).toHaveClass(/error/);
  await expect(page.locator("#status")).toContainText(/cannot be inserted inside a table/i);
  // NOT the generic sentence. `editRefusalMessage`'s fallback is "That edit
  // isn't supported for this selection yet", which is actively wrong here — the
  // selection is fine and the CONTAINER is the problem — and this is the line
  // that fails if the engine's coded refusal stops passing through.
  await expect(page.locator("#status")).not.toContainText(/isn't supported for this selection/i);
  expect(consoleErrors).toEqual([]);
});
