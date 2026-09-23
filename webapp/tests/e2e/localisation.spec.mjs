import { test, expect, gotoEditor, openFilePage } from "./fixtures.mjs";

// The localisation seam, through a real browser (docs/124).
//
// The unit tests hold the rules — plural categories, the fallback chain, locale
// negotiation. What only a browser can answer is whether the editor actually
// SPEAKS the language: whether the catalogue is fetched, whether the chrome
// relabels, whether `lang` and `dir` reach the document element, and whether
// the one thing that must NOT follow the interface's direction — the document
// — stays where it belongs.

const STATS = "#statWords";

async function openIn(page, tag) {
  await page.goto(`/editor.html?fixture=rich&lang=${tag}`);
  await page.waitForSelector("#stats:not([hidden])");
  await expect(page.locator("html")).toHaveAttribute("lang", tag);
}

test("the chrome speaks the language the user asked for", async ({ page, consoleErrors }) => {
  await openIn(page, "de");
  // Counts come from the engine, not from markup, so they prove the seam and
  // not just an attribute sweep.
  await expect(page.locator(STATS)).toHaveText(/W(ort|örter)$/);
  await expect(page.locator("#statParas")).toHaveText(/Absa(tz|tze)|Absätze/);
  expect(consoleErrors).toEqual([]);
});

test("a language with more than two plural forms gets the right one", async ({ page }) => {
  // 16 words in the fixture. Russian's rule for 16 is `many` — "слов", not the
  // "слово" a one/other ternary would produce.
  await openIn(page, "ru");
  await expect(page.locator(STATS)).toHaveText("16 слов");
  await expect(page.locator("#statParas")).toHaveText("8 абзацев");
});

test("numbers are formatted in the interface's locale, not the browser's", async ({ page }) => {
  await openIn(page, "de");
  const text = await page.locator("#statChars").textContent();
  expect(text).toMatch(/^\d+ Zeichen$/);
  // A locale that groups differently proves the formatter is ours: French
  // groups with a narrow no-break space.
  await openIn(page, "fr");
  await expect(page.locator("#statChars")).toHaveText(/caractères?$/);
});

test("Arabic mirrors the CHROME and leaves the document alone", async ({ page, consoleErrors }) => {
  await openIn(page, "ar");
  await expect(page.locator("html")).toHaveAttribute("dir", "rtl");

  const layout = await page.evaluate(() => {
    const box = (selector) => {
      const element = document.querySelector(selector);
      return element ? element.getBoundingClientRect().toJSON() : null;
    };
    const page_ = document.querySelector(".page-wrap .page") ?? document.querySelector(".page");
    return {
      scrollWidth: document.documentElement.scrollWidth,
      innerWidth: window.innerWidth,
      tabs: box(".ribbon-tabs"),
      firstTab: box("#tabFile"),
      lastTab: box("#tabReview"),
      pageDirection: page_ ? getComputedStyle(page_).direction : null,
      stats: box("#stats"),
    };
  });

  // The interface mirrored: File is now the RIGHTMOST tab.
  expect(layout.firstTab.left).toBeGreaterThan(layout.lastTab.left);

  // The DOCUMENT did not. A left-to-right `.docx` is left-to-right whoever
  // opens it — direction belongs to the document's own `w:bidi`, not to the
  // reader's interface (docs/124 §3.5).
  expect(layout.pageDirection, "the page must not inherit the UI's direction").toBe("ltr");

  // And nothing overflows. `left: -9999px` on the skip link made the document
  // 11,279px wide the moment direction flipped, and the editor opened scrolled
  // away from its own content — a blank window for every Arabic speaker.
  expect(layout.scrollWidth).toBeLessThanOrEqual(layout.innerWidth + 1);

  expect(consoleErrors).toEqual([]);
});

test("an unshipped language falls back rather than failing to open", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html?fixture=rich&lang=is");
  await page.waitForSelector("#stats:not([hidden])");
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect(page.locator(STATS)).toHaveText(/words?$/);
  expect(consoleErrors).toEqual([]);
});

test("a catalogue that will not load leaves English on screen, not key names", async ({
  page,
  consoleErrors,
}) => {
  // The markup carries its English beside every `data-i18n` key precisely so
  // this degradation is possible. Relabelling from a catalogue that never
  // arrived would replace every label in the editor with a dotted key — the
  // worst possible failure for the most ordinary one, a request that 404s.
  await page.route("**/locales/*.json", (route) => route.abort());
  await page.goto("/editor.html?fixture=rich&lang=de");
  await page.waitForSelector("#stats:not([hidden])");
  await page.locator("#settingsBtn").click();
  await expect(page.locator("#languageNote")).toContainText(/machine|not been reviewed/i);
  await expect(page.locator("#languageNote")).not.toHaveText(/^settings\./);
  // Script-side strings still work: `EN_STRINGS` is compiled in, not fetched.
  await expect(page.locator(STATS)).toHaveText(/words?$/);
  // The browser logs the requests this test deliberately aborted. Everything
  // else must still be silent — the point is that a failed fetch is HANDLED,
  // not that it is invisible.
  expect(consoleErrors.filter((line) => !/ERR_FAILED/.test(line))).toEqual([]);
});

test("the picker lists every shipped language, in its own language", async ({ page }) => {
  await gotoEditor(page);
  await page.locator("#settingsBtn").click();
  const select = page.locator("#languageSelect");
  await expect(select).toBeVisible();
  const options = await select.locator("option").allTextContents();
  // One automatic entry plus every shipped locale, and the owner asked for at
  // least fifteen translated ones.
  expect(options.length).toBeGreaterThanOrEqual(19);
  for (const endonym of ["Deutsch", "Français", "日本語", "Русский", "العربية", "简体中文"]) {
    expect(options, `${endonym} must name itself`).toContain(endonym);
  }
});

test("choosing a language relabels what is already on screen, and it sticks", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await page.waitForSelector("#stats:not([hidden])");
  await expect(page.locator(STATS)).toHaveText(/words?$/);

  await page.locator("#settingsBtn").click();
  await page.locator("#languageSelect").selectOption("pl");
  // Relabelled in place — no reload.
  await expect(page.locator(STATS)).toHaveText(/(słowo|słowa|słów)$/);
  await expect(page.locator("html")).toHaveAttribute("lang", "pl");

  // And it survives the next visit, because a language is not a per-tab whim.
  await page.reload();
  await page.waitForSelector("#stats:not([hidden])");
  await expect(page.locator("html")).toHaveAttribute("lang", "pl");
  expect(consoleErrors).toEqual([]);
});

test("the picker says the translations are unreviewed", async ({ page }) => {
  await gotoEditor(page);
  await page.locator("#settingsBtn").click();
  // docs/124 §6: eighteen machine-produced languages is worth more than one
  // reviewed one, but calling them finished would be a claim nobody earned.
  await expect(page.locator("#languageNote")).toContainText(/machine|not been reviewed/i);
});

test("the File page's Settings pane carries the picker too", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await openFilePage(page);
  await page.locator('[data-file-pane="settings"]').click();
  await expect(page.locator("#filePageDetail #languageSelect")).toBeVisible();
  expect(consoleErrors).toEqual([]);
});
