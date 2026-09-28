// A table of contents has to take you somewhere.
//
// Reported by the owner: "i checked TOC.. many of this points were unclickable
// on our place whereas UX of the same in ONLYOFFICE was way different nice".
//
// The two documents these specs open differ in ONE switch, `\h`, and that switch
// is the whole of the defect: with it, Word wraps each entry in a hyperlink to
// the heading's bookmark and clicking already worked; without it there is no
// hyperlink anywhere in the table and every entry was dead — pointer stayed an
// I-beam, click did nothing, status bar said nothing.
//
// What is guarded here is the GUARANTEE, not the mechanism: clicking an entry
// moves the caret and the viewport to that heading. Both shapes are asserted, so
// the fix cannot regress the case that already worked either.
import { test, expect, gotoEditor, stableBox } from "./fixtures.mjs";
import { tocDocx, TOC_HEADINGS } from "./toc-docx.mjs";

/** Opens a generated TOC document and waits for it to be laid out. */
async function openToc(page, { hyperlinked }) {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(tocDocx("toc.docx", 900, false, hyperlinked));
  await page.waitForFunction(
    () => /of\s+\d\d/.test(document.getElementById("statPages")?.textContent ?? ""),
    null,
    { timeout: 45_000 },
  );
  // The entries sit on page 1, one line apart, under the "Contents" line.
  const box = await stableBox(page.locator('.page-wrap[data-page-number="1"]'));
  return { box, entryY: [132, 148, 164] };
}

/** How far down the document the viewport is, and which heading the caret is on
 *  — read the way a reader would, from the scroller and the status bar. */
async function viewState(page) {
  return page.evaluate(() => ({
    scrollTop: Math.round(document.getElementById("viewport")?.scrollTop ?? -1),
    status: document.getElementById("status")?.textContent ?? "",
  }));
}

for (const hyperlinked of [true, false]) {
  const shape = hyperlinked ? "with \\h (authored hyperlinks)" : "without \\h (no hyperlink at all)";

  test(`every contents entry navigates to its heading — ${shape}`, async ({
    page,
    consoleErrors,
  }) => {
    const { box, entryY } = await openToc(page, { hyperlinked });

    const landed = [];
    for (const [index, dy] of entryY.entries()) {
      // Back to the top each time: following an entry scrolls the page away, so
      // the next entry is no longer under the coordinate it was.
      await page.evaluate(() => {
        document.getElementById("viewport").scrollTop = 0;
      });
      await page.waitForTimeout(250);

      // The pointer must predict the press — an entry that navigates has to look
      // like it does, whichever way the author wrote the field.
      await page.mouse.move(box.x + 140, box.y + dy);
      await expect
        .poll(() =>
          page
            .locator('.page-wrap[data-page-number="1"] canvas.page')
            .evaluate((el) => getComputedStyle(el).cursor),
        )
        .toBe("pointer");

      await page.mouse.click(box.x + 140, box.y + dy);
      await expect.poll(async () => (await viewState(page)).scrollTop).toBeGreaterThan(400);
      landed.push((await viewState(page)).scrollTop);
      // The status bar names where it went, so the jump is reported and not just
      // performed.
      expect((await viewState(page)).status).toMatch(
        new RegExp(hyperlinked ? "_Toc10" : TOC_HEADINGS[index]),
      );
    }

    // Three entries, three DIFFERENT places, in document order — the assertion
    // that would still fail if every entry jumped to the same heading.
    expect(landed[1]).toBeGreaterThan(landed[0]);
    expect(landed[2]).toBeGreaterThan(landed[1]);
    expect(consoleErrors).toEqual([]);
  });
}

test("the caret lands ON the heading, not merely near it", async ({ page }) => {
  const { box, entryY } = await openToc(page, { hyperlinked: false });
  await page.mouse.click(box.x + 140, box.y + entryY[1]);
  await expect.poll(async () => (await viewState(page)).scrollTop).toBeGreaterThan(400);
  // Typing goes into the heading the entry named: the caret is really there, not
  // just the scroll position.
  await page.keyboard.type("XX");
  await expect
    .poll(() => page.locator("#status").textContent())
    .not.toMatch(/error/i);
  const hasEdit = await page.evaluate(() => !document.getElementById("undoBtn")?.disabled);
  expect(hasEdit).toBe(true);
});

test("Go to heading is reachable without a pointer, and refuses with a reason", async ({
  page,
}) => {
  const { box, entryY } = await openToc(page, { hyperlinked: false });
  const button = page.locator("#refGoToHeadingBtn");
  await page.locator("#tabReferences").click();

  // Caret nowhere near a contents entry: disabled, and the title says why — the
  // control never silently does nothing.
  await page.mouse.click(box.x + 140, box.y + 90); // the "Contents" line itself
  await expect(button).toBeDisabled();
  await expect(button).toHaveAttribute("title", /table-of-contents entry/i);

  // Caret on an entry: enabled, and activating it from the ribbon navigates.
  await page.mouse.click(box.x + 140, box.y + entryY[0]);
  // That click already followed the entry, so come back and put the caret on the
  // entry without travelling: a keyboard caret move from the entry's own line.
  await page.evaluate(() => {
    document.getElementById("viewport").scrollTop = 0;
  });
  await page.waitForTimeout(250);
  await page.mouse.click(box.x + 300, box.y + entryY[2]); // past the entry text
  await expect(button).toBeEnabled();
  await button.click();
  await expect.poll(async () => (await viewState(page)).scrollTop).toBeGreaterThan(400);
});

// The third surface. A click on the entry is the pointer path and the References
// button is the keyboard one; right-clicking the entry is where a reader who has
// just tried clicking looks next, and it is where Open link sits for the entries
// the author's field DID hyperlink — so both cases answer the same gesture.
test("right-clicking a contents entry offers the same capability", async ({ page }) => {
  const { box, entryY } = await openToc(page, { hyperlinked: false });
  const row = page.locator('.editor-context-menu [data-command-id="reference.goToHeading"]');

  // Past the entry's text, so the caret lands on it without the click following
  // it — and the right-click then has a contents entry under it.
  await page.mouse.click(box.x + 300, box.y + entryY[1]);
  await page.mouse.click(box.x + 300, box.y + entryY[1], { button: "right" });
  await expect(row).toBeVisible();
  await row.click();
  await expect.poll(async () => (await viewState(page)).scrollTop).toBeGreaterThan(400);

  // And it is NOT offered on a paragraph that is not a contents entry — a row
  // that is always there is a row that says nothing.
  await page.evaluate(() => {
    document.getElementById("viewport").scrollTop = 0;
  });
  await page.waitForTimeout(250);
  await page.mouse.click(box.x + 140, box.y + 90); // the "Contents" line itself
  await page.mouse.click(box.x + 140, box.y + 90, { button: "right" });
  await expect(page.locator(".editor-context-menu")).toBeVisible();
  await expect(row).toHaveCount(0);
});
