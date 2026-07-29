// Automates the "IME live preedit" P0 row from
// docs/67-EDITOR-UX-GAP-ANALYSIS.md: composition text must be visible at the
// caret (compositionupdate) without ever being committed to the document
// until compositionend — the gap this closes is that no compositionupdate
// listener existed at all (P1G-IME-001 only handled start/end).
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
} from "./fixtures.mjs";

const MOD = process.platform === "darwin" ? "Meta" : "Control";

async function findStatusFor(page, query) {
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(query);
  const status = await page.locator("#findStatus").textContent();
  await page.keyboard.press("Escape");
  return status;
}

test("live preedit shows composing text without committing it, then commits on compositionend", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await page.evaluate(() => {
    document.dispatchEvent(
      new CompositionEvent("compositionstart", { data: "", bubbles: true, cancelable: true }),
    );
  });
  await page.evaluate(() => {
    document.dispatchEvent(
      new CompositionEvent("compositionupdate", {
        data: "PREEDITWORD",
        bubbles: true,
        cancelable: true,
      }),
    );
  });

  await expect(page.locator(".ime-preedit")).toHaveText("PREEDITWORD");
  expect(await findStatusFor(page, "PREEDITWORD")).toBe("No match");

  await page.evaluate(() => {
    document.dispatchEvent(
      new CompositionEvent("compositionend", {
        data: "PREEDITWORD",
        bubbles: true,
        cancelable: true,
      }),
    );
  });

  await expect(page.locator(".ime-preedit")).toHaveCount(0);
  expect(await findStatusFor(page, "PREEDITWORD")).toBe("1 match");

  await page.locator("#undoBtn").click();
  expect(consoleErrors).toEqual([]);
});
