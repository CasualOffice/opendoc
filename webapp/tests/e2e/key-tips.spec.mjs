// Alt key tips over the ribbon (`109` UX-044).
//
// Word and ONLYOFFICE both draw a letter on every tab when Alt is pressed and
// released; a letter opens that tab, letters on its controls run them, Escape
// steps back. Measured before this change: Alt did nothing at all
// (`keyboardToRibbon.Alt.keytips: 0`), so the ribbon's ~120 controls were
// reachable from the keyboard only by Tab-and-arrow walking.
//
// What is asserted is the GUARANTEE a keyboard user relies on, end to end:
// Alt, R, and the Track changes letter really turns tracking on — read off the
// engine-backed mode the status bar reflects, not off a badge — and the letters
// typed to the tips never reach the document. The refusals are asserted as hard
// as the feature: Alt as a modifier, Alt in a text field and Alt during IME
// composition must all leave the keyboard exactly where it was.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  MOD,
} from "./fixtures.mjs";

const tip = (page, target) => page.locator(`.key-tip[data-target="${target}"]:not([hidden])`);
const layer = (page) => page.locator(".key-tips");

async function mirrorText(page) {
  return page.locator("#a11yDocument").textContent();
}

test("Alt, R, then the Track changes tip turns tracking on, and no letter reaches the document", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const before = await mirrorText(page);

  await page.keyboard.press("Alt");
  await expect(tip(page, "tabReview")).toHaveText("R");
  await expect(tip(page, "tabHome")).toHaveText("H");
  await expect(tip(page, "tabFile")).toHaveText("F");

  await page.keyboard.press("r");
  await expect(page.locator("#panelReview")).toBeVisible();
  await expect(tip(page, "tabReview")).toHaveCount(0);
  const letter = (await tip(page, "reviewTrackBtn").textContent()).trim();
  expect(letter).toMatch(/^[A-Z0-9]{1,2}$/);

  for (const key of letter.toLowerCase()) await page.keyboard.press(key);
  await expect(page.locator("#reviewTrackBtn")).toHaveAttribute("aria-pressed", "true");
  await expect(
    page.locator('#reviewModeControl [data-review-mode="suggesting"]'),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(layer(page)).toBeHidden();
  // Neither the "r" nor the tip's letter was typed into the document.
  expect(await mirrorText(page)).toBe(before);
  expect(consoleErrors).toEqual([]);
});

test("Escape steps back from a band's tips to the tabs', then puts them away", async ({
  page,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press("Alt");
  await page.keyboard.press("h");
  await expect(tip(page, "bold")).toHaveText("B");
  await page.keyboard.press("Escape");
  await expect(tip(page, "bold")).toHaveCount(0);
  await expect(tip(page, "tabReview")).toHaveText("R");
  await page.keyboard.press("Escape");
  await expect(layer(page)).toBeHidden();
  // And the keyboard is the document's again.
  const before = await mirrorText(page);
  await page.keyboard.type("KT");
  await expect.poll(() => mirrorText(page)).not.toBe(before);
});

test("Alt held as a modifier never enters key tips, and a chord while they are up still runs", async ({
  page,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // Alt with another key in between — the shape of Alt+Shift+Arrow, which sizes a
  // table column — is a modifier, not a tap.
  await page.keyboard.down("Alt");
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.up("Alt");
  await expect(layer(page)).toBeHidden();

  // With the tips up, an application chord leaves them AND reaches its command.
  await page.keyboard.press("Alt");
  await expect(tip(page, "tabReview")).toBeVisible();
  await page.keyboard.press(`${MOD}+Shift+E`);
  await expect(layer(page)).toBeHidden();
  await expect(
    page.locator('#reviewModeControl [data-review-mode="suggesting"]'),
  ).toHaveAttribute("aria-pressed", "true");
});

test("Alt in a text field belongs to the field: no key tips", async ({ page }) => {
  await gotoEditor(page);
  const zoom = page.locator("#zoom");
  await zoom.focus();
  await page.keyboard.press("Alt");
  await expect(layer(page)).toBeHidden();
  await page.keyboard.press("r");
  // The letter went where the person was typing, and no tab moved.
  await expect(zoom).toBeFocused();
  await expect(page.locator("#tabHome")).toHaveAttribute("aria-selected", "true");
});

test("Alt during IME composition does nothing", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // A REAL composition, driven through Chromium's own IME input path — not a
  // synthetic CompositionEvent, which no IME produces (`SKILL.md` §4).
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Input.imeSetComposition", { text: "に", selectionStart: 1, selectionEnd: 1 });
  await page.keyboard.press("Alt");
  await expect(layer(page)).toBeHidden();
  await cdp.send("Input.insertText", { text: "に" });
});
