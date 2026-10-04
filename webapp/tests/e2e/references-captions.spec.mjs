// References ▸ Insert caption and Cross-reference, end to end (OO-005).
//
// What this spec is for is the half of the feature that is not in
// `cross_reference_model.test.mjs`: the node tests prove the RULES — which
// options a reference type offers, what number a caption takes, what the preview
// reads as — and prove nothing at all about whether a user can reach any of it.
// This is the reachability and the round trip: the dialogs open from the ribbon,
// the caption lands in the document with the right number, a second one numbers
// 2, a cross-reference to it inserts, one Undo reverses each, Escape inserts
// nothing, and Viewing mode refuses out loud rather than silently.
//
// Read through the accessibility mirror rather than the canvas: the canvas is
// pixels, and the mirror is the same model text a screen reader is given, so an
// assertion on it is an assertion about the DOCUMENT. That is also the stronger
// claim — a caption that renders and is not in the mirror is a caption a screen
// reader cannot read.
import {
  MOD,
  clickIntoFirstPage,
  expect,
  gotoEditor,
  mirrorBlocks,
  runPaletteCommand,
  setReviewMode,
  stableBox,
  test,
} from "./fixtures.mjs";

/** The References band, with its panel shown. */
async function openReferences(page) {
  await page.locator('[data-tab="references"]').click();
  await expect(page.locator("#panelReferences")).toBeVisible();
}

/** Opens Insert caption from the ribbon. */
async function openCaption(page) {
  await openReferences(page);
  await page.locator("#refCaptionBtn").click();
  await expect(page.locator("#captionDialog")).toBeVisible();
}

/** Fills the caption text and inserts, returning the preview it showed. */
async function insertCaption(page, text) {
  await page.locator("#captionText").fill(text);
  const preview = await page.locator("#captionPreview").textContent();
  await page.locator("#captionInsert").click();
  await expect(page.locator("#captionDialog")).toBeHidden();
  return preview;
}

/** Every mirror block that contains `needle`. */
const blocksWith = (blocks, needle) => blocks.filter((text) => text.includes(needle));

/** Waits until exactly `count` mirror blocks contain `needle`.
 *
 *  POLLED, not read once: the accessibility mirror is rebuilt after the repaint
 *  that follows an edit, so a single read races the edit it is checking. That is
 *  not hypothetical — one-shot reads made the undo assertions here fail about one
 *  run in three while the product was behaving correctly, which is exactly the
 *  "wait on an observable state change" rule in SKILL.md §6. */
async function expectBlockCount(page, needle, count) {
  await expect
    .poll(async () => blocksWith(await mirrorBlocks(page), needle).length, {
      message: `mirror blocks containing ${JSON.stringify(needle)}`,
    })
    .toBe(count);
}

/** Waits until some mirror block matches `pattern` — or, with `present: false`,
 *  until none does. Same reason as above. */
async function expectSomeBlock(page, pattern, present = true) {
  await expect
    .poll(async () => (await mirrorBlocks(page)).some((text) => pattern.test(text)), {
      message: `a mirror block matching ${pattern}`,
    })
    .toBe(present);
}

test("a caption lands in the document reading exactly what the preview promised", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openCaption(page);

  // The label combo is populated from the engine, which unions Word's three
  // built-ins into whatever the document uses — so a document with no captions
  // still offers Figure, Table and Equation.
  const labels = await page.locator("#captionLabel option").allTextContents();
  expect(labels).toEqual(expect.arrayContaining(["Equation", "Figure", "Table"]));

  await page.locator("#captionLabel").selectOption("Figure");
  const preview = await insertCaption(page, ": Wiring diagram");
  // Word's dialog shows "Figure 1" in the input; ours composes it here, so the
  // preview is the only place the label and the number are visible before the
  // insert. If it disagreed with the document it would be worse than nothing.
  expect(preview).toBe("Figure 1: Wiring diagram");

  await expectBlockCount(page, "Figure 1: Wiring diagram", 1);

  expect(consoleErrors).toEqual([]);
});

test("the second caption of a label numbers 2, and ONE undo removes it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await openCaption(page);
  await page.locator("#captionLabel").selectOption("Figure");
  await insertCaption(page, ": first");

  await clickIntoFirstPage(page);
  await openCaption(page);
  await page.locator("#captionLabel").selectOption("Figure");
  // The number comes from the engine's SEQ sequence, counted over what precedes
  // the insertion point — not from "how many captions are there".
  const second = await insertCaption(page, ": second");
  expect(second).toMatch(/^Figure \d+: second$/);

  await expectBlockCount(page, ": first", 1);
  await expectBlockCount(page, ": second", 1);
  // Two captions, two distinct numbers: a renumbering bug that gave both the
  // same number would still satisfy "the text is there".
  const withBoth = await mirrorBlocks(page);
  const numbers = withBoth
    .filter((text) => /^Figure \d+: (first|second)$/.test(text.trim()))
    .map((text) => text.trim().match(/^Figure (\d+)/)[1]);
  expect(new Set(numbers).size).toBe(numbers.length);

  // ONE undo. Inserting a caption is a paragraph, a literal run, a field and a
  // renumber of the captions after it; if any of those were a separate history
  // entry the user would have to press Undo several times to get back.
  await page.keyboard.press(`${MOD}+z`);
  await expectBlockCount(page, ": second", 0);
  await expectBlockCount(page, ": first", 1);

  expect(consoleErrors).toEqual([]);
});

test("Escape closes the caption dialog and inserts nothing", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const before = await mirrorBlocks(page);

  await openCaption(page);
  await page.locator("#captionText").fill(": abandoned");
  await page.keyboard.press("Escape");
  await expect(page.locator("#captionDialog")).toBeHidden();

  expect(await mirrorBlocks(page)).toEqual(before);
  expect(consoleErrors).toEqual([]);
});

test("the caption dialog is operable from the keyboard alone", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openCaption(page);

  // Word opens on the Caption box (`CaptionDialog.js:341`), and so does this —
  // the one field the author must fill.
  await expect(page.locator("#captionText")).toBeFocused();
  // Typing updates the preview live, which is the whole reason the preview
  // exists: nothing about the composed caption is hidden until after the insert.
  await page.keyboard.type(": typed");
  await expect(page.locator("#captionPreview")).toHaveText(/: typed$/);

  // The chapter controls are DISABLED until the switch is on, as in Word
  // (`CaptionDialog.js:301-303`), and the note explains the `#` the preview
  // shows in place of a chapter number nothing can resolve yet.
  await expect(page.locator("#captionChapterLevel")).toBeDisabled();
  await expect(page.locator("#captionSeparator")).toBeDisabled();
  await expect(page.locator("#captionChapterNote")).toBeHidden();
  await page.locator("#captionIncludeChapter").check();
  await expect(page.locator("#captionChapterLevel")).toBeEnabled();
  await expect(page.locator("#captionSeparator")).toBeEnabled();
  await expect(page.locator("#captionChapterNote")).toBeVisible();
  await expect(page.locator("#captionPreview")).toHaveText(/#-\d+: typed$/);
  await page.locator("#captionSeparator").selectOption("period");
  await expect(page.locator("#captionPreview")).toHaveText(/#\.\d+: typed$/);

  // Exclude label drops the word and keeps the number, which is what Word's own
  // checkbox does — losing the number too would make the caption unreferenceable.
  await page.locator("#captionIncludeChapter").uncheck();
  await page.locator("#captionExcludeLabel").check();
  await expect(page.locator("#captionPreview")).not.toHaveText(/Figure/);
  await expect(page.locator("#captionPreview")).toHaveText(/^\d+: typed$/);

  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

test("a new label is added in the dialog and is offered immediately", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openCaption(page);

  // ONLYOFFICE opens a second modal for this (`CaptionDialog.js:212-241`); ours
  // reveals an inline row, which is one surface instead of two and keyboard-
  // reachable without a nested focus trap.
  await expect(page.locator("#captionNewLabelRow")).toBeHidden();
  await page.locator("#captionNewLabelBtn").click();
  await expect(page.locator("#captionNewLabelRow")).toBeVisible();
  await expect(page.locator("#captionNewLabelInput")).toBeFocused();
  await page.locator("#captionNewLabelInput").fill("Illustration");
  // Enter adds the LABEL rather than submitting the form: the row is inside
  // `#captionForm`, so without that the key would insert a caption the author
  // has not finished describing.
  await page.keyboard.press("Enter");
  await expect(page.locator("#captionNewLabelRow")).toBeHidden();
  await expect(page.locator("#captionDialog")).toBeVisible();

  await expect(page.locator("#captionLabel")).toHaveValue("Illustration");
  await expect(page.locator("#captionPreview")).toHaveText(/^Illustration 1/);
  // A label the author invented is deletable; Word's three built-ins are not
  // (`CaptionDialog.js:165-168`, `type: 0`).
  await expect(page.locator("#captionDeleteLabelBtn")).toBeEnabled();
  await page.locator("#captionLabel").selectOption("Figure");
  await expect(page.locator("#captionDeleteLabelBtn")).toBeDisabled();

  const preview = await insertCaption(page, ": a figure");
  expect(preview).toBe("Figure 1: a figure");
  expect(consoleErrors).toEqual([]);
});

test("a cross-reference to a caption inserts, and ONE undo removes it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openCaption(page);
  await page.locator("#captionLabel").selectOption("Figure");
  await insertCaption(page, ": Wiring diagram");

  await clickIntoFirstPage(page);
  await openReferences(page);
  await page.locator("#refCrossRefBtn").click();
  await expect(page.locator("#crossRefDialog")).toBeVisible();

  // The type list carries the engine's four kinds and then one row per caption
  // label the document can offer — a hardcoded list of three could not serve a
  // document that invented its own.
  const types = await page.locator("#crossRefType option").allTextContents();
  expect(types).toEqual(expect.arrayContaining(["Heading", "Bookmark", "Figure"]));

  await page.locator("#crossRefType").selectOption("c:Figure");
  // The "For which …" heading follows the type, as Word's does.
  await expect(page.locator("#crossRefWhich")).toHaveText("For which caption");
  // …and the options are the caption five, not the heading six.
  expect(await page.locator("#crossRefTo option").allTextContents()).toEqual([
    "Entire caption",
    "Only label and number",
    "Only caption text",
    "Page number",
    "Above/below",
  ]);

  const target = page.locator("#crossRefList .crossref-target").first();
  await expect(target).toHaveText(/Wiring diagram/);
  await target.click();
  await expect(page.locator("#crossRefInsert")).toBeEnabled();
  await page.locator("#crossRefInsert").click();

  // Insert does NOT close the dialog — Word's does not, and ONLYOFFICE's
  // `insertReference()` never calls `close()` — because a document being
  // cross-referenced usually needs several in a row.
  await expect(page.locator("#crossRefDialog")).toBeVisible();
  await page.locator("#crossRefDone").click();
  await expect(page.locator("#crossRefDialog")).toBeHidden();

  // Two blocks now carry the caption's text: the caption itself and the
  // reference to it.
  await expectBlockCount(page, "Figure 1: Wiring diagram", 2);

  await page.keyboard.press(`${MOD}+z`);
  await expectBlockCount(page, "Figure 1: Wiring diagram", 1);

  expect(consoleErrors).toEqual([]);
});

test("'Only label and number' and 'Only caption text' insert DIFFERENT text", async ({
  page,
  consoleErrors,
}) => {
  // All three caption references are a plain REF field with identical
  // instructions; what differs is the bookmark extent either side of the SEQ
  // field. An engine that bookmarked the whole caption for all three would give
  // three dialog rows that do the same thing — which is what the first engine cut
  // did — and a spec asserting only that "something was inserted" would pass.
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openCaption(page);
  await page.locator("#captionLabel").selectOption("Figure");
  await insertCaption(page, ": Wiring diagram");

  for (const [option, expected] of [
    ["labelAndNumber", "Figure 1"],
    ["captionText", ": Wiring diagram"],
  ]) {
    await clickIntoFirstPage(page);
    await openReferences(page);
    await page.locator("#refCrossRefBtn").click();
    await page.locator("#crossRefType").selectOption("c:Figure");
    await page.locator("#crossRefTo").selectOption(option);
    await page.locator("#crossRefList .crossref-target").first().click();
    await page.locator("#crossRefInsert").click();
    await page.locator("#crossRefDone").click();
    await expect(page.locator("#crossRefDialog")).toBeHidden();

    // Wait for the insert to reach the mirror before reading it, or this races
    // the repaint the way the undo assertions did.
    await expectSomeBlock(
      page,
      new RegExp(`${expected.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}.+\\S`),
    );
    const blocks = await mirrorBlocks(page);
    // The referencing block is the one that is NOT the caption paragraph.
    const referencing = blocks.filter(
      (text) => text.includes(expected) && text.trim() !== "Figure 1: Wiring diagram",
    );
    expect(
      referencing.length,
      `${option} inserted nothing readable: ${blocks.join(" | ")}`,
    ).toBeGreaterThanOrEqual(1);
    if (option === "labelAndNumber") {
      // The label-and-number reference must NOT drag the caption's sentence in.
      expect(referencing.some((text) => !text.includes("Wiring diagram"))).toBe(true);
    }
    await page.keyboard.press(`${MOD}+z`);
  }

  expect(consoleErrors).toEqual([]);
});

test("Include above/below is live where Word offers it, greyed with a reason where it does not", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openCaption(page);
  await page.locator("#captionLabel").selectOption("Figure");
  await insertCaption(page, ": Wiring diagram");

  await clickIntoFirstPage(page);
  await openReferences(page);
  await page.locator("#refCrossRefBtn").click();
  await page.locator("#crossRefType").selectOption("c:Figure");

  // ONLYOFFICE's rule, `CrossReferenceDialog.js:441`: for a caption the switch
  // applies only alongside a page number.
  await page.locator("#crossRefTo").selectOption("entireCaption");
  await expect(page.locator("#crossRefAboveBelow")).toBeDisabled();
  await expect(page.locator("#crossRefAboveBelowRow")).toHaveAttribute(
    "title",
    /does not offer above\/below/i,
  );
  await page.locator("#crossRefTo").selectOption("pageNumber");
  await expect(page.locator("#crossRefAboveBelow")).toBeEnabled();
  // A heading never offers it, whatever the reference (`:441`, `type === 1`).
  await page.locator("#crossRefType").selectOption("k:heading");
  await expect(page.locator("#crossRefAboveBelow")).toBeDisabled();

  // And checking it changes what is inserted. The DIRECTION is asserted, not
  // merely the presence of a word: a build that hard-coded one of the two would
  // satisfy "something was inserted". The caption was placed BELOW the first
  // paragraph and the caret is back in that first paragraph, so the target is
  // later in the document and the word must be "below". Writing this test
  // expecting "above" is what proved the assertion has teeth — it failed.
  await page.locator("#crossRefType").selectOption("c:Figure");
  await page.locator("#crossRefTo").selectOption("pageNumber");
  await page.locator("#crossRefAboveBelow").check();
  await page.locator("#crossRefList .crossref-target").first().click();
  await page.locator("#crossRefInsert").click();
  await page.locator("#crossRefDone").click();

  // A trailing `\b` would be wrong here, and cost a run to learn: the mirror
  // concatenates a paragraph's runs with no separator, so the reference reads
  // "1 belowRich Document" — the word is at the START of a run, not the end of it.
  await expectSomeBlock(page, /\bbelow/i);
  // …and it is not the OTHER word, which a build that emitted both would pass.
  await expectSomeBlock(page, /\babove/i, false);

  // One undo removes the reference AND its above/below field: Word writes two
  // fields and a space, and they are one action.
  await page.keyboard.press(`${MOD}+z`);
  await expectSomeBlock(page, /\bbelow/i, false);

  expect(consoleErrors).toEqual([]);
});

// ≥2 surfaces, and specifically the surfaces WORD and ONLYOFFICE put it on.
// `one-axis-navigation.spec.mjs` already refuses a palette-only command, and the
// ribbon plus the palette would satisfy it — but ONLYOFFICE puts Insert caption on
// the picture, table and equation context menus (`DocumentHolderExt.js:46`)
// because right-clicking the figure is how a reader reaches it, and a rule
// satisfied in the abstract is not the same as the affordance being there.
test("Insert caption is on the right-click menu of a table cell and of a picture", async ({
  page,
  consoleErrors,
}) => {
  // The menu is created in script and has no id; `.editor-context-menu` is what
  // every other context-menu spec addresses it by.
  const menu = page.locator(".editor-context-menu");
  const row = menu.locator('[data-command-id="reference.caption"]');

  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();

  // Keyboard route to the menu, so this is not a mouse-gated claim.
  await page.locator("#pages").focus();
  await page.waitForFunction(() => document.activeElement?.id === "editorTextInput");
  await page.keyboard.press("Shift+F10");
  await expect(row).toBeVisible();
  await row.click();
  await expect(page.locator("#captionDialog")).toBeVisible();
  await page.keyboard.press("Escape");

  // A selected picture: the OBJECT menu, which is a different builder.
  await page.goto("/editor.html?fixture=float");
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });
  const canvas = page.locator(".page-wrap .page").first();
  // `stableBox`, not `boundingBox`: under load the canvas reports null and the
  // failure then reads like a broken editor rather than a busy machine.
  const box = await stableBox(canvas);
  await canvas.click({ button: "right", position: { x: box.width * 0.14, y: box.height * 0.11 } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await expect(row).toBeVisible();
  // FIRST in the object menu, as it is first in ONLYOFFICE's.
  const order = await menu
    .locator(".menu-item")
    .evaluateAll((nodes) => nodes.map((node) => node.dataset.commandId));
  expect(order[0]).toBe("reference.caption");

  expect(consoleErrors).toEqual([]);
});

test("Viewing mode refuses both commands, and says so", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const before = await mirrorBlocks(page);
  await setReviewMode(page, "viewing");

  // Through the PALETTE, because Viewing mode composes the References band away
  // with the rest of the ribbon. The commands stay ENABLED — the document is open
  // and there is a caret, which is what they require — and the refusal is the
  // shared review-mode gate every other edit path answers to, spoken in the status
  // line. A command that went quiet here would be the silent failure the house
  // rule forbids, so `runPaletteCommand` asserts the row is enabled before it runs
  // it, and a greyed row fails this test rather than passing it.
  for (const [command, query] of [
    ["reference.caption", "Insert caption"],
    ["reference.crossReference", "Cross-reference"],
  ]) {
    await runPaletteCommand(page, command, query);
    await expect(page.locator("#status")).toHaveText(/viewing|editing/i);
    await expect(page.locator("#captionDialog")).toBeHidden();
    await expect(page.locator("#crossRefDialog")).toBeHidden();
  }
  expect(await mirrorBlocks(page)).toEqual(before);
  expect(consoleErrors).toEqual([]);
});

test("Insert caption refuses in a header, with its reason on the control", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openReferences(page);
  await expect(page.locator("#refCaptionBtn")).toBeEnabled();

  // The engine refuses a caption outside the body, as Word and ONLYOFFICE do, so
  // the control has to refuse BEFORE the click rather than throw on it.
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertHeaderBtn").click();
  await expect(page.locator("#pages")).toHaveAttribute("data-running-edit", "header");
  await openReferences(page);
  await expect(page.locator("#refCaptionBtn")).toBeDisabled();
  await expect(page.locator("#refCaptionBtn")).toHaveAttribute("title", /document body/i);

  expect(consoleErrors).toEqual([]);
});

test("Update caption numbers is disabled with its reason until a caption is stale", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openReferences(page);
  // Nothing to fix in a document with no captions, and the button says which of
  // the two it is rather than being greyed in silence.
  await expect(page.locator("#refUpdateCaptionsBtn")).toBeDisabled();
  await expect(page.locator("#refUpdateCaptionsBtn")).toHaveAttribute(
    "title",
    /already shows the right number/i,
  );

  await openCaption(page);
  await page.locator("#captionLabel").selectOption("Figure");
  await insertCaption(page, ": first");
  await clickIntoFirstPage(page);
  await openCaption(page);
  await page.locator("#captionLabel").selectOption("Figure");
  await insertCaption(page, ": second");

  // Still nothing stale: an insert renumbers in the same action.
  await openReferences(page);
  await expect(page.locator("#refUpdateCaptionsBtn")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});
