// Two keyboard/clipboard parity fixes from the editing-standard audit (P2):
//   1. macOS ⌘Backspace deletes from the caret to the start of the line.
//   2. The paste-options chip offers "Merge formatting" — keep the pasted
//      emphasis (bold/italic/underline) but adopt the destination's font,
//      size, and color.
// The ⌘-chord direction logic is also unit-tested in tests/keyboard.test.mjs;
// this exercises the real delete/paste behaviour end-to-end.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

async function dispatchClipboardEvent(page, type, data = {}) {
  return page.evaluate(
    ({ type, data }) => {
      const dt = new DataTransfer();
      for (const [mime, value] of Object.entries(data)) dt.setData(mime, value);
      const event = new ClipboardEvent(type, {
        clipboardData: dt,
        bubbles: true,
        cancelable: true,
      });
      document.dispatchEvent(event);
    },
    { type, data },
  );
}

async function findCount(page, query) {
  await page.keyboard.press(`${process.platform === "darwin" ? "Meta" : "Control"}+f`);
  await page.locator("#findInput").fill(query);
  const status = await page.locator("#findStatus").textContent();
  await page.keyboard.press("Escape");
  return status;
}

test("macOS Cmd+Backspace deletes from the caret to the line start (one undo restores it)", async ({
  page,
  consoleErrors,
}) => {
  // The editor keymap is derived from the real navigator at load, so this
  // ⌘ chord only reaches the engine on an Apple platform. On Windows/Linux CI
  // the chord is left to the browser, so the scenario is not applicable there
  // (the direction logic itself is covered by the platform-parameterized unit
  // test in tests/keyboard.test.mjs).
  test.skip(process.platform !== "darwin", "Cmd+Backspace is a macOS-only chord");

  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Type a marker at the very start of the first line; the caret ends right
  // after it, so the whole marker sits between the caret and the line start.
  const marker = "LINEKILLME";
  await page.keyboard.type(marker);
  await expect.poll(() => findCount(page, marker)).toBe("1 match");

  await page.keyboard.press("Meta+Backspace");
  // Everything before the caret on that line (the marker) is gone.
  await expect.poll(() => findCount(page, marker)).toBe("No match");

  // A single undo restores the deleted span.
  await page.locator("#undoBtn").click();
  await expect.poll(() => findCount(page, marker)).toBe("1 match");

  await page.locator("#undoBtn").click(); // typed marker
  expect(consoleErrors).toEqual([]);
});

test("paste options: 'Merge formatting' keeps emphasis but drops the pasted font/size/color", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Land on a genuinely non-bold body line so the toolbar's pressed/size state
  // reflects the pasted (then merged) run, not an inherited heading style.
  for (let line = 0; line < 20; line++) {
    if ((await page.locator("#bold").getAttribute("aria-pressed")) === "false") break;
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Home");
  }
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "false");

  // Paste a run that is bold + italic AND carries a distinctive font family,
  // size, and color — the properties Merge formatting must strip.
  const marker = "MERGEFMT";
  await dispatchClipboardEvent(page, "paste", {
    "text/html": `<span style="font-weight:700;font-style:italic;font-size:40pt;color:#ff0000;font-family:Georgia">${marker}</span>`,
    "text/plain": marker,
  });

  // The paste-options chip now offers "Merge formatting". Click it before any
  // caret/selection keystroke, which would dismiss the chip (matching how the
  // user reaches for the affordance right after pasting).
  const mergeBtn = page.locator("#pasteOptionsMerge");
  await expect(mergeBtn).toBeVisible();
  await mergeBtn.click();

  // The merged run keeps bold + italic but sheds the pasted font (Georgia),
  // size (40), and color — inheriting the destination paragraph instead.
  await page.keyboard.press("Shift+Home");
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#italic")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#fontSize")).not.toHaveValue("40");
  await expect(page.locator("#fontFamilyLabel")).not.toHaveText("Georgia");

  await page.locator("#undoBtn").click(); // the merged paste
  expect(consoleErrors).toEqual([]);
});

// HF-046 (docs/104-HOTFIX-TRACKER.md): the chip's two actions work by undoing
// the paste and redoing it differently. Nothing checked that the paste was still
// the change an undo would remove, and no edit ever retired the offer — so
// pressing ⌘Z first (dropping the paste, chip still on screen) and then clicking
// "Keep text only" deleted the sentence typed BEFORE the paste and replaced it
// with the clipboard. The offer now expires with the edit it is about.
test("paste options: the chip retires when any later edit lands, so it cannot undo someone else's work", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("TYPEDFIRST");

  await dispatchClipboardEvent(page, "paste", {
    "text/html": "<b>PASTEDAFTER</b>",
    "text/plain": "PASTEDAFTER",
  });
  await expect(page.locator("#pasteOptions")).toBeVisible();

  // ⌘Z drops the paste. The chip is an offer about that paste; with the paste
  // gone, the offer goes with it.
  await page.keyboard.press(`${process.platform === "darwin" ? "Meta" : "Control"}+z`);
  await expect(page.locator("#pasteOptions")).toBeHidden();

  // The undo removed the paste and only the paste: the typed sentence survives.
  await expect
    .poll(() =>
      page.evaluate(() => {
        const el = [...document.querySelectorAll("#a11yDocument p")].find((node) =>
          (node.textContent || "").includes("TYPEDFIRST"),
        );
        return el ? el.textContent : null;
      }),
    )
    .toBe("TYPEDFIRST");

  await page.locator("#undoBtn").click(); // the typed marker
  expect(consoleErrors).toEqual([]);
});
