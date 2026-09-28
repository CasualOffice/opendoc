import { test, expect } from "./fixtures.mjs";

// The site's language control (`109` HF-198).
//
// The unit tests hold the seam's rules against hand-built objects. What only a
// browser can answer is whether the thing is REACHABLE: that a real reader can
// find the control on any page, that choosing a language actually changes the
// words in front of them, that it is still chosen on the next page, and that an
// Arabic page turns around without falling apart.
//
// Every page of the site, because the control lives in the shared header and a
// picker that is on four pages out of five is a picker a reader loses.
const PAGES = ["/", "/docs.html", "/embedding.html", "/fidelity.html", "/playground.html"];

const picker = (page) => page.locator("#siteLanguage");

test("the language control is reachable on every page, with every shipped language in it", async ({
  page,
  consoleErrors,
}) => {
  for (const path of PAGES) {
    await page.goto(path);
    await expect(picker(page)).toBeVisible();
    // Nineteen languages and the automatic entry. Each language names itself:
    // a picker that lists "German" cannot be used by the reader who needs it.
    await expect(picker(page).locator("option")).toHaveCount(20);
    await expect(picker(page).locator("option", { hasText: "Deutsch" })).toHaveCount(1);
    await expect(picker(page).locator("option", { hasText: "日本語" })).toHaveCount(1);
    await expect(picker(page).locator("option", { hasText: "العربية" })).toHaveCount(1);
  }
  // Reference pages sit one directory down and load the module from `../`; if
  // that rebase were wrong the request would 404 and the page would be English
  // with an empty, hidden control.
  await page.goto("/reference/security.html");
  await expect(picker(page)).toBeVisible();
  await expect(picker(page).locator("option")).toHaveCount(20);
  expect(consoleErrors).toEqual([]);
});

test("choosing a language changes the words, and is still chosen on the next page", async ({
  page,
}) => {
  await page.goto("/");
  const nav = page.locator("header nav.site-nav"); // by CLASS, not by aria-label: the label is itself translated
  await expect(nav.getByRole("link", { name: "Overview", exact: true })).toBeVisible();

  await picker(page).selectOption("de");
  // The relabel is the whole point: German words, from the catalogue, in place.
  await expect(nav.getByRole("link", { name: "Überblick", exact: true })).toBeVisible();
  await expect(page.locator("header .header-cta")).toHaveText("Editor öffnen");
  await expect(page.locator("html")).toHaveAttribute("lang", "de");

  // And it survives a navigation, because the choice is persisted where the
  // editor keeps it rather than held in a page's memory.
  await page.goto("/docs.html");
  await expect(nav.getByRole("link", { name: "Überblick", exact: true })).toBeVisible();
  await expect(picker(page)).toHaveValue("de");

  // Back to English through the same control — a picker you cannot use to
  // escape is the failure this one exists to avoid.
  await picker(page).selectOption("en");
  await expect(nav.getByRole("link", { name: "Overview", exact: true })).toBeVisible();
});

test("?lang= names a locale, so a bug report and a screenshot can", async ({ page }) => {
  await page.goto("/?lang=ja");
  await expect(page.locator("html")).toHaveAttribute("lang", "ja");
  await expect(page.locator("header .header-cta")).toHaveText("エディターを開く");
});

test("an English reader pays nothing: no catalogue is fetched", async ({ page }) => {
  // English is the served markup. Fetching `locales/en.json` to replace every
  // string with the string already there would be a request and a relabel for
  // no change at all — and it is the obvious way to write this module wrong.
  const catalogueRequests = [];
  page.on("request", (request) => {
    if (request.url().includes("/locales/")) catalogueRequests.push(request.url());
  });
  await page.goto("/");
  await expect(picker(page)).toBeVisible();
  expect(catalogueRequests).toEqual([]);
});

test("Arabic turns the chrome around, at both widths, without a sideways scrollbar", async ({
  page,
}) => {
  for (const size of [
    { width: 1280, height: 900 },
    { width: 390, height: 844 },
  ]) {
    await page.setViewportSize(size);
    await page.goto("/?lang=ar");
    await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
    await expect(page.locator("html")).toHaveAttribute("lang", "ar");
    await expect(page.locator("header .header-cta")).toHaveText("افتح المحرر");

    // The document must not scroll sideways in either direction. This is the
    // assertion that catches a mirrored layout pushing content off the edge,
    // which is the usual way a right-to-left page ships half-done.
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `horizontal overflow at ${size.width}px`).toBeLessThanOrEqual(1);

    // And the decorative arrows are mirrored rather than still pointing at the
    // margin the reader is reading away from.
    const arrow = page.locator(".site-arrow").first();
    if (await arrow.count()) {
      const transform = await arrow.evaluate((el) => getComputedStyle(el).transform);
      expect(transform, "a decorative arrow is not mirrored in an RTL page").toContain("-1");
    }
  }
});

test("a locale whose catalogue cannot be fetched leaves a readable English page", async ({
  page,
}) => {
  // The failure that must not take the site down with it. English is sitting in
  // the markup beside every key precisely so a 404 degrades to it rather than
  // to a screen of dotted identifiers.
  await page.route("**/locales/fr.json", (route) => route.fulfill({ status: 404, body: "" }));
  await page.goto("/?lang=fr");
  const nav = page.locator("header nav.site-nav"); // by CLASS, not by aria-label: the label is itself translated
  await expect(nav.getByRole("link", { name: "Overview", exact: true })).toBeVisible();
  await expect(page.locator("header .header-cta")).toHaveText("Open the editor");
});
