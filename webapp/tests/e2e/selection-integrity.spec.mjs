// Selection integrity across content mutation — reported as "adding and
// removing content messes up the complete selection and highlights everything".
//
// Two distinct defects were measured; the literal "text highlight painted over
// everything" symptom was NOT reproducible and is pinned green at the bottom of
// this file so a future change cannot regress it unnoticed.
//
// FINDING A (P0, silent input loss) — "stranded caret".
//   `applyEditResult` (src/main.js:7785-7797) trusts the EditResult's
//   `node`/`offset` unconditionally. Several mutations return a position whose
//   node the SAME mutation removed. The result: `doc.caretRect()` returns an
//   empty rect so `paintSelection` (src/main.js:5215-5231) paints NOTHING, and
//   every following keystroke throws `start node not found` /
//   `paragraph not found` inside `runEdit`, which swallows it as a generic
//   status line. The document silently stops accepting input until the user
//   clicks (or presses ⌘A). Confirmed on five independent paths:
//     A1 insert a table, then Undo
//     A2 Suggesting: select a line, Backspace, then Undo
//     A3 Suggesting: select text, type over it, then Undo
//     A4 Reject all changes
//     A5 Accept all changes
//
// FINDING B (P2, the "highlights everything" the report describes) — the
//   table row/column/table selection overlay (`.table-cell-selection`, an
//   accent fill visually identical to `.highlight`) is never dropped when
//   content is added or removed, nor when the caret navigates away.
//   `tableSelection` is cleared on pointerdown (src/main.js:5534), context menu
//   (7195), document open (2783), object select (4382) and running-content
//   entry (4690) — but NOT in `applyEditResult` (7785) and NOT in `navCaret`
//   (7925). Measured: Select Table paints 4 rects / 25,020 px² of accent
//   fill; a Backspace then removes exactly ONE character and the 25,020 px²
//   "whole table is selected" paint stays on screen over a collapsed caret.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";

// ---- measurement helpers ---------------------------------------------------

/** Painted selection chrome: rect counts and total painted area in CSS px². */
async function paint(page) {
  return page.evaluate(() => {
    const measure = (sel) => {
      const els = [...document.querySelectorAll(sel)];
      let area = 0;
      for (const el of els) {
        const r = el.getBoundingClientRect();
        area += r.width * r.height;
      }
      return { rects: els.length, area: Math.round(area) };
    };
    return {
      highlight: measure(".page-wrap .overlay .highlight"),
      tableCells: measure(".page-wrap .overlay .table-cell-selection"),
      carets: document.querySelectorAll(".page-wrap .overlay .caret").length,
    };
  });
}

/** The text the ENGINE believes is selected — the native copy payload, i.e. the
 *  ground truth the painted highlight is supposed to be a picture of. */
async function selectedText(page) {
  return page.evaluate(() => {
    const dt = new DataTransfer();
    document.dispatchEvent(
      new ClipboardEvent("copy", { clipboardData: dt, bubbles: true, cancelable: true }),
    );
    return dt.getData("text/plain");
  });
}

/** The engine's own character count — an independent proof of document content. */
async function docChars(page) {
  const text = await page.locator("#statChars").textContent();
  return Number.parseInt(text.replace(/[^0-9]/g, ""), 10);
}

async function pasteText(page, text) {
  await page.evaluate((t) => {
    const dt = new DataTransfer();
    dt.setData("text/plain", t);
    document.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }),
    );
  }, text);
  await page.waitForTimeout(200);
}

/** Types three characters into the canvas and reports whether they landed.
 *  This is the whole point of finding A: the keystrokes are accepted by the
 *  browser, produce no error the user can see, and change nothing. */
async function threeCharsLand(page) {
  await page.locator("#pages").focus();
  const before = await docChars(page);
  await page.keyboard.type("XYZ");
  await page.waitForTimeout(300);
  return (await docChars(page)) === before + 3;
}

async function insertTwoByTwoTable(page) {
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
}

async function setSuggesting(page) {
  await page.locator('#reviewModeControl [data-review-mode="suggesting"]').click();
  await expect(page.locator('#reviewModeControl [data-review-mode="suggesting"]')).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.locator("#pages").focus();
}

async function setEditing(page) {
  await page.locator('#reviewModeControl [data-review-mode="editing"]').click();
  await expect(page.locator('#reviewModeControl [data-review-mode="editing"]')).toHaveAttribute(
    "aria-pressed",
    "true",
  );
}

// ---- FINDING A: the caret is stranded on a node the mutation removed --------

// A1. RED. Insert ▸ Table 2×2, then ⌘Z. `doc.undo()` reports a position inside
// the table cell it just removed, so no caret is painted and the next three
// keystrokes are dropped with `edit ignored: start node not found`.
test("undoing an inserted table leaves a live caret and keeps accepting typing", async ({
  page,
  consoleErrors,
}) => {
  const warnings = [];
  page.on("console", (m) => {
    if (m.type() === "warning") warnings.push(m.text());
  });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await insertTwoByTwoTable(page);
  await page.locator("#pages").focus();
  expect((await paint(page)).carets).toBe(1);

  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(400);

  expect(await paint(page)).toMatchObject({ carets: 1 }); // caret vanishes today
  expect(await threeCharsLand(page)).toBe(true);
  expect(warnings.filter((w) => w.includes("edit ignored"))).toEqual([]);
  expect(await page.locator("#status").textContent()).not.toMatch(/isn't supported/i);
  expect(consoleErrors).toEqual([]);
});

// A2. RED. Suggesting mode: select the first line, Backspace (a tracked
// deletion), then ⌘Z. The undone deletion's paragraph no longer exists, so the
// caret is stranded — `edit ignored: paragraph not found` on every later key.
test("undoing a tracked deletion leaves a live caret and keeps accepting typing", async ({
  page,
  consoleErrors,
}) => {
  const warnings = [];
  page.on("console", (m) => {
    if (m.type() === "warning") warnings.push(m.text());
  });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await setSuggesting(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("Shift+End");
  const doomed = await selectedText(page);
  expect(doomed.length).toBeGreaterThan(0);
  await page.keyboard.press("Backspace");
  await page.waitForTimeout(300);

  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(400);

  expect(await paint(page)).toMatchObject({ carets: 1 });
  expect(await threeCharsLand(page)).toBe(true);
  expect(warnings.filter((w) => w.includes("edit ignored"))).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

// A3. RED. Same shape through the `suggestReplace` path: select text, type over
// it in Suggesting mode, then ⌘Z.
test("undoing a tracked replacement leaves a live caret and keeps accepting typing", async ({
  page,
  consoleErrors,
}) => {
  const warnings = [];
  page.on("console", (m) => {
    if (m.type() === "warning") warnings.push(m.text());
  });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await setSuggesting(page);
  await moveCaretToDocStart(page);
  for (let i = 0; i < 4; i++) await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.type("Q");
  await page.waitForTimeout(300);

  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(400);

  expect(await paint(page)).toMatchObject({ carets: 1 });
  expect(await threeCharsLand(page)).toBe(true);
  expect(warnings.filter((w) => w.includes("edit ignored"))).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

// A4. RED. Reject all changes removes the suggested insertion the caret sits
// inside; `decideAllRevisions` reports that now-deleted position back and the
// editor goes deaf. No table and no undo involved — this is the plainest repro.
test("Reject all changes leaves a live caret and keeps accepting typing", async ({
  page,
  consoleErrors,
}) => {
  const warnings = [];
  page.on("console", (m) => {
    if (m.type() === "warning") warnings.push(m.text());
  });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await setSuggesting(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("SUGGESTED");
  await page.waitForTimeout(400);
  await setEditing(page);

  await page.locator("#reviewRejectAll").click();
  await page.waitForTimeout(500);

  expect(await paint(page)).toMatchObject({ carets: 1 });
  expect(await threeCharsLand(page)).toBe(true);
  expect(warnings.filter((w) => w.includes("edit ignored"))).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

// A5. RED. Accept all changes, same failure — the accepted deletion's paragraph
// is gone and the reported caret position goes with it.
test("Accept all changes leaves a live caret and keeps accepting typing", async ({
  page,
  consoleErrors,
}) => {
  const warnings = [];
  page.on("console", (m) => {
    if (m.type() === "warning") warnings.push(m.text());
  });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await setSuggesting(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("Shift+End");
  await page.keyboard.press("Backspace");
  await page.waitForTimeout(300);
  await setEditing(page);

  await page.locator("#reviewAcceptAll").click();
  await page.waitForTimeout(500);

  expect(await paint(page)).toMatchObject({ carets: 1 });
  expect(await threeCharsLand(page)).toBe(true);
  expect(warnings.filter((w) => w.includes("edit ignored"))).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

// ---- FINDING B: the table selection paint outlives the selection -----------

// B1. RED. Select Table paints 4 accent-filled cell rects (~25,020 px²). Typing
// inserts 2 characters at a collapsed caret in ONE cell — and the "whole table
// is selected" paint is still there. This is the "highlights everything" the
// report describes: the paint claims a whole-table selection while ⌘C copies "".
test("typing drops the table selection paint instead of leaving it over the whole table", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await insertTwoByTwoTable(page);
  await page.locator("#tabTable").click();
  await page.locator('.table-ribbon [data-table-select="table"]').click();

  const selected = await paint(page);
  expect(selected.tableCells.rects).toBe(4);
  expect(selected.tableCells.area).toBeGreaterThan(20_000);
  // The paint already lies before any mutation: nothing is actually selected.
  expect(await selectedText(page)).toBe("");

  await page.locator("#pages").focus();
  const before = await docChars(page);
  await page.keyboard.type("QQ");
  await page.waitForTimeout(300);
  expect(await docChars(page)).toBe(before + 2); // two chars went into ONE cell

  const after = await paint(page);
  expect(after.tableCells).toEqual({ rects: 0, area: 0 });
  expect(consoleErrors).toEqual([]);
});

// B2. RED. Backspace with a whole-table selection painted removes exactly ONE
// character (the caret's), and the 4-rect table paint survives the deletion —
// so what the user sees selected and what the deletion touched disagree by the
// entire table.
test("deleting with a table selection painted does not leave the paint behind", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await insertTwoByTwoTable(page);
  await page.locator("#tabTable").click();
  await page.locator("#pages").focus();
  await page.keyboard.type("abcdef");
  await page.waitForTimeout(300);
  await page.locator('.table-ribbon [data-table-select="table"]').click();
  expect((await paint(page)).tableCells.rects).toBe(4);

  await page.locator("#pages").focus();
  const before = await docChars(page);
  await page.keyboard.press("Backspace");
  await page.waitForTimeout(300);
  expect(await docChars(page)).toBe(before - 1); // one character, not the table

  expect((await paint(page)).tableCells).toEqual({ rects: 0, area: 0 });
  expect(consoleErrors).toEqual([]);
});

// B3. RED. Moving the caret out of a selected row must drop the row paint the
// way clicking already does (`navCaret`, src/main.js:7925, clears
// `objectSelection` but never `tableSelection`).
test("arrow-navigating away from a selected table row drops the row paint", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await insertTwoByTwoTable(page);
  await page.locator("#tabTable").click();
  await page.locator('.table-ribbon [data-table-select="row"]').click();
  expect((await paint(page)).tableCells.rects).toBe(2);

  await page.locator("#pages").focus();
  await page.keyboard.press("ArrowDown");
  await page.waitForTimeout(250);

  expect((await paint(page)).tableCells).toEqual({ rects: 0, area: 0 });
  expect(consoleErrors).toEqual([]);
});

// ---- Refutation: the TEXT highlight itself is correct across mutation -------

// GREEN today. The reported "highlight painted over everything" does not happen
// for ordinary text selections: every mutation kind collapses the highlight to
// exactly one caret, and undo/redo never smears it. Pinned so a fix for A/B
// cannot silently break what already works.
test("a text-range highlight is fully cleared by type, delete, paste and cut", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Select-all: 7 highlight rects over ~17,900 px² for 95 characters.
  await page.keyboard.press(`${MOD}+a`);
  const all = await paint(page);
  expect(all.highlight.rects).toBeGreaterThan(1);
  expect((await selectedText(page)).length).toBeGreaterThan(80);
  await page.keyboard.type("Z");
  await page.waitForTimeout(300);
  expect(await paint(page)).toMatchObject({ highlight: { rects: 0, area: 0 }, carets: 1 });
  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(300);
  expect(await paint(page)).toMatchObject({ highlight: { rects: 0, area: 0 }, carets: 1 });

  // A cross-paragraph, table-spanning range: 3 rects / ~12,470 px² for 47 chars.
  await moveCaretToDocStart(page);
  await page.keyboard.press("Shift+ArrowDown");
  await page.keyboard.press("Shift+ArrowDown");
  await page.keyboard.press("Shift+End");
  const span = await paint(page);
  const spanText = await selectedText(page);
  expect(span.highlight.rects).toBeGreaterThan(1);
  expect(spanText.length).toBeGreaterThan(20);
  await page.keyboard.press("Backspace");
  await page.waitForTimeout(300);
  expect(await paint(page)).toMatchObject({ highlight: { rects: 0, area: 0 }, carets: 1 });
  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(300);
  expect(await paint(page)).toMatchObject({ highlight: { rects: 0, area: 0 }, carets: 1 });

  // Paste over a range, and cut a range.
  await moveCaretToDocStart(page);
  await page.keyboard.press("Shift+End");
  expect((await paint(page)).highlight.rects).toBe(1);
  await pasteText(page, "PASTED");
  expect(await paint(page)).toMatchObject({ highlight: { rects: 0, area: 0 }, carets: 1 });
  await page.keyboard.press("Shift+Home");
  expect((await paint(page)).highlight.rects).toBe(1);
  await page.evaluate(() => {
    const dt = new DataTransfer();
    document.dispatchEvent(
      new ClipboardEvent("cut", { clipboardData: dt, bubbles: true, cancelable: true }),
    );
  });
  await page.waitForTimeout(300);
  expect(await paint(page)).toMatchObject({ highlight: { rects: 0, area: 0 }, carets: 1 });

  expect(consoleErrors).toEqual([]);
});

// GREEN today. Ten keydowns fired back-to-back with no repaint between them
// replace the selection exactly once and leave no residual highlight — the
// "rapid typing smears the highlight" hypothesis does not hold.
test("rapid typing over a selection replaces it exactly once", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("Shift+End");
  const doomed = await selectedText(page);
  expect(doomed.length).toBeGreaterThan(5);
  const before = await docChars(page);

  await page.evaluate(() => {
    for (const ch of "abcdefghij") {
      document
        .getElementById("pages")
        .dispatchEvent(new KeyboardEvent("keydown", { key: ch, bubbles: true, cancelable: true }));
    }
  });
  await page.waitForTimeout(600);

  expect(await docChars(page)).toBe(before - doomed.length + 10);
  expect(await paint(page)).toMatchObject({ highlight: { rects: 0, area: 0 }, carets: 1 });
  await page.keyboard.press("Home");
  await page.keyboard.press("Shift+End");
  expect(await selectedText(page)).toBe("abcdefghij");
  expect(consoleErrors).toEqual([]);
});
