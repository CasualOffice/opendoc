// An edited save states the document's CURRENT counts in `docProps/app.xml`
// (`docs/109` FID-AT-04). File browsers, search indexers and document libraries
// show those numbers as the document. `sample.docx` is Word's, and its file says
// 0 words and 1 page; the engine lane's save stops repeating counts an edit made
// stale, and the editor — which shows the real ones in its status bar — writes
// those instead. The guarantee asserted is the reader's: the saved file says the
// word count the status bar said.
import { readFile } from "node:fs/promises";

import { MOD, expect, test } from "./fixtures.mjs";
import { opcPart } from "./opc-part.mjs";

/** Opens the shipped `sample.docx`, waiting on the painted document (the wait
 *  `save-findings-count.spec.mjs` uses, for the reason `compat-findings` gives). */
async function openSample(page) {
  await page.goto("/editor.html");
  await page.waitForFunction(
    () =>
      document.querySelectorAll(".page-wrap").length > 0 && document.body.dataset.fontsReady === "true",
    null,
    { timeout: 45_000 },
  );
}

/** The status bar's word count, as a number. */
async function statusWords(page) {
  return Number((await page.locator("#statWords").textContent()).replace(/[^\d]/g, ""));
}

/** `<Tag>n</Tag>` in an `app.xml`, or null when the save left it out. */
function statistic(app, tag) {
  const match = new RegExp(`<${tag}>(\\d+)</${tag}>`).exec(app);
  return match ? Number(match[1]) : null;
}

test("an edited save of sample.docx states the word count the status bar shows, not the file's stale one", async ({
  page,
  consoleErrors,
}) => {
  const original = opcPart(await readFile(new URL("../../sample.docx", import.meta.url)), "docProps/app.xml");
  expect(statistic(original, "Words"), "Word's file says 0 words").toBe(0);

  await openSample(page);
  const before = await statusWords(page);
  expect(before).toBeGreaterThan(100);
  const sheet = page.locator(".page-wrap .page").first();
  await sheet.click({ position: { x: 120, y: 160 } });
  await page.keyboard.type(" alpha beta gamma ");
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");
  // Three more words — or four, if the click split a word in two.
  await expect.poll(() => statusWords(page), { message: "the status bar counted the typing" }).toBeGreaterThan(before + 2);
  const now = await statusWords(page);

  const download = page.waitForEvent("download");
  await page.keyboard.press(`${MOD}+s`);
  const saved = await readFile(await (await download).path());
  const app = opcPart(saved, "docProps/app.xml");
  expect(app, "the save has application properties").not.toBeNull();
  expect(statistic(app, "Words"), `the file says what the status bar says: ${app}`).toBe(now);
  expect(statistic(app, "Characters")).toBeGreaterThan(statistic(app, "Words"));
  expect(statistic(app, "CharactersWithSpaces")).toBeGreaterThan(statistic(app, "Characters"));
  expect(statistic(app, "Pages"), "and its page count is the editor's, not Word's template's 1").toBeGreaterThan(1);
  expect(consoleErrors).toEqual([]);
});
