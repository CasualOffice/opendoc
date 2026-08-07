// "Find in selection" (the #findSelection "Selection only" scope) must honor an
// arbitrary MULTI-paragraph selection, matching Word / Google Docs. Previously
// the scope was captured only when the whole selection lived inside a single
// paragraph node; a selection spanning two or more paragraphs left the scope
// null and Find/Replace-All silently matched NOTHING. This locks the fix:
//   - Find within a 3-paragraph selection counts only the in-scope matches.
//   - Replace-All within that selection touches only the in-scope matches.
//   - A single-paragraph selection scope still narrows correctly (unchanged).
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

test("Find and Replace-All honor a multi-paragraph 'Selection only' scope", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Seed four short paragraphs, each holding one unique token, at the very top
  // of the document. Typing at doc-start pushes the demo's original first-line
  // prose to the tail of the last seeded paragraph; the token is unique, so the
  // document-wide count is exactly four regardless of that trailing text.
  const token = "ZQWORDTOKEN";
  await page.keyboard.type(`${token} aaa`);
  await page.keyboard.press("Enter");
  await page.keyboard.type(`${token} bbb`);
  await page.keyboard.press("Enter");
  await page.keyboard.type(`${token} ccc`);
  await page.keyboard.press("Enter");
  await page.keyboard.type(`${token} ddd`);

  const findInput = page.locator("#findInput");
  const findStatus = page.locator("#findStatus");
  const findSelection = page.locator("#findSelection");

  // Baseline: with no scope, the whole document reports all four occurrences.
  // Open via the ribbon button so the flow stays platform-agnostic.
  await moveCaretToDocStart(page);
  await page.locator("#findBtn").click();
  await expect(page.locator("#findPanel")).toBeVisible();
  await findInput.fill(token);
  await expect(findStatus).toHaveText(/of 4$/);
  await page.keyboard.press("Escape");

  // Build a selection that spans paragraphs 1..3 (excluding paragraph 4):
  // from doc-start, extend down two paragraph lines, then to end of line three.
  await moveCaretToDocStart(page);
  await page.keyboard.press("Shift+ArrowDown");
  await page.keyboard.press("Shift+ArrowDown");
  await page.keyboard.press("Shift+End");

  // Open Find and enable "Selection only" BEFORE typing the query — the scope is
  // captured from the live selection at the moment the checkbox flips; typing
  // the query afterwards moves the model selection to the first match but must
  // not disturb the already-captured scope.
  await page.locator("#findBtn").click();
  await expect(page.locator("#findPanel")).toBeVisible();
  await findSelection.check();
  await findInput.fill(token);
  // Only the three in-scope occurrences count — not the fourth in paragraph 4.
  await expect(findStatus).toHaveText(/of 3$/);

  // Replace-All is likewise scoped: exactly the three in-scope tokens change.
  await page.locator("#replaceInput").fill("NEWX");
  await page.locator("#replaceAll").click();
  await expect(findStatus).toHaveText("Replaced 3");

  // Turn the scope off and confirm the single surviving token is the one that
  // sat outside the selection (paragraph 4).
  await findSelection.uncheck();
  await findInput.fill(token);
  await expect(findStatus).toHaveText("1 match");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("a single-paragraph 'Selection only' scope still narrows to that range", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // One paragraph, four copies of the token. Selecting only the first two must
  // scope Find to those two (the pre-existing single-node behavior).
  const token = "TOKENX";
  await page.keyboard.type(`${token} ${token} ${token} ${token}`);

  const findInput = page.locator("#findInput");
  const findStatus = page.locator("#findStatus");
  const findSelection = page.locator("#findSelection");

  // Baseline whole-document count is four.
  await moveCaretToDocStart(page);
  await page.locator("#findBtn").click();
  await expect(page.locator("#findPanel")).toBeVisible();
  await findInput.fill(token);
  await expect(findStatus).toHaveText(/of 4$/);
  await page.keyboard.press("Escape");

  // Select exactly the first two tokens: "TOKENX TOKENX" == 13 characters.
  await moveCaretToDocStart(page);
  for (let i = 0; i < 13; i++) await page.keyboard.press("Shift+ArrowRight");

  await page.locator("#findBtn").click();
  await expect(page.locator("#findPanel")).toBeVisible();
  await findSelection.check();
  await findInput.fill(token);
  await expect(findStatus).toHaveText(/of 2$/);
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});
