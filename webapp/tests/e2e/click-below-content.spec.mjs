// A click anywhere on a page puts the caret SOMEWHERE, and the next keystroke
// goes into the document.
//
// A click (or a drag) in the empty part of a page below the last paragraph
// placed no caret: the hit-test found nothing there, the handler returned
// without a position, focus fell to <body>, and the next typing was discarded
// with no message — and Cmd+Z stopped working too, because the keystrokes never
// reached the editor (`docs/174` OL-01, `docs/109` HF-301). The press that
// lost it was on the grey scroll surface below the last page, which is not
// focusable; a press ON a page always resolved to the nearest line. Word, Google Docs and
// ONLYOFFICE all put the caret on the nearest line: below the last line, the end
// of the last paragraph.
//
// Every case types after the gesture and asserts BOTH that the text reached the
// saved document and that the editor kept focus, because each alone has a false
// green: focus can sit on the proxy while the engine refuses the edit, and text
// can land while focus is lost for the next keystroke.
import { readFile } from "node:fs/promises";
import {
  test,
  expect,
  clickIntoFirstPage,
  expectEditorFocused,
  runAppMenuCommand,
  stableBox,
  MOD,
} from "./fixtures.mjs";
import { opcPart } from "./opc-part.mjs";

async function blankDocumentWithText(page, text) {
  await page.goto("/editor.html?blank=1");
  await page.waitForFunction(
    () => document.getElementById("status")?.textContent.startsWith("Ready — open"),
    null,
    { timeout: 45_000 },
  );
  await runAppMenuCommand(page, "file", "file.new");
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });
  await clickIntoFirstPage(page);
  await page.locator("#pages").pressSequentially(text);
}

/** Every saved `w:t`, concatenated: what a reader of the saved file sees. */
async function savedText(page) {
  const download = page.waitForEvent("download");
  await page.keyboard.press(`${MOD}+s`);
  const xml = opcPart(await readFile(await (await download).path()), "word/document.xml");
  return [...xml.matchAll(/<w:t(?:\s[^>]*)?>([^<]*)<\/w:t>/g)].map((m) => m[1]).join("");
}

/** The lowest y of the page that is on screen and not under the footer. */
async function lowestVisibleY(page, box) {
  const footTop = await page.evaluate(() => {
    const foot = document.querySelector(".footer");
    return foot ? foot.getBoundingClientRect().top : window.innerHeight;
  });
  return Math.min(box.y + box.height - 4, footTop - 6);
}

const GESTURES = {
  "a click below the last paragraph": async (page, box) => {
    await page.mouse.click(box.x + box.width / 2, box.y + 300);
  },
  "a click at the bottom of the visible page": async (page, box) => {
    await page.mouse.click(box.x + 300, await lowestVisibleY(page, box));
  },
  "a drag below the last paragraph": async (page, box) => {
    await page.mouse.move(box.x + box.width * 0.3, box.y + 300);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.6, box.y + 450, { steps: 8 });
    await page.mouse.up();
  },
  "a click on the grey surface below the last page": async (page, box) => {
    await page.mouse.click(box.x + box.width / 2, box.y + box.height + 40);
  },
  "a click below the last page with the caret at the start": async (page, box) => {
    // The caret is elsewhere first, so "it went to the end" is the click's doing.
    await page.keyboard.press(`${MOD}+Home`);
    await page.mouse.click(box.x + box.width / 2, box.y + box.height + 40);
  },
  "a click on the grey surface beside the page": async (page, box) => {
    await page.mouse.click(box.x - 60, box.y + 300);
  },
  "a drag from below the text off the bottom of the window": async (page, box) => {
    await page.mouse.move(box.x + box.width * 0.3, box.y + 300);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.6, (await page.viewportSize()).height + 60, { steps: 8 });
    await page.mouse.up();
  },
};

for (const [gesture, act] of Object.entries(GESTURES)) {
  test(`typing after ${gesture} lands at the end of the last paragraph`, async ({
    page,
    consoleErrors,
  }) => {
    // Tall enough that the grey surface below the one page is on screen.
    await page.setViewportSize({ width: 1440, height: 1400 });
    await blankDocumentWithText(page, "Alpha");
    const box = await stableBox(page.locator(".page-wrap .page").first());
    await act(page, box);
    await expectEditorFocused(page);
    await page.keyboard.type("Zq");
    await expect(page.locator("#a11yDocument")).toContainText("AlphaZq");
    await expectEditorFocused(page);
    // And the keystroke after that is still the editor's: undo works.
    await page.keyboard.press(`${MOD}+z`);
    await expect(page.locator("#a11yDocument")).not.toContainText("AlphaZq");
    await page.keyboard.type("Zq");
    expect(await savedText(page)).toBe("AlphaZq");
    expect(consoleErrors).toEqual([]);
  });
}
