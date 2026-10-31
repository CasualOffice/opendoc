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
  // The marker element IS the word's text box (`decorationRectsOnPages`; the wave
  // is its `::after`, at the bottom), so the word is at its centre — as
  // `spell-check.spec.mjs` clicks it. Aiming 5 px ABOVE the box, as this did,
  // double-clicked the line above and set French on one space of it.
  await page.mouse.dblclick(box.x + box.width / 2, box.y + box.height / 2);
  await expect(page.locator(".overlay .highlight"), "the double-click selected the word").toHaveCount(1);
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
  // The caret leaves the word first: the checker never marks the word the caret
  // is in (`paint` skips it while it is being typed), so with the selection still
  // on it a count of 0 would hold even if the French tag were ignored. A count
  // of 0 is also what the overlay shows for a moment while it repaints, so the
  // POSITIVE half comes first: the checker itself says it met French text it
  // has no list for, which it says only once it has split the paragraph by
  // language (`languageParts`).
  await expect(page.locator("#status")).toHaveText("No spelling dictionary for fr-FR", { timeout: 20_000 });
  await clickIntoFirstPage(page);
  await expect(squiggle).toHaveCount(0, { timeout: 20_000 });

  // One Undo puts the English back, and with it the squiggle. (The button, not
  // the chord: focus went back to the status-bar item that opened the dialog.)
  await page.locator("#undoBtn").click();
  await clickIntoFirstPage(page);
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
