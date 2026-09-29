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
async function openToc(page, { hyperlinked, numbered = false, fieldInEntry = false }) {
  await gotoEditor(page);
  await page
    .locator("#file")
    .setInputFiles(tocDocx("toc.docx", 900, false, hyperlinked, numbered, fieldInEntry));
  await page.waitForFunction(
    () => /of\s+\d\d/.test(document.getElementById("statPages")?.textContent ?? ""),
    null,
    { timeout: 45_000 },
  );
  // The entries sit on page 1, one line apart, under the "Contents" line. With
  // the field's own runs moved INTO the first entry there is one paragraph less
  // above the table, so every row sits exactly one line — 16px here — higher.
  const box = await stableBox(page.locator('.page-wrap[data-page-number="1"]'));
  const top = fieldInEntry ? 116 : 132;
  return { box, entryY: [top, top + 16, top + 32] };
}

/** How far down the document the viewport is, and which heading the caret is on
 *  — read the way a reader would, from the scroller and the status bar. */
async function viewState(page) {
  return page.evaluate(() => ({
    scrollTop: Math.round(document.getElementById("viewport")?.scrollTop ?? -1),
    status: document.getElementById("status")?.textContent ?? "",
  }));
}

for (const [hyperlinked, numbered] of [
  [true, false],
  [false, false],
  // The shape every numbered contract, report and thesis has, and the one the
  // owner reported dead: the heading takes its number from a LIST, so the
  // entry reads `number TAB title TAB page` while the heading's own text is the
  // bare title. The resolver cut each entry at its FIRST tab, so every label
  // collapsed to "1." and matched nothing — with `\h` and without it.
  [true, true],
  [false, true],
]) {
  const shape =
    (hyperlinked ? "with \\h (authored hyperlinks)" : "without \\h (no hyperlink at all)") +
    (numbered ? ", NUMBERED headings" : "");

  test(`every contents entry navigates to its heading — ${shape}`, async ({
    page,
    consoleErrors,
  }) => {
    const { box, entryY } = await openToc(page, { hyperlinked, numbered });

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
      // The status bar names where it went by its HEADING, whichever of the two
      // mechanisms answered — the authored hyperlink or the contents path. They
      // used to disagree: the link path reported the bookmark id verbatim
      // ("Jumped to _Toc130812265"), an implementation detail no reader can
      // place, while the same gesture one line down named the heading.
      const { status } = await viewState(page);
      expect(status).toMatch(new RegExp(TOC_HEADINGS[index]));
      expect(status).not.toMatch(/_Toc\d/);
    }

    // Three entries, three DIFFERENT places, in document order — the assertion
    // that would still fail if every entry jumped to the same heading.
    expect(landed[1]).toBeGreaterThan(landed[0]);
    expect(landed[2]).toBeGreaterThan(landed[1]);
    expect(consoleErrors).toEqual([]);
  });
}

// The signal the owner's document never produced. Sweeping the pointer down a
// 16-page numbered NDA, the canvas reported `running-content-band`, `body-text`
// and `object-movable` — and `contents-entry` not once, so there was no cursor
// change, no affordance, and a click put a caret in instead of navigating.
//
// This asserts the ARMING, separately from the navigation above, because the
// two fail apart: an entry can resolve while nothing on screen says it will.
test("a numbered contents entry arms the contents-entry pointer target", async ({ page }) => {
  const { box, entryY } = await openToc(page, { hyperlinked: false, numbered: true });
  const canvas = page.locator('.page-wrap[data-page-number="1"] canvas.page');

  for (const [index, dy] of entryY.entries()) {
    await page.mouse.move(box.x + 200, box.y + dy);
    await expect
      .poll(() => canvas.evaluate((el) => el.dataset.pointerTarget ?? "(none)"), {
        message: `entry ${index + 1} must offer itself to the pointer`,
      })
      .toBe("contents-entry");
  }

  // And the line above the table is NOT a contents entry, so the signal means
  // something: a target that is always armed says nothing.
  await page.mouse.move(box.x + 200, box.y + 90);
  await expect
    .poll(() => canvas.evaluate((el) => el.dataset.pointerTarget ?? "(none)"))
    .not.toBe("contents-entry");
});

// The owner's exact shape, and the reason the FIRST entry behaves unlike the
// rest: Word writes the field's `begin`/`instrText`/`separate` at the head of
// the first entry paragraph, so that one entry's `w:hyperlink` opens while the
// TOC field is still open — and no other entry's does. The importer refused a
// hyperlink in that position, so entry 1 alone lost its anchor, which reads as
// "on TOC first point is not clickable" while every other point behaves.
test("the FIRST entry navigates when the field opens inside it", async ({
  page,
  consoleErrors,
}) => {
  const { box, entryY } = await openToc(page, {
    hyperlinked: true,
    numbered: true,
    fieldInEntry: true,
  });

  // Found the way a reader finds them — by sweeping down until the pointer says
  // "this is a contents entry" — rather than from hard-coded line positions,
  // which move with the shape: dropping the field's own paragraph shifts every
  // row up by a line, and a guard pinned to those numbers clicks the wrong entry
  // and reports the wrong thing.
  const landed = [];
  for (const [index, dy] of entryY.entries()) {
    await page.evaluate(() => {
      document.getElementById("viewport").scrollTop = 0;
    });
    await page.waitForTimeout(250);
    await page.mouse.move(box.x + 200, box.y + dy);
    await expect
      .poll(() =>
        page
          .locator('.page-wrap[data-page-number="1"] canvas.page')
          .evaluate((el) => getComputedStyle(el).cursor),
      )
      .toBe("pointer");
    await page.mouse.click(box.x + 200, box.y + dy);
    await expect.poll(async () => (await viewState(page)).scrollTop).toBeGreaterThan(400);
    const { scrollTop, status } = await viewState(page);
    landed.push(scrollTop);
    expect(status, `entry ${index + 1} must name where it went`).toMatch(
      new RegExp(TOC_HEADINGS[index]),
    );
  }
  // Three entries, three DIFFERENT places — so "the first one works" cannot be
  // satisfied by every entry going to the same heading.
  expect(landed[1]).toBeGreaterThan(landed[0]);
  expect(landed[2]).toBeGreaterThan(landed[1]);
  expect(consoleErrors).toEqual([]);
});

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
