// The six cheap table rows from `docs/141` — TBL-01 … TBL-05 and TBL-08's interim
// refusal — driven as gestures.
//
// Every assertion here is about the DOCUMENT or about what a person can see, not
// about a function returning Ok. That distinction is the reason this file exists:
// TBL-02 was a live command that returned Ok and never unmerged anything, because
// the argument list it passed reached a different engine branch. So unmerge is
// proven by counting the row's cells in the model-derived accessibility mirror
// before and after, and Tab-appends-a-row by typing into the row it created.
//
// `command-activation-contract` is deliberately NOT used for any of this: its
// inert-control assertion cannot fail (`109` HF-186 — eight unrelated `.overlay`
// and `#statusToast` mutations arrive within 3s of idle), and it was proven unable
// to notice the Table band's sort control being made a complete no-op.
import { test, expect, gotoEditor, clickIntoFirstPage, stableBox } from "./fixtures.mjs";

/** Typed into the first cell so the assertions can find OUR table in the mirror.
 *  The `rich` fixture already contains a nested table, and reading
 *  `querySelector("#a11yDocument table")` picked whichever came first in the
 *  window the mirror happens to be showing — which changed as the caret moved, so
 *  the same code read three rows in one run and four in the next. A marker makes
 *  the reading unambiguous; the merge keeps it, so it survives the round trip. */
const MARK = "TBLMARK";

/** Inserts a `rows x cols` table through the grid picker, marks its first cell and
 *  opens the Table band. */
async function insertTable(page, rows, cols) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator(`.gc[data-r="${rows}"][data-c="${cols}"]`).click();
  await expect(page.locator(".cell-outline")).toBeVisible();
  await focusActiveCell(page);
  await page.keyboard.type(MARK);
  await expect(page.locator("#tabTable")).toBeEnabled();
  await page.locator("#tabTable").click();
}

/** OUR table's cell texts, row by row, as the accessibility mirror projects the
 *  MODEL: one `<td>` per model cell, no `colspan`. A merge really removes cells
 *  from the row, so this is a document-level reading and not a repaint artefact. */
async function modelCells(page) {
  return page.evaluate((mark) => {
    const tables = [...document.querySelectorAll("#a11yDocument table")];
    const table = tables.find((candidate) => candidate.textContent.includes(mark));
    if (!table) return null;
    // `querySelectorAll` reaches into a NESTED table too, so every row and cell is
    // filtered back to the one that owns it.
    return [...table.querySelectorAll("tr")]
      .filter((row) => row.closest("table") === table)
      .map((row) =>
        [...row.children]
          .filter((cell) => cell.closest("table") === table)
          .map((cell) => cell.textContent.trim()),
      );
  }, MARK);
}

/** Just the cell COUNTS, which is what a merge and an unmerge change. */
async function modelRows(page) {
  return (await modelCells(page))?.map((row) => row.length) ?? null;
}

async function statusText(page) {
  return page.locator("#status").textContent();
}

/** Puts the pointer through the centre of the active cell's engine-drawn outline.
 *  Geometry from the ENGINE rather than a guess at where an opaque canvas put a
 *  cell — and a real mouse event, because the overlay is `pointer-events: none`
 *  and the canvas intercepts a click aimed at the outline element itself. */
async function activeCellCentre(page) {
  // `stableBox`, not `boundingBox`: every one of these rects is an `.overlay` child
  // that `drawSelection` destroys and rebuilds, so a bare read lands on null
  // mid-repaint — the flake `fixtures.mjs` wrote that helper for.
  const box = await stableBox(page.locator(".cell-outline"));
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

/** Returns the caret to the canvas after a ribbon click, by clicking the cell it
 *  is already in — so a keyboard gesture reaches the document and not the band. */
async function focusActiveCell(page) {
  const centre = await activeCellCentre(page);
  await page.mouse.click(centre.x, centre.y);
  await expect(page.locator(".cell-outline")).toBeVisible();
}

/** The first selection rect the engine painted for a row/column/table selection. */
async function selectionFillBox(page) {
  return stableBox(page.locator(".table-cell-selection").first());
}

/** Clicks the row BELOW `box`: inside the table, outside the selected row's fill.
 *  One cell height down from a rect the engine drew, rather than a guess at the page
 *  margin — a click in the margin resolves to no anchor at all, and `onPointerDown`
 *  returns before it decides anything about the selection. The box is passed in
 *  because it has to be measured while the fill is still on screen. */
async function clickBelow(page, box) {
  await page.mouse.click(box.x + box.width / 2, box.y + box.height * 1.5);
}

test("merging a row and then unmerging it puts the cells back in the document", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 3);
  // Polled, not read once: the mirror is rebuilt on the same coalesced frame as the
  // outline, so a bare read races the edit that is being asserted about.
  await expect.poll(() => modelRows(page)).toEqual([3, 3, 3]);

  // Merge the caret's row, which is the first thing anyone does to a table header.
  await page.locator('[data-table-select="row"]').click();
  await expect(page.locator("#mergeCellsBtn")).toBeEnabled();
  await page.locator("#mergeCellsBtn").click();
  await expect(page.locator("#tableContext")).toContainText("merged/spanned");
  await expect
    .poll(() => modelRows(page), {
      message: "the merge must really remove cells from the model, or nothing below is a test",
    })
    .toEqual([1, 3, 3]);

  // …and now undo it the way Word and Google Docs do: right-click the merged cell,
  // Unmerge cells. Before this row there was NO value a person could type into the
  // split dialog that reached the engine's unmerge (`docs/141` TBL-02).
  const centre = await activeCellCentre(page);
  await page.mouse.click(centre.x, centre.y, { button: "right" });
  const menu = page.locator(".editor-context-menu");
  await expect(menu).toBeVisible();
  const unmerge = menu.locator('[data-command-id="table.unmerge"]');
  await expect(unmerge, "Unmerge must be offered on a merged cell").toBeVisible();
  await expect(unmerge, "…and enabled, not a dead row").toBeEnabled();
  await unmerge.click();

  // THE ASSERTION: the document, not a return value.
  await expect.poll(() => modelRows(page)).toEqual([3, 3, 3]);
  await expect(page.locator("#tableContext")).not.toContainText("merged/spanned");
  // And the capabilities the merge had taken away are back, which is what makes the
  // round trip worth having at all (the merged-table cliff, `docs/141` §1.12).
  await expect(page.locator('[data-table-sort="ascending"]')).toBeEnabled();
  await expect(page.locator('[data-table-action="insert-column-right"]')).toBeEnabled();
  expect(consoleErrors).toEqual([]);
});

test("Unmerge is offered but refused, with a reason, on a table that has no merge", async ({
  page,
}) => {
  await insertTable(page, 2, 2);
  const centre = await activeCellCentre(page);
  await page.mouse.click(centre.x, centre.y, { button: "right" });
  const row = page.locator('.editor-context-menu [data-command-id="table.unmerge"]');
  await expect(row, "never a hidden capability: the row is present").toBeVisible();
  await expect(row, "…and disabled, because there is nothing to unmerge").toBeDisabled();
  await expect(row).toHaveAttribute("title", "This table has no merged cells");
});

test("Tab in the last cell appends a row and puts the caret in it", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 2, 2);
  await expect.poll(() => modelRows(page)).toEqual([2, 2]);

  // Walk to the last cell with the gesture itself, so the precondition is created
  // rather than assumed: three Tabs from the first cell of a 2x2.
  await focusActiveCell(page);
  for (let i = 0; i < 3; i += 1) await page.keyboard.press("Tab");
  await expect(page.locator("#tableContext")).toContainText("row 2, column 2");

  // The gesture under test. It used to reach an empty `catch {}`: no row, no
  // refusal, no status line — the only table gesture that said nothing at all.
  await page.keyboard.press("Tab");
  await expect(page.locator("#tableContext")).toContainText("3×2 table");
  await expect.poll(() => modelRows(page)).toEqual([2, 2, 2]);
  expect(await statusText(page)).toBe("Row added at the end of the table");

  // …and the caret is in the new row's first cell, proven by typing into it.
  await page.keyboard.type("APPENDED");
  await expect.poll(() => modelCells(page).then((rows) => rows[2])).toEqual(["APPENDED", ""]);
  expect(consoleErrors).toEqual([]);
});

test("Shift+Tab in the first cell refuses out loud instead of silently", async ({ page }) => {
  await insertTable(page, 2, 2);
  await focusActiveCell(page);
  await expect(page.locator("#tableContext")).toContainText("row 1, column 1");
  await page.keyboard.press("Shift+Tab");
  expect(await statusText(page)).toBe("The caret is already in the first cell of the table");
  expect(await modelRows(page), "and it must not add anything").toEqual([2, 2]);
});

test("formatting with a row selected refuses with a reason rather than formatting one cell", async ({
  page,
}) => {
  await insertTable(page, 2, 2);
  await page.locator('[data-table-select="row"]').click();
  await expect(page.locator(".table-cell-selection")).toHaveCount(2);
  // Measured while the fill is on screen, because the second half of this test needs
  // to click OUTSIDE it after the selection is gone.
  const fill = await selectionFillBox(page);
  // What the last undoable action is, before the gesture. A refused edit must not
  // move this — which is how "nothing was applied" is asserted about the DOCUMENT
  // rather than about the absence of a repaint.
  const undoBefore = await page.locator("#undoBtn").getAttribute("title");

  await page.locator("#tableBtn").click();
  await expect(page.locator("#tableMenu")).toBeVisible();
  await page.locator('#cellVAlign [data-valign="center"]').click();

  expect(await statusText(page)).toBe(
    "Shading, alignment and cell borders apply to one cell — put the caret in the cell to format it",
  );
  await expect(page.locator("#status")).toHaveClass(/error/);
  expect(
    await page.locator("#undoBtn").getAttribute("title"),
    "an interim refusal must leave the document exactly as it was",
  ).toBe(undoBefore);

  // With no row selection the same gesture still WORKS — the refusal is about the
  // selection, not about the command, and a guard that only proved the refusal
  // would be satisfied by breaking cell formatting outright.
  // One click in the row below: it closes the popover AND leaves the selection, which
  // is the state the same gesture has to succeed in.
  await clickBelow(page, fill);
  await expect(page.locator(".table-cell-selection")).toHaveCount(0);
  await page.locator("#tableBtn").click();
  await page.locator('#cellVAlign [data-valign="center"]').click();
  await expect(page.locator('#cellVAlign [data-valign="center"]')).toHaveAttribute(
    "aria-checked",
    "true",
  );
  expect(await page.locator("#undoBtn").getAttribute("title")).not.toBe(undoBefore);
});

test("a refused table edit never puts the engine's own prose on the status line", async ({
  page,
}) => {
  // `runNodeEdit` used to do `setStatus(err?.message ?? …)`, bypassing
  // `edit_errors.mjs` — the module that exists to keep internal vocabulary away from
  // a reader — so a refused table change announced the facade's wording verbatim and
  // in English only (`docs/141` TBL-04). The table formula is the shortest real path
  // to an engine refusal on that function.
  await insertTable(page, 3, 3);
  await page.locator('[data-table-select="row"]').click();
  await page.locator("#mergeCellsBtn").click();
  await expect(page.locator("#tableContext")).toContainText("merged/spanned");

  await page.locator("#tablePropertiesBtn").click();
  await expect(page.locator("#tablePropertiesPanel")).toBeVisible();
  await page.locator("#tableFormula").fill("=SUM(ABOVE)");
  await page.locator("#tableFormulaApply").click();

  const said = await statusText(page);
  await expect(page.locator("#status")).toHaveClass(/error/);
  // The guarantee, not the sentence: nothing the ENGINE says about its own internals
  // reaches the reader. The facade's wording here is "formulas require a regular
  // table" — readable-looking, unlocalised, and not ours to show.
  expect(said).not.toMatch(/regular table|formulas require|Unsupported|CrossParagraph/);
  expect(said).toBe("That edit isn't supported for this selection yet");
});

test("a disabled Table-band control states its reason and looks disabled", async ({ page }) => {
  await insertTable(page, 3, 3);
  const sort = page.locator('[data-table-sort="ascending"]');
  // The authored tooltip while the command is available.
  await expect(sort).toBeEnabled();
  await expect(sort).toHaveAttribute("title", "Sort rows ascending");

  // CREATE THE CONDITION. A merged table is what disables nine of these, and
  // measuring the band's default state is the mistake the `.dialog-button` guard
  // made a day earlier: it passed under a mutation that reinstated the defect.
  await page.locator('[data-table-select="row"]').click();
  await page.locator("#mergeCellsBtn").click();
  await expect(page.locator("#tableContext")).toContainText("merged/spanned");

  await expect(sort).toBeDisabled();
  await expect(
    sort,
    "a disabled control with no reason cannot be told from a broken one; it used to " +
      "keep advertising 'Sort rows ascending' (`docs/141` TBL-03)",
  ).toHaveAttribute("title", "Unavailable for merged or spanned tables");

  // Every disabled control in the band, not just this one — the class, not the
  // instance. And each must LOOK disabled, the property `.dialog-button` failed.
  const band = await page.evaluate(() => {
    const authored = [];
    for (const button of document.querySelectorAll(".table-ribbon button")) {
      if (!button.disabled) continue;
      const style = getComputedStyle(button);
      authored.push({
        id: button.id || button.dataset.tableAction || button.dataset.tableSort ||
          button.dataset.tableDistribute || button.dataset.tableSelect,
        title: button.title,
        opacity: Number.parseFloat(style.opacity),
        cursor: style.cursor,
      });
    }
    return authored;
  });
  expect(band.length, "no disabled band control was on screen, so this run proves nothing")
    .toBeGreaterThan(0);
  const REASONS = new Set([
    "Place the caret in a table",
    "Unavailable for merged or spanned tables",
    "Rows need a fixed or minimum height before distribution",
    "Select a row, column, or table before merging",
  ]);
  expect(
    band.filter((control) => !REASONS.has(control.title)).map((c) => `${c.id}: ${c.title}`),
    "a disabled Table-band control is still advertising what it would do",
  ).toEqual([]);
  expect(
    band.filter((c) => c.opacity > 0.9 && c.cursor === "pointer").map((c) => c.id),
    "these render exactly like live controls — the `.dialog-button` defect, in the band",
  ).toEqual([]);
  // `#mergeCellsBtn` is in that sweep and it is the one the pointer has TOUCHED, which
  // is the case the hover tooltip used to break: `armTip` parks `title` in
  // `data-tip-title` and removes the attribute, and `disarmTip` put the parked copy
  // back over the reason the sweep had written in the meantime — permanently, on
  // exactly the controls a reader had hovered to find out why.
  const merge = band.find((control) => control.id === "mergeCellsBtn");
  expect(merge, "Merge must be disabled once the merge has consumed the selection").toBeTruthy();
  expect(merge.title).toBe("Select a row, column, or table before merging");

  // …and the authored tooltip comes BACK rather than the reason sticking, on that same
  // hovered control. Unmerging restores the grid, then reselecting a row restores the
  // precondition Merge was refused for.
  const centre = await activeCellCentre(page);
  await page.mouse.click(centre.x, centre.y, { button: "right" });
  await page.locator('.editor-context-menu [data-command-id="table.unmerge"]').click();
  await expect(page.locator("#tableContext")).not.toContainText("merged/spanned");
  await expect(sort).toBeEnabled();
  await expect(sort).toHaveAttribute("title", "Sort rows ascending");
  await page.locator('[data-table-select="row"]').click();
  await expect(page.locator("#mergeCellsBtn")).toBeEnabled();
  await expect(page.locator("#mergeCellsBtn")).toHaveAttribute("title", "Merge selected cells");
});

test("a click inside a row selection keeps it; a click outside drops it", async ({ page }) => {
  await insertTable(page, 2, 2);
  await page.locator('[data-table-select="row"]').click();
  await expect(page.locator(".table-cell-selection")).toHaveCount(2);
  expect(await statusText(page)).toBe("Row selected");

  // The gesture: clicking the row you just selected, to confirm it. Every
  // left-click used to clear the selection, so Merge cells went grey again and
  // nothing said why (`docs/141` TBL-05).
  const box = await selectionFillBox(page);
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(
    page.locator(".table-cell-selection"),
    "a left-click inside the fill must not destroy the selection it is confirming",
  ).toHaveCount(2);
  await expect(page.locator("#mergeCellsBtn")).toBeEnabled();

  // A click OUTSIDE it still drops it, which is the half that keeps the selection
  // dismissible — Google Docs' rule, and the reason this is not just "never clear".
  await clickBelow(page, box);
  await expect(page.locator(".table-cell-selection")).toHaveCount(0);
  await expect(page.locator("#mergeCellsBtn")).toBeDisabled();
});
