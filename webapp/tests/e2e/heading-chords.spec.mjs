// Ctrl+Alt+1/2/3 apply Heading 1/2/3 and Ctrl+Alt+0 applies Normal — Word's
// heading chords and Google Docs' — where the editor had none.
//
// `?fixture=styled` is the document for it because it stores Word's built-ins
// the way Word does, `heading 1` / `heading 2` in lower case, and defines no
// Heading 3. So it proves three things at once: the chord resolves the
// document's own spelling (an exact-name lookup for "Heading 1" would find
// nothing here), the style lands under its STORED name, and a chord whose style
// the document lacks refuses out loud instead of doing nothing.
import { test, expect, MOD, openFilePage, shortcutHint } from "./fixtures.mjs";

async function gotoStyled(page) {
  await page.goto("/editor.html?fixture=styled");
  await page.waitForFunction(() => document.body.classList.contains("doc-loaded"), null, { timeout: 45_000 });
  await page.locator(".page-wrap .page").first().click({ position: { x: 120, y: 120 } });
  await expect(page.locator("#stylesTrigger")).toBeEnabled();
  // The LAST paragraph carries no style. The first one is already `heading 1`,
  // so starting there would let Ctrl+Alt+1 "pass" by doing nothing at all.
  await page.keyboard.press(`${MOD}+End`);
  await expect.poll(() => activeStyle(page)).toBe("");
}

const activeStyle = (page) => page.locator("#stylesTrigger").getAttribute("data-active-style");

test("the heading chords apply the document's own Heading 1, Heading 2 and Normal", async ({
  page,
  consoleErrors,
}) => {
  await gotoStyled(page);

  await page.keyboard.press(`${MOD}+Alt+1`);
  await expect.poll(() => activeStyle(page)).toBe("heading 1");
  await expect(page.locator("#stylesTriggerLabel")).toHaveText("Heading 1");

  await page.keyboard.press(`${MOD}+Alt+2`);
  await expect.poll(() => activeStyle(page)).toBe("heading 2");

  await page.keyboard.press(`${MOD}+Alt+0`);
  await expect.poll(() => activeStyle(page)).toBe("Normal");

  // One chord, one undo step — it is a style change, not a sequence of edits.
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(() => activeStyle(page)).toBe("heading 2");
  expect(consoleErrors).toEqual([]);
});

test("a heading the document does not define is refused with the reason, and nothing changes", async ({
  page,
  consoleErrors,
}) => {
  await gotoStyled(page);
  await page.keyboard.press(`${MOD}+Alt+1`);
  await expect.poll(() => activeStyle(page)).toBe("heading 1");

  await page.keyboard.press(`${MOD}+Alt+3`);
  await expect(page.locator("#status")).toHaveText("This document has no Heading 3 style");
  await expect.poll(() => activeStyle(page)).toBe("heading 1");
  expect(consoleErrors).toEqual([]);
});

test("the heading chords are listed in the palette and in File ▸ Shortcuts", async ({ page, consoleErrors }) => {
  await gotoStyled(page);
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("Apply Heading");
  for (const [id, chord] of [
    ["paragraph.heading.1", "⌘⌥1"],
    ["paragraph.heading.2", "⌘⌥2"],
    ["paragraph.heading.3", "⌘⌥3"],
  ]) {
    await expect(page.locator(`#cmdList .cmd-item[data-command-id="${id}"]`)).toHaveAttribute(
      "data-command-shortcut",
      shortcutHint(chord),
    );
  }
  await page.keyboard.press("Escape");

  await openFilePage(page);
  await page.locator('#filePageBody [data-file-pane="shortcuts"]').click();
  // One row for the three headings, as Google Docs' own reference lists them —
  // four rows that differ by a digit cost the 1280x720 card the height
  // `dialog-fit` holds it to — and Normal on its own.
  const rows = page.locator("#shortcutsDialog .shortcuts-row");
  await expect(rows.filter({ hasText: "Apply Heading 1–3" }).locator(".shortcuts-keys")).toHaveText(
    `${shortcutHint("⌘⌥1")}–3`,
  );
  await expect(rows.filter({ hasText: "Apply Normal style" }).locator(".shortcuts-keys")).toHaveText(shortcutHint("⌘⌥0"));
  expect(consoleErrors).toEqual([]);
});
