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

test("an untranslated string reads as ENGLISH, never as its key", async ({
  page,
  consoleErrors,
}) => {
  // The markup carries its English beside every `data-i18n` key, and that is
  // what makes translating incremental: routing a surface through the seam and
  // translating it are two different days' work. Between them the product must
  // stay readable — a chrome full of `panelReview.acceptAllChanges.label` is
  // worse than a chrome in English.
  await openIn(page, "de");
  const leaked = await page.evaluate(() =>
    [...document.querySelectorAll("[data-i18n]")]
      .map((element) => element.textContent.trim())
      .filter((text) => /^[a-z][\w]*(\.[\w]+){2,}$/.test(text))
      .slice(0, 5),
  );
  expect(leaked, "these elements are showing their key instead of words").toEqual([]);
  // And the attributes, which a screen reader reads and a mouse user hovers.
  const leakedAttributes = await page.evaluate(() =>
    [...document.querySelectorAll("[data-i18n-label], [data-i18n-title]")]
      .flatMap((element) => [element.getAttribute("aria-label"), element.getAttribute("title")])
      .filter((value) => value && /^[a-z][\w]*(\.[\w]+){2,}$/.test(value))
      .slice(0, 5),
  );
  expect(leakedAttributes).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("relabelling replaces an element's own words and keeps what it contains", async ({
  page,
  consoleErrors,
}) => {
  // Half the labels in this editor are `<label>Initials<input …></label>`.
  // `textContent = …` is the obvious way to relabel one and it deletes the
  // input — every field in Settings and Document properties, gone on the first
  // language change.
  await openIn(page, "de");
  await page.locator("#settingsBtn").click();
  await expect(page.locator("#authorInitials")).toBeVisible();
  await expect(page.locator("#authorName")).toBeVisible();
  const controls = await page.evaluate(
    () => document.querySelectorAll("#settingsPanel input, #settingsPanel select").length,
  );
  expect(controls, "the settings form lost its controls to a relabel").toBeGreaterThan(6);
  // The label still says something, and it is the label's own words that moved.
  await expect(page.locator('label:has(#authorInitials)')).toContainText(/\w/);
  expect(consoleErrors).toEqual([]);
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

test("the status bar names the language, and is a way back out", async ({
  page,
  consoleErrors,
}) => {
  // Word and Docs both put language at the bottom right. It matters more here
  // than status usually does: a person who cannot read the chrome should not
  // have to open a dialog written in a language they cannot read to change it,
  // so the control names the language in ITSELF and opens the picker.
  await openIn(page, "fr");
  await expect(page.locator("#languageStatusLabel")).toHaveText("Français");
  await page.locator("#languageStatus").click();
  await expect(page.locator("#settingsPanel")).toBeVisible();

  // And the CHOICE wins over `?lang=`. That parameter exists so a bug report
  // can name a locale; once someone has picked one from the dialog in front of
  // them, re-deriving the answer put the URL's locale straight back and the
  // picker appeared to do nothing.
  await page.locator("#languageSelect").selectOption("ja");
  await expect(page.locator("#languageStatusLabel")).toHaveText("日本語");
  await expect(page.locator("html")).toHaveAttribute("lang", "ja");
  await expect(page.locator("#statWords")).toHaveText(/単語$/);
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

test("no routed string shows its English at FIRST PAINT", async ({ page, consoleErrors }) => {
  // The class of defect this catches, rather than the three instances of it
  // that prompted it.
  //
  // `localizeTree` sweeps the markup once at boot. Anything the shell writes
  // AFTER that sweep — a tooltip restored from a boot-time snapshot, an
  // aria-label recomputed on every state sync, a status pill repainted when
  // the document saves — overwrites the translation with the English literal
  // it was built from, and the control stays English until the user switches
  // language and forces a re-sweep. Which is to say: the bug is invisible to
  // anyone testing by switching language, and visible to every user who
  // simply opens the editor in their own.
  //
  // So this asserts at first paint, without touching the picker, and does it
  // by comparing against the CATALOGUES rather than a list of expected
  // strings: any routed key whose English and translation differ must not be
  // showing the English.
  const [english, german] = await Promise.all([
    page.request.get("/locales/en.json").then((r) => r.json()),
    page.request.get("/locales/de.json").then((r) => r.json()),
  ]);

  await page.goto("/editor.html?fixture=rich&lang=de");
  await page.waitForFunction(() => document.body.dataset.fontsReady === "true");

  const leaked = await page.evaluate(
    ({ en, de }) => {
      const out = [];
      const check = (key, actual) => {
        const source = en[key];
        const translated = de[key];
        if (!source || !translated || source === translated) return;
        if (actual === source) out.push(`${key} is showing "${source}"`);
      };
      for (const el of document.querySelectorAll("[data-i18n]")) {
        check(el.dataset.i18n, el.textContent.trim());
      }
      for (const el of document.querySelectorAll("[data-i18n-title]")) {
        check(el.dataset.i18nTitle, el.getAttribute("title"));
      }
      for (const el of document.querySelectorAll("[data-i18n-label]")) {
        check(el.dataset.i18nLabel, el.getAttribute("aria-label"));
      }
      return out;
    },
    { en: english, de: german },
  );

  expect(leaked, "these controls were re-rendered from English after the sweep").toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("the strings the shell writes itself are localised too", async ({ page }) => {
  // The three that were wrong, named — a routed key can regress without the
  // sweep guard above noticing if its English and its translation ever
  // coincide, and these are the ones a person reads constantly.
  await page.goto("/editor.html?fixture=rich&lang=de");
  await page.waitForFunction(() => document.body.dataset.fontsReady === "true");
  await expect(page.locator("#statPages")).toHaveText(/^Seite \d+ von/);
  await expect(page.locator("#documentStateText")).toHaveText("Geöffnet");
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", /Rückgängig/);
});

test("the sweep does not re-assert text the shell owns", async ({ page, consoleErrors }) => {
  // `#status` is transient status the shell writes and clears. Giving it a
  // `data-i18n` key made `localizeTree` write "Loading engine…" back over the
  // cleared value every time a catalogue landed, so the no-document editor
  // said it was still loading — forever. The markup text is the PRE-BOOT
  // placeholder, on screen before any script runs and therefore before a
  // catalogue could exist; English is the only thing it can honestly say, and
  // the shell owns it from the first frame onwards.
  await page.goto("/editor.html?blank=1");
  // The SETTLED value, not merely a value. A poll for "not the loading text"
  // passes on the instant between the shell clearing it and the sweep putting
  // it back, which is exactly the window the defect lived in — the first
  // version of this test did that and stayed green with the key restored.
  await page.waitForFunction(
    () => document.getElementById("status")?.textContent !== "Loading engine…",
    null,
    { timeout: 20_000 },
  );
  await page.waitForTimeout(1_500);
  await expect(page.locator("#status")).not.toHaveText("Loading engine…");

  // And with a document, where the shell clears it outright.
  await page.goto("/editor.html?fixture=rich&lang=de");
  await page.waitForFunction(() => document.body.dataset.fontsReady === "true");
  await expect(page.locator("#status")).toHaveText("");
  expect(consoleErrors).toEqual([]);
});
