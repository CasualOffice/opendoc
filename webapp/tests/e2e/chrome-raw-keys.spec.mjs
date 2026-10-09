// No chrome string is ever a catalogue key — in either chrome, at any width.
//
// The phone half of this lives in `phone-command-surface.spec.mjs`, because the
// `phone` Playwright project only runs `phone-*.spec.mjs`; this file is the
// desktop project's half, and it covers the three places the defect class can
// show up there: the ribbon chrome, the compact chrome on a laptop, and the
// phone RUNG reached by narrowing a laptop window (a rung is not a device —
// `playwright.config.mjs` — but it renders the same phone roster).
//
// The sweep itself is `raw-keys.mjs`. What this file adds is the two conditions
// under which a key leaks, each created deliberately rather than hoped for:
//
//   * the bar renders BEFORE the catalogue arrives. That is not a race a test has
//     to win: `setChromeMode` renders the compact bar synchronously at module
//     load, and `locales/en.json` is a fetch, so with compact chrome stored as
//     the preference every boot is the losing order;
//   * the catalogue NEVER arrives. `localize.mjs` promises that "a catalogue
//     that fails to load degrades to English rather than to a screen of key
//     names" — for the markup. This holds the script-built chrome to the same
//     promise.
import { clickIntoFirstPage, expect, gotoEditor, test } from "./fixtures.mjs";
import { rawKeyLeaks } from "./raw-keys.mjs";

const COMPACT_AT_BOOT = () => {
  // The stored preference, exactly as a returning compact-chrome user has it.
  localStorage.setItem("opendoc.chromeMode", "compact");
};

test("the ribbon chrome names nothing with a catalogue key", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await expect(page.locator("body")).toHaveClass(/ribbon-mode/);
  expect(await rawKeyLeaks(page)).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("the compact chrome names nothing with a catalogue key when it renders before the catalogue", async ({
  page,
  consoleErrors,
}) => {
  await page.addInitScript(COMPACT_AT_BOOT);
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await expect(page.locator("body")).toHaveClass(/compact-mode/);
  // The contextual Table trigger is on the bar from the first render, hidden
  // until the caret is in a table; its name is read hidden or not, because a
  // screen reader that reaches it later reads the name it was given then.
  await expect(page.locator("#compactTableBtn")).toHaveCount(1);
  expect(await rawKeyLeaks(page)).toEqual([]);
  // And named in words, not merely in something that is not a key.
  await expect(page.locator("#compactTableBtn")).toHaveAttribute("aria-label", "Table");
  expect(consoleErrors).toEqual([]);
});

test("the phone rung's sheets are named in words, not keys", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await gotoEditor(page);
  await expect(page.locator("body")).toHaveClass(/phone-mode/);
  await clickIntoFirstPage(page);
  await expect(page.locator("#compactFormatBtn")).toHaveAttribute("aria-label", "Format");
  await expect(page.locator("#compactFormatBtn")).toHaveAttribute("title", "Format");
  await expect(page.locator("#compactInsertBtn")).toHaveAttribute("aria-label", "Insert");
  for (const [trigger, sheet] of [
    ["#compactFormatBtn", "#compactFormatMenu"],
    ["#compactInsertBtn", "#compactInsertMenu"],
  ]) {
    await page.locator(trigger).click();
    await expect(page.locator(sheet)).toBeVisible();
    expect(await rawKeyLeaks(page), `with ${sheet} open`).toEqual([]);
    await page.keyboard.press("Escape");
  }
  expect(consoleErrors).toEqual([]);
});

test("a language chosen at boot relabels the compact bar once its catalogue lands", async ({
  page,
  consoleErrors,
}) => {
  // The other half of caching a name at render time: even a name resolved
  // CORRECTLY is resolved in the language in force at that moment, and the bar
  // renders before any catalogue but English's script strings is here. The
  // markup is relabelled by `localizeTree` when the catalogue lands; a
  // script-built control that is not addressable by that sweep stays English
  // for the session. Russian, because its word for Table shares no letters with
  // the English one, so the assertion cannot pass on an untranslated label.
  await page.addInitScript(COMPACT_AT_BOOT);
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page, "&lang=ru");
  const russian = await page.evaluate(async () => (await (await fetch("/locales/ru.json")).json())["appMenuBar.table"]);
  expect(russian, "the Russian catalogue names the Table menu").toBeTruthy();
  await expect(page.locator("#compactTableBtn")).toHaveAttribute("aria-label", russian);
  await expect(page.locator("#compactTableMenu")).toHaveAttribute("aria-label", russian);
  expect(await rawKeyLeaks(page)).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("a catalogue that never arrives leaves the compact bar in English, not in keys", async ({ page }) => {
  // No console assertion here, deliberately: the refused request IS a console
  // error, and the thing under test is what the chrome says despite it.
  await page.route("**/locales/en.json", (route) => route.abort());
  await page.addInitScript(COMPACT_AT_BOOT);
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await expect(page.locator("body")).toHaveClass(/compact-mode/);
  expect(await rawKeyLeaks(page)).toEqual([]);
  await expect(page.locator("#compactTableBtn")).toHaveAttribute("aria-label", "Table");
});
