// Regression coverage for docs/67's highest-priority merged-main editing gaps:
// history must reflect real availability, a typing burst/paste/replace-with-break
// must each undo as one user action, and plain horizontal navigation must collapse
// a range to its ordered edge before the next insertion.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  MOD,
} from "./fixtures.mjs";

async function findStatusFor(page, query) {
  // `findText` does not yet wrap a query whose current caret lies inside that
  // same match; start from the document boundary so this test measures editing,
  // not that separately tracked search limitation.
  await moveCaretToDocStart(page);
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(query);
  const status = await page.locator("#findStatus").textContent();
  await page.keyboard.press("Escape");
  return status;
}

async function pastePlainText(page, text) {
  await page.evaluate((value) => {
    const data = new DataTransfer();
    data.setData("text/plain", value);
    document.dispatchEvent(
      new ClipboardEvent("paste", {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      }),
    );
  }, text);
}

test("history buttons reflect real state and one typing burst undoes/redoes once", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await expect(page.locator("#undoBtn")).toBeDisabled();
  await expect(page.locator("#redoBtn")).toBeDisabled();
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const marker = "GROUPED_TYPING_MARKER";
  await page.keyboard.type(marker);
  await expect(page.locator("#undoBtn")).toBeEnabled();
  await expect(page.locator("#redoBtn")).toBeDisabled();

  await page.locator("#undoBtn").click();
  await expect(page.locator("#undoBtn")).toBeDisabled();
  await expect(page.locator("#redoBtn")).toBeEnabled();
  expect(await findStatusFor(page, marker)).toBe("No match");

  await page.locator("#redoBtn").click();
  await expect(page.locator("#undoBtn")).toBeEnabled();
  await expect(page.locator("#redoBtn")).toBeDisabled();
  expect(await findStatusFor(page, marker)).toBe("1 match");

  await page.locator("#undoBtn").click();
  expect(consoleErrors).toEqual([]);
});

test("multiline paste and Enter over a selection are each atomic", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const first = "ATOMIC_PASTE_LINE_ONE";
  const second = "ATOMIC_PASTE_LINE_TWO";
  await pastePlainText(page, `${first}\r\n${second}`);
  await expect.poll(() => findStatusFor(page, second)).toBe("1 match");
  await page.locator("#undoBtn").click();
  expect(await findStatusFor(page, first)).toBe("No match");
  expect(await findStatusFor(page, second)).toBe("No match");

  await moveCaretToDocStart(page);
  const marker = "ENTER_REPLACE_MARKER";
  await page.keyboard.type(marker);
  for (let i = 0; i < 6; i++) await page.keyboard.press("Shift+ArrowLeft");
  await page.keyboard.press("Enter");
  expect(await findStatusFor(page, marker)).toBe("No match");

  // One undo must restore both the deleted selection and the paragraph split.
  await page.locator("#undoBtn").click();
  expect(await findStatusFor(page, marker)).toBe("1 match");
  await page.locator("#undoBtn").click();
  expect(consoleErrors).toEqual([]);
});

test("plain Left collapses a backward range to its ordered start", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await page.keyboard.type("COLLAPSEABCDE");
  for (let i = 0; i < 4; i++) await page.keyboard.press("Shift+ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.type("X");

  expect(await findStatusFor(page, "COLLAPSEAXBCDE")).toBe("1 match");
  expect(await findStatusFor(page, "COLLAPSEXABCDE")).toBe("No match");

  await page.locator("#undoBtn").click(); // X
  await page.locator("#undoBtn").click(); // original typing burst
  expect(consoleErrors).toEqual([]);
});

test("native-platform word movement and deletion use one semantic engine action", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const marker = "WORD_DELETE_ALPHA BETA";
  await page.keyboard.type(marker);
  await page.keyboard.press("Alt+ArrowLeft");
  await page.keyboard.type("X");
  expect(await findStatusFor(page, "WORD_DELETE_ALPHA XBETA")).toBe("1 match");

  await page.keyboard.press(`${MOD}+z`); // inserted X
  expect(await findStatusFor(page, marker)).toBe("1 match");
  await moveCaretToDocStart(page);
  for (let i = 0; i < marker.length; i++) await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Alt+Backspace");
  expect(await findStatusFor(page, marker)).toBe("No match");
  expect(await findStatusFor(page, "WORD_DELETE_ALPHA ")).toBe("1 match");

  await page.keyboard.press(`${MOD}+z`);
  await expect(page.locator("#undoBtn")).toBeEnabled();
  expect(await findStatusFor(page, marker)).toBe("1 match");
  await page.keyboard.press(`${MOD}+z`); // original typing burst
  expect(consoleErrors).toEqual([]);
});

test("Page Down moves by the editor viewport and Shift extends the model selection", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const lines = Array.from(
    { length: 90 },
    (_, index) => `PAGE_NAVIGATION_LINE_${String(index).padStart(3, "0")}`,
  ).join("\n");
  await pastePlainText(page, lines);
  await expect
    .poll(async () => Number((await page.locator("#statPages").textContent()).match(/of (\d+)/)?.[1]))
    .toBeGreaterThan(1);
  await moveCaretToDocStart(page);

  const caretPosition = () =>
    page.evaluate(() => {
      const caret = document.querySelector(".overlay .caret");
      const wraps = [...document.querySelectorAll(".page-wrap")];
      return caret
        ? {
            page: wraps.indexOf(caret.closest(".page-wrap")),
            top: Number.parseFloat(caret.style.top),
          }
        : null;
    });
  const before = await caretPosition();
  await page.keyboard.press("PageDown");
  await expect.poll(caretPosition).not.toEqual(before);

  await page.keyboard.press("Shift+PageDown");
  await expect(page.locator(".overlay .highlight")).not.toHaveCount(0);
  await page.keyboard.press("PageUp");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  await page.locator("#undoBtn").click();
  expect(consoleErrors).toEqual([]);
});
