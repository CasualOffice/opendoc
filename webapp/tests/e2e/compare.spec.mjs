// Compare with another document, on the page (ADR-065).
//
// THE GUARD IS THE EFFECT. A spec that found a Compare button and a panel would
// pass over a chrome that compared the document with itself, or compared the
// two sides the wrong way round — the mistake a reader cannot detect, because
// every addition would be reported as a removal and the count would be right.
// So the fixtures are built here, by hand, with KNOWN differences in a known
// direction, and the assertions are about which differences come back, what
// the PAGE paints, and that the reader's document is left alone.
//
// Pixels, where the claim is about the page: `apply_revision_markup` paints a
// change in its author's hue, and every hue in the ten-colour palette is
// strongly chromatic while black text and its antialiasing are not. So
// "saturated ink on page one" is a question a plain document answers no to and
// a redline answers yes to — and it cannot be satisfied by a call merely having
// happened (`compare-on-canvas.spec.mjs` measured the threshold).
import { test, expect, gotoEditor, mirrorBlocks, runAppMenuCommand } from "./fixtures.mjs";

/** A plain-text document: a comparison imports both sides through the format
 *  registry, so `.txt` exercises the same path a `.docx` would, and every
 *  difference it reports can be accounted for by reading this file. */
function textFile(name, body) {
  return { name, mimeType: "text/plain", buffer: Buffer.from(body, "utf8") };
}

async function openText(page, file) {
  await page.goto("/editor.html?blank=1");
  await expect(page.locator("#file")).toBeEnabled();
  await page.locator("#file").setInputFiles(file);
  await expect(page.locator("#docTitle")).toHaveValue(file.name);
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });
}

/** Author-coloured pixels on page one. See the header. */
async function saturatedInk(page) {
  await page.evaluate(() => document.fonts.ready);
  return page.evaluate(() => {
    const canvas = document
      .querySelector('#pages .page-wrap[data-page-number="1"]')
      ?.querySelector("canvas.page");
    if (!canvas) return 0;
    const { data } = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height);
    let saturated = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] < 128) continue;
      const max = Math.max(data[i], data[i + 1], data[i + 2]);
      const min = Math.min(data[i], data[i + 1], data[i + 2]);
      if (max - min > 48) saturated += 1;
    }
    return saturated;
  });
}

/** Opens Compare through one of its surfaces and compares against `other`,
 *  waiting on the comparison's heading rather than on a clock. */
async function compareAgainst(page, other, { via = "rail" } = {}) {
  if (via === "rail") {
    await page.locator("#railCompare").click();
  } else {
    await page.locator('[data-tab="review"]').click();
    await page.locator("#reviewCompareBtn").click();
  }
  await expect(page.locator("#comparePanel")).toBeVisible();
  await expect(page.locator('#compareBody [data-compare-action="choose-file"]')).toBeEnabled();
  await page.locator("#compareFile").setInputFiles(other);
  await expect(page.locator("#compareBody [data-compare-view]")).toBeVisible({ timeout: 45_000 });
}

const changes = (page, kind) => page.locator(`#compareBody .compare-change[data-diff-kind="${kind}"]`);

test("Compare is reachable from the Review band, the rail and the menu, and says it changes nothing", async ({
  page,
}) => {
  await gotoEditor(page);
  await expect(page.locator("#railCompare")).toBeEnabled();
  await page.locator("#railCompare").click();
  await expect(page.locator("#comparePanel")).toBeVisible();
  await expect(page.locator("#railCompare")).toHaveAttribute("aria-pressed", "true");
  // Said BEFORE a file is picked: the differences are shown, and nothing is
  // written into this document unless the reader keeps them. The sentence it
  // replaced — "written into this document as tracked changes" — is asserted
  // gone, so the old behaviour's warning cannot come back with the old wording.
  await expect(page.locator("#compareBody")).toContainText(/shown on the page/i);
  await expect(page.locator("#compareBody")).toContainText(/not changed unless you keep/i);
  await expect(page.locator("#compareBody")).not.toContainText(/are written into this document/i);
  await page.locator("#compareClose").click();
  await expect(page.locator("#comparePanel")).toBeHidden();

  await page.locator('[data-tab="review"]').click();
  await expect(page.locator("#reviewCompareBtn")).toBeEnabled();
  await page.locator("#reviewCompareBtn").click();
  await expect(page.locator("#comparePanel")).toBeVisible();

  await runAppMenuCommand(page, "review", "review.compare");
  await expect(page.locator("#comparePanel")).toBeVisible();
});

test("a paragraph only this document has is ADDED, on the page, and closing leaves the document untouched", async ({
  page,
  consoleErrors,
}) => {
  // THE DIRECTION IS THE POINT: the other document is the older side, so a
  // paragraph only this one has is an ADDITION. Wired the other way round, the
  // count would be the same and every addition a removal.
  await openText(page, textFile("mine.txt", "Alpha\nBeta\nGamma\nDelta only in mine\n"));
  expect(await saturatedInk(page), "precondition: black text paints no author ink").toBe(0);

  await compareAgainst(page, textFile("theirs.txt", "Alpha\nBeta\nGamma\n"));

  await expect(page.locator("#compareBody [data-compare-view]")).toContainText("theirs.txt");
  await expect(changes(page, "added")).toHaveCount(1);
  await expect(changes(page, "added")).toContainText("Delta only in mine");
  await expect(changes(page, "removed")).toHaveCount(0);

  // ON THE PAGE: the redline paints the addition in the author's colour, with
  // the markup view on — no toggle for the reader to find.
  await expect(async () => {
    expect(await saturatedInk(page)).toBeGreaterThan(0);
  }).toPass({ timeout: 45_000 });
  await expect(page.locator("body")).toHaveClass(/showing-changes/);

  // READ-ONLY: an edit on the comparison is refused with the comparison's own
  // sentence, not silently swallowed and not applied.
  await page.locator(".page-wrap").first().click();
  await page.keyboard.insertText("X");
  await expect(page.locator("#status")).toContainText(/looking at a comparison/i);

  // CLOSING gives the page back, and the reader's document never changed: no
  // author ink, no tracked change, and its own text intact.
  await page.locator('#compareBody [data-compare-action="close"]').click();
  await expect(page.locator("#comparePanel")).toBeHidden();
  await expect(async () => {
    expect(await saturatedInk(page)).toBe(0);
  }).toPass({ timeout: 15_000 });
  await expect(page.locator("#reviewSidebar .review-margin-revision")).toHaveCount(0);
  expect((await mirrorBlocks(page)).join(" ")).toContain("Delta only in mine");
  expect((await mirrorBlocks(page)).join(" ")).not.toContain("X");
  expect(consoleErrors).toEqual([]);
});

test("a whole paragraph only the OTHER document has is shown struck where it was", async ({
  page,
  consoleErrors,
}) => {
  // THE CASE THE OLD COMPARE COULD NOT SHOW. A tracked change edits a
  // paragraph's inlines and cannot add one, so a removed paragraph used to be
  // "found, but not marked". The redline puts it back, struck, at the place it
  // stood (`DiffChange::place`). Only a deletion differs here, so author ink on
  // the page can only be that restored paragraph.
  await openText(page, textFile("mine.txt", "Alpha\nBeta\n"));
  expect(await saturatedInk(page)).toBe(0);
  await compareAgainst(page, textFile("theirs.txt", "Alpha\nGamma only in theirs\nBeta\n"), {
    via: "review-band",
  });

  await expect(changes(page, "removed")).toHaveCount(1);
  await expect(changes(page, "removed")).toContainText("Gamma only in theirs");
  await expect(changes(page, "added")).toHaveCount(0);
  await expect(async () => {
    expect(await saturatedInk(page), "the removed paragraph is painted on the page").toBeGreaterThan(0);
  }).toPass({ timeout: 45_000 });
  // And it is not reported as something the page could not show.
  await expect(page.locator('#compareBody [data-compare-unmarked="blockDeletion"]')).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("Swap order reverses the direction, and Keep is only offered where it describes this document", async ({
  page,
  consoleErrors,
}) => {
  await openText(page, textFile("mine.txt", "Alpha\nBeta\nDelta only in mine\n"));
  await compareAgainst(page, textFile("theirs.txt", "Alpha\nBeta\n"));
  await expect(changes(page, "added")).toContainText("Delta only in mine");
  await expect(page.locator('#compareBody [data-compare-action="keep"]')).toBeVisible();

  await page.locator('#compareBody [data-compare-action="swap"]').click();
  // The same paragraph, the other way round: in "theirs" it is gone.
  await expect(changes(page, "removed")).toContainText("Delta only in mine", { timeout: 45_000 });
  await expect(changes(page, "added")).toHaveCount(0);
  await expect(page.locator("#compareBody [data-compare-view]")).toContainText(/to theirs\.txt/);
  // Applied to this document, the swapped comparison would describe the other
  // file — so Keep is not there to press.
  await expect(page.locator('#compareBody [data-compare-action="keep"]')).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("comparing a document with itself finds no differences", async ({ page, consoleErrors }) => {
  // The control: without it, a chrome that reported every block as changed
  // would satisfy the tests above by finding far too many.
  const body = "One\nTwo\nThree\n";
  await openText(page, textFile("same.txt", body));
  await compareAgainst(page, textFile("same-copy.txt", body));
  await expect(page.locator("#compareBody")).toContainText(/No differences/i);
  await expect(page.locator("#compareBody .compare-change")).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("an entry takes the reader to its change, and Next steps in document order", async ({
  page,
  consoleErrors,
}) => {
  // Far enough down that reaching it needs a scroll: sixty unchanged lines, then
  // the difference. A click that did nothing would leave the viewport at the top.
  const lines = Array.from({ length: 60 }, (_, index) => `Line ${index + 1}`);
  await openText(page, textFile("mine.txt", `${[...lines, "The added ending"].join("\n")}\n`));
  await compareAgainst(page, textFile("theirs.txt", `${lines.join("\n")}\n`));
  await expect(changes(page, "added")).toHaveCount(1);
  expect(await page.locator("#viewport").evaluate((element) => element.scrollTop)).toBeLessThan(50);

  await changes(page, "added").click();
  await expect(changes(page, "added")).toHaveClass(/is-current/);
  await expect(async () => {
    expect(await page.locator("#viewport").evaluate((element) => element.scrollTop)).toBeGreaterThan(200);
  }).toPass({ timeout: 10_000 });

  // The navigator agrees: one of one, and Next wraps to it.
  await expect(page.locator("#compareBody .diff-nav-position")).toContainText("1 of 1");
  expect(consoleErrors).toEqual([]);
});
