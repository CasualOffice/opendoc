// The caret and story contract, from the keyboard and the pointer
// (`docs/109` UX-057..UX-063; audit CRITIC-01, CRITIC-03, CANVAS-01..05,
// CANVAS-V-01).
//
// Operation × container. Each test asserts the GUARANTEE a reader relies on,
// never the mechanism: after Ctrl+End and typing, the BODY gained the text; a
// Tab-ed bullet is drawn where the nested bullet is; text typed after a paste
// that ended in a table is not in front of the pasted heading; the cell the
// caret was in still receives the typing after a row-border drag.
//
// `sample.docx` is the container matrix: three sections with a header and a
// footer each, bullets whose list comes from the List Bullet STYLE (the case
// every list command used to miss), an Arabic/Hebrew paragraph, and tables.
import {
  MOD,
  clickIntoFirstPage,
  expect,
  expectEditorFocused,
  gotoEditor,
  gotoSampleDocument,
  reflectedParagraphStyle,
  stableBox,
  test,
} from "./fixtures.mjs";

/** The body's text, as the accessibility mirror projects it (headers and
 *  footers are not part of it). */
const bodyText = (page) => page.locator("#a11yDocument").innerText();

/** Puts the selection on `text` with Find; Escape hands focus back to the page
 *  with the match selected. */
async function selectWithFind(page, text) {
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(text);
  await expect(page.locator("#findStatus")).not.toHaveText("No match");
  await page.keyboard.press("Escape");
  await expectEditorFocused(page);
}

/** Puts a collapsed caret at the START of `text` (Latin text: ArrowLeft
 *  collapses a range to its start). */
async function caretBefore(page, text) {
  await selectWithFind(page, text);
  await page.keyboard.press("ArrowLeft");
}

/** The painted caret's client rect. */
async function caretRect(page) {
  const caret = page.locator(".overlay .caret");
  await expect(caret).toHaveCount(1);
  return stableBox(caret);
}

/** Dispatches a clipboard event the way the OS does; returns what was written. */
function clipboard(page, type, data = {}) {
  return page.evaluate(
    ({ type, data }) => {
      const dt = new DataTransfer();
      for (const [mime, value] of Object.entries(data)) dt.setData(mime, value);
      document.dispatchEvent(new ClipboardEvent(type, { clipboardData: dt, bubbles: true, cancelable: true }));
      return dt.getData("text/plain");
    },
    { type, data },
  );
}

test.describe("navigation never enters a header or footer (CRITIC-01)", () => {
  test("Ctrl+End then typing edits the end of the BODY, not every page's footer", async ({ page, consoleErrors }) => {
    await gotoSampleDocument(page);
    await clickIntoFirstPage(page);
    await page.keyboard.press(`${MOD}+End`);
    expect(await reflectedParagraphStyle(page), "the caret is not in a footer").not.toBe("Footer");
    await page.keyboard.type("QQENDMARK");
    await expect.poll(() => bodyText(page), { message: "the body gained the text" }).toContain("QQENDMARK");

    await page.keyboard.press(`${MOD}+Home`);
    expect(await reflectedParagraphStyle(page)).not.toBe("Header");
    await page.keyboard.type("QQSTARTMARK");
    await expect.poll(() => bodyText(page)).toContain("QQSTARTMARK");
    expect(consoleErrors).toEqual([]);
  });

  test("Select All selects the body: no header text comes with it", async ({ page, consoleErrors }) => {
    await gotoSampleDocument(page);
    await clickIntoFirstPage(page);
    await page.keyboard.press(`${MOD}+a`);
    await expect(page.locator("#status")).toContainText("Document selected");
    const copied = await clipboard(page, "copy");
    expect(copied.length, "precondition: the body was copied").toBeGreaterThan(1000);
    expect(copied, "the header story is not part of the document selection").not.toContain(
      "OpenDoc • Compatibility Fixture",
    );
    expect(consoleErrors).toEqual([]);
  });
});

test.describe("list levels behave like Word on style-numbered bullets (CANVAS-01/02)", () => {
  test("Tab moves the item one level RIGHT, Shift+Tab back, and never into the margin", async ({ page, consoleErrors }) => {
    await gotoSampleDocument(page);
    await clickIntoFirstPage(page);
    await caretBefore(page, "Nested bullet");
    const nested = (await caretRect(page)).x;
    await caretBefore(page, "Second bullet");
    const home = (await caretRect(page)).x;
    expect(nested, "precondition: the nested bullet is further in").toBeGreaterThan(home + 8);
    await expect(page.locator("#bulletList"), "a List Bullet item is a list item").toHaveAttribute("aria-pressed", "true");

    await page.keyboard.press("Tab");
    await expect.poll(async () => Math.round((await caretRect(page)).x)).toBe(Math.round(nested));
    await expect(page.locator("#bulletList"), "it keeps its bullet").toHaveAttribute("aria-pressed", "true");

    await page.keyboard.press("Shift+Tab");
    await expect.poll(async () => Math.round((await caretRect(page)).x)).toBe(Math.round(home));
    // Already outermost: nothing moves left, and the refusal is said.
    await page.keyboard.press("Shift+Tab");
    await expect(page.locator("#status")).toContainText("outermost");
    expect(Math.round((await caretRect(page)).x)).toBe(Math.round(home));
    expect(consoleErrors).toEqual([]);
  });

  test("Enter on an empty list item ends the list", async ({ page, consoleErrors }) => {
    await gotoSampleDocument(page);
    await clickIntoFirstPage(page);
    await caretBefore(page, "Second bullet");
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await expect(page.locator("#bulletList"), "a new, empty bullet").toHaveAttribute("aria-pressed", "true");
    await page.keyboard.press("Enter");
    await expect(page.locator("#bulletList"), "out of the list").toHaveAttribute("aria-pressed", "false");
    await page.keyboard.type("QQOUTOFLIST");
    await expect.poll(() => bodyText(page)).toContain("QQOUTOFLIST");
    await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "false");
    expect(consoleErrors).toEqual([]);
  });
});

test("after pasting HTML that ends in a table, typing goes AFTER the paste (CRITIC-03)", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await clipboard(page, "paste", {
    "text/html": "<h2>Pasted heading</h2><p>Pasted body</p><ul><li>Pasted item</li></ul><table><tr><td>Pasted cell</td></tr></table>",
    "text/plain": "Pasted heading\nPasted body\nPasted item\nPasted cell",
  });
  await expect.poll(() => bodyText(page)).toContain("Pasted cell");
  await page.keyboard.type("QQAFTERPASTE");
  await expect.poll(() => bodyText(page)).toContain("QQAFTERPASTE");
  const text = await bodyText(page);
  expect(text, "not in front of the pasted heading").not.toContain("QQAFTERPASTEPasted heading");
  expect(text.indexOf("QQAFTERPASTE"), "after the pasted table").toBeGreaterThan(text.indexOf("Pasted cell"));
  expect(consoleErrors).toEqual([]);
});

test("ArrowLeft in right-to-left text moves the caret LEFT on screen (CANVAS-03)", async ({ page, consoleErrors }) => {
  await gotoSampleDocument(page);
  await clickIntoFirstPage(page);
  await selectWithFind(page, "המסמך");
  await page.keyboard.press("ArrowLeft"); // collapses the match
  const xs = [(await caretRect(page)).x];
  for (let press = 0; press < 3; press++) {
    await page.keyboard.press("ArrowLeft");
    await expect.poll(async () => (await caretRect(page)).x).not.toBe(xs.at(-1));
    xs.push((await caretRect(page)).x);
  }
  for (let i = 1; i < xs.length; i++) expect(xs[i], `press ${i} moved left: ${xs.join(", ")}`).toBeLessThan(xs[i - 1]);
  expect(consoleErrors).toEqual([]);
});

test("double-click-drag selects whole words from the first word (CANVAS-04)", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.press("Enter");
  await page.keyboard.type("QQalpha QQbeta QQgamma QQdelta");
  await caretBefore(page, "QQbeta");
  const beta = await caretRect(page);
  await caretBefore(page, "QQdelta");
  const delta = await caretRect(page);
  const y = beta.y + beta.height / 2;

  await page.mouse.move(beta.x + 12, y);
  await page.mouse.down({ clickCount: 1 });
  await page.mouse.up({ clickCount: 1 });
  await page.mouse.down({ clickCount: 2 });
  await page.mouse.move(delta.x + 12, delta.y + delta.height / 2, { steps: 6 });
  await page.mouse.up({ clickCount: 2 });

  expect(await clipboard(page, "copy")).toBe("QQbeta QQgamma QQdelta");
  expect(consoleErrors).toEqual([]);
});

test("the document opens focused, with a caret at the start of the body (CANVAS-05)", async ({ page, consoleErrors }) => {
  await gotoSampleDocument(page);
  await expectEditorFocused(page);
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
  await page.keyboard.type("QQTYPED");
  await expect.poll(() => bodyText(page)).toContain("QQTYPED");
  expect(consoleErrors).toEqual([]);
});

test("dragging a row border leaves the caret in its cell (CANVAS-V-01)", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.press("Enter");
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="3"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  // Row 2, column 2: four cells on from the first.
  for (let step = 0; step < 4; step++) await page.keyboard.press("Tab");
  await page.keyboard.type("QQKEEP");
  const cell = await stableBox(page.locator(".overlay .cell-outline"));

  // The border between rows 1 and 2, found the way a reader finds it: by the cursor.
  const canvas = await stableBox(page.locator(".page-wrap .page").first());
  const x = cell.x - cell.width / 2;
  let found = null;
  for (let y = cell.y - 8; y <= cell.y + 8; y += 1) {
    await page.mouse.move(x, y);
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
    const target = await page.evaluate(
      () => document.querySelector(".page-wrap .page")?.dataset.pointerTarget ?? "",
    );
    if (target === "table-row-boundary") {
      found = y;
      break;
    }
  }
  expect(found, "a row boundary between rows 1 and 2").not.toBeNull();
  expect(canvas.width).toBeGreaterThan(0);
  await page.mouse.down();
  await page.mouse.move(x, found + 40, { steps: 6 });
  await page.mouse.up();
  await expect(page.locator("#status")).toContainText("Row height");

  await page.keyboard.type("Z");
  await expect.poll(() => bodyText(page), { message: "the typing stayed in the caret's cell" }).toContain("QQKEEPZ");
  expect(consoleErrors).toEqual([]);
});
