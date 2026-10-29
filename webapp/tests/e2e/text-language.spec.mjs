// The language of selected text, with real gestures (`docs/109` RM-42;
// `docs/153` `text.language-of-selection`, `review.proofing-language-selection`).
//
// Set Language is reached from the status bar's text-language item and from the
// Review menu, writes `w:lang` on the selection as one formatting step, and the
// spell checker then proofs that word in its own language: an English typo
// marked French stops being flagged as bad English. The word sits in the middle
// of an English paragraph, so the paragraph MIXES languages — the case the
// checker used to send wholesale to the default list.
import {
  clickIntoFirstPage,
  documentPageCount,
  expect,
  gotoEditor,
  runAppMenuCommand,
  stableBox,
  test,
} from "./fixtures.mjs";
import { makeSpellingDocx, spellingTypoForPage } from "./large-docx.mjs";

const DOCX_MIME = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

test("Set Language marks one word French: the status bar says so and spell check stops flagging it", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.setInputFiles("#file", { name: "spelling.docx", mimeType: DOCX_MIME, buffer: Buffer.from(makeSpellingDocx(2)) });
  await expect.poll(() => documentPageCount(page), { timeout: 45_000 }).toBe(2);
  const typo = spellingTypoForPage(1);
  const squiggle = page.locator(`.overlay .spell-error[data-spell-word="${typo}"]`);
  // The positive case first: the English typo is flagged.
  await expect(squiggle).toHaveCount(1, { timeout: 20_000 });

  // Select the word (double-click), then click the status bar's text language.
  const box = await stableBox(squiggle);
  // The mark is drawn under the text, so the word itself is a few pixels above it.
  await page.mouse.dblclick(box.x + box.width / 2, box.y - 5);
  const status = page.locator("#textLanguageStatus");
  await expect(status).toBeVisible();
  await expect(status).toHaveAttribute("aria-label", /English/);
  await status.click();
  const dialog = page.getByRole("dialog", { name: "Language" });
  await expect(dialog).toBeVisible();
  // Keyboard only: type in the search box, arrow into the list, Enter applies.
  await page.keyboard.type("French (France)");
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("listbox", { name: "Languages" })).toHaveValue("fr-FR");
  // The dialog says, before anything is applied, that French text goes unchecked.
  await expect(page.locator("#textLanguageNote")).toContainText("no spelling dictionary");
  await page.keyboard.press("Enter");
  await expect(dialog).toBeHidden();

  // The word is French now: the status bar names it, the undo step is one
  // formatting change, and the checker no longer flags it as bad English.
  await expect(status).toHaveAttribute("aria-label", /French/);
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Formatting");
  await expect(squiggle).toHaveCount(0, { timeout: 20_000 });

  // One Undo puts the English back, and with it the squiggle. (The button, not
  // the chord: focus went back to the status-bar item that opened the dialog.)
  await page.locator("#undoBtn").click();
  await expect(squiggle).toHaveCount(1, { timeout: 20_000 });
  expect(consoleErrors).toEqual([]);
});

test("Set Language is on the Review menu as well as the status bar", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await runAppMenuCommand(page, "review", "format.language");
  const dialog = page.getByRole("dialog", { name: "Language" });
  await expect(dialog).toBeVisible();
  await expect(page.getByRole("searchbox", { name: "Search languages" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  expect(consoleErrors).toEqual([]);
});
