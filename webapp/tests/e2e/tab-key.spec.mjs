// Tab TYPES A TAB in body text (`docs/109` UX-073), as it does in Word, Google
// Docs and ONLYOFFICE. It used to indent the whole paragraph wherever the caret
// was — and demote a list item from anywhere in it — so no keystroke typed a tab
// at all. It keeps a structural meaning only where those editors give it one: at
// the start of a list item it demotes the item, and the nested level it demotes
// to is Word's `a.` (HF-227), not `1.1.`.
//
// Read from the SAVED file, because both defects are about what is stored: a
// typed tab must be `<w:tab/>` with no new `w:ind`, and a demoted item must name
// a level whose marker is `%2.` lower-letter.
import { readFile } from "node:fs/promises";
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";
import { opcPart } from "./opc-part.mjs";

/** A fresh body paragraph at the top of the document (`line-break.spec.mjs`). */
async function freshParagraph(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("Enter");
  await page.keyboard.press("ArrowUp");
}

async function save(page) {
  const download = page.waitForEvent("download");
  await page.keyboard.press(`${MOD}+s`);
  return readFile(await (await download).path());
}

/** The saved `<w:p>` whose text contains `text`. */
function paragraphHolding(xml, text) {
  const paragraphs = xml.match(/<w:p\b[\s\S]*?<\/w:p>/g) ?? [];
  return paragraphs.find((p) => [...p.matchAll(/<w:t(?:\s[^>]*)?>([^<]*)<\/w:t>/g)].map((m) => m[1]).join("").includes(text));
}

/** Text runs and tabs of a saved paragraph, in order: `"alpha"`, `"<tab>"`, … */
function inlineOrder(paragraph) {
  return [...paragraph.matchAll(/<w:t(?:\s[^>]*)?>([^<]*)<\/w:t>|<w:tab\/>/g)].map((m) => (m[0] === "<w:tab/>" ? "<tab>" : m[1]));
}

test("Tab in body text types a tab character, and indents nothing", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await freshParagraph(page);
  await page.keyboard.type("Alpha"); // capitalised, so AutoCorrect leaves it alone
  await page.keyboard.press("Tab");
  await page.keyboard.type("beta");

  const xml = opcPart(await save(page), "word/document.xml");
  const paragraph = paragraphHolding(xml, "Alphabeta");
  expect(paragraph, "the typed paragraph was saved").toBeTruthy();
  // Mutation proof: restoring the always-indent branch saves "Alphabeta" with
  // no tab and a `<w:ind w:start="360"/>`.
  expect(inlineOrder(paragraph)).toEqual(["Alpha", "<tab>", "beta"]);
  expect(paragraph).not.toMatch(/<w:ind\b/);
  expect(consoleErrors).toEqual([]);
});

test("Tab at the start of a list item demotes it to a. ; Tab inside the item types a tab", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await freshParagraph(page);
  await page.locator("#numberedList").click();
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.type("One");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Tab"); // at the start of the new item: demote
  await page.keyboard.type("Kiwi");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Tab"); // mid-item: a tab, the level unchanged

  const bytes = await save(page);
  const xml = opcPart(bytes, "word/document.xml");
  const item = paragraphHolding(xml, "Kiwi");
  expect(item, "the demoted item was saved").toBeTruthy();
  expect(item).toMatch(/<w:ilvl w:val="1"\/>/);
  expect(inlineOrder(item)).toEqual(["Kiw", "<tab>", "i"]);

  // The level it demoted to, read through ITS OWN numbering instance — the rich
  // fixture has other lists, and a level of theirs must not stand in for ours.
  const numId = item.match(/<w:numId w:val="(\d+)"\/>/)?.[1];
  expect(numId, "the item names a numbering instance").toBeTruthy();
  const numbering = opcPart(bytes, "word/numbering.xml");
  const abstractId = numbering.match(new RegExp(`<w:num w:numId="${numId}"[^>]*>[\\s\\S]*?<w:abstractNumId w:val="(\\d+)"\\/>`))?.[1];
  expect(abstractId, "the instance names an abstract definition").toBeTruthy();
  const definition = numbering.match(new RegExp(`<w:abstractNum\\b[^>]*w:abstractNumId="${abstractId}"[^>]*>[\\s\\S]*?<\\/w:abstractNum>`))?.[0] ?? "";
  const levelOne = definition.match(/<w:lvl w:ilvl="1"[^>]*>[\s\S]*?<\/w:lvl>/)?.[0] ?? "";
  // Mutation proof: restoring the joined `%1.%2.` decimal default saves
  // `decimal` / `%1.%2.` here.
  expect(levelOne).toMatch(/<w:numFmt w:val="lowerLetter"\/>/);
  expect(levelOne).toMatch(/<w:lvlText w:val="%2."\/>/);
  expect(consoleErrors).toEqual([]);
});
